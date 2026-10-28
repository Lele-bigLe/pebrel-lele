//! A bounded tail supplies current metrics while full usage is still backfilling.
use super::*;

pub(super) fn recent(
    path: &std::path::Path,
    source: AgentKind,
    session: &str,
    month: NaiveDate,
) -> Option<SessionMetrics> {
    if !matches!(source, AgentKind::Codex | AgentKind::Claude) {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let offset = length.saturating_sub(1024 * 1024);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = Vec::new();
    file.take(1024 * 1024).read_to_end(&mut bytes).ok()?;
    let mut lines = bytes.split(|byte| *byte == b'\n');
    if offset > 0 {
        lines.next();
    }
    let mut state = FileUsage {
        session: session.to_owned(),
        partial_metrics: offset > 0,
        ..FileUsage::default()
    };
    for line in lines {
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(line) else { continue };
        if offset > 0
            && source == AgentKind::Codex
            && state.previous.is_none()
            && record.pointer("/payload/type").and_then(|v| v.as_str()) == Some("token_count")
            && let Some(total) = record.pointer("/payload/info/total_token_usage")
        {
            // A lifetime total in a truncated file is a baseline, never new output.
            state.previous =
                Some((total.get("input_tokens")?.as_u64()?, total.get("output_tokens")?.as_u64()?));
            if state.partial_metrics {
                state.started =
                    record.get("timestamp").and_then(time).map(|at| at.timestamp_millis());
            }
            continue;
        }
        if offset > 0 && source == AgentKind::Claude && state.started.is_none() {
            state.started = record.get("timestamp").and_then(time).map(|at| at.timestamp_millis());
        }
        if source == AgentKind::Claude
            && state.partial_metrics
            && state.messages.is_empty()
            && let (Some(id), Some(output), Some(at)) = (
                record.pointer("/message/id").and_then(|value| value.as_str()),
                record.pointer("/message/usage/output_tokens").and_then(|value| value.as_u64()),
                record.get("timestamp").and_then(time),
            )
        {
            state.messages.insert(id.to_owned(), (at.date_naive(), 0, output));
        }
        records::line(&mut state, source, &record, month);
    }
    state.metric
}
