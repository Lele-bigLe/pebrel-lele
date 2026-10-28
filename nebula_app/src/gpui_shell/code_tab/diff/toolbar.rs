use super::*;

impl CodeTabView {
    pub(in crate::gpui_shell) fn render_hosted_diff_toolbar(
        &self,
        count: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.render_diff_toolbar(Some(count), cx)
    }

    pub(super) fn render_diff_toolbar(
        &self,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let diff = self.diff.as_ref().expect("diff toolbar requires a diff");
        let language = crate::gpui_shell::config::ui_language(cx);
        let hint = diff.key.display_path();
        let name = Path::new(&diff.key.relative_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        h_flex()
            .h(px(60.0))
            .w_full()
            .min_w_0()
            .gap_2()
            .px_4()
            .flex_shrink_0()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                v_flex()
                    .id("diff-path")
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .child(div().truncate().text_sm().font_semibold().child(name))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(diff.key.relative_path.clone()),
                    ),
            )
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(language.text(
                if diff.historical {
                    Message::TurnChangesSnapshot
                } else if diff.staged {
                    Message::DiffStaged
                } else {
                    Message::DiffWorking
                },
            )))
            .child(
                Button::new("diff-layout")
                    .ghost()
                    .small()
                    .disabled(diff.loading || diff.comparison.is_none())
                    .label(language.text(if diff.raw {
                        Message::DiffViewVisual
                    } else {
                        Message::DiffViewRaw
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(diff) = &mut this.diff {
                            diff.raw = !diff.raw;
                            cx.emit(CodeTabViewEvent::Changed);
                            cx.notify();
                        }
                    })),
            )
            .when(!diff.staged && !diff.historical, |row| row.child(self.render_stage_hunk(cx)))
            .when(!diff.raw && diff.comparison.is_some(), |row| {
                row.child(
                    Button::new("diff-wrap")
                        .ghost()
                        .small()
                        .label(language.text(Message::DiffCompareWrap))
                        .selected(diff.wrap)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(diff) = &mut this.diff {
                                diff.wrap = !diff.wrap;
                                diff.comparison_layout.set(None);
                                cx.emit(CodeTabViewEvent::Changed);
                                cx.notify();
                            }
                        })),
                )
            })
            .when(!diff.historical, |row| {
                row.child(
                    Button::new("diff-review")
                        .ghost()
                        .small()
                        .label(language.text(if count.is_some() {
                            Message::WorkbenchAddComment
                        } else {
                            Message::ReviewToggle
                        }))
                        .selected(diff.review.open)
                        .disabled(
                            count.is_some()
                                && (diff.loading
                                    || diff.snapshot.is_none()
                                    || self.input.read(cx).selected_range().is_empty()),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(diff) = &mut this.diff {
                                diff.review.open = count.is_some() || !diff.review.open;
                                if diff.review.open {
                                    this.focus_review_comment(window, cx);
                                }
                                cx.emit(CodeTabViewEvent::Changed);
                                cx.notify();
                            }
                        })),
                )
            })
            .when_some(count.filter(|_| !diff.historical), |row, count| {
                row.child(
                    Button::new("diff-review-queue")
                        .small()
                        .label(
                            language.format(
                                Message::WorkbenchFeedback,
                                &[("count", &count.to_string())],
                            ),
                        )
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(CodeTabViewEvent::ReviewRequested)),
                        ),
                )
            })
            .when(!diff.historical, |row| {
                row.child(
                    Button::new("diff-refresh")
                        .ghost()
                        .small()
                        .icon(IconName::Redo)
                        .tooltip(language.text(Message::UsageRefresh))
                        .disabled(diff.loading)
                        .loading(diff.loading)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.reload_git_diff(window, cx)),
                        ),
                )
            })
            .into_any_element()
    }
}
