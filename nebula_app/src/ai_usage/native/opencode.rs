use super::*;
use chrono::TimeZone as _;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

pub(super) fn append(
    report: &mut NativeReport,
    period: Period,
    selected: &[(String, String)],
) -> Result<(), Issue> {
    let Some(root) = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::platform::dirs::home_dir().map(|p| p.join(".local/share")))
    else {
        return Ok(());
    };
    let path = root.join("opencode/opencode.db");
    match path.metadata() {
        Ok(metadata) if metadata.is_file() => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        _ => return Err(Issue::Database),
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| Issue::Database)?;
    connection.busy_timeout(std::time::Duration::from_millis(1000)).map_err(|_| Issue::Database)?;
    let now = Local::now();
    let day = if period == Period::Today { now.day() } else { 1 };
    let start = Local
        .with_ymd_and_hms(now.year(), now.month(), day, 0, 0, 0)
        .earliest()
        .ok_or(Issue::DateBoundary)?
        .timestamp_millis();
    let mut statement = connection.prepare("SELECT session_id, time_created, json_extract(data, '$.tokens'), json_extract(data, '$.modelID'), json_extract(data, '$.time.completed') FROM message WHERE time_created >= ?1 AND time_created <= ?2 AND json_valid(data) AND json_extract(data, '$.role') = 'assistant' ORDER BY time_created DESC LIMIT 50001").map_err(|_| Issue::Database)?;
    let rows = statement
        .query_map(rusqlite::params![start, now.timestamp_millis()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|_| Issue::Database)?;
    let mut total = Count::default();
    let mut metrics_seen = HashSet::new();
    for (index, row) in rows.enumerate() {
        if index == 50000 {
            report.partial = true;
            break;
        }
        let (session, created, tokens, model, completed) = row.map_err(|_| Issue::Database)?;
        let Some(tokens) = tokens.and_then(|v| serde_json::from_str::<Value>(&v).ok()) else {
            continue;
        };
        let (Some(input), Some(output)) = (
            tokens.get("input").and_then(Value::as_u64),
            tokens.get("output").and_then(Value::as_u64),
        ) else {
            report.partial = true;
            continue;
        };
        // OpenCode stores non-cached input and visible output as separate buckets.
        let cached = tokens.pointer("/cache/read").and_then(Value::as_u64).unwrap_or(0);
        let written = tokens.pointer("/cache/write").and_then(Value::as_u64).unwrap_or(0);
        let reasoning = tokens.get("reasoning").and_then(Value::as_u64).unwrap_or(0);
        let generated = output.saturating_add(reasoning);
        let amount = tokens.get("total").and_then(Value::as_u64).unwrap_or_else(|| {
            input
                .saturating_add(output)
                .saturating_add(reasoning)
                .saturating_add(cached)
                .saturating_add(written)
        });
        if amount > 0 {
            total.tokens = total.tokens.saturating_add(amount);
            total.records = total.records.saturating_add(1);
        }
        if selected.iter().any(|(app, id)| app == "opencode" && id == &session)
            && generated > 0
            && let Some(ended) = completed.filter(|t| *t > created)
            && metrics_seen.insert(session.clone())
        {
            report.metrics.push(SessionMetrics {
                app: "opencode".to_owned(),
                session_id: session,
                model: model.unwrap_or_default(),
                created_at: ended / 1000,
                output_tokens: generated.min(i64::MAX as u64) as i64,
                first_token_ms: None,
                tokens_per_second: Some(generated as f64 * 1000.0 / (ended - created) as f64),
                estimated: true,
                in_progress: false,
            });
        }
    }
    if total.records > 0 {
        report.apps.push(AppUsage {
            app: "opencode".to_owned(),
            requests: total.records.min(i64::MAX as u64) as i64,
            tokens: total.tokens.min(i64::MAX as u64) as i64,
        });
    }
    Ok(())
}
