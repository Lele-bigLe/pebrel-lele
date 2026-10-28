//! Narrow, bounded token sampling. Conversation content never leaves this reader.

use super::super::Issue;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read as _, Seek as _, SeekFrom};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Model {
    pub name: String,
    pub effort: String,
    pub tier: String,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub(super) struct Tokens {
    pub input: u64,
    pub cached: u64,
    pub output: u64,
}

impl Tokens {
    pub(super) fn total(self) -> u64 {
        self.input.saturating_add(self.output)
    }
    pub(super) fn plus(self, other: Self) -> Self {
        Self {
            input: self.input.saturating_add(other.input),
            cached: self.cached.saturating_add(other.cached),
            output: self.output.saturating_add(other.output),
        }
    }
    fn delta(self, previous: Self) -> Option<Self> {
        let delta = Self {
            input: self.input.checked_sub(previous.input)?,
            cached: self.cached.checked_sub(previous.cached)?,
            output: self.output.checked_sub(previous.output)?,
        };
        (delta.cached <= delta.input).then_some(delta)
    }
}

fn usage(value: &Value) -> Option<Tokens> {
    let tokens = Tokens {
        input: value.get("input_tokens")?.as_u64()?,
        cached: value.get("cached_input_tokens")?.as_u64()?,
        output: value.get("output_tokens")?.as_u64()?,
    };
    (tokens.cached <= tokens.input && tokens.total() < 10_000_000_000).then_some(tokens)
}

fn service_tier(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Null) => Some("default".to_owned()),
        Some(Value::String(tier)) if !tier.is_empty() => Some(tier.chars().take(32).collect()),
        _ => None,
    }
}

pub(super) struct Interval {
    pub model: Option<Model>,
    pub tokens: Tokens,
    pub complete: bool,
    pub requests: u32,
    pub issue: Option<super::SamplingIssue>,
}

pub(super) fn read(since: DateTime<Utc>, until: DateTime<Utc>) -> Result<Interval, Issue> {
    let mut files = crate::ai_sessions::codex_rollouts();
    files.retain(|(_, _, modified)| DateTime::<Utc>::from(*modified) >= since);
    files.sort_by_key(|(_, _, modified)| std::cmp::Reverse(*modified));
    let mut interval = Interval {
        model: None,
        tokens: Tokens::default(),
        complete: files.len() <= 64,
        requests: 0,
        issue: None,
    };
    let mut remaining = 16 * 1024 * 1024_u64;
    let mut seen = std::collections::HashSet::new();
    for (id, path, _) in files.into_iter().take(64) {
        if !seen.insert(id.clone()) {
            continue;
        }
        if remaining == 0 {
            interval.complete = false;
            break;
        }
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(_) => {
                interval.complete = false;
                continue;
            },
        };
        let length = file.metadata().map_err(|_| Issue::Storage)?.len();
        let mut header_bytes = Vec::new();
        (&mut file).take(64 * 1024).read_to_end(&mut header_bytes).map_err(|_| Issue::Storage)?;
        let header = String::from_utf8_lossy(&header_bytes);
        let metadata = header.lines().find_map(|line| {
            let record: Value = serde_json::from_str(line).ok()?;
            (record.get("type")?.as_str()? == "session_meta")
                .then(|| record.get("payload").cloned())
                .flatten()
        });
        let Some(metadata) = metadata else {
            interval.complete = false;
            interval.issue = Some(super::SamplingIssue::Source);
            continue;
        };
        if metadata.get("id").and_then(Value::as_str) != Some(id.as_str())
            || metadata.get("model_provider").and_then(Value::as_str) != Some("openai")
            || metadata.get("forked_from_id").is_some_and(|value| !value.is_null())
            || metadata.get("source").is_some_and(Value::is_object)
        {
            // Parent replay and unclassified providers cannot calibrate an
            // account-wide subscription window without an attribution contract.
            interval.complete = false;
            interval.issue = Some(super::SamplingIssue::Source);
            continue;
        }
        let bytes = length.min(2 * 1024 * 1024).min(remaining);
        remaining -= bytes;
        let offset = length.saturating_sub(bytes);
        file.seek(SeekFrom::Start(offset)).map_err(|_| Issue::Storage)?;
        let mut buffer = Vec::new();
        file.take(bytes).read_to_end(&mut buffer).map_err(|_| Issue::Storage)?;
        let text = String::from_utf8_lossy(&buffer);
        let mut lines = text.lines();
        if offset > 0 {
            lines.next();
        }
        let mut model: Option<Model> = None;
        let mut previous: Option<Tokens> = None;
        let mut tier: Option<String> = None;
        let mut first_time = None;
        for line in lines {
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                interval.complete = false;
                continue;
            };
            let Some(payload) = record.get("payload") else { continue };
            let time = record
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
                .map(|time| time.with_timezone(&Utc));
            if first_time.is_none() {
                first_time = time;
            }
            match record.get("type").and_then(Value::as_str) {
                Some("turn_context") => {
                    if let Some(value) = service_tier(payload.get("service_tier")) {
                        tier = Some(value);
                    }
                    model = payload
                        .get("model")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(|name| Model {
                            name: name.chars().take(128).collect(),
                            effort: payload
                                .get("effort")
                                .or_else(|| payload.get("reasoning_effort"))
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .chars()
                                .take(32)
                                .collect(),
                            tier: tier.clone().unwrap_or_else(|| "unknown".to_owned()),
                        });
                },
                Some("event_msg")
                    if payload.get("type").and_then(Value::as_str)
                        == Some("thread_settings_applied") =>
                {
                    if let Some(value) = service_tier(payload.get("service_tier")) {
                        tier = Some(value);
                        if let Some(model) = &mut model {
                            model.tier = tier.clone().unwrap();
                        }
                    }
                },
                Some("event_msg")
                    if payload.get("type").and_then(Value::as_str) == Some("token_count") =>
                {
                    let Some(total) = payload.pointer("/info/total_token_usage").and_then(usage)
                    else {
                        continue;
                    };
                    let delta = match previous {
                        Some(previous) => total.delta(previous),
                        None if offset == 0 => Some(total),
                        None => None,
                    };
                    previous = Some(total);
                    let Some(time) = time else {
                        interval.complete = false;
                        continue;
                    };
                    if time <= since || time > until {
                        continue;
                    }
                    let Some(delta) = delta else {
                        interval.complete = false;
                        continue;
                    };
                    if delta.total() == 0 {
                        continue;
                    }
                    let Some(model) = &model else {
                        interval.complete = false;
                        continue;
                    };
                    if interval.model.as_ref().is_some_and(|current| current != model) {
                        interval.complete = false;
                        interval.issue = Some(super::SamplingIssue::Mixed);
                    }
                    interval.model = Some(model.clone());
                    interval.tokens = interval.tokens.plus(delta);
                    interval.requests = interval.requests.saturating_add(1);
                },
                _ => {},
            }
        }
        if offset > 0 && first_time.is_none_or(|time| time > since) {
            interval.complete = false;
        }
    }
    Ok(interval)
}
