//! A dialog owns one immutable thread target; it never resumes the CLI session.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ai_sessions::changes::{self, Edit, Error, Turn};
use crate::gpui_shell::code_tab::CodeTabView;
use crate::gpui_shell::prelude::*;
use crate::i18n::Message;
use gpui::{App, AppContext as _, Context, Entity, ParentElement as _, Styled as _, Window, px};

mod panel;

pub(super) fn open(thread: String, root: PathBuf, window: &mut Window, cx: &mut App) {
    let panel = cx.new(|cx| {
        let mut panel = Changes {
            thread,
            root,
            turns: Vec::new(),
            next: None,
            selected: None,
            task: None,
            error: None,
            turns_scroll: gpui::UniformListScrollHandle::new(),
            files_scroll: gpui::UniformListScrollHandle::new(),
        };
        panel.load_turns(None, cx);
        panel
    });
    let language = crate::gpui_shell::config::ui_language(cx);
    window.open_dialog(cx, move |dialog, window, _| {
        dialog
            .title(language.text(Message::TurnChangesTitle))
            .w(px((f32::from(window.viewport_size().width) - 48.0).clamp(280.0, 1200.0)))
            .child(panel.clone())
    });
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

struct Changes {
    thread: String,
    root: PathBuf,
    turns: Vec<Turn>,
    next: Option<String>,
    selected: Option<Selection>,
    task: Option<gpui::Task<()>>,
    error: Option<Error>,
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
