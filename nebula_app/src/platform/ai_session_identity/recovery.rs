//! Provider-native ownership evidence. Lock files are never removed or rewritten.
use std::fs::{File, TryLockError};
use std::io::Read as _;

use crate::session::AgentSession;

pub(crate) fn available(agent: &AgentSession) -> Result<bool, ()> {
    let id = agent.session_id.as_deref().filter(|_| agent.resume_command().is_some()).ok_or(())?;
    match agent.source.as_str() {
        "codex" => {
            let root = agent
                .session_file
                .as_deref()
                .map(std::path::Path::new)
                .filter(|path| path.is_absolute())
                .and_then(|path| {
                    path.ancestors()
                        .find(|parent| {
                            parent.file_name().is_some_and(|name| {
                                name == "sessions" || name == "archived_sessions"
                            })
                        })
                        .and_then(|parent| parent.parent())
                        .map(|path| path.to_owned())
                })
                .or_else(crate::codex_config::codex_home)
                .ok_or(())?;
            let file = match File::open(root.join("thread-writer-locks").join(format!("{id}.lock")))
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
                Err(_) => return Err(()),
            };
            match file.try_lock() {
                Ok(()) => Ok(true),
                Err(TryLockError::WouldBlock) => Ok(false),
                Err(TryLockError::Error(_)) => Err(()),
            }
        },
        "claude" => {
            let (sessions, uncertain) = claude_sessions()?;
            if sessions.iter().any(|(_, session)| session.session_id.as_deref() == Some(id)) {
                Ok(false)
            } else if uncertain {
                Err(())
            } else {
                Ok(true)
            }
        },
        _ => Err(()),
    }
}

pub(crate) fn probe_claude(shell_pid: u32) -> Option<AgentSession> {
    let descendants = crate::process_tree::descendants(shell_pid).ok()?;
    let mut candidates = claude_sessions()
        .ok()?
        .0
        .into_iter()
        .filter(|(pid, _)| descendants.iter().any(|process| process.pid == *pid));
    let (_, session) = candidates.next()?;
    if candidates.any(|(_, next)| next != session) {
        return None;
    }
    Some(session)
}

fn claude_sessions() -> Result<(Vec<(u32, AgentSession)>, bool), ()> {
    let root = crate::ai_sessions::claude_root().ok_or(())?.join("sessions");
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(()),
    };
    let processes = crate::platform::process_snapshot::snapshot().map_err(|_| ())?;
    let mut out = Vec::new();
    for (index, entry) in entries.into_iter().flatten().enumerate() {
        if index >= 4096 {
            return Err(());
        }
        let path = entry.map_err(|_| ())?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Some(pid) =
            path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(process) = processes.iter().find(|process| process.pid == pid) else { continue };
        let name = crate::process_tree::display_name(&process.executable);
        if !matches!(name.to_ascii_lowercase().as_str(), "claude" | "node" | "bun") {
            continue;
        }
        let file = match File::open(path) {
            Ok(file) => file,
            Err(_) => return Err(()),
        };
        #[cfg(windows)]
        if process.created != 0 {
            let modified =
                file.metadata().and_then(|metadata| metadata.modified()).map_err(|_| ())?;
            let ticks = process.created.checked_sub(116_444_736_000_000_000).ok_or(())?;
            let created =
                std::time::UNIX_EPOCH + std::time::Duration::from_nanos(ticks.saturating_mul(100));
            if created > modified + std::time::Duration::from_secs(1) {
                continue;
            }
        }
        let value: serde_json::Value =
            serde_json::from_reader(file.take(64 * 1024)).map_err(|_| ())?;
        if value
            .get("pid")
            .and_then(|value| value.as_u64())
            .is_some_and(|saved| saved != u64::from(pid))
        {
            continue;
        }
        let Some(id) = value.get("sessionId").and_then(|value| value.as_str()) else {
            return Err(());
        };
        let session = AgentSession {
            source: "claude".to_owned(),
            session_id: Some(id.to_owned()),
            session_file: None,
        };
        if session.resume_command().is_some() {
            out.push((pid, session));
        }
    }
    let uncertain = processes.iter().any(|process| {
        crate::process_tree::display_name(&process.executable).eq_ignore_ascii_case("claude")
            && !out.iter().any(|(pid, _)| *pid == process.pid)
    });
    Ok((out, uncertain))
}
