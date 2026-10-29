use super::*;
use crate::gpui_shell::widgets::{SCROLLBAR_GUTTER, scrollbar_gutter};
use gpui::InteractiveElement as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{IntoElement, ParentElement as _, Render, SharedString, Styled as _, div, uniform_list};

impl Render for Changes {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let mut body = v_flex().size_full().min_h_0().gap_2().track_focus(&self.focus);
        let mut header = h_flex().w_full().min_w_0().gap_2();
        if let Some(selected) = &self.selected {
            let turn = &self.turns[selected.turn];
            header = header
                .child(
                    Button::new("turns-back")
                        .ghost()
                        .small()
                        .label(language.text(Message::TurnChangesBack))
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.task.is_none() {
                                this.selected = None;
                                this.error = None;
                                cx.notify();
                            }
                        })),
                )
                .child(div().flex_1().min_w_0().truncate().text_sm().child(
                    if turn.title.is_empty() {
                        language.text(Message::TurnChangesUntitled).to_owned()
                    } else {
                        turn.title.clone()
                    },
                ));
        } else {
            header = header
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(language.text(Message::TurnChangesHint)),
                )
                .child(
                    Button::new("turns-refresh")
                        .ghost()
                        .small()
                        .icon(IconName::Redo)
                        .tooltip(language.text(Message::UsageRefresh))
                        .disabled(busy)
                        .loading(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.load_turns(None, cx))),
                );
        }
        body = body.child(header);
        if let Some(error) = &self.error {
            let detail = match error {
                Error::Unavailable => language.text(Message::TurnChangesUnavailable).to_owned(),
                Error::MissingHome => language.text(Message::TurnChangesMissingHome).to_owned(),
                Error::Unsupported => language.text(Message::TurnChangesUnsupported).to_owned(),
                Error::TooLarge => language.text(Message::DiffTooLarge).to_owned(),
                Error::Rejected(code) => {
                    language.format(Message::TurnChangesRejected, &[("code", &code.to_string())])
                },
                Error::Transport | Error::InvalidResponse => {
                    language.text(Message::TurnChangesFailed).to_owned()
                },
            };
            body = body.child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().text_sm().text_color(cx.theme().danger).child(detail))
                    .when(self.selected.is_some(), |row| {
                        row.child(
                            Button::new("turn-edits-retry")
                                .ghost()
                                .small()
                                .label(language.text(Message::UsageRefresh))
                                .disabled(busy)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.load_edits(window, cx)),
                                ),
                        )
                    }),
            );
        }
        if busy {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(language.text(Message::DiffLoading)),
            );
        }
        if self.selected.is_some() {
            body = body.child(self.render_files(cx));
        } else {
            body = body.child(self.render_turns(cx));
        }
        body
    }
}

impl Changes {
    fn render_turns(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let mut body = v_flex().flex_1().min_h_0().gap_2();
        if self.turns.is_empty() && !busy && self.error.is_none() {
            body = body.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(language.text(Message::TurnChangesEmpty)),
            );
        }
        body = body.child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .pr(px(SCROLLBAR_GUTTER))
                .child(
                    uniform_list(
                        "codex-turns",
                        self.turns.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| {
                                    let turn = &this.turns[index];
                                    let title = if turn.title.is_empty() {
                                        language.text(Message::TurnChangesUntitled).to_owned()
                                    } else {
                                        turn.title.clone()
                                    };
                                    let time = turn
                                        .started_at
                                        .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
                                        .map(|time| {
                                            time.with_timezone(&chrono::Local)
                                                .format("%m-%d %H:%M")
                                                .to_string()
                                        })
                                        .unwrap_or_default();
                                    let status = match turn.status.as_str() {
                                        "inProgress" => Message::TurnChangesRunning,
                                        "failed" => Message::TurnChangesTurnFailed,
                                        "interrupted" => Message::TurnChangesInterrupted,
                                        "completed" => Message::TurnChangesCompleted,
                                        _ => Message::TurnChangesUnknown,
                                    };
                                    h_flex()
                                        .h(px(72.0))
                                        .w_full()
                                        .min_w_0()
                                        .gap_3()
                                        .px_2()
                                        .border_b_1()
                                        .border_color(cx.theme().border)
                                        .child(
                                            v_flex()
                                                .flex_1()
                                                .min_w_0()
                                                .gap_1()
                                                .child(div().truncate().text_sm().child(title))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(format!(
                                                            "{time} · {}",
                                                            language.text(status)
                                                        )),
                                                ),
                                        )
                                        .child(
                                            Button::new(("turn-open", index))
                                                .small()
                                                .label(language.text(Message::TurnChangesView))
                                                .disabled(busy)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.select_turn(index, window, cx)
                                                    },
                                                )),
                                        )
                                        .into_any_element()
                                })
                                .collect()
                        }),
                    )
                    .track_scroll(&self.turns_scroll)
                    .size_full(),
                )
                .child(scrollbar_gutter(gpui_component::scroll::Scrollbar::vertical(
                    &self.turns_scroll,
                ))),
        );
        if self.next.is_some() {
            body = body.child(
                Button::new("turns-more")
                    .ghost()
                    .small()
                    .label(language.text(Message::TurnChangesOlder))
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.load_turns(this.next.clone(), cx))),
            );
        }
        body.into_any_element()
    }

    fn render_files(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let selected = self.selected.as_ref().unwrap();
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let mut body = v_flex().flex_1().min_h_0().gap_2();
        if selected.files.is_empty() && !busy && self.error.is_none() && selected.next.is_none() {
            body = body.child(div().p_3().text_sm().text_color(cx.theme().muted_foreground).child(
                language.text(if self.turns[selected.turn].status == "inProgress" {
                    Message::TurnChangesPending
                } else {
                    Message::TurnChangesNoPatch
                }),
            ));
        }
        if selected.next.is_some() {
            body = body.child(
                Button::new("edits-more")
                    .ghost()
                    .small()
                    .label(language.text(Message::TurnChangesContinue))
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| this.load_edits(window, cx))),
            );
        }
        let mut detail = v_flex().flex_1().min_w_0().min_h_0();
        if let Some(file) = selected.files.get(selected.file) {
            if file.edits.len() > 1 {
                let file_index = selected.file;
                let step = selected.step;
                detail = detail.child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .px_3()
                        .py_1()
                        .child(
                            Button::new("edit-prev")
                                .ghost()
                                .small()
                                .label("‹")
                                .disabled(step == 0)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_edit(file_index, step.saturating_sub(1), window, cx)
                                })),
                        )
                        .child(div().flex_1().text_xs().child(language.format(
                            Message::TurnChangesStep,
                            &[
                                ("index", &(step + 1).to_string()),
                                ("count", &file.edits.len().to_string()),
                            ],
                        )))
                        .child(
                            Button::new("edit-next")
                                .ghost()
                                .small()
                                .label("›")
                                .disabled(step + 1 >= file.edits.len())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_edit(file_index, step + 1, window, cx)
                                })),
                        ),
                );
            }
        }
        if let Some(viewer) = &selected.viewer {
            detail = detail.child(div().flex_1().min_h_0().child(viewer.clone()));
        }
        body.child(
            h_flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .items_stretch()
                .child(
                    div()
                        .relative()
                        .w(gpui::relative(0.26))
                        .min_w(px(120.0))
                        .flex_shrink_0()
                        .border_r_1()
                        .border_color(cx.theme().border)
                        .pr(px(SCROLLBAR_GUTTER))
                        .child(
                            uniform_list(
                                "turn-files",
                                selected.files.len(),
                                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                    range
                                        .map(|index| {
                                            let selected = this.selected.as_ref().unwrap();
                                            let file = &selected.files[index];
                                            let path = this.path_label(&file.path);
                                            let name = Path::new(&path)
                                                .file_name()
                                                .map(|name| name.to_string_lossy().into_owned())
                                                .unwrap_or_else(|| path.clone());
                                            let directory = Path::new(&path)
                                                .parent()
                                                .map(|parent| parent.display().to_string())
                                                .unwrap_or_default();
                                            let hint = file.path.clone();
                                            div()
                                                .h(px(44.0))
                                                .w_full()
                                                .min_w_0()
                                                .px_1()
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "turn-file-{index}"
                                                    )))
                                                    .ghost()
                                                    .small()
                                                    .w_full()
                                                    .h(px(42.0))
                                                    .aria_label(path)
                                                    .child(
                                                        v_flex()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .text_left()
                                                            .child(
                                                                div()
                                                                    .truncate()
                                                                    .text_sm()
                                                                    .child(name),
                                                            )
                                                            .child(
                                                                div()
                                                                    .truncate()
                                                                    .text_xs()
                                                                    .text_color(
                                                                        cx.theme().muted_foreground,
                                                                    )
                                                                    .child(directory),
                                                            ),
                                                    )
                                                    .tooltip(hint)
                                                    .selected(selected.file == index)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.select_edit(index, 0, window, cx)
                                                        },
                                                    )),
                                                )
                                                .into_any_element()
                                        })
                                        .collect()
                                }),
                            )
                            .track_scroll(&self.files_scroll)
                            .size_full(),
                        )
                        .child(scrollbar_gutter(gpui_component::scroll::Scrollbar::vertical(
                            &self.files_scroll,
                        ))),
                )
                .child(detail),
        )
        .into_any_element()
    }
}
