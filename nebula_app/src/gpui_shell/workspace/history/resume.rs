use super::*;

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn open_agent_pane(
        &self,
        source: AgentKind,
        id: &str,
        cx: &App,
    ) -> Option<u64> {
        for tab in &self.tabs {
            let WorkspaceTab::Terminal { panes, .. } = tab else { continue };
            for pane in panes {
                let view = pane.view.read(cx);
                if view.ssh_destination.is_some() {
                    continue;
                }
                if view
                    .runtime_chat_agent()
                    .is_some_and(|agent| AgentKind::parse(&agent.kind) != Some(source))
                {
                    continue;
                }
                if view.runtime_chat_agent().is_some_and(|agent| {
                    AgentKind::parse(&agent.kind) == Some(source)
                        && agent.session_id.as_deref() == Some(id)
                }) {
                    return Some(pane.id);
                }
                // Hooks may arrive after a second click. This reservation is
                // short-lived and is never persisted as native session identity.
                if self.history.restoring.iter().any(|(kind, session, target, at)| {
                    *kind == source
                        && session == id
                        && *target == pane.id
                        && at.elapsed() < Duration::from_secs(30)
                        && !matches!(
                            view.sidebar_activity(),
                            SidebarActivity::Failed | SidebarActivity::CommandFailed
                        )
                        && view.session_agent().is_none_or(|agent| {
                            AgentKind::parse(&agent.source) == Some(source)
                                && agent.session_id.as_deref().is_none_or(|actual| actual == id)
                        })
                }) {
                    return Some(pane.id);
                }
            }
        }
        None
    }

    pub(in crate::gpui_shell::workspace) fn focus_open_agent(
        &mut self,
        source: AgentKind,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(pane) = self.open_agent_pane(source, id, cx) {
            let Some(tab) = self.tab_of_pane(pane) else { return false };
            self.dismiss_palette_state();
            self.activate_tab(tab, window, cx);
            if let Some(WorkspaceTab::Terminal { zoomed, .. }) = self.tabs.get_mut(tab) {
                *zoomed = false;
            }
            self.focus_pane(tab, pane, window, cx);
            self.focus_active(window, cx);
            cx.notify();
            return true;
        }
        if windowing::focus_open_agent(self.runtime_window_id, source, id, cx) {
            self.dismiss_palette_state();
            cx.notify();
            return true;
        }
        false
    }

    pub(in crate::gpui_shell::workspace) fn reserve_agent_resume(
        &mut self,
        source: AgentKind,
        id: String,
        pane: u64,
    ) {
        self.history.restoring.retain(|(_, _, _, at)| at.elapsed() < Duration::from_secs(30));
        self.history.restoring.push((source, id, pane, std::time::Instant::now()));
    }
}
