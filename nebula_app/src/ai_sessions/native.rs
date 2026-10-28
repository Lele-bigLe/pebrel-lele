//! Read native indexes without copying transcripts or changing CLI storage.

use super::*;
use rusqlite::{Connection, OpenFlags};

pub(super) fn apply_codex_titles(sessions: &mut [AiSession]) {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let Some(path) = crate::codex_config::codex_home().map(|root| root.join("session_index.jsonl"))
    else {
        return;
    };
    let Ok(mut file) = std::fs::File::open(path) else { return };
    let Ok(metadata) = file.metadata() else { return };
    const LIMIT: u64 = 2 * 1024 * 1024;
    if file.seek(SeekFrom::Start(metadata.len().saturating_sub(LIMIT))).is_err() {
        return;
    }
    let mut bytes = Vec::new();
    if file.take(LIMIT).read_to_end(&mut bytes).is_err() {
        return;
    }
    let mut titles = HashMap::new();
    for line in String::from_utf8_lossy(&bytes).lines() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let (Some(id), Some(name), Some(updated)) = (
            record.get("id").and_then(|v| v.as_str()),
            record.get("thread_name").and_then(|v| v.as_str()),
            record.get("updated_at").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let Ok(time) = chrono::DateTime::parse_from_rfc3339(updated) else { continue };
        if name.trim().is_empty() {
            continue;
        }
        let entry = titles.entry(id.to_owned()).or_insert((time, truncate_title(name)));
        if time >= entry.0 {
            *entry = (time, truncate_title(name));
        }
    }
    for session in sessions {
        if session.source == AiSessionSource::Codex
            && let Some((_, title)) = titles.get(&session.id)
        {
            session.title.clone_from(title);
        }
    }
}

pub(super) fn scan_opencode(limit: usize) -> Vec<AiSession> {
    let Some(root) = std::env::var_os("XDG_DATA_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".local/share")))
    else {
        return Vec::new();
    };
    let path = root.join("opencode/opencode.db");
    if !path.is_file() {
        return Vec::new();
    }
    let Ok(connection) = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return Vec::new();
    };
    if connection.busy_timeout(Duration::from_millis(1500)).is_err() {
        return Vec::new();
    }
    let Ok(mut statement) = connection.prepare(
        "SELECT id, title, directory, time_updated FROM session WHERE parent_id IS NULL ORDER BY time_updated DESC LIMIT ?1"
    ) else { return Vec::new() };
    let Ok(rows) = statement.query_map([limit.min(i64::MAX as usize) as i64], |row| {
        let time: i64 = row.get(3)?;
        let millis = u64::try_from(time).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let modified = UNIX_EPOCH
            .checked_add(Duration::from_millis(millis))
            .ok_or(rusqlite::Error::InvalidQuery)?;
        Ok(AiSession {
            source: AiSessionSource::OpenCode,
            id: row.get(0)?,
            title: row.get::<_, String>(1).map(|title| truncate_title(&title))?,
            project: row.get(2)?,
            modified,
            path: None,
        })
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok).collect()
}

pub(super) fn gemini_files(partial: &mut bool) -> Vec<(SystemTime, PathBuf, String)> {
    let Some(root) = home_dir().map(|h| h.join(".gemini")) else { return Vec::new() };
    let projects = read_json(&root.join("projects.json")).unwrap_or_default();
    let paths: HashMap<String, String> = projects
        .get("projects")
        .and_then(|v| v.as_object())
        .into_iter()
        .flatten()
        .filter_map(|(path, name)| Some((name.as_str()?.to_owned(), path.clone())))
        .collect();
    let mut candidates = Vec::new();
    for directory in scan_entries(&root.join("tmp"), partial) {
        if !scan_directory(&directory.path(), partial) {
            continue;
        }
        let project = directory.file_name().to_string_lossy().into_owned();
        for chat in scan_entries(&directory.path().join("chats"), partial) {
            let path = chat.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("session-"))
            {
                candidates.push((
                    modified_of(&path, partial),
                    path,
                    paths.get(&project).cloned().unwrap_or_default(),
                ));
            }
        }
    }
    candidates
}

pub(super) fn scan_gemini(limit: usize) -> Vec<AiSession> {
    let mut candidates = gemini_files(&mut false);
    candidates.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));
    candidates
        .into_iter()
        .take(limit)
        .filter_map(|(modified, path, project)| {
            let record = read_json(&path)?;
            let id = record.get("sessionId")?.as_str()?.to_owned();
            AiSessionSource::Gemini.resume_command(&id)?;
            let title = record
                .get("messages")?
                .as_array()?
                .iter()
                .find_map(|message| {
                    if message.get("type")?.as_str()? != "user" {
                        return None;
                    }
                    let content = message.get("content")?;
                    let text = content.as_str().map(str::to_owned).or_else(|| {
                        content
                            .as_array()?
                            .iter()
                            .find_map(|part| part.get("text")?.as_str().map(str::to_owned))
                    })?;
                    (!text.trim().is_empty() && !looks_injected(text.trim()))
                        .then(|| truncate_title(&text))
                })
                .unwrap_or_default();
            Some(AiSession {
                source: AiSessionSource::Gemini,
                id,
                title,
                project,
                modified,
                path: Some(path),
            })
        })
        .collect()
}

pub(super) fn read_json(path: &Path) -> Option<serde_json::Value> {
    use std::io::Read as _;
    // Older Gemini clients store a whole conversation in JSON. Bound both the
    // allocation and parser input; large native transcripts remain with the CLI.
    const LIMIT: u64 = 8 * 1024 * 1024;
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > LIMIT {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
