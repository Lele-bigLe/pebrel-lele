//! Read persisted Codex turns without resuming or taking the session writer lock.
use crate::ai_hook::integrations::codex_rpc::{self, Client, Purpose};
pub(crate) use codex_rpc::Error;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

pub(crate) struct Turn {
    pub id: String,
    pub title: String,
    pub status: String,
    pub started_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeTurn {
    id: String,
    status: String,
    started_at: Option<i64>,
    #[serde(default)]
    items: Vec<Value>,
}

impl NativeTurn {
    fn title(&self) -> String {
        self.items
            .iter()
            .filter(|item| item["type"] == "userMessage")
            .filter_map(|item| item["content"].as_array())
            .flatten()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str())
            .map(str::trim)
            .find(|text| !text.is_empty() && !super::looks_injected(text))
            .map(super::truncate_title)
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
pub(crate) struct Edit {
    pub path: String,
    pub kind: Kind,
    pub diff: String,
}

#[derive(Deserialize)]
pub(crate) struct Kind {
    #[serde(rename = "type")]
    pub name: String,
    pub move_path: Option<String>,
}

impl Edit {
    /// Add/delete records contain complete content; updates contain unified hunks.
    pub(crate) fn patch(&self) -> String {
        let prefix = match self.kind.name.as_str() {
            "add" => '+',
            "delete" => '-',
            _ => return self.diff.clone(),
        };
        let count = self.diff.lines().count();
        let mut patch = if prefix == '+' {
            format!("@@ -0,0 +1,{count} @@\n")
        } else {
            format!("@@ -1,{count} +0,0 @@\n")
        };
        for line in self.diff.lines() {
            patch.push(prefix);
            patch.push_str(line);
            patch.push('\n');
        }
        patch
    }
}

pub(crate) fn turns(thread: &str, cursor: Option<String>) -> Result<Page<Turn>, Error> {
    codex_rpc::run(async {
        let mut client = Client::start(Purpose::History).await?;
        let result = client
            .call(
                "thread/turns/list",
                json!({
                    "threadId": thread, "cursor": cursor, "limit": 15,
                    "sortDirection": "desc", "itemsView": "summary",
                }),
            )
            .await;
        client.close().await;
        let page: Page<NativeTurn> =
            serde_json::from_value(result?).map_err(|_| Error::InvalidResponse)?;
        Ok(Page {
            next_cursor: page.next_cursor,
            data: page
                .data
                .into_iter()
                .map(|turn| {
                    let title = turn.title();
                    Turn { id: turn.id, title, status: turn.status, started_at: turn.started_at }
                })
                .collect(),
        })
    })
}

pub(crate) fn edits(thread: &str, turn: &str, cursor: Option<String>) -> Result<Page<Edit>, Error> {
    codex_rpc::run(async {
        let mut client = Client::start(Purpose::History).await?;
        let result = read_edits(&mut client, thread, turn, cursor).await;
        client.close().await;
        result
    })
}

async fn read_edits(
    client: &mut Client,
    thread: &str,
    turn: &str,
    mut cursor: Option<String>,
) -> Result<Page<Edit>, Error> {
    let mut edits = Vec::new();
    let mut bytes = 0usize;
    // Bound one interaction; the caller can continue with the returned cursor.
    for _ in 0..16 {
        let result = client
            .call(
                "thread/items/list",
                json!({
                    "threadId": thread, "turnId": turn, "cursor": cursor,
                    "limit": 25, "sortDirection": "asc",
                }),
            )
            .await;
        let result = match result {
            Err(Error::Unsupported) if cursor.is_none() && edits.is_empty() => {
                return read_legacy_edits(client, thread, turn).await;
            },
            result => result?,
        };
        let page: Page<Value> =
            serde_json::from_value(result).map_err(|_| Error::InvalidResponse)?;
        for entry in page.data {
            let item = &entry["item"];
            if item["type"] != "fileChange" || item["status"] != "completed" {
                continue;
            }
            let changes = item["changes"].as_array().ok_or(Error::InvalidResponse)?;
            for change in changes {
                let edit: Edit =
                    serde_json::from_value(change.clone()).map_err(|_| Error::InvalidResponse)?;
                bytes = bytes.saturating_add(edit.diff.len());
                if bytes > 16 * 1024 * 1024 || edits.len() >= 1000 {
                    return Err(Error::TooLarge);
                }
                edits.push(edit);
            }
        }
        if page.next_cursor.is_some() && page.next_cursor == cursor {
            return Err(Error::InvalidResponse);
        }
        cursor = page.next_cursor;
        if cursor.is_none() || bytes > 4 * 1024 * 1024 {
            break;
        }
    }
    Ok(Page { data: edits, next_cursor: cursor })
}

async fn read_legacy_edits(
    client: &mut Client,
    thread: &str,
    turn: &str,
) -> Result<Page<Edit>, Error> {
    // Older rollout stores expose turns but reject item pagination. This read
    // neither resumes the thread nor substitutes the current working tree.
    let result =
        client.call("thread/read", json!({"threadId": thread, "includeTurns": true})).await?;
    let record = &result["thread"];
    if record["id"].as_str() != Some(thread) {
        return Err(Error::InvalidResponse);
    }
    let turns = record["turns"].as_array().ok_or(Error::InvalidResponse)?;
    let selected = turns
        .iter()
        .find(|entry| entry["id"].as_str() == Some(turn))
        .ok_or(Error::InvalidResponse)?;
    let items = selected["items"].as_array().ok_or(Error::InvalidResponse)?;
    let mut edits = Vec::new();
    let mut bytes = 0usize;
    for item in items {
        if item["type"] != "fileChange" || item["status"] != "completed" {
            continue;
        }
        for change in item["changes"].as_array().ok_or(Error::InvalidResponse)? {
            let edit: Edit =
                serde_json::from_value(change.clone()).map_err(|_| Error::InvalidResponse)?;
            bytes = bytes.saturating_add(edit.diff.len());
            if bytes > 16 * 1024 * 1024 || edits.len() >= 1000 {
                return Err(Error::TooLarge);
            }
            edits.push(edit);
        }
    }
    Ok(Page { data: edits, next_cursor: None })
}
