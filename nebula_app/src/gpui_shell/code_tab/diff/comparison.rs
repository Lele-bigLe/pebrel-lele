//! Paired variable-height rows keep wrapped text aligned; native panels own resizing.
use super::*;
use crate::gpui_shell::widgets::{SCROLLBAR_GUTTER, scrollbar_gutter};
use crate::runtime_api::git::patch::{AlignedLine, Comparison};
use gpui::{App, HighlightStyle, MouseButton, MouseDownEvent, Pixels, relative};
use gpui_component::highlighter::SyntaxHighlighter;
use std::ops::Range;
use std::rc::Rc;

pub(super) struct Prepared {
    data: Comparison,
    before: SyntaxHighlighter,
    after: SyntaxHighlighter,
    before_ranges: Vec<Range<usize>>,
    after_ranges: Vec<Range<usize>>,
    before_widest: gpui::SharedString,
    after_widest: gpui::SharedString,
}

impl Prepared {
    pub(super) fn new(mut data: Comparison, language: &str) -> Self {
        fn side(
            lines: &mut [AlignedLine],
            before: bool,
            language: &str,
        ) -> (SyntaxHighlighter, Vec<Range<usize>>) {
            let mut source = String::new();
            let mut ranges = Vec::with_capacity(lines.len());
            for row in lines {
                let start = source.len();
                if let Some(line) = if before { &mut row.before } else { &mut row.after } {
                    line.text = line.text.replace('\t', "    ");
                    source.push_str(&line.text);
                }
                ranges.push(start..source.len());
                // An absent alignment cell isn't a line in this side's source.
                if if before { row.before.is_some() } else { row.after.is_some() } {
                    source.push('\n');
                }
            }
            let mut syntax = SyntaxHighlighter::new(language);
            let mut text = syntax.text().clone();
            text.insert(0, &source);
            syntax.update(None, &text, Some(std::time::Duration::from_millis(100)));
            (syntax, ranges)
        }
        let (before, before_ranges) = side(&mut data.lines, true, language);
        let (after, after_ranges) = side(&mut data.lines, false, language);
        let widest = |before: bool| {
            data.lines
                .iter()
                .filter_map(|row| if before { row.before.as_ref() } else { row.after.as_ref() })
                .max_by_key(|line| unicode_width::UnicodeWidthStr::width(line.text.as_str()))
                .map(|line| line.text.clone())
                .unwrap_or_default()
                .into()
        };
        let before_widest = widest(true);
        let after_widest = widest(false);
        Self { data, before, after, before_ranges, after_ranges, before_widest, after_widest }
    }
}

#[derive(Clone)]
enum DisplayRow {
    Omitted,
    Line(usize),
    Gap(Range<usize>),
}

fn display_rows(
    lines: &[AlignedLine],
    expanded: &std::collections::HashSet<usize>,
) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        if lines[start].before.is_none() && lines[start].after.is_none() {
            rows.push(DisplayRow::Omitted);
            start += 1;
            continue;
        }
        if lines[start].changed {
            rows.push(DisplayRow::Line(start));
            start += 1;
            continue;
        }
        let end = (start..lines.len())
            .find(|&index| {
                lines[index].changed
                    || (lines[index].before.is_none() && lines[index].after.is_none())
            })
            .unwrap_or(lines.len());
        if end - start > 10 && !expanded.contains(&start) {
            rows.extend((start..start + 3).map(DisplayRow::Line));
            rows.push(DisplayRow::Gap(start..end));
            rows.extend((end - 3..end).map(DisplayRow::Line));
        } else {
            rows.extend((start..end).map(DisplayRow::Line));
        }
        start = end;
    }
    rows
}

pub(super) fn row_count(diff: &DiffPreview) -> usize {
    diff.comparison.as_ref().map_or(0, |data| display_rows(&data.data.lines, &diff.expanded).len())
}

pub(super) fn split_ratio(diff: &DiffPreview, cx: &App) -> f32 {
    let sizes = diff.split.read(cx).sizes();
    if sizes.len() == 2 && sizes[0] + sizes[1] > px(0.0) {
        sizes[0] / (sizes[0] + sizes[1])
    } else {
        0.5
    }
}

impl CodeTabView {
    pub(super) fn render_diff_comparison(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let diff = self.diff.as_ref().expect("comparison requires a diff");
        let Some(data) = &diff.comparison else { return self.render_diff_rows(cx) };
        let rows = Rc::new(display_rows(&data.data.lines, &diff.expanded));
        let ratio = split_ratio(diff, cx);
        let sizes = diff.split.read(cx).sizes();
        let layout = (
            sizes.first().copied().unwrap_or_default(),
            sizes.get(1).copied().unwrap_or_default(),
            cx.theme().mono_font_size,
            diff.wrap,
        );
        if diff.comparison_layout.get() != Some(layout)
            || diff.comparison_scroll.item_count() != rows.len()
        {
            if diff.comparison_scroll.item_count() != rows.len() {
                let offset = diff.comparison_scroll.logical_scroll_top();
                diff.comparison_scroll.reset(rows.len());
                diff.comparison_scroll.scroll_to(offset);
            } else {
                diff.comparison_scroll.remeasure();
            }
            diff.comparison_layout.set(Some(layout));
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let added = data.data.lines.iter().filter(|row| row.changed && row.after.is_some()).count();
        let removed =
            data.data.lines.iter().filter(|row| row.changed && row.before.is_some()).count();
        let hint = language.text(if diff.historical {
            Message::TurnChangesContextHint
        } else {
            Message::DiffCompareHint
        });
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
                    .text_xs()
                    .child(div().text_color(cx.theme().success).child(format!("+{added}")))
                    .child(div().text_color(cx.theme().danger).child(format!("−{removed}")))
                    .child(
                        div()
                            .id("diff-comparison-help")
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .tooltip(move |window, cx| Tooltip::new(hint).build(window, cx))
                            .child(hint),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .pr(px(SCROLLBAR_GUTTER))
                    .child(
                        v_flex().size_full().min_h_0()
                            .child(h_flex().h(px(28.0)).flex_shrink_0().w_full()
                                .child(self.comparison_heading(true, ratio, cx))
                                .child(self.comparison_heading(false, 1.0 - ratio, cx)))
                            .child(div().relative().flex_1().min_h_0().overflow_hidden()
                                .child(gpui::list(diff.comparison_scroll.clone(), cx.processor({
                                    let data = data.clone();
                                    move |this, index: usize, _, cx| {
                                        match &rows[index] {
                                            DisplayRow::Omitted => div().w_full().h(px(24.0)).px_3()
                                                .text_xs().text_color(cx.theme().muted_foreground)
                                                .child(crate::gpui_shell::config::ui_language(cx).text(Message::TurnChangesOmitted))
                                                .into_any_element(),
                                            DisplayRow::Gap(range) => {
                                                let start = range.start;
                                                Button::new(("diff-gap", start)).ghost().small()
                                                    .h(px(24.0)).w_full()
                                                    .label(crate::gpui_shell::config::ui_language(cx).format(
                                                        Message::DiffCompareExpand, &[("count", &(range.len() - 6).to_string())]))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        if let Some(diff) = &mut this.diff {
                                                            diff.expanded.insert(start);
                                                            diff.comparison_layout.set(None);
                                                        }
                                                        cx.notify();
                                                    })).into_any_element()
                                            },
                                            DisplayRow::Line(index) => h_flex().w_full().min_w_0().items_stretch()
                                                .child(div().w(relative(ratio)).flex_shrink_0().min_w_0().flex().items_stretch().overflow_hidden()
                                                    .child(this.comparison_line(&data, *index, true, cx)))
                                                .child(div().w(relative(1.0 - ratio)).flex_shrink_0().min_w_0().flex().items_stretch().overflow_hidden()
                                                    .child(this.comparison_line(&data, *index, false, cx)))
                                                .into_any_element(),
                                        }
                                    }
                                })).size_full()))
                            .when(!diff.wrap, |column| column.child(
                                h_flex().h(px(14.0)).flex_shrink_0().w_full()
                                    .child(self.comparison_horizontal(data.as_ref(), true, ratio, window, cx))
                                    .child(self.comparison_horizontal(data.as_ref(), false, 1.0 - ratio, window, cx)))),
                    )
                    // Empty native panels add only the full-height resize handle above the list.
                    // Text remains in one list, so both sides always share each row's height.
                    .child(div().absolute().top_0().left_0().bottom_0().right(px(SCROLLBAR_GUTTER)).child(
                        h_resizable("diff-columns").with_state(&diff.split)
                            .child(resizable_panel().size_range(px(120.0)..Pixels::MAX))
                            .child(resizable_panel().size_range(px(120.0)..Pixels::MAX))))
                    .child(scrollbar_gutter(
                        gpui_component::scroll::Scrollbar::vertical(&diff.comparison_scroll),
                    ).top(px(28.0)).bottom(px(if diff.wrap { 0.0 } else { 14.0 }))),
            )
            .into_any_element()
    }

    fn comparison_heading(&self, before: bool, ratio: f32, cx: &App) -> gpui::AnyElement {
        let diff = self.diff.as_ref().expect("comparison requires a diff");
        let language = crate::gpui_shell::config::ui_language(cx);
        let title = language.text(if diff.historical {
            if before { Message::TurnChangesBefore } else { Message::TurnChangesAfter }
        } else {
            match (before, diff.staged) {
                (true, true) => Message::DiffCompareHead,
                (true, false) | (false, true) => Message::DiffCompareIndex,
                (false, false) => Message::DiffCompareWorking,
            }
        });
        div()
            .w(relative(ratio))
            .min_w_0()
            .px_3()
            .text_xs()
            .truncate()
            .text_color(cx.theme().muted_foreground)
            .child(title)
            .into_any_element()
    }

    fn comparison_horizontal(
        &self,
        data: &Prepared,
        before: bool,
        ratio: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let diff = self.diff.as_ref().unwrap();
        let scroll = if before { &diff.before_scroll } else { &diff.after_scroll };
        let text = if before { &data.before_widest } else { &data.after_widest };
        let width = window
            .text_system()
            .shape_line(
                text.clone(),
                cx.theme().mono_font_size,
                &[gpui::TextRun {
                    len: text.len(),
                    font: gpui::font(cx.theme().mono_font_family.clone()),
                    color: cx.theme().foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
            + px(110.0);
        div()
            .w(relative(ratio))
            .h_full()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .child(
                div()
                    .id(if before { "diff-before-x" } else { "diff-after-x" })
                    .size_full()
                    .overflow_x_scroll()
                    .track_scroll(scroll)
                    .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                    .child(div().w(width).h(px(1.0))),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(gpui_component::scroll::Scrollbar::horizontal(scroll)),
            )
            .into_any_element()
    }

    fn comparison_line(
        &self,
        data: &Prepared,
        index: usize,
        before: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row = &data.data.lines[index];
        let line = if before { &row.before } else { &row.after };
        let Some(line) = line else { return empty_cell(cx) };
        let diff = self.diff.as_ref().expect("comparison requires diff state");
        let selected = self.input.read(cx).selected_range();
        let selected = line.patch.as_ref().is_some_and(|range| {
            !selected.is_empty() && selected.start < range.end && selected.end > range.start
        });
        let background = if selected {
            cx.theme().primary.opacity(0.18)
        } else if row.changed {
            if before { cx.theme().danger.opacity(0.14) } else { cx.theme().success.opacity(0.14) }
        } else {
            gpui::transparent_black()
        };
        let range = if before { &data.before_ranges[index] } else { &data.after_ranges[index] };
        let syntax = if before { &data.before } else { &data.after };
        let highlights: Vec<(Range<usize>, HighlightStyle)> = syntax
            .styles(range, &cx.theme().highlight_theme)
            .into_iter()
            .map(|(span, style)| (span.start - range.start..span.end - range.start, style))
            .collect();
        let patch = line.patch.clone();
        let busy = diff.loading;
        h_flex()
            .w_full()
            .flex_1()
            .min_w_0()
            .min_h(px(24.0))
            .gap_2()
            .pr_3()
            .items_start()
            .line_height(px(24.0))
            .when(!diff.wrap, |row| row.h(px(24.0)))
            .bg(background)
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .w(px(54.0))
                    .flex_shrink_0()
                    .text_right()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(line.number.to_string()),
            )
            .child(
                div()
                    .w(px(12.0))
                    .flex_shrink_0()
                    .text_color(if before { cx.theme().danger } else { cx.theme().success })
                    .child(if row.changed { if before { "−" } else { "+" } } else { "" }),
            )
            .child(
                div().flex_1().min_w_0().overflow_hidden().child(
                    div()
                        .relative()
                        .when(diff.wrap, |text| text.w_full().whitespace_normal())
                        .when(!diff.wrap, |text| {
                            text.whitespace_nowrap().left(if before {
                                diff.before_scroll.offset().x
                            } else {
                                diff.after_scroll.offset().x
                            })
                        })
                        .child(
                            gpui::StyledText::new(line.text.clone()).with_highlights(highlights),
                        ),
                ),
            )
            .when(patch.is_some(), |cell| cell.cursor_pointer())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if busy {
                        return;
                    }
                    let Some(diff) = &mut this.diff else { return };
                    let Some(range) = &patch else {
                        diff.anchor = None;
                        this.input.update(cx, |input, cx| input.set_selected_range(0..0, cx));
                        cx.notify();
                        return;
                    };
                    let selected = if event.modifiers.shift {
                        let anchor = diff.anchor.as_ref().unwrap_or(range);
                        anchor.start.min(range.start)..anchor.end.max(range.end)
                    } else {
                        diff.anchor = Some(range.clone());
                        range.clone()
                    };
                    this.input.update(cx, |input, cx| input.set_selected_range(selected, cx));
                    cx.notify();
                }),
            )
            .into_any_element()
    }
}

fn empty_cell(cx: &App) -> gpui::AnyElement {
    let color = cx.theme().muted_foreground.opacity(0.12);
    div()
        .w_full()
        .flex_1()
        .min_h(px(24.0))
        .relative()
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let width = f32::from(bounds.size.width);
                    let height = f32::from(bounds.size.height);
                    for x in (0..(width as usize + 24)).step_by(8) {
                        let mut path = gpui::PathBuilder::stroke(px(1.0));
                        path.move_to(bounds.origin + gpui::point(px(x as f32), px(0.0)));
                        path.line_to(
                            bounds.origin + gpui::point(px(x as f32 - height), px(height)),
                        );
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color);
                        }
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}
