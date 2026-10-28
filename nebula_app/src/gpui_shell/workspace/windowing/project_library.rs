use super::*;

pub(in crate::gpui_shell::workspace) fn publish_project_library(
    origin: u64,
    library: crate::project_library::Library,
    cx: &mut App,
) {
    for entry in entries_by_mru(cx).into_iter().filter(|entry| entry.runtime_window_id != origin) {
        if let Some(owner) = entry.workspace.upgrade() {
            owner.update(cx, |workspace, cx| workspace.accept_project_library(library.clone(), cx));
        }
    }
}
