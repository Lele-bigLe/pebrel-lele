use super::*;
use crate::display::side_panel::GitLocation;
use std::io::Write as _;

pub(super) fn with_repository<T>(
    location: &GitLocation,
    operation: impl FnOnce(&Repository) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let (context, cwd) = desktop_context(location);
    let mut repo =
        Repository { context: context.for_git(), root: cwd.clone(), cwd, started: Instant::now() };
    let root = repo.run(&["rev-parse", "--show-toplevel"], false)?;
    repo.root = root.strip_suffix('\n').unwrap_or(&root).into();
    let lock = repository_lock(format!(
        "{:?}:{:?}:{}",
        repo.context.wsl_distribution(),
        repo.context.wsl_user(),
        repo.root
    ));
    let _guard = lock.try_lock().map_err(|_| {
        ApiError::new("git_busy", "Another Git operation is running for this repository")
    })?;
    operation(&repo)
}

#[derive(Clone)]
pub(crate) struct StashInfo {
    pub oid: String,
    pub subject: String,
}
pub(crate) struct StashSnapshot {
    pub entries: Vec<StashInfo>,
    pub has_more: bool,
    pub tracked: bool,
    pub untracked: bool,
}

pub(crate) fn read_stashes(location: &GitLocation) -> Result<StashSnapshot, ApiError> {
    with_repository(location, |repo| {
        let (status, _) = repo.status()?;
        let text =
            repo.run(&["stash", "list", "--max-count=257", "--format=%H%x00%gs%x00"], false)?;
        let mut fields = text.split('\0');
        let mut entries = Vec::new();
        while let Some(oid) = fields.next() {
            let oid = oid.trim_matches('\n');
            if oid.is_empty() {
                continue;
            }
            let subject = fields
                .next()
                .ok_or_else(|| ApiError::new("git_failed", "Invalid stash listing"))?;
            validate_oid(oid)?;
            entries.push(StashInfo { oid: oid.into(), subject: subject.into() });
        }
        let has_more = entries.len() > 256;
        entries.truncate(256);
        Ok(StashSnapshot {
            entries,
            has_more,
            tracked: status.entries.iter().any(|entry| entry.index != '?'),
            untracked: status.entries.iter().any(|entry| entry.index == '?'),
        })
    })
}

fn validate_oid(oid: &str) -> Result<(), ApiError> {
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::invalid_params("A full stash object ID is required"));
    }
    Ok(())
}

pub(crate) fn read_stash_diff(location: &GitLocation, oid: &str) -> Result<String, ApiError> {
    validate_oid(oid)?;
    with_repository(location, |repo| {
        repo.run(
            &[
                "stash",
                "show",
                "--patch",
                "--include-untracked",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                oid,
            ],
            false,
        )
    })
}

pub(crate) fn stage_hunk(
    location: &GitLocation,
    path: String,
    expected: [u8; 32],
    selection: std::ops::Range<usize>,
) -> Result<(), ApiError> {
    if !matches!(location, GitLocation::Local { .. }) {
        return Err(ApiError::new(
            "git_hunk_unsupported",
            "Hunk staging currently requires a local repository",
        ));
    }
    with_repository(location, |repo| {
        let value = repo.diff(&Request { path, operation: "diff".into(), ..Default::default() })?;
        let text = value["text"]
            .as_str()
            .ok_or_else(|| ApiError::new("git_failed", "Git did not return a patch"))?;
        if Sha256::digest(text.as_bytes()).as_slice() != expected {
            return Err(ApiError::new(
                "git_stale",
                "The displayed patch changed; refresh before staging",
            ));
        }
        let patch = super::patch::extract(text, selection).ok_or_else(|| ApiError::new("git_hunk_unsupported", "Select one whole hunk in a tracked text modification; use file staging for other changes"))?;
        let mut temporary = tempfile::Builder::new()
            .prefix("pebrel-stage-")
            .suffix(".patch")
            .tempfile_in(crate::platform::dirs::data_dir())
            .map_err(|error| ApiError::new("git_failed", error.to_string()))?;
        temporary
            .write_all(patch.as_bytes())
            .map_err(|error| ApiError::new("git_failed", error.to_string()))?;
        temporary.flush().map_err(|error| ApiError::new("git_failed", error.to_string()))?;
        let path = temporary
            .path()
            .to_str()
            .ok_or_else(|| ApiError::new("git_encoding", "Temporary patch path is not UTF-8"))?;
        repo.run(&["apply", "--cached", "--check", "--", path], false)?;
        repo.run(&["apply", "--cached", "--", path], false)?;
        Ok(())
    })
}
