//! A new CLI requires the user to finish login/trust before sending the saved feedback.
//! No timer injects text into a startup or permission prompt.
use super::*;

pub(in crate::gpui_shell::workspace) struct PendingChat {
    pane: u64,
    view: WeakEntity<TerminalView>,
    kind: AgentKind,
    message: String,
}

impl NebulaWorkspace {
    pub(super) fn start_chat_target(
        &mut self,
        kind: AgentKind,
        project: Option<PathBuf>,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), crate::runtime_api::ApiError> {
        let language = workspace_ui_language();
        if self.pending_chat.is_some() {
            return Err(crate::runtime_api::ApiError::new(
                "input_in_progress",
                language.text(Message::ChatTargetAlreadyPending),
            ));
        }
        crate::runtime_api::validate_chat_message(&message)?;
        let root = project.ok_or_else(|| {
            crate::runtime_api::ApiError::new(
                "invalid_project",
                language.text(Message::ChatTargetEmpty),
            )
        })?;
        let command = kind.start_command().ok_or_else(|| {
            crate::runtime_api::ApiError::new(
                "invalid_agent",
                language.text(Message::ChatTargetEmpty),
            )
        })?;
        self.projects.filter = super::super::projects::ProjectFilter::Root(root.clone());
        self.queue_project_change(crate::project_library::Change::Open(root.clone()), cx);
        let pane = self.add_terminal_at(Some(root), Some(command), window, cx);
        let view = self
            .tabs
            .iter()
            .find_map(|tab| match tab {
                WorkspaceTab::Terminal { panes, .. } => {
                    panes.iter().find(|item| item.id == pane).map(|item| item.view.downgrade())
                },
                _ => None,
            })
            .ok_or_else(|| {
                crate::runtime_api::ApiError::new(
                    "target_not_found",
                    language.text(Message::ChatTargetClosed),
                )
            })?;
        self.pending_chat = Some(PendingChat { pane, view, kind, message });
        cx.notify();
        Ok(())
    }

    pub(in crate::gpui_shell::workspace) fn render_pending_chat(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = self.pending_chat.as_ref()?;
        let language = workspace_ui_language();
        let ready = pending.view.upgrade().is_some_and(|view| {
            view.read(cx)
                .runtime_chat_agent()
                .is_some_and(|agent| agent.kind == pending.kind.slug())
        });
        Some(
            h_flex()
                .w_full()
                .flex_shrink_0()
                .gap_2()
                .px_3()
                .py_2()
                .bg(cx.theme().secondary)
                .child(div().flex_1().min_w_0().text_sm().whitespace_normal().child(
                    language.format(
                        Message::ChatTargetAwaiting,
                        &[("agent", pending.kind.display_name())],
                    ),
                ))
                .child(
                    Button::new("pending-chat-copy")
                        .ghost()
                        .small()
                        .label(language.text(Message::CommonCopy))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(pending) = &this.pending_chat {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    pending.message.clone(),
                                ));
                            }
                        })),
                )
                .child(
                    Button::new("pending-chat-send")
                        .primary()
                        .small()
                        .label(language.text(Message::ReviewSend))
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let Some(pending) = &this.pending_chat else { return };
                            let Some(view) = pending.view.upgrade() else { return };
                            if !view
                                .read(cx)
                                .runtime_chat_agent()
                                .is_some_and(|agent| agent.kind == pending.kind.slug())
                            {
                                return;
                            }
                            match view.update(cx, |view, cx| {
                                view.runtime_chat_message(
                                    pending.message.clone(),
                                    InputOrigin::User,
                                    cx,
                                )
                            }) {
                                Ok(()) => {
                                    this.pending_chat = None;
                                    cx.notify();
                                },
                                Err(error) => crate::gpui_shell::toast::toast(
                                    window,
                                    cx,
                                    crate::display::ToastKind::Warning,
                                    error.message,
                                ),
                            }
                        })),
                )
                .child(
                    Button::new("pending-chat-focus")
                        .ghost()
                        .small()
                        .icon(IconName::ArrowRight)
                        .tooltip(language.text(Message::ChatTargetFocus))
                        .disabled(pending.view.upgrade().is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let Some(pending) = &this.pending_chat else { return };
                            let pane = pending.pane;
                            cx.defer(move |cx| {
                                super::super::windowing::focus_notification(Some(pane), cx)
                            });
                        })),
                )
                .child(
                    Button::new("pending-chat-cancel")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .tooltip(language.text(Message::ChatTargetCancel))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pending_chat = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }
}
