//! Automatic preview and explicit send share canonical drafts and the native Agent picker.
use super::*;
use crate::gpui_shell::workspace::send_to_chat::ChatTarget;

impl Queue {
    pub(super) fn prepare(&mut self, send: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let Some(owner) = self.owner.upgrade() else { return };
        let expected = match owner.read(cx).review_drafts(&self.location, cx) {
            Ok(drafts) if !drafts.is_empty() => drafts,
            Ok(_) => return,
            Err(issue) => {
                self.issue = Some(issue);
                self.prepared = None;
                cx.notify();
                return;
            },
        };
        let target = if send { self.picker.selected(cx) } else { None };
        if send && target.is_none() {
            return;
        }
        if send && self.prepared.as_ref().is_none_or(|(drafts, _)| drafts != &expected) {
            self.prepared = None;
            self.issue = Some(language.text(Message::ReviewStale).to_owned());
            cx.notify();
            return;
        }
        let preview =
            if send { self.prepared.as_ref().map(|(_, message)| message.clone()) } else { None };
        let snapshots = expected.clone();
        let worker = cx.background_executor().spawn(async move {
            super::super::validate(&snapshots, language).map(|message| preview.unwrap_or(message))
        });
        self.issue = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.task = None;
                let Some(owner) = this.owner.upgrade() else { return };
                let unchanged = owner.read(cx).review_drafts(&this.location, cx).as_ref().ok()
                    == Some(&expected);
                if !unchanged {
                    this.prepared = None;
                    this.issue = Some(language.text(Message::ReviewStale).to_owned());
                    cx.notify();
                    return;
                }
                match result {
                    Err(error) => {
                        this.prepared = None;
                        this.issue =
                            Some(if error.code == "review_stale" || error.code == "review_limit" {
                                error.message
                            } else {
                                language
                                    .format(Message::ReviewReadFailed, &[("error", &error.message)])
                            });
                    },
                    Ok(message) => {
                        this.prepared = Some((expected, message.clone()));
                        if let Some(target) = target {
                            let result = owner.update(cx, |owner, cx| {
                                owner.deliver_project_chat(
                                    &this.picker,
                                    target,
                                    message,
                                    window,
                                    cx,
                                )
                            });
                            match result {
                                Ok(()) => {
                                    let text = if matches!(target, ChatTarget::New(_)) {
                                        language.text(Message::ChatTargetPending).to_owned()
                                    } else {
                                        language.format(
                                            Message::CommonSentTo,
                                            &[("target", &this.picker.title(target))],
                                        )
                                    };
                                    crate::gpui_shell::toast::toast(
                                        window,
                                        cx,
                                        crate::display::ToastKind::Info,
                                        text,
                                    );
                                    window.close_dialog(cx);
                                },
                                Err(error) => this.issue = Some(error.message),
                            }
                        }
                    },
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn preview_ready(&self, cx: &App) -> bool {
        self.prepared.as_ref().is_some_and(|(drafts, _)| {
            self.owner
                .upgrade()
                .and_then(|owner| owner.read(cx).review_drafts(&self.location, cx).ok())
                .as_ref()
                == Some(drafts)
        })
    }

    pub(super) fn render_actions(
        &mut self,
        blocked: bool,
        ready: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let selected = self.picker.selected(cx).is_some();
        v_flex()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .pt_3()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(language.text(Message::ReviewTarget)),
            )
            .child(self.picker.render_control(busy, window, cx))
            .when(self.picker.empty(), |body| {
                body.child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(cx.theme().warning)
                        .child(language.text(Message::ChatTargetEmpty)),
                )
            })
            .when(busy, |body| {
                body.child(
                    h_flex()
                        .gap_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(Spinner::new().xsmall())
                        .child(language.text(Message::ReviewWorking)),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("review-preview-copy")
                            .ghost()
                            .small()
                            .icon(IconName::Copy)
                            .label(language.text(Message::ReviewCopyPreview))
                            .disabled(self.prepared.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some((_, message)) = &this.prepared {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        message.clone(),
                                    ));
                                    let language = crate::gpui_shell::config::ui_language(cx);
                                    crate::gpui_shell::toast::toast(
                                        window,
                                        cx,
                                        crate::display::ToastKind::Info,
                                        language.text(Message::ReviewCopied),
                                    );
                                }
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("review-queue-send")
                            .primary()
                            .small()
                            .label(language.text(Message::ReviewDeliver))
                            .loading(busy)
                            .disabled(blocked || busy || !ready || !selected)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.prepare(true, window, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}
