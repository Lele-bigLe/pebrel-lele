//! Window-local focus return uses live tab and pane identity, never a tab index.
use super::*;

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn remember_terminal_focus(&mut self) {
        if let Some(WorkspaceTab::Terminal { focused, .. }) = self.tabs.get(self.active) {
            self.projects.previous_focus = Some((self.meta(self.active).runtime_id, *focused));
        }
    }

    pub(in crate::gpui_shell::workspace) fn previous_focus_available(&self) -> bool {
        self.projects.previous_focus.as_ref().is_some_and(|(id, pane)| {
            self.tab_meta.iter().position(|meta| &meta.runtime_id == id).is_some_and(|index| {
                matches!(&self.tabs[index], WorkspaceTab::Terminal { panes, .. }
                    if panes.iter().any(|entry| entry.id == *pane))
            })
        })
    }

    pub(in crate::gpui_shell::workspace) fn return_to_previous_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((id, pane)) = self.projects.previous_focus.clone() else { return };
        let Some(index) = self.tab_meta.iter().position(|meta| meta.runtime_id == id) else {
            self.projects.previous_focus = None;
            return;
        };
        self.reveal_session_pane(index, pane, window, cx);
    }

    pub(in crate::gpui_shell::workspace) fn reveal_session_pane(
        &mut self,
        index: usize,
        pane: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(WorkspaceTab::Terminal { panes, zoomed, .. }) = self.tabs.get_mut(index) else {
            return;
        };
        if !panes.iter().any(|entry| entry.id == pane) {
            return;
        }
        let was_zoomed = std::mem::replace(zoomed, false);
        // Set the destination's pane before activating its tab so history records
        // the source once, rather than the destination's old focused pane.
        self.focus_pane(index, pane, window, cx);
        self.activate_tab(index, window, cx);
        self.reveal_active_project();
        self.reveal_active_tab();
        self.focus_active(window, cx);
        if was_zoomed {
            self.mark_structural_resize(index, cx);
        }
        cx.notify();
    }
}
