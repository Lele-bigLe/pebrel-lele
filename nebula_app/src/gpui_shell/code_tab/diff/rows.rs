//! One patch-to-line map supplies both visual gutters and review anchors.
use super::*;
use crate::gpui_shell::widgets::{SCROLLBAR_GUTTER, scrollbar_gutter};
use gpui::{MouseButton, MouseDownEvent, uniform_list};
use std::ops::Range;

pub(super) use crate::runtime_api::git::patch::{Kind, PatchLine, parse};

pub(in crate::gpui_shell::code_tab) fn selected_context(
    text: &str,
    range: Range<usize>,
) -> Option<(String, String)> {
    if range.is_empty() || range.end > text.len() {
        return None;
    }
    let excerpt = text.get(range.clone())?.trim_end_matches(['\r', '\n']);
    if excerpt.is_empty() {
        return None;
    }
    let row = parse(text).into_iter().find(|row| {
        row.range.start < range.end
            && row.range.end > range.start
            && (row.old.is_some() || row.new.is_some())
    })?;
    Some((
        format!(
            "-{} +{}",
            row.old.map(|n| n.to_string()).unwrap_or_else(|| "—".into()),
            row.new.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
        ),
        excerpt.to_owned(),
    ))
}

impl CodeTabView {
    pub(super) fn render_diff_rows(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let diff = self.diff.as_ref().expect("visual diff requires patch state");
        let language = crate::gpui_shell::config::ui_language(cx);
        let rows = diff.rows.clone();
        let widest = rows.iter().enumerate().max_by_key(|(_, row)| row.text.len()).map(|(i, _)| i);
        let added = rows.iter().filter(|row| row.kind == Kind::Added).count();
        let removed = rows.iter().filter(|row| row.kind == Kind::Removed).count();
        v_flex()
            .size_full()
            .min_h_0()
            .child(
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .child(
                        div().text_xs().text_color(cx.theme().success).child(format!("+{added}")),
                    )
                    .child(
                        div().text_xs().text_color(cx.theme().danger).child(format!("−{removed}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(language.text(Message::DiffViewSelectHint)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .pr(px(SCROLLBAR_GUTTER))
                    .overflow_hidden()
                    .child(
                        uniform_list(
                            "visual-diff-rows",
                            rows.len(),
                            cx.processor(move |this, visible: Range<usize>, _, cx| {
                                let selected = this.input.read(cx).selected_range();
                                visible
                                    .filter_map(|index| rows.get(index))
                                    .map(|row| {
                                        let theme = cx.theme();
                                        let selected = !selected.is_empty()
                                            && selected.start < row.range.end
                                            && selected.end > row.range.start;
                                        let (background, foreground) = if selected {
                                            (theme.primary.opacity(0.18), theme.foreground)
                                        } else {
                                            match row.kind {
                                                Kind::Added => {
                                                    (theme.success.opacity(0.10), theme.foreground)
                                                },
                                                Kind::Removed => {
                                                    (theme.danger.opacity(0.10), theme.foreground)
                                                },
                                                Kind::Hunk => (
                                                    theme.primary.opacity(0.08),
                                                    theme.muted_foreground,
                                                ),
                                                _ => (gpui::transparent_black(), theme.foreground),
                                            }
                                        };
                                        let range = row.range.clone();
                                        h_flex()
                                            .h(px(24.0))
                                            .min_w_full()
                                            .gap_2()
                                            .pr_3()
                                            .bg(background)
                                            .font_family(theme.mono_font_family.clone())
                                            .text_size(theme.mono_font_size)
                                            .text_color(foreground)
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .w(px(56.0))
                                                    .flex_shrink_0()
                                                    .text_right()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child(
                                                        row.old
                                                            .map(|n| n.to_string())
                                                            .unwrap_or_default(),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .w(px(56.0))
                                                    .flex_shrink_0()
                                                    .text_right()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child(
                                                        row.new
                                                            .map(|n| n.to_string())
                                                            .unwrap_or_default(),
                                                    ),
                                            )
                                            .child(div().flex_shrink_0().child(row.text.clone()))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(
                                                    move |this, event: &MouseDownEvent, _, cx| {
                                                        let Some(diff) = &mut this.diff else {
                                                            return;
                                                        };
                                                        if diff.loading {
                                                            return;
                                                        }
                                                        let selection = if event.modifiers.shift {
                                                            let anchor = diff
                                                                .anchor
                                                                .as_ref()
                                                                .unwrap_or(&range);
                                                            anchor.start.min(range.start)
                                                                ..anchor.end.max(range.end)
                                                        } else {
                                                            diff.anchor = Some(range.clone());
                                                            range.clone()
                                                        };
                                                        this.input.update(cx, |input, cx| {
                                                            input.set_selected_range(selection, cx)
                                                        });
                                                        cx.notify();
                                                    },
                                                ),
                                            )
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                        )
                        .size_full()
                        .with_width_from_item(widest)
                        .with_horizontal_sizing_behavior(
                            gpui::ListHorizontalSizingBehavior::Unconstrained,
                        )
                        .track_scroll(&diff.scroll),
                    )
                    .child(scrollbar_gutter(gpui_component::scroll::Scrollbar::vertical(
                        &diff.scroll,
                    ))),
            )
            .into_any_element()
    }
}
