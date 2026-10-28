use super::*;
use gpui::{EventEmitter, prelude::FluentBuilder as _};
use gpui_component::input::InputEvent;

enum DetailEvent {
    Saved(annotations::Store),
    Resume(AiSession),
}

impl NebulaWorkspace {
    fn selected_history_session(&self, cx: &Context<Self>) -> Option<AiSession> {
        let rows = self.filtered_palette_rows(cx);
        let WorkspacePaletteAction::RunAiSession { resume: Some((source, id)), .. } =
            &rows.get(self.command_palette_selected)?.action
        else {
            return None;
        };
        self.history
            .entries
            .iter()
            .find(|entry| entry.session.source == *source && entry.session.id == *id)
            .map(|entry| entry.session.clone())
    }

    pub(in crate::gpui_shell::workspace) fn open_history_details(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.selected_history_session(cx) else { return };
        let detail = cx.new(|cx| Details::new(session, window, cx));
        cx.subscribe_in(&detail, window, |this, _, event: &DetailEvent, window, cx| match event {
            DetailEvent::Saved(store) => {
                this.history.annotations = Some(Ok(store.clone()));
                this.reset_history_selection(cx);
            },
            DetailEvent::Resume(session) => {
                let Some(command) = session.resume_command() else { return };
                let cwd = PathBuf::from(&session.project);
                window.close_dialog(cx);
                this.run_workspace_palette_action(
                    WorkspacePaletteAction::RunAiSession {
                        command,
                        cwd: cwd.is_absolute().then_some(cwd),
                        resume: Some((session.source, session.id.clone())),
                    },
                    window,
                    cx,
                );
            },
        })
        .detach();
        let language = crate::gpui_shell::config::ui_language(cx);
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(language.text(Message::HistoryDetailsTitle))
                .w(px(760.0))
                .child(detail.clone())
        });
    }
}

struct Details {
    session: AiSession,
    body: Entity<InputState>,
    note: Entity<InputState>,
    expected: Option<annotations::Annotation>,
    favorite: bool,
    loading: bool,
    partial: bool,
    error: Option<String>,
    metadata_error: Option<String>,
    saved: bool,
    task: Option<gpui::Task<()>>,
    copy: Entity<crate::gpui_shell::copy_feedback::CopyFeedback>,
}
impl EventEmitter<DetailEvent> for Details {}

impl Details {
    fn new(session: AiSession, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let body = cx.new(|cx| InputState::new(window, cx).multi_line(true).soft_wrap(true));
        let note =
            cx.new(|cx| InputState::new(window, cx).multi_line(true).soft_wrap(true).rows(2));
        cx.subscribe(&note, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.saved = false;
                cx.notify();
            }
        })
        .detach();
        let copy = cx.new(|_| crate::gpui_shell::copy_feedback::CopyFeedback::new());
        cx.observe(&copy, |_, _, cx| cx.notify()).detach();
        let mut this = Self {
            session,
            body,
            note,
            expected: None,
            favorite: false,
            loading: true,
            partial: false,
            error: None,
            metadata_error: None,
            saved: false,
            task: None,
            copy,
        };
        let session = this.session.clone();
        let worker = cx.background_executor().spawn(async move {
            (
                crate::ai_sessions::transcript::read(&session).map_err(|error| error.to_string()),
                annotations::load().map_err(|error| error.to_string()),
            )
        });
        let handle = window.window_handle();
        this.task = Some(cx.spawn(async move |this, cx| {
            let (preview, store) = worker.await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.loading = false;
                    this.task = None;
                    match preview {
                        Ok(preview) => {
                            this.partial = preview.partial;
                            let language = crate::gpui_shell::config::ui_language(cx);
                            let body = preview
                                .messages
                                .into_iter()
                                .map(|message| {
                                    format!(
                                        "{}\n\n{}",
                                        if message.user {
                                            language.text(Message::HistoryDetailsUser)
                                        } else {
                                            this.session.source.display_name()
                                        },
                                        message.text
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n\n────────\n\n");
                            this.body.update(cx, |input, cx| input.set_value(body, window, cx));
                        },
                        Err(error) => this.error = Some(error),
                    }
                    match store {
                        Ok(store) => {
                            let annotation = store.get(&this.session);
                            this.favorite = annotation.favorite;
                            this.note.update(cx, |input, cx| {
                                input.set_value(annotation.note.clone(), window, cx)
                            });
                            this.expected = Some(annotation);
                        },
                        Err(error) => this.metadata_error = Some(error),
                    }
                    cx.notify();
                });
            });
        }));
        this
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let Some(expected) = self.expected.clone() else { return };
        let session = self.session.clone();
        let note = self.note.read(cx).value().to_string();
        let favorite = self.favorite;
        self.metadata_error = None;
        self.saved = false;
        let worker = cx.background_executor().spawn(async move {
            annotations::save(&session, expected, favorite, note).map_err(|error| error.to_string())
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(store) => {
                        this.expected = Some(store.get(&this.session));
                        this.saved = true;
                        cx.emit(DetailEvent::Saved(store));
                    },
                    Err(error) => this.metadata_error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl gpui::Render for Details {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let copied = self.copy.read(cx).is_copied();
        v_flex()
            .gap_2()
            .w_full()
            .child(div().font_semibold().truncate().child(self.session.title.clone()))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).truncate().child(
                format!("{} · {}", self.session.source.display_name(), self.session.project),
            ))
            .when(self.loading, |view| {
                view.child(div().text_sm().child(language.text(Message::HistoryLoading)))
            })
            .when_some(self.error.as_ref(), |view, error| {
                view.child(
                    div().text_sm().text_color(cx.theme().warning).child(
                        language.format(Message::HistoryDetailsReadError, &[("error", error)]),
                    ),
                )
            })
            .when(self.partial, |view| {
                view.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(language.text(Message::HistoryDetailsPartial)),
                )
            })
            .when(
                !self.loading && self.error.is_none() && self.body.read(cx).value().is_empty(),
                |view| {
                    view.child(
                        div().text_sm().child(language.text(Message::HistoryDetailsNoMessages)),
                    )
                },
            )
            .child(
                div()
                    .w_full()
                    .h(px((f32::from(window.viewport_size().height) * 0.62 - 180.0)
                        .clamp(160.0, 380.0)))
                    .child(Input::new(&self.body).h_full().disabled(true)),
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Button::new("history-detail-favorite")
                            .ghost()
                            .small()
                            .icon(IconName::Star)
                            .selected(self.favorite)
                            .label(language.text(Message::HistoryDetailsFavorite))
                            .disabled(busy || self.expected.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.favorite = !this.favorite;
                                this.saved = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("history-detail-copy")
                            .ghost()
                            .small()
                            .icon(if copied { IconName::Check } else { IconName::Copy })
                            .label(language.text(if copied {
                                Message::HistoryDetailsCopied
                            } else {
                                Message::CommonCopy
                            }))
                            .disabled(self.loading || self.body.read(cx).value().is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let text = this.body.read(cx).value().to_string();
                                if !text.is_empty() {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                                    this.copy.update(cx, |copy, cx| copy.mark_copied(cx));
                                }
                            })),
                    ),
            )
            .child(div().text_sm().child(language.text(Message::HistoryDetailsNote)))
            .child(
                div().w_full().h(px(68.0)).child(
                    Input::new(&self.note).h_full().disabled(busy || self.expected.is_none()),
                ),
            )
            .when_some(self.metadata_error.as_ref(), |view, error| {
                view.child(div().text_xs().text_color(cx.theme().warning).child(
                    language.format(Message::HistoryDetailsMetadataError, &[("error", error)]),
                ))
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .when(self.saved, |view| {
                        view.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(language.text(Message::HistoryDetailsSaved)),
                        )
                    })
                    .child(
                        Button::new("history-detail-save")
                            .small()
                            .label(language.text(Message::HistoryDetailsSave))
                            .disabled(busy || self.expected.is_none())
                            .loading(busy && !self.loading)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
                    .child(
                        Button::new("history-detail-resume")
                            .small()
                            .primary()
                            .label(language.text(Message::HistoryDetailsResume))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.emit(DetailEvent::Resume(this.session.clone()))
                            })),
                    ),
            )
    }
}
