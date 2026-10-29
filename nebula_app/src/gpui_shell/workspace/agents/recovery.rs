//! A saved workspace pane starts its provider only after an explicit user action.
use crate::gpui_shell::workspace::*;
use crate::i18n::Message;

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn render_saved_agent(
        &self,
        view: &Entity<TerminalView>,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let state = view.read(cx);
        let agent = state.session_agent()?;
        let ready = state.can_retry_recovery();
        let pane_id = state.pane_id;
        let name = crate::ai_agents::AgentKind::parse(&agent.source)?.display_name();
        let language = crate::gpui_shell::config::ui_language(cx);
        let resume = view.clone();
        let choose = view.clone();
        let workspace = cx.entity().downgrade();
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .h(px(36.0))
                .flex_shrink_0()
                .gap_2()
                .px_2()
                .items_center()
                .bg(cx.theme().background)
                .border_t_1()
                .border_color(cx.theme().border)
                .text_xs()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(language.format(Message::SessionSavedPending, &[("agent", name)])),
                )
                .when(agent.session_id.is_some(), |row| {
                    row.child(
                        Button::new(("resume-saved-agent", pane_id as usize))
                            .small()
                            .label(language.text(Message::SessionResumeSaved))
                            .disabled(!ready)
                            .on_click(move |_, window, cx| {
                                let focus = resume.read(cx).focus_handle.clone();
                                resume.update(cx, |view, cx| view.continue_saved_agent(None, cx));
                                focus.focus(window, cx);
                            }),
                    )
                })
                .child(
                    Button::new(("choose-saved-agent", pane_id as usize))
                        .small()
                        .label(language.text(Message::SessionChooseConversation))
                        .disabled(!ready)
                        .on_click(move |_, window, cx| {
                            if let Some(workspace) = workspace.upgrade() {
                                workspace.update(cx, |workspace, cx| {
                                    workspace.choose_saved_agent_history(&choose, window, cx);
                                });
                            }
                        }),
                )
                .into_any_element(),
        )
    }
}
