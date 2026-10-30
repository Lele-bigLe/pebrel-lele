//! Stash reads stay with the dialog; writes use the existing VCS queue and exact source location.
use super::*;
use crate::display::side_panel::GitLocation;
use crate::runtime_api::git::StashInfo;
use gpui::{EventEmitter, WeakEntity, uniform_list};

enum Operation {
    Create { message: String, untracked: bool },
    Apply { oid: String, index: bool },
}
struct StashAction {
    location: GitLocation,
    operation: Operation,
}

impl NebulaWorkspace {
    pub(super) fn git_tools_menu(
        target: &WeakEntity<Self>,
        language: UiLanguage,
        local: bool,
        running: bool,
    ) -> gpui::AnyElement {
        let target = target.clone();
        Button::new("git-tools")
            .ghost()
            .xsmall()
            .icon(IconName::Ellipsis)
            .disabled(running)
            .aria_label(language.text(Message::GitStashesTools))
            .tooltip(language.text(Message::GitStashesTools))
            .dropdown_menu(move |menu, _, cx| {
                let stash = target.clone();
                let worktree = target.clone();
                let review = target.clone();
                let pull = target.clone();
                let push = target.clone();
                let fetch = target.clone();
                let (location, busy, ahead, publish, can_push) = target
                    .upgrade()
                    .map(|owner| {
                        let workspace = owner.read(cx);
                        (
                            workspace.side_panel.git_location(),
                            workspace.side_panel.op_running(),
                            workspace.side_panel.git_snapshot().map_or(0, |info| info.ahead),
                            workspace
                                .side_panel
                                .git_snapshot()
                                .is_some_and(|info| info.upstream.is_none()),
                            workspace.side_panel.git_snapshot().is_some_and(|info| info.can_push()),
                        )
                    })
                    .unwrap_or((None, true, 0, false, false));
                let push_location = location.clone();
                let fetch_location = location.clone();
                menu.item(
                    PopupMenuItem::new(language.text(Message::GitControlsFetch))
                        .disabled(busy || fetch_location.is_none())
                        .on_click(move |_, _, cx| {
                            if let Some(owner) = fetch.upgrade() {
                                owner.update(cx, |this, cx| {
                                    if fetch_location.is_some()
                                        && this.side_panel.git_location() == fetch_location
                                    {
                                        this.side_panel.git_fetch();
                                        cx.notify();
                                    }
                                });
                            }
                        }),
                )
                .item(
                    PopupMenuItem::new(language.text(Message::VcsPull))
                        .disabled(busy || location.is_none())
                        .on_click(move |_, _, cx| {
                            if let Some(owner) = pull.upgrade() {
                                owner.update(cx, |this, cx| {
                                    if location.is_some()
                                        && this.side_panel.git_location() == location
                                    {
                                        this.side_panel.git_pull();
                                        cx.notify();
                                    }
                                });
                            }
                        }),
                )
                .item(
                    PopupMenuItem::new(if publish {
                        language.text(Message::GitControlsPublishBranch).to_owned()
                    } else if ahead > 0 {
                        language.format(Message::VcsPushAhead, &[("count", &ahead.to_string())])
                    } else {
                        language.text(Message::VcsPush).to_owned()
                    })
                    .disabled(busy || push_location.is_none() || !can_push)
                    .on_click(move |_, _, cx| {
                        if let Some(owner) = push.upgrade() {
                            owner.update(cx, |this, cx| {
                                if push_location.is_some()
                                    && this.side_panel.git_location() == push_location
                                {
                                    this.side_panel.git_push();
                                    cx.notify();
                                }
                            });
                        }
                    }),
                )
                .separator()
                .item(PopupMenuItem::new(language.text(Message::ReviewQueueTitle)).on_click(
                    move |_, window, cx| {
                        if let Some(owner) = review.upgrade() {
                            owner.update(cx, |this, cx| {
                                if let Some(location) = this.side_panel.git_location() {
                                    this.open_repository_review_queue(location, window, cx);
                                }
                            });
                        }
                    },
                ))
                .item(PopupMenuItem::new(language.text(Message::GitStashesTitle)).on_click(
                    move |_, window, cx| {
                        if let Some(owner) = stash.upgrade() {
                            owner.update(cx, |this, cx| this.open_stash_manager(window, cx));
                        }
                    },
                ))
                .item(
                    PopupMenuItem::new(language.text(Message::WorktreeTitle))
                        .disabled(!local)
                        .on_click(move |_, window, cx| {
                            if let Some(owner) = worktree.upgrade() {
                                owner.update(cx, |this, cx| this.open_worktree_manager(window, cx));
                            }
                        }),
                )
            })
            .into_any_element()
    }
    pub(super) fn open_stash_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.side_panel.git_location() else { return };
        let manager = cx.new(|cx| Stashes::new(location, window, cx));
        cx.subscribe_in(&manager, window, |this, _, action: &StashAction, window, cx| {
            let accepted = this.side_panel.git_location().as_ref() == Some(&action.location)
                && match &action.operation {
                    Operation::Create { message, untracked } => {
                        this.side_panel.git_stash_create(message, *untracked)
                    },
                    Operation::Apply { oid, index } => this.side_panel.git_stash_apply(oid, *index),
                };
            if accepted {
                window.close_dialog(cx);
                cx.notify();
            } else {
                let message =
                    crate::gpui_shell::config::ui_language(cx).text(Message::GitControlsChanged);
                crate::gpui_shell::toast::toast(
                    window,
                    cx,
                    crate::display::ToastKind::Warning,
                    message,
                );
            }
        })
        .detach();
        let language = crate::gpui_shell::config::ui_language(cx);
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(language.text(Message::GitStashesTitle))
                .w(px(760.0))
                .child(manager.clone())
        });
    }
}

struct Stashes {
    location: GitLocation,
    message: Entity<InputState>,
    preview: Entity<InputState>,
    entries: Vec<StashInfo>,
    selected: Option<usize>,
    has_more: bool,
    tracked: bool,
    untracked: bool,
    include_untracked: bool,
    restore_index: bool,
    error: Option<String>,
    task: Option<gpui::Task<()>>,
}
impl EventEmitter<StashAction> for Stashes {}

impl Stashes {
    fn new(location: GitLocation, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let language = crate::gpui_shell::config::ui_language(cx);
        let message = cx.new(|cx| {
            InputState::new(window, cx).placeholder(language.text(Message::GitStashesMessage))
        });
        cx.subscribe(&message, |_, _, _: &gpui_component::input::InputEvent, cx| cx.notify())
            .detach();
        let mut this = Self {
            location,
            message,
            preview: cx.new(|cx| {
                InputState::new(window, cx).code_editor("diff").line_number(false).soft_wrap(false)
            }),
            entries: vec![],
            selected: None,
            has_more: false,
            tracked: false,
            untracked: false,
            include_untracked: false,
            restore_index: false,
            error: None,
            task: None,
        };
        this.refresh(window, cx);
        this
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let location = self.location.clone();
        self.selected = None;
        self.tracked = false;
        self.untracked = false;
        self.preview.update(cx, |input, cx| input.set_value("", window, cx));
        self.error = None;
        let work = cx
            .background_executor()
            .spawn(async move { crate::runtime_api::git::read_stashes(&location) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(snapshot) => {
                        this.entries = snapshot.entries;
                        this.has_more = snapshot.has_more;
                        this.tracked = snapshot.tracked;
                        this.untracked = snapshot.untracked;
                        this.selected = None;
                    },
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let Some(entry) = self.entries.get(index) else { return };
        let oid = entry.oid.clone();
        let location = self.location.clone();
        self.selected = Some(index);
        self.error = None;
        self.preview.update(cx, |input, cx| input.set_value("", window, cx));
        let work = cx
            .background_executor()
            .spawn(async move { crate::runtime_api::git::read_stash_diff(&location, &oid) });
        let handle = window.window_handle();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.task = None;
                    match result {
                        Ok(text) => {
                            this.preview.update(cx, |input, cx| input.set_value(text, window, cx))
                        },
                        Err(error) => this.error = Some(error.message),
                    }
                    cx.notify();
                });
            });
        }));
        cx.notify();
    }
}

impl gpui::Render for Stashes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let can_create = self.tracked || (self.include_untracked && self.untracked);
        v_flex()
            .gap_2()
            .w_full()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().min_w_0().truncate().text_xs().child(
                        match &self.location {
                            GitLocation::Local { root } => root.display().to_string(),
                            GitLocation::Wsl { distro, root } => format!("{distro}: {root}"),
                        },
                    ))
                    .child(
                        Button::new("stash-refresh")
                            .ghost()
                            .small()
                            .label(language.text(Message::UsageRefresh))
                            .disabled(busy)
                            .loading(busy)
                            .on_click(cx.listener(|this, _, window, cx| this.refresh(window, cx))),
                    ),
            )
            .when_some(self.error.as_ref(), |view, error| {
                view.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().warning)
                        .child(language.format(Message::GitStashesFailed, &[("error", error)])),
                )
            })
            .when(!busy && self.error.is_none() && self.entries.is_empty(), |view| {
                view.child(div().text_sm().child(language.text(Message::GitStashesEmpty)))
            })
            .child(
                uniform_list(
                    "stash-list",
                    self.entries.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                let entry = this.entries.get(index)?;
                                Some(
                                    div()
                                        .id(("stash", index))
                                        .h(px(30.0))
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .w_full()
                                        .cursor_pointer()
                                        .when(this.selected == Some(index), |row| {
                                            row.bg(cx.theme().selection)
                                        })
                                        .child(div().min_w_0().truncate().text_sm().child(format!(
                                            "stash@{{{index}}} · {} · {}",
                                            &entry.oid[..8],
                                            entry.subject
                                        )))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.select(index, window, cx)
                                        }))
                                        .into_any_element(),
                                )
                            })
                            .collect()
                    }),
                )
                .h(px(120.0))
                .w_full(),
            )
            .when(self.has_more, |view| {
                view.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(language.text(Message::GitStashesMore)),
                )
            })
            .child(
                div()
                    .w_full()
                    .h(px((f32::from(window.viewport_size().height) * 0.55 - 220.0)
                        .clamp(100.0, 240.0)))
                    .child(Input::new(&self.preview).h_full().disabled(true)),
            )
            .child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        Button::new("stash-restore-index")
                            .ghost()
                            .small()
                            .selected(self.restore_index)
                            .label(language.text(Message::GitStashesIndex))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.restore_index = !this.restore_index;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("stash-apply")
                            .small()
                            .label(language.text(Message::GitStashesApply))
                            .disabled(busy || self.selected.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(entry) =
                                    this.selected.and_then(|index| this.entries.get(index))
                                {
                                    cx.emit(StashAction {
                                        location: this.location.clone(),
                                        operation: Operation::Apply {
                                            oid: entry.oid.clone(),
                                            index: this.restore_index,
                                        },
                                    });
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(language.text(Message::GitStashesCreateHint)),
            )
            .child(Input::new(&self.message).disabled(busy))
            .child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        Button::new("stash-include-untracked")
                            .ghost()
                            .small()
                            .selected(self.include_untracked)
                            .label(language.text(Message::GitStashesUntracked))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.include_untracked = !this.include_untracked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("stash-create")
                            .small()
                            .primary()
                            .label(language.text(Message::GitStashesCreate))
                            .disabled(
                                busy || !can_create
                                    || self.message.read(cx).value().chars().count() > 256,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.emit(StashAction {
                                    location: this.location.clone(),
                                    operation: Operation::Create {
                                        message: this.message.read(cx).value().to_string(),
                                        untracked: this.include_untracked,
                                    },
                                })
                            })),
                    ),
            )
    }
}
