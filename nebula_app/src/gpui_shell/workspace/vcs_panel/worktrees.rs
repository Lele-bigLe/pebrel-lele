//! Git owns the worktree list; opening one reuses the project library and pane selector.

use super::*;
use crate::display::side_panel::GitLocation;
use gpui::EventEmitter;

impl NebulaWorkspace {
    pub(super) fn open_worktree_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location @ GitLocation::Local { .. }) = self.side_panel.git_location() else {
            return;
        };
        let manager = cx.new(|cx| Worktrees::new(location, window, cx));
        cx.subscribe_in(&manager, window, |this, _, path: &PathBuf, window, cx| {
            window.close_dialog(cx);
            this.select_project(projects::ProjectFilter::Root(path.clone()), window, cx);
        })
        .detach();
        let language = crate::gpui_shell::config::ui_language(cx);
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title(language.text(Message::WorktreeTitle)).w(px(620.0)).child(manager.clone())
        });
    }
}

struct Worktrees {
    location: GitLocation,
    branch: Entity<InputState>,
    base: Entity<InputState>,
    path: Entity<InputState>,
    allow_dirty: bool,
    entries: Vec<crate::runtime_api::git::WorktreeInfo>,
    task: Option<gpui::Task<()>>,
    error: Option<String>,
}

impl EventEmitter<PathBuf> for Worktrees {}

impl Worktrees {
    fn new(location: GitLocation, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let language = crate::gpui_shell::config::ui_language(cx);
        let mut this = Self {
            location,
            branch: cx.new(|cx| {
                InputState::new(window, cx).placeholder(language.text(Message::WorktreeBranch))
            }),
            base: cx.new(|cx| {
                InputState::new(window, cx).placeholder(language.text(Message::WorktreeBase))
            }),
            path: cx.new(|cx| {
                InputState::new(window, cx).placeholder(language.text(Message::WorktreePath))
            }),
            allow_dirty: false,
            entries: vec![],
            task: None,
            error: None,
        };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        self.error = None;
        let location = self.location.clone();
        let work = cx
            .background_executor()
            .spawn(async move { crate::runtime_api::git::read_worktrees(&location) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(entries) => this.entries = entries,
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let GitLocation::Local { root } = &self.location else { return };
        let branch = self.branch.read(cx).value().trim().to_owned();
        if branch.is_empty() {
            self.error = Some(
                crate::gpui_shell::config::ui_language(cx)
                    .text(Message::WorktreeBranchRequired)
                    .into(),
            );
            cx.notify();
            return;
        }
        let base = self.base.read(cx).value().trim().to_owned();
        let path = self.path.read(cx).value().trim().to_owned();
        let request = crate::git_worktree::WorktreeRequest {
            source_cwd: root.clone(),
            agent_name: branch.clone(),
            branch: Some(branch),
            base: (!base.is_empty()).then_some(base),
            path: (!path.is_empty()).then(|| PathBuf::from(path)),
            allow_dirty_source: self.allow_dirty,
        };
        self.error = None;
        // Once started, the synchronous transaction finishes in the background. Git remains
        // the durable authority even if the dialog closes before its completion is displayed.
        let work = cx.background_executor().spawn(async move {
            let path = crate::git_worktree::WorktreeTransaction::prepare(request)?.commit().path;
            crate::platform::update_installation::canonical(&path).map_err(|error| {
                crate::runtime_api::ApiError::new(
                    "worktree_open_failed",
                    format!(
                        "Worktree created at {}, but its directory could not be opened: {error}",
                        path.display()
                    ),
                )
            })
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(path) => cx.emit(path),
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn open(&mut self, path: String, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let work = cx.background_executor().spawn(async move {
            let path = PathBuf::from(path);
            crate::platform::update_installation::canonical(&path).and_then(|canonical| {
                if canonical.is_dir() {
                    Ok(canonical)
                } else {
                    Err(std::io::Error::other("Worktree directory is unavailable"))
                }
            })
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(path) => cx.emit(path),
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl Render for Worktrees {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let source = match &self.location {
            GitLocation::Local { root } => root.display().to_string(),
            _ => String::new(),
        };
        let muted = cx.theme().muted_foreground;
        let mut list = v_flex().gap_2();
        for (index, entry) in self.entries.iter().cloned().enumerate() {
            let path = entry.path.clone();
            let label = if entry.bare {
                language.text(Message::WorktreeBare).to_owned()
            } else if entry.branch.is_empty() {
                language.text(Message::WorktreeDetached).to_owned()
            } else {
                entry.branch.clone()
            };
            list = list.child(
                h_flex()
                    .gap_2()
                    .p_2()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded_md()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().truncate().child(label))
                            .child(div().text_xs().text_color(muted).truncate().child(entry.path))
                            .when(entry.locked, |row| {
                                row.child(
                                    div().text_xs().child(language.text(Message::WorktreeLocked)),
                                )
                            })
                            .when(entry.prunable, |row| {
                                row.child(
                                    div()
                                        .text_xs()
                                        .child(language.text(Message::WorktreeUnavailable)),
                                )
                            }),
                    )
                    .child(
                        Button::new(("worktree-open", index))
                            .small()
                            .label(language.text(Message::WorktreeOpen))
                            .disabled(busy || entry.bare || entry.prunable)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open(path.clone(), cx)),
                            ),
                    ),
            );
        }
        v_flex()
            .w_full()
            .gap_3()
            .child(div().text_xs().text_color(muted).whitespace_normal().child(source))
            .child(div().text_sm().text_color(muted).child(language.text(Message::WorktreeHint)))
            .child(Input::new(&self.branch).disabled(busy))
            .child(Input::new(&self.base).disabled(busy))
            .child(Input::new(&self.path).disabled(busy))
            .child(
                Checkbox::new("worktree-dirty")
                    .checked(self.allow_dirty)
                    .disabled(busy)
                    .label(language.text(Message::WorktreeAllowDirty))
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.allow_dirty = *checked;
                        cx.notify();
                    })),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("worktree-create")
                            .label(language.text(Message::WorktreeCreate))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                    )
                    .child(
                        Button::new("worktree-refresh")
                            .ghost()
                            .label(language.text(Message::WorktreeRefresh))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
            .when(busy, |root| {
                root.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(language.text(Message::WorktreeLoading)),
                )
            })
            .when_some(self.error.clone(), |root, error| {
                root.child(
                    div().text_sm().text_color(cx.theme().danger).whitespace_normal().child(error),
                )
            })
            .child(div().id("worktree-list").max_h(px(240.0)).overflow_y_scrollbar().child(list))
    }
}
