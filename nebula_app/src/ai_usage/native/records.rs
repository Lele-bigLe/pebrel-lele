use super::*;
use serde_json::Value;

pub(super) fn line(state: &mut FileUsage, source: AgentKind, record: &Value, month: NaiveDate) {
    match source {
        AgentKind::Codex => codex(state, record, month),
        AgentKind::Claude => claude(state, record, month),
        _ => {},
    }
}

fn codex(state: &mut FileUsage, record: &Value, month: NaiveDate) {
    let Some(payload) = record.get("payload") else { return };
    match record.get("type").and_then(Value::as_str) {
        Some("session_meta") => {
            if let Some(id) = payload.get("id").and_then(Value::as_str) {
                state.session = id.to_owned();
            }
            state.forked = payload.get("forked_from_id").is_some_and(|v| !v.is_null());
            if state.forked {
                state.invalid = true;
            }
        },
        Some("turn_context") => {
            if let Some(model) = payload.get("model").and_then(Value::as_str) {
                state.model = model.to_owned();
            }
        },
        Some("event_msg") => match payload.get("type").and_then(Value::as_str) {
            Some("task_started") => {
                state.started =
                    record.get("timestamp").and_then(time).map(|t| t.timestamp_millis());
                state.output = 0;
                state.partial_metrics &= state.previous.is_none();
                // Keep the last completed turn until this turn has usage samples.
            },
            Some("token_count") => {
                let Some(total) = payload.pointer("/info/total_token_usage") else { return };
                let (Some(input), Some(output)) = (
                    total.get("input_tokens").and_then(Value::as_u64),
                    total.get("output_tokens").and_then(Value::as_u64),
                ) else {
                    state.invalid = true;
                    return;
                };
                let previous = state.previous.replace((input, output)).unwrap_or((0, 0));
                if state.forked && previous == (0, 0) && (input > 0 || output > 0) {
                    state.partial_metrics = true;
                    state.started =
                        record.get("timestamp").and_then(time).map(|at| at.timestamp_millis());
                    return;
                }
                let (Some(input_delta), Some(output_delta)) =
                    (input.checked_sub(previous.0), output.checked_sub(previous.1))
                else {
                    state.invalid = true;
                    return;
                };
                state.output = state.output.saturating_add(output_delta);
                let Some(at) = record.get("timestamp").and_then(time) else {
                    state.invalid = true;
                    return;
                };
                if !state.forked {
                    add(state, at.date_naive(), input_delta.saturating_add(output_delta), month);
                }
                if output_delta > 0 && state.started.is_some() {
                    let output = state.output;
                    metric(state, "codex", at.timestamp_millis(), output);
                }
            },
            Some("task_complete" | "task_completed") => {
                if let Some(at) = record.get("timestamp").and_then(time) {
                    let output = state.output;
                    if output == 0 {
                        state.started = None;
                        return;
                    }
                    metric(state, "codex", at.timestamp_millis(), output);
                    if let Some(metric) = &mut state.metric {
                        metric.in_progress = false;
                        metric.first_token_ms = payload
                            .get("time_to_first_token_ms")
                            .and_then(Value::as_i64)
                            .filter(|ms| *ms >= 0);
                        if let Some(duration) = payload
                            .get("duration_ms")
                            .and_then(Value::as_i64)
                            .filter(|ms| *ms > 0 && !state.partial_metrics)
                        {
                            metric.tokens_per_second =
                                (output > 0).then(|| output as f64 * 1000.0 / duration as f64);
                        }
                    }
                    state.started = None;
                }
            },
            _ => {},
        },
        _ => {},
    }
}

fn claude(state: &mut FileUsage, record: &Value, month: NaiveDate) {
    if let Some(id) = record.get("sessionId").and_then(Value::as_str) {
        state.session = id.to_owned();
    }
    let Some(at) = record.get("timestamp").and_then(time) else { return };
    if record.get("type").and_then(Value::as_str) == Some("user") {
        let tool_result =
            record.pointer("/message/content").and_then(Value::as_array).is_some_and(|parts| {
                !parts.is_empty()
                    && parts
                        .iter()
                        .all(|part| part.get("type").and_then(Value::as_str) == Some("tool_result"))
            });
        if !tool_result {
            state.started = Some(at.timestamp_millis());
            state.output = 0;
            state.partial_metrics = false;
            // Keep the last completed turn until this turn has usage samples.
        }
        return;
    }
    if record.get("type").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    if record.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let Some(message) = record.get("message") else { return };
    let (Some(id), Some(usage)) = (message.get("id").and_then(Value::as_str), message.get("usage"))
    else {
        return;
    };
    if id.len() > 256 {
        state.invalid = true;
        return;
    }
    let (Some(input), Some(output)) = (
        usage.get("input_tokens").and_then(Value::as_u64),
        usage.get("output_tokens").and_then(Value::as_u64),
    ) else {
        state.invalid = true;
        return;
    };
    let cached = usage.get("cache_read_input_tokens").and_then(Value::as_u64).unwrap_or(0);
    let written = usage.get("cache_creation_input_tokens").and_then(Value::as_u64).unwrap_or(0);
    let tokens = input.saturating_add(output).saturating_add(cached).saturating_add(written);
    let mut generated = 0;
    if at.date_naive().year() == month.year()
        && at.date_naive().month() == month.month()
        && tokens > 0
    {
        if state.messages.len() >= 8192 && !state.messages.contains_key(id) {
            state.invalid = true;
            return;
        }
        let entry = state.messages.entry(id.to_owned()).or_insert((at.date_naive(), 0, 0));
        let previous = entry.1;
        let day = entry.0;
        entry.1 = entry.1.max(tokens);
        let output_delta = output.saturating_sub(entry.2);
        generated = output_delta;
        entry.2 = entry.2.max(output);
        state.output = state.output.saturating_add(output_delta);
        let row = state.days.entry(day).or_default();
        row.tokens = row.tokens.saturating_add(tokens.saturating_sub(previous));
        if previous == 0 {
            row.records = row.records.saturating_add(1);
        }
    }
    if let Some(model) = message.get("model").and_then(Value::as_str) {
        state.model = model.to_owned();
    }
    if generated > 0 && state.started.is_some() {
        let output = state.output;
        metric(state, "claude", at.timestamp_millis(), output);
        if let Some(metric) = &mut state.metric {
            metric.in_progress = message.get("stop_reason").is_none_or(Value::is_null);
        }
    }
}

pub(super) fn gemini(state: &mut FileUsage, record: &Value, month: NaiveDate) {
    if let Some(id) = record.get("sessionId").and_then(Value::as_str) {
        state.session = id.to_owned();
    }
    let Some(messages) = record.get("messages").and_then(Value::as_array) else {
        state.invalid = true;
        return;
    };
    let mut seen = HashSet::new();
    for message in messages {
        let Some(at) = message.get("timestamp").and_then(time) else { continue };
        if message.get("type").and_then(Value::as_str) == Some("user") {
            state.started = Some(at.timestamp_millis());
            continue;
        }
        let Some(tokens) = message.get("tokens") else { continue };
        let Some(id) = message.get("id").and_then(Value::as_str) else {
            state.invalid = true;
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let Some(total) = tokens.get("total").and_then(Value::as_u64) else {
            state.invalid = true;
            continue;
        };
        add(state, at.date_naive(), total, month);
        state.model = message.get("model").and_then(Value::as_str).unwrap_or("").to_owned();
        metric(
            state,
            "gemini",
            at.timestamp_millis(),
            tokens
                .get("output")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .saturating_add(tokens.get("thoughts").and_then(Value::as_u64).unwrap_or(0)),
        );
        if let Some(metric) = &mut state.metric {
            metric.in_progress = false;
        }
    }
}
