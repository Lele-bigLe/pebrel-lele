//! Incremental, process-local aggregates. Native files are never modified.

use super::{AppUsage, Issue, NativeReport, Period, SessionMetrics};
use crate::ai_agents::AgentKind;
use chrono::{Datelike as _, Local, NaiveDate};
use std::collections::{BTreeMap, HashSet};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

mod metrics;
mod opencode;
mod records;

#[derive(Clone, Copy, Default)]
struct Count {
    tokens: u64,
    records: u64,
}

#[derive(Default)]
struct FileUsage {
    offset: u64,
    length: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    days: BTreeMap<NaiveDate, Count>,
    previous: Option<(u64, u64)>,
    messages: BTreeMap<String, (NaiveDate, u64, u64)>,
    session: String,
    model: String,
    started: Option<i64>,
    output: u64,
    partial_metrics: bool,
    forked: bool,
    metric: Option<SessionMetrics>,
    invalid: bool,
    discarding: bool,
    tail: Vec<u8>,
}

#[derive(Default)]
struct Cache {
    month: Option<NaiveDate>,
    offset: i32,
    files: BTreeMap<PathBuf, FileUsage>,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

pub(crate) fn read(period: Period, selected: Vec<(String, String)>) -> Result<NativeReport, Issue> {
    let now = Local::now();
    let month = now.date_naive().with_day(1).ok_or(Issue::DateBoundary)?;
    let start = if period == Period::Today { now.date_naive() } else { month };
    let mut cache = CACHE.get_or_init(Mutex::default).lock().map_err(|_| Issue::Storage)?;
    if cache.month != Some(month) || cache.offset != now.offset().local_minus_utc() {
        cache.files.clear();
        cache.month = Some(month);
        cache.offset = now.offset().local_minus_utc();
    }
    let (mut files, discovery_partial) = crate::ai_sessions::usage_files();
    files.retain(|s| {
        chrono::DateTime::<Local>::from(s.modified).date_naive() >= month && s.path.is_some()
    });
    files.sort_by_key(|s| std::cmp::Reverse(s.modified));
    let mut partial = discovery_partial || files.len() > 4096;
    files.truncate(4096);
    let paths: HashSet<_> = files.iter().filter_map(|s| s.path.clone()).collect();
    cache.files.retain(|path, _| paths.contains(path));
    let mut budget = 16 * 1024 * 1024u64;
    let mut seen = HashSet::new();
    let mut apps = BTreeMap::<String, Count>::new();
    let mut metrics = Vec::new();
    for session in files {
        let path = session.path.unwrap();
        let file = cache.files.entry(path.clone()).or_default();
        if file.session.is_empty() {
            file.session.clone_from(&session.id);
        }
        if update(file, &path, session.source, month, &mut budget).is_err() {
            partial = true;
            continue;
        }
        partial |= file.invalid || file.offset < file.length;
        let app = session.source.slug().to_owned();
        let identity = if file.session.is_empty() {
            path.to_string_lossy().into_owned()
        } else {
            file.session.clone()
        };
        if !seen.insert((app.clone(), identity)) {
            continue;
        }
        let total = apps.entry(app.clone()).or_default();
        for (_, count) in file.days.range(start..=now.date_naive()) {
            total.tokens = total.tokens.saturating_add(count.tokens);
            total.records = total.records.saturating_add(count.records);
        }
        if selected.iter().any(|(source, id)| source == &app && id == &file.session)
            && let Some(metric) = &file.metric
        {
            if file.offset < file.length {
                if let Some(recent) = metrics::recent(&path, session.source, &file.session, month) {
                    metrics.push(recent);
                }
            } else {
                metrics.push(metric.clone());
            }
        } else if selected.iter().any(|(source, id)| source == &app && id == &file.session)
            && file.offset < file.length
            && let Some(recent) = metrics::recent(&path, session.source, &file.session, month)
        {
            metrics.push(recent);
        }
    }
    let mut report = NativeReport {
        apps: apps
            .into_iter()
            .filter(|(_, c)| c.records > 0)
            .map(|(app, c)| AppUsage {
                app,
                requests: c.records.min(i64::MAX as u64) as i64,
                tokens: c.tokens.min(i64::MAX as u64) as i64,
            })
            .collect(),
        metrics,
        partial,
    };
    if opencode::append(&mut report, period, &selected).is_err() {
        report.partial = true;
    }
    Ok(report)
}

fn update(
    state: &mut FileUsage,
    path: &std::path::Path,
    source: AgentKind,
    month: NaiveDate,
    budget: &mut u64,
) -> Result<(), Issue> {
    let mut file = std::fs::File::open(path).map_err(|_| Issue::Storage)?;
    let meta = file.metadata().map_err(|_| Issue::Storage)?;
    let changed = state.modified != meta.modified().ok()
        || state.length != meta.len()
        || state.created != meta.created().ok();
    if !changed && state.offset == meta.len() {
        return Ok(());
    }
    if state.offset > meta.len()
        || state.created != meta.created().ok()
        || (changed && state.length == meta.len())
    {
        *state = FileUsage::default();
    }
    if changed && source != AgentKind::Gemini && !state.tail.is_empty() {
        file.seek(SeekFrom::Start(state.offset - state.tail.len() as u64))
            .map_err(|_| Issue::Storage)?;
        let mut tail = vec![0; state.tail.len()];
        file.read_exact(&mut tail).map_err(|_| Issue::Storage)?;
        if tail != state.tail {
            *state = FileUsage::default();
        }
    }
    state.created = meta.created().ok();
    state.modified = meta.modified().ok();
    state.length = meta.len();
    if source == AgentKind::Gemini {
        if meta.len() > 8 * 1024 * 1024 || meta.len() > *budget {
            state.invalid = true;
            return Ok(());
        }
        let mut bytes = Vec::new();
        file.take(meta.len()).read_to_end(&mut bytes).map_err(|_| Issue::Storage)?;
        *budget = budget.saturating_sub(bytes.len() as u64);
        if let Ok(value) = serde_json::from_slice(&bytes) {
            let mut fresh = FileUsage {
                session: state.session.clone(),
                created: state.created,
                modified: state.modified,
                length: state.length,
                offset: bytes.len() as u64,
                ..FileUsage::default()
            };
            records::gemini(&mut fresh, &value, month);
            *state = fresh;
        } else {
            state.days.clear();
            state.metric = None;
            state.invalid = true;
        }
        return Ok(());
    }
    let amount = meta.len().saturating_sub(state.offset).min(2 * 1024 * 1024).min(*budget);
    if amount == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::Start(state.offset)).map_err(|_| Issue::Storage)?;
    let mut bytes = Vec::new();
    file.take(amount).read_to_end(&mut bytes).map_err(|_| Issue::Storage)?;
    *budget = budget.saturating_sub(bytes.len() as u64);
    let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        if bytes.len() as u64 == 2 * 1024 * 1024 {
            remember_tail(state, &bytes);
            state.offset += bytes.len() as u64;
            state.discarding = true;
            state.invalid = true;
        }
        return Ok(());
    };
    for line in bytes[..=end].split(|byte| *byte == b'\n') {
        if state.discarding {
            state.discarding = false;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice(line) {
            Ok(record) => records::line(state, source, &record, month),
            Err(_) => state.invalid = true,
        }
    }
    remember_tail(state, &bytes[..=end]);
    state.offset += end as u64 + 1;
    Ok(())
}

fn remember_tail(state: &mut FileUsage, bytes: &[u8]) {
    const ANCHOR_BYTES: usize = 64;
    if bytes.len() >= ANCHOR_BYTES {
        state.tail.clear();
        state.tail.extend_from_slice(&bytes[bytes.len() - ANCHOR_BYTES..]);
    } else {
        state.tail.extend_from_slice(bytes);
        if state.tail.len() > ANCHOR_BYTES {
            state.tail.drain(..state.tail.len() - ANCHOR_BYTES);
        }
    }
}

fn time(value: &serde_json::Value) -> Option<chrono::DateTime<Local>> {
    chrono::DateTime::parse_from_rfc3339(value.as_str()?).ok().map(|t| t.with_timezone(&Local))
}

fn add(state: &mut FileUsage, day: NaiveDate, tokens: u64, month: NaiveDate) {
    if day.year() != month.year() || day.month() != month.month() || tokens == 0 {
        return;
    }
    let row = state.days.entry(day).or_default();
    row.tokens = row.tokens.saturating_add(tokens);
    row.records = row.records.saturating_add(1);
}

fn metric(state: &mut FileUsage, app: &str, ended: i64, output: u64) {
    let duration = state.started.and_then(|start| ended.checked_sub(start)).filter(|d| *d > 0);
    state.metric = Some(SessionMetrics {
        app: app.to_owned(),
        session_id: state.session.clone(),
        model: state.model.clone(),
        created_at: ended / 1000,
        output_tokens: output.min(i64::MAX as u64) as i64,
        first_token_ms: None,
        tokens_per_second: duration
            .filter(|_| output > 0)
            .map(|ms| output as f64 * 1000.0 / ms as f64),
        estimated: true,
        in_progress: true,
    });
}
