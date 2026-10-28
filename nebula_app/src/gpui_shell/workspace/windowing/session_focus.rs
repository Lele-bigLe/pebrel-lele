use super::*;

pub(in crate::gpui_shell::workspace) fn focus_open_agent(
    origin: u64,
    source: crate::ai_agents::AgentKind,
    id: &str,
    cx: &mut App,
) -> bool {
    if !cx.has_global::<WindowRegistry>() {
        return false;
    }
    for entry in entries_by_mru(cx).into_iter().filter(|entry| entry.runtime_window_id != origin) {
        let Some(owner) = entry.workspace.upgrade() else { continue };
        let pane = owner.read(cx).open_agent_pane(source, id, cx);
        if let Some(pane) = pane {
            cx.activate(true);
            focus_entry(&entry, Some(pane), cx);
            return true;
        }
    }
    false
}
