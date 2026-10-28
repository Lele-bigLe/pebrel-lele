//! Bounded, read-only native transcript previews. Tool payloads and reasoning are excluded.
use super::*;
use crate::ai_agents::AgentKind;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{self, Read as _, Seek as _, SeekFrom};

const READ_LIMIT: usize = 2 * 1024 * 1024;
const TEXT_LIMIT: usize = 128 * 1024;
const MESSAGE_LIMIT: usize = 80;

pub(crate) struct Message {
    pub user: bool,
    pub text: String,
}
#[derive(Default)]
pub(crate) struct Preview {
    pub messages: VecDeque<Message>,
    pub partial: bool,
    bytes: usize,
}

fn text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| {
            part.get("type")
                .and_then(Value::as_str)
                .is_none_or(|kind| matches!(kind, "text" | "input_text" | "output_text"))
        })
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

impl Preview {
    fn push(&mut self, user: bool, mut body: String) {
        if body.trim().is_empty() || (user && looks_injected(body.trim())) {
            return;
        }
        if let Some((end, _)) = body.char_indices().nth(32_000) {
            body.truncate(end);
            self.partial = true;
        }
        self.bytes += body.len();
        self.messages.push_back(Message { user, text: body });
        while self.messages.len() > MESSAGE_LIMIT || self.bytes > TEXT_LIMIT {
            if let Some(old) = self.messages.pop_front() {
                self.bytes -= old.text.len();
                self.partial = true;
            }
        }
    }
}

pub(crate) fn read(session: &AiSession) -> io::Result<Preview> {
    match session.source {
        AgentKind::Codex | AgentKind::Claude => read_jsonl(session),
        AgentKind::Gemini => read_gemini(session),
        AgentKind::OpenCode => read_opencode(session),
        _ => Err(io::Error::other(
            "Transcript preview is unavailable for this tool; continue the native session",
        )),
    }
}

fn read_jsonl(session: &AiSession) -> io::Result<Preview> {
    let path = session.path.as_ref().ok_or_else(|| {
        io::Error::other("Native transcript location is unavailable; continue the session")
    })?;
    let matches = if session.source == AgentKind::Codex {
        codex_session_id(path).as_deref() == Some(&session.id)
    } else {
        path.file_stem().and_then(|stem| stem.to_str()) == Some(&session.id)
    };
    if !matches {
        return Err(io::Error::other("Native transcript identity changed"));
    }
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let offset = length.saturating_sub(READ_LIMIT as u64);
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(READ_LIMIT as u64).read_to_end(&mut bytes)?;
    let mut preview = Preview { partial: offset > 0, ..Default::default() };
    let mut lines = bytes.split(|byte| *byte == b'\n').peekable();
    if offset > 0 {
        lines.next();
    }
    while let Some(line) = lines.next() {
        if line.is_empty() {
            continue;
        }
        if lines.peek().is_none() && !bytes.ends_with(b"\n") {
            preview.partial = true;
            break;
        }
        let record: Value = match serde_json::from_slice(line) {
            Ok(record) => record,
            Err(_) => {
                preview.partial = true;
                continue;
            },
        };
        if session.source == AgentKind::Codex {
            if record.get("type").and_then(Value::as_str) != Some("response_item") {
                continue;
            }
            let Some(payload) = record.get("payload") else { continue };
            if payload.get("type").and_then(Value::as_str) != Some("message")
                || payload.get("channel").and_then(Value::as_str) == Some("analysis")
            {
                continue;
            }
            let role = payload.get("role").and_then(Value::as_str);
            if matches!(role, Some("user" | "assistant")) {
                preview.push(role == Some("user"), text(&payload["content"]));
            }
        } else {
            let role = record.get("type").and_then(Value::as_str);
            if record.get("isMeta").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            if matches!(role, Some("user" | "assistant")) {
                preview.push(role == Some("user"), text(&record["message"]["content"]));
            }
        }
    }
    Ok(preview)
}

fn read_gemini(session: &AiSession) -> io::Result<Preview> {
    let path = session.path.as_ref().ok_or_else(|| {
        io::Error::other("Native transcript location is unavailable; refresh history")
    })?;
    let record = native::read_json(path).ok_or_else(|| {
        io::Error::other("Native transcript is unavailable, invalid or exceeds 8 MiB")
    })?;
    if record.get("sessionId").and_then(Value::as_str) != Some(&session.id) {
        return Err(io::Error::other("Native transcript identity changed"));
    }
    let mut preview = Preview::default();
    let messages = record
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| io::Error::other("Invalid native transcript"))?;
    preview.partial = messages.len() > MESSAGE_LIMIT;
    for message in messages.iter().rev().take(MESSAGE_LIMIT).rev() {
        let role = message.get("type").and_then(Value::as_str);
        if matches!(role, Some("user" | "gemini" | "assistant")) {
            preview.push(role == Some("user"), text(&message["content"]));
        }
    }
    Ok(preview)
}

fn read_opencode(session: &AiSession) -> io::Result<Preview> {
    use rusqlite::{Connection, OpenFlags};
    let root = std::env::var_os("XDG_DATA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local/share")))
        .ok_or_else(|| io::Error::other("OpenCode data directory is unavailable"))?;
    let connection = Connection::open_with_flags(
        root.join("opencode/opencode.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(io::Error::other)?;
    connection.busy_timeout(Duration::from_millis(1000)).map_err(io::Error::other)?;
    let mut statement = connection.prepare("SELECT m.id, json_extract(m.data, '$.role'), substr(json_extract(p.data, '$.text'), 1, 32001) FROM message m JOIN part p ON p.message_id = m.id WHERE m.session_id = ?1 AND json_valid(m.data) AND json_valid(p.data) AND json_extract(m.data, '$.role') IN ('user', 'assistant') AND json_extract(p.data, '$.type') = 'text' ORDER BY m.time_created DESC, m.id DESC, p.time_created ASC, p.id ASC LIMIT 241").map_err(io::Error::other)?;
    let rows = statement
        .query_map([&session.id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(io::Error::other)?;
    let mut groups: Vec<(String, bool, String)> = Vec::new();
    let mut partial = false;
    let mut total = 0;
    for (index, row) in rows.enumerate() {
        if index >= 240 {
            partial = true;
            break;
        }
        let (id, role, body) = row.map_err(io::Error::other)?;
        total += body.len();
        if total > TEXT_LIMIT {
            partial = true;
            break;
        }
        if let Some((_, _, text)) = groups.last_mut().filter(|(last, _, _)| *last == id) {
            text.push('\n');
            text.push_str(&body);
        } else {
            groups.push((id, role == "user", body));
        }
    }
    let mut preview = Preview { partial, ..Default::default() };
    for (_, user, body) in groups.into_iter().rev() {
        preview.push(user, body);
    }
    Ok(preview)
}
