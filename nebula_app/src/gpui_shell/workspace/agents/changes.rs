//! A workspace review owns one immutable thread target; it never resumes the CLI.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ai_sessions::changes::{self, Edit, Error, Turn};
use crate::gpui_shell::code_tab::CodeTabView;
use crate::gpui_shell::prelude::*;
use crate::i18n::Message;
use gpui::prelude::FluentBuilder as _;
use gpui::{App, AppContext as _, Context, Entity, ParentElement as _, Styled as _, Window, px};
use gpui::{InteractiveElement as _, IntoElement as _, div};

mod panel;

pub(in crate::gpui_shell::workspace) struct Review {
    panel: Entity<Changes>,
    tab: crate::runtime_api::tabs::TabId,
    pane: u64,
    open: bool,
}

impl super::super::NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn open_turn_review(
        &mut self,
        thread: String,
        root: PathBuf,
        pane: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tab_of_pane(pane).filter(|index| *index == self.active) else {
            return;
        };
        let current =
            self.tabs[index].focused_view().and_then(|view| view.read(cx).session_agent());
        if current.is_none_or(|mut agent| {
            agent.normalize_identity();
            agent.source != "codex" || agent.session_id.as_deref() != Some(thread.as_str())
        }) {
            return;
        }
        let tab = self.meta(index).runtime_id;
        let reusable = self.workbench.turn_review.as_ref().is_some_and(|review| {
            let panel = review.panel.read(cx);
            review.tab == tab && review.pane == pane && panel.thread == thread && panel.root == root
        });
        if !reusable {
            let panel = cx.new(|cx| {
                let mut panel = Changes {
                    thread,
                    root,
                    turns: Vec::new(),
                    next: None,
                    selected: None,
                    task: None,
                    error: None,
                    focus: cx.focus_handle(),
                    turns_scroll: gpui::UniformListScrollHandle::new(),
                    files_scroll: gpui::UniformListScrollHandle::new(),
                };
                panel.load_turns(None, cx);
                panel
            });
            self.workbench.turn_review = Some(Review { panel, tab, pane, open: true });
        } else if let Some(review) = &mut self.workbench.turn_review {
            review.open = true;
        }
        self.side_panel.open = false;
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(in crate::gpui_shell::workspace) fn turn_review_visible(&self) -> bool {
        !self.settings_open && self.workbench.turn_review.as_ref().is_some_and(|review| {
            review.open && self.tab_meta.get(self.active).is_some_and(|meta| meta.runtime_id == review.tab)
                && matches!(self.tabs.get(self.active), Some(super::super::WorkspaceTab::Terminal { focused, panes, .. })
                    if *focused == review.pane && panes.iter().any(|pane| pane.id == review.pane))
        })
    }

    pub(in crate::gpui_shell::workspace) fn turn_review_view(&self) -> Option<Entity<Changes>> {
        self.workbench
            .turn_review
            .as_ref()
            .filter(|_| self.turn_review_visible())
            .map(|review| review.panel.clone())
    }

    pub(in crate::gpui_shell::workspace) fn turn_review_focus(
        &self,
        cx: &App,
    ) -> Option<gpui::FocusHandle> {
        self.turn_review_view().map(|view| view.read(cx).focus.clone())
    }

    pub(in crate::gpui_shell::workspace) fn hide_turn_review(&mut self) {
        if let Some(review) = &mut self.workbench.turn_review {
            review.open = false;
        }
    }

    pub(in crate::gpui_shell::workspace) fn close_turn_review(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_turn_review();
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(in crate::gpui_shell::workspace) fn render_turn_review_header(
        &self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let panel = self.turn_review_view();
        h_flex()
            .h(px(60.0))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_semibold()
                            .child(language.text(Message::TurnChangesTitle)),
                    )
                    .child(
                        div().truncate().text_xs().text_color(cx.theme().muted_foreground).child(
                            self.workbench
                                .turn_review
                                .as_ref()
                                .map(|review| review.panel.read(cx).root.display().to_string())
                                .unwrap_or_default(),
                        ),
                    ),
            )
            .child(
                Button::new("turn-review-return")
                    .ghost()
                    .small()
                    .icon(IconName::ArrowLeft)
                    .label(language.text(Message::TurnChangesReturnSession))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.close_turn_review(window, cx)),
                    ),
            )
            .when_some(panel, |row, panel| {
                let busy = panel.read(cx).task.is_some();
                row.child(
                    Button::new("turn-review-refresh")
                        .ghost()
                        .small()
                        .icon(IconName::Redo)
                        .tooltip(language.text(Message::UsageRefresh))
                        .disabled(busy)
                        .loading(busy)
                        .on_click(move |_, _, cx| {
                            panel.update(cx, |panel, cx| {
                                if panel.task.is_none() {
                                    panel.selected = None;
                                    panel.load_turns(None, cx);
                                }
                            })
                        }),
                )
            })
            .into_any_element()
    }
}

struct FileEdits {
    path: String,
    edits: Vec<usize>,
}

struct Selection {
    turn: usize,
    edits: Vec<Arc<Edit>>,
    files: Vec<FileEdits>,
    next: Option<String>,
    file: usize,
    step: usize,
    viewer: Option<Entity<CodeTabView>>,
}

pub(in crate::gpui_shell::workspace) struct Changes {
    thread: String,
    root: PathBuf,
    turns: Vec<Turn>,
    next: Option<String>,
    selected: Option<Selection>,
    task: Option<gpui::Task<()>>,
    error: Option<Error>,
    focus: gpui::FocusHandle,
    turns_scroll: gpui::UniformListScrollHandle,
    files_scroll: gpui::UniformListScrollHandle,
}

impl Changes {
    fn load_turns(&mut self, cursor: Option<String>, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let thread = self.thread.clone();
        let reset = cursor.is_none();
        let previous = cursor.clone();
        self.error = None;
        let worker = cx.background_executor().spawn(async move { changes::turns(&thread, cursor) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(page) if page.next_cursor.is_none() || page.next_cursor != previous => {
                        if reset {
                            this.turns.clear();
                        }
                        for turn in page.data {
                            if !this.turns.iter().any(|old| old.id == turn.id) {
                                this.turns.push(turn);
                            }
                        }
                        this.next = page.next_cursor;
                        if reset {
                            this.turns_scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
                        }
                    },
                    Ok(_) => this.error = Some(Error::InvalidResponse),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn select_turn(&mut self, turn: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.task.is_some() || turn >= self.turns.len() {
            return;
        }
        self.selected = Some(Selection {
            turn,
            edits: Vec::new(),
            files: Vec::new(),
            next: None,
            file: 0,
            step: 0,
            viewer: None,
        });
        self.files_scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        self.load_edits(window, cx);
    }

    fn load_edits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let Some(selected) = &self.selected else { return };
        let turn = self.turns[selected.turn].id.clone();
        let thread = self.thread.clone();
        let cursor = selected.next.clone();
        self.error = None;
        let worker =
            cx.background_executor().spawn(async move { changes::edits(&thread, &turn, cursor) });
        let handle = window.window_handle();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ =
                    this.update(cx, |this, cx| {
                        this.task = None;
                        let Some(selected) = &mut this.selected else { return };
                        match result {
                            Ok(page) => {
                                let bytes: usize =
                                    selected.edits.iter().map(|edit| edit.diff.len()).sum();
                                let incoming: usize =
                                    page.data.iter().map(|edit| edit.diff.len()).sum();
                                if bytes.saturating_add(incoming) > 32 * 1024 * 1024
                                    || selected.edits.len() + page.data.len() > 4000
                                {
                                    this.error = Some(Error::TooLarge);
                                    cx.notify();
                                    return;
                                }
                                selected.next = page.next_cursor;
                                for edit in page.data {
                                    let destination =
                                        edit.kind.move_path.as_deref().unwrap_or(&edit.path);
                                    let index = selected.edits.len();
                                    if let Some(file) = selected.files.iter_mut().find(|file| {
                                        file.path == edit.path || file.path == destination
                                    }) {
                                        file.path = destination.to_owned();
                                        file.edits.push(index);
                                    } else {
                                        selected.files.push(FileEdits {
                                            path: destination.to_owned(),
                                            edits: vec![index],
                                        });
                                    }
                                    selected.edits.push(Arc::new(edit));
                                }
                                if selected.viewer.is_none() && !selected.files.is_empty() {
                                    this.select_edit(0, 0, window, cx);
                                }
                            },
                            Err(error) => this.error = Some(error),
                        }
                        cx.notify();
                    });
            });
        }));
        cx.notify();
    }

    fn select_edit(
        &mut self,
        file: usize,
        step: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = &mut self.selected else { return };
        let Some(edit) = selected
            .files
            .get(file)
            .and_then(|file| file.edits.get(step))
            .and_then(|index| selected.edits.get(*index))
        else {
            return;
        };
        let root = self.root.clone();
        let path = edit.kind.move_path.as_ref().unwrap_or(&edit.path).clone();
        let patch = edit.patch();
        selected.file = file;
        selected.step = step;
        selected.viewer =
            Some(cx.new(|cx| CodeTabView::new_turn_diff(root, path, patch, window, cx)));
        cx.notify();
    }

    fn path_label(&self, path: &str) -> String {
        Path::new(path)
            .strip_prefix(&self.root)
            .unwrap_or(Path::new(path))
            .to_string_lossy()
            .into_owned()
    }
}
