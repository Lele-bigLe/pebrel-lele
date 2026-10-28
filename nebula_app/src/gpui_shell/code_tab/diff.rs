//! Read-only, syntax-highlighted Git patches in the existing virtual code editor.

use super::*;
use crate::i18n::Message;
use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};

mod comparison;
mod historical;
pub(super) mod rows;
mod staging;
mod toolbar;

pub(super) struct DiffPreview {
    pub(super) key: MergeKey,
    pub(super) staged: bool,
    pub(super) historical: bool,
    pub(super) loading: bool,
    task: Option<gpui::Task<()>>,
    _input_subscription: gpui::Subscription,
    pub(super) snapshot: Option<[u8; 32]>,
    pub(super) review: super::review::ReviewState,
    hosted: bool,
    restore_position: Option<DiffPosition>,
    rows: std::sync::Arc<Vec<rows::PatchLine>>,
    pub(super) raw: bool,
    anchor: Option<std::ops::Range<usize>>,
    scroll: gpui::UniformListScrollHandle,
    comparison: Option<std::rc::Rc<comparison::Prepared>>,
    expanded: std::collections::HashSet<usize>,
    before_scroll: gpui::ScrollHandle,
    after_scroll: gpui::ScrollHandle,
    split: Entity<ResizableState>,
    _split_subscription: gpui::Subscription,
    wrap: bool,
    comparison_scroll: gpui::ListState,
    comparison_layout: std::cell::Cell<Option<(gpui::Pixels, gpui::Pixels, gpui::Pixels, bool)>>,
}

#[derive(Clone)]
pub(in crate::gpui_shell) struct DiffPosition {
    key: MergeKey,
    staged: bool,
    snapshot: [u8; 32],
    before: gpui::Point<gpui::Pixels>,
    after: gpui::Point<gpui::Pixels>,
    raw_offset: gpui::Point<gpui::Pixels>,
    raw: bool,
    expanded: std::collections::HashSet<usize>,
    comparison_offset: gpui::ListOffset,
    split_ratio: f32,
    wrap: bool,
}

impl DiffPosition {
    pub(in crate::gpui_shell) fn matches(&self, view: &CodeTabView) -> bool {
        view.matches_git_diff(&self.key.location, &self.key.relative_path, self.staged)
    }
    pub(in crate::gpui_shell) fn same_file(&self, other: &Self) -> bool {
        self.key.matches(&other.key.location, &other.key.relative_path)
            && self.staged == other.staged
    }
}

impl CodeTabView {
    pub(in crate::gpui_shell) fn set_hosted_diff(&mut self, hosted: bool, cx: &mut Context<Self>) {
        if let Some(diff) = &mut self.diff {
            if diff.hosted != hosted {
                diff.hosted = hosted;
                cx.notify();
            }
        }
    }

    pub(in crate::gpui_shell) fn diff_position(&self, cx: &gpui::App) -> Option<DiffPosition> {
        let diff = self.diff.as_ref()?;
        Some(DiffPosition {
            key: diff.key.clone(),
            staged: diff.staged,
            snapshot: diff.snapshot?,
            before: diff.before_scroll.offset(),
            after: diff.after_scroll.offset(),
            raw_offset: self.input.read(cx).scroll_offset(),
            raw: diff.raw,
            expanded: diff.expanded.clone(),
            comparison_offset: diff.comparison_scroll.logical_scroll_top(),
            split_ratio: comparison::split_ratio(diff, cx),
            wrap: diff.wrap,
        })
    }

    pub(in crate::gpui_shell) fn restore_diff_position(
        &mut self,
        position: DiffPosition,
        cx: &mut Context<Self>,
    ) {
        if let Some(diff) = &mut self.diff {
            diff.restore_position = Some(position);
            cx.notify();
        }
    }

    pub(crate) fn new_git_diff(
        location: GitLocation,
        relative_path: String,
        staged: bool,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_diff(location, relative_path, staged, path, false, window, cx)
    }

    fn new_diff(
        location: GitLocation,
        relative_path: String,
        staged: bool,
        path: PathBuf,
        historical: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let language = crate::gpui_shell::config::ui_language(cx);
        let name = Path::new(&relative_path)
            .file_name()
            .unwrap_or(Path::new(&relative_path).as_os_str())
            .to_string_lossy();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor("diff")
                .line_number(false)
                .indent_guides(false)
                .soft_wrap(false)
        });
        let input_subscription = cx.observe(&input, |_, _, cx| {
            cx.emit(CodeTabViewEvent::Changed);
            cx.notify();
        });
        let split = cx.new(|_| ResizableState::default());
        let split_subscription = cx.observe(&split, |this, split, cx| {
            let sizes = split.read(cx).sizes();
            if let Some(diff) = &this.diff {
                let widths = (
                    sizes.first().copied().unwrap_or_default(),
                    sizes.get(1).copied().unwrap_or_default(),
                );
                if diff.comparison_layout.get().is_none_or(|layout| (layout.0, layout.1) != widths)
                {
                    cx.notify();
                }
            }
        });
        let mut this = Self {
            title: language.format(
                if staged { Message::DiffStagedTabTitle } else { Message::DiffTabTitle },
                &[("name", &name)],
            ),
            path,
            input,
            notice: None,
            lines: 0,
            merge: None,
            file: None,
            _file_subscription: None,
            diff: Some(DiffPreview {
                key: MergeKey { location, relative_path },
                staged,
                historical,
                loading: false,
                task: None,
                _input_subscription: input_subscription,
                snapshot: None,
                review: super::review::ReviewState::new(window, cx),
                hosted: false,
                restore_position: None,
                rows: Default::default(),
                raw: false,
                anchor: None,
                scroll: gpui::UniformListScrollHandle::new(),
                comparison: None,
                expanded: Default::default(),
                before_scroll: gpui::ScrollHandle::new(),
                after_scroll: gpui::ScrollHandle::new(),
                split,
                _split_subscription: split_subscription,
                wrap: true,
                comparison_scroll: gpui::ListState::new(0, gpui::ListAlignment::Top, px(240.0)),
                comparison_layout: Default::default(),
            }),
        };
        if !historical {
            this.reload_git_diff(window, cx);
        }
        this
    }

    pub(crate) fn matches_git_diff(
        &self,
        location: &GitLocation,
        path: &str,
        staged: bool,
    ) -> bool {
        self.diff.as_ref().is_some_and(|diff| {
            !diff.historical && diff.key.matches(location, path) && diff.staged == staged
        })
    }

    pub(crate) fn reload_git_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(diff) = self.diff.as_mut() else { return };
        if diff.loading || diff.historical {
            return;
        }
        diff.loading = true;
        self.notice = None;
        let key = diff.key.clone();
        let staged = diff.staged;
        let language = super::language_for_path(&key.relative_path);
        diff.rows = Default::default();
        diff.comparison = None;
        diff.expanded.clear();
        diff.anchor = None;
        self.input.update(cx, |input, cx| input.set_value("", window, cx));
        let worker = cx.background_executor().spawn(async move {
            crate::runtime_api::git::read_comparison(&key.location, key.relative_path, staged).map(
                |data| {
                    let text = data.patch;
                    let snapshot = super::review::digest(&text);
                    let rows = rows::parse(&text);
                    let comparison = data
                        .original
                        .map_err(|error| error.message)
                        .and_then(|before| {
                            crate::runtime_api::git::patch::Comparison::build(&before, &text)
                                .map_err(str::to_owned)
                        })
                        .map(|data| comparison::Prepared::new(data, language));
                    (text, snapshot, rows, comparison)
                },
            )
        });
        let handle = window.window_handle();
        diff.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let Some(diff) = this.diff.as_mut() else { return };
                    diff.loading = false;
                    diff.task = None;
                    match result {
                        Ok((text, snapshot, rows, comparison)) => {
                            diff.snapshot = Some(snapshot);
                            diff.rows = std::sync::Arc::new(rows);
                            this.lines = text.lines().count();
                            match comparison {
                                Ok(comparison) => {
                                    diff.comparison = Some(std::rc::Rc::new(comparison))
                                },
                                Err(error) => {
                                    diff.raw = true;
                                    this.notice =
                                        Some(crate::gpui_shell::config::ui_language(cx).format(
                                            Message::DiffCompareUnavailable,
                                            &[("error", &error)],
                                        ));
                                },
                            }
                            if text.is_empty() {
                                this.notice = Some(
                                    crate::gpui_shell::config::ui_language(cx)
                                        .text(Message::DiffEmpty)
                                        .to_owned(),
                                );
                            }
                            this.input.update(cx, |input, cx| input.set_value(text, window, cx));
                            let count = comparison::row_count(diff);
                            diff.comparison_scroll.reset(count);
                            diff.comparison_layout.set(None);
                            if let Some(position) = diff
                                .restore_position
                                .take()
                                .filter(|position| position.snapshot == snapshot)
                            {
                                diff.raw = position.raw || diff.comparison.is_none();
                                diff.expanded = position.expanded;
                                diff.wrap = position.wrap;
                                diff.comparison_scroll.reset(comparison::row_count(diff));
                                diff.comparison_scroll.scroll_to(position.comparison_offset);
                                // The native panels acquire their container bounds on the first frame.
                                let split = diff.split.clone();
                                window.on_next_frame(move |window, cx| {
                                    split.update(cx, |split, cx| {
                                        let width: gpui::Pixels =
                                            split.sizes().iter().copied().sum();
                                        if width > px(0.0) {
                                            split.resize_panel(
                                                0,
                                                width * position.split_ratio,
                                                window,
                                                cx,
                                            );
                                        }
                                    });
                                });
                                diff.before_scroll.set_offset(position.before);
                                diff.after_scroll.set_offset(position.after);
                                this.input.update(cx, |input, cx| {
                                    input.set_scroll_offset(position.raw_offset, cx)
                                });
                            }
                        },
                        Err(error) => {
                            diff.snapshot = None;
                            this.lines = 0;
                            let language = crate::gpui_shell::config::ui_language(cx);
                            this.notice = Some(match error.code.as_str() {
                                "git_too_large" => language.text(Message::DiffTooLarge).to_owned(),
                                "git_stale" => language.text(Message::DiffStale).to_owned(),
                                "git_busy" => language.text(Message::DiffBusy).to_owned(),
                                "git_outcome_unknown" => {
                                    language.text(Message::DiffTimeout).to_owned()
                                },
                                "git_encoding" => language.text(Message::DiffEncoding).to_owned(),
                                _ => language
                                    .format(Message::DiffReadFailed, &[("error", &error.message)]),
                            });
                        },
                    }
                    if let Some(id) = diff.review.pending_locate.take() {
                        this.locate_review_comment(id, window, cx);
                    }
                    cx.emit(CodeTabViewEvent::Changed);
                    cx.notify();
                });
            });
        }));
        cx.emit(CodeTabViewEvent::Changed);
        cx.notify();
    }

    pub(super) fn render_git_diff(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let diff = self.diff.as_ref().expect("diff renderer requires preview state");
        let language = crate::gpui_shell::config::ui_language(cx);
        let toolbar = (!diff.hosted).then(|| self.render_diff_toolbar(None, cx));
        let theme = cx.theme();
        v_flex()
            .size_full()
            .min_w_0()
            .children(toolbar)
            .when(diff.loading, |body| {
                body.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(language.text(Message::DiffLoading)),
                )
            })
            .when_some(self.notice.as_ref(), |body, notice| {
                body.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .whitespace_normal()
                        .child(notice.clone()),
                )
            })
            .child(div().flex_1().min_h_0().child(if diff.raw {
                editor(&self.input, true, theme.mono_font_family.clone(), theme.mono_font_size)
                    .into_any_element()
            } else {
                self.render_diff_comparison(window, cx)
            }))
            .when(diff.review.open, |body| body.child(self.render_review_tools(cx)))
            .into_any_element()
    }
}
