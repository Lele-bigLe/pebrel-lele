use super::*;
use crate::display::side_panel::GitLocation;

pub(crate) struct Comparison {
    pub patch: String,
    pub original: Result<String, ApiError>,
}

/// Read one bounded Git baseline and its patch. The other side is reconstructed
/// from that exact patch, so no second filesystem read can race the displayed edit.
pub(crate) fn read_comparison(
    location: &GitLocation,
    path: String,
    staged: bool,
) -> Result<Comparison, ApiError> {
    desktop_changes::with_repository(location, |repo| {
        let (status, revision) = repo.status()?;
        let entry =
            status.entries.iter().find(|entry| entry.path == path).ok_or_else(|| {
                ApiError::new("git_stale", "The selected change no longer exists")
            })?;
        let request =
            Request { path: path.clone(), staged, operation: "diff".into(), ..Default::default() };
        let value = repo.diff(&request)?;
        let patch = value["text"]
            .as_str()
            .ok_or_else(|| ApiError::new("git_failed", "Git did not return a diff"))?
            .to_owned();
        let original = if entry.conflict {
            Err(ApiError::new("git_unmerged", "Resolve this conflict in the merge view"))
        } else if entry.index == '?' || (staged && entry.index == 'A') {
            Ok(String::new())
        } else {
            let spec = if staged {
                format!("HEAD:{}", entry.original.as_deref().unwrap_or(&path))
            } else {
                format!(":{path}")
            };
            repo.run(&["show", "--no-ext-diff", "--no-textconv", &spec], false)
        };
        if repo.status()?.1 != revision || repo.diff(&request)?["text"].as_str() != Some(&patch) {
            return Err(ApiError::new(
                "git_stale",
                "The Git comparison changed while loading; refresh",
            ));
        }
        Ok(Comparison { patch, original })
    })
}
