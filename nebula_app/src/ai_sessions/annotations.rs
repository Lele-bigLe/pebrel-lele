//! Only user annotations and native locators are persisted; transcript content stays with the CLI.
use super::*;
use crate::ai_agents::AgentKind;
use std::collections::BTreeMap;
use std::io::{self, Read as _};

const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Annotation {
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    locator: Option<Locator>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Locator {
    source: String,
    id: String,
    title: String,
    project: String,
    path: Option<PathBuf>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    #[serde(default)]
    pub records: BTreeMap<String, Annotation>,
}

pub(crate) fn key(session: &AiSession) -> String {
    format!("{}:{}", session.source.slug(), session.id)
}
fn path() -> PathBuf {
    crate::platform::dirs::data_dir().join("ai_session_annotations.json")
}

pub(crate) fn load() -> io::Result<Store> {
    let file = match std::fs::File::open(path()) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::other("Session annotations exceed the size limit"));
    }
    let store: Store = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    validate(&store)?;
    Ok(store)
}

fn validate(store: &Store) -> io::Result<()> {
    if store.records.len() > 4096
        || store.records.iter().any(|(key, item)| {
            key.len() > 600
                || item.note.len() > 4000
                || item.note.chars().count() > 1000
                || item.locator.as_ref().is_some_and(|locator| {
                    locator.id.len() > 512
                        || locator.title.len() > 4096
                        || locator.project.len() > 4096
                        || AgentKind::parse(&locator.source)
                            .is_none_or(|kind| kind.resume_command(&locator.id).is_none())
                        || key != &format!("{}:{}", locator.source, locator.id)
                })
        })
    {
        return Err(io::Error::other("Invalid session annotations"));
    }
    Ok(())
}

pub(crate) fn save(
    session: &AiSession,
    expected: Annotation,
    favorite: bool,
    note: String,
) -> io::Result<Store> {
    if note.len() > 4000 || note.chars().count() > 1000 {
        return Err(io::Error::other("Note exceeds 1000 characters"));
    }
    let path = path();
    let _lock = crate::atomic_file::try_lock(&path)?.ok_or_else(|| {
        io::Error::new(io::ErrorKind::WouldBlock, "Session annotations are busy; retry")
    })?;
    let mut store = load()?;
    let key = key(session);
    if store.records.get(&key).cloned().unwrap_or_default() != expected {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "This annotation changed in another window. Reopen the preview before saving.",
        ));
    }
    if !favorite && note.is_empty() {
        store.records.remove(&key);
    } else {
        store.records.insert(
            key,
            Annotation {
                favorite,
                note,
                locator: Some(Locator {
                    source: session.source.slug().into(),
                    id: session.id.clone(),
                    title: session.title.clone(),
                    project: session.project.clone(),
                    path: session.path.clone(),
                }),
            },
        );
    }
    validate(&store)?;
    let bytes = serde_json::to_vec(&store).map_err(io::Error::other)?;
    if bytes.len() > MAX_BYTES || store.records.len() > 4096 {
        return Err(io::Error::other("Session annotations exceed the size limit"));
    }
    crate::atomic_file::write_private(&path, &bytes)?;
    Ok(store)
}

impl Store {
    pub(crate) fn get(&self, session: &AiSession) -> Annotation {
        self.records.get(&key(session)).cloned().unwrap_or_default()
    }
    pub(crate) fn append_saved(&self, sessions: &mut Vec<AiSession>) {
        let mut seen: std::collections::HashSet<_> =
            sessions.iter().map(|session| (session.source, session.id.clone())).collect();
        for item in self.records.values().filter(|item| item.favorite || !item.note.is_empty()) {
            let Some(locator) = &item.locator else { continue };
            let Some(source) = AgentKind::parse(&locator.source) else { continue };
            if !seen.insert((source, locator.id.clone())) {
                continue;
            }
            sessions.push(AiSession {
                source,
                id: locator.id.clone(),
                title: locator.title.clone(),
                project: locator.project.clone(),
                path: locator.path.clone(),
                modified: SystemTime::UNIX_EPOCH,
            });
        }
    }
}
