//! Project metadata outlives terminals. Every edit refreshes under the shared file lock.

use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

static LAST_REVISION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
    pub root: PathBuf,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub last_opened: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Library {
    version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub projects: Vec<Project>,
}

impl Default for Library {
    fn default() -> Self {
        Self { version: 1, revision: 0, projects: Vec::new() }
    }
}

pub(crate) enum Change {
    Remember(Vec<PathBuf>),
    Open(PathBuf),
    Edit(Project),
}

fn path() -> PathBuf {
    crate::platform::dirs::data_dir().join("projects.json")
}

pub(crate) fn same_root(a: &Path, b: &Path) -> bool {
    crate::platform::local_paths::same_project_root(a, b)
}

pub(crate) fn load() -> io::Result<Library> {
    let file = match std::fs::File::open(path()) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Library::default()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(io::Error::other("Project library exceeds its size limit"));
    }
    let library = decode(&bytes)?;
    LAST_REVISION.fetch_max(library.revision, Ordering::Relaxed);
    Ok(library)
}

pub(crate) fn decode(bytes: &[u8]) -> io::Result<Library> {
    if bytes.len() > 1024 * 1024 {
        return Err(io::Error::other("Project library exceeds its size limit"));
    }
    let library: Library = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    if library.version != 1
        || library.projects.len() > 1000
        || library.projects.iter().enumerate().any(|(index, project)| {
            !project.root.is_absolute()
                || project.name.len() > 256
                || project.group.len() > 256
                || library.projects[..index]
                    .iter()
                    .any(|existing| same_root(&existing.root, &project.root))
        })
    {
        return Err(io::Error::other("Invalid project library"));
    }
    Ok(library)
}

pub(crate) fn update(change: Change) -> io::Result<Library> {
    let path = path();
    let mut retries = 10;
    let _lock = loop {
        if let Some(lock) = crate::atomic_file::try_lock(&path)? {
            break lock;
        }
        if retries == 0 {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "Project library is busy"));
        }
        retries -= 1;
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let mut library = load()?;
    let previous = serde_json::to_vec(&library)?;
    let (roots, edit, opened) = match change {
        Change::Remember(roots) => (roots, None, false),
        Change::Open(root) => (vec![root], None, true),
        Change::Edit(project) => (vec![project.root.clone()], Some(project), false),
    };
    for root in roots {
        if !root.is_absolute() {
            return Err(io::Error::other("Project directory must be absolute"));
        }
        let index = match library.projects.iter().position(|p| same_root(&p.root, &root)) {
            Some(index) => index,
            None => {
                if library.projects.len() >= 1000 {
                    return Err(io::Error::other("Project library is full"));
                }
                library.projects.push(Project {
                    root,
                    name: String::new(),
                    group: String::new(),
                    favorite: false,
                    last_opened: 0,
                });
                library.projects.len() - 1
            },
        };
        let project = &mut library.projects[index];
        if let Some(edit) = &edit {
            if edit.name.len() > 256 || edit.group.len() > 256 {
                return Err(io::Error::other("Project name or group is too long"));
            }
            project.name = edit.name.trim().to_owned();
            project.group = edit.group.trim().to_owned();
            project.favorite = edit.favorite;
        }
        if opened {
            project.last_opened = chrono::Utc::now().timestamp();
        }
    }
    if previous != serde_json::to_vec(&library)?
        || library.revision < LAST_REVISION.load(Ordering::Relaxed)
    {
        library.revision = library
            .revision
            .max(LAST_REVISION.load(Ordering::Relaxed))
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Project revision overflow"))?;
        let bytes = serde_json::to_vec_pretty(&library)?;
        if bytes.len() > 1024 * 1024 {
            return Err(io::Error::other("Project library exceeds its size limit"));
        }
        crate::atomic_file::write(&path, &bytes)?;
        LAST_REVISION.fetch_max(library.revision, Ordering::Relaxed);
    }
    Ok(library)
}

/// Restoring user metadata is an explicit replacement, but must not roll back
/// the revision seen by open windows or race an ordinary project transaction.
pub(crate) fn restore(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let _lock = crate::atomic_file::try_lock(path)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "Project library is busy"))?;
    let mut library = decode(bytes)?;
    match std::fs::File::open(path) {
        Ok(file) => {
            let mut existing = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut existing)?;
            // An authenticated restore may replace a corrupt file. Only valid
            // existing metadata participates in the revision comparison.
            if let Ok(current) = decode(&existing) {
                library.revision = library.revision.max(current.revision);
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
        Err(error) => return Err(error),
    }
    library.revision = library
        .revision
        .max(LAST_REVISION.load(Ordering::Relaxed))
        .checked_add(1)
        .ok_or_else(|| io::Error::other("Project revision overflow"))?;
    let bytes = serde_json::to_vec_pretty(&library)?;
    if bytes.len() > 1024 * 1024 {
        return Err(io::Error::other("Project library exceeds its size limit"));
    }
    crate::atomic_file::write(path, &bytes)?;
    LAST_REVISION.fetch_max(library.revision, Ordering::Relaxed);
    Ok(())
}
