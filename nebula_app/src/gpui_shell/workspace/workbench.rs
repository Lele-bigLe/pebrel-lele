//! Project workbench composition. Git/files still use the single SidePanel model.

use super::*;
use crate::display::side_panel::PanelView;
use crate::gpui_shell::code_tab::{CodeTabView, DiffPosition};
use crate::i18n::Message;
use crate::runtime_api::tabs::TabId;
use gpui_component::button::DropdownButton;
use gpui_component::menu::PopupMenuItem;

#[derive(Default)]
pub(super) struct State {
    preview: Option<TabId>,
    return_tab: Option<TabId>,
    last_active: Option<TabId>,
    was_changes: bool,
    positions: Vec<DiffPosition>,
    pub(super) turn_review: Option<super::agents::changes::Review>,
}

impl NebulaWorkspace {
    pub(super) fn workbench_changes(&self, cx: &App) -> bool {
        self.project_tree_mode(cx) && self.side_panel.open && self.side_panel.view == PanelView::Git
    }

    fn workbench_diff(&self, cx: &App) -> Option<Entity<CodeTabView>> {
        let WorkspaceTab::Code { view, .. } = self.tabs.get(self.active)? else { return None };
        let location = self.side_panel.git_location()?;
        (view.read(cx).review_location() == Some(&location)).then(|| view.clone())
    }

    pub(super) fn prepare_workbench_activation(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_turn_review();
        let diff = matches!(self.tabs.get(index), Some(WorkspaceTab::Code { view, .. }) if view.read(cx).review_location().is_some());
        if self.workbench_changes(cx) && !diff {
            self.side_panel.open = false;
            if index == self.active {
                self.focus_active(window, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn sync_workbench_route(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open {
            return;
        }
        let mut changes = self.workbench_changes(cx);
        let active = self.tab_meta.get(self.active).map(|meta| meta.runtime_id.clone());
        let diff = self.workbench_diff(cx).is_some();
        if changes && !self.workbench.was_changes {
            if active != self.workbench.preview {
                self.workbench.return_tab = active.clone();
            }
            if !diff {
                if let Some(index) = self
                    .workbench
                    .preview
                    .as_ref()
                    .and_then(|id| self.tab_meta.iter().position(|meta| &meta.runtime_id == id))
                    .filter(|index| self.sidebar_tab_indices().contains(index))
                {
                    self.activate_tab(index, window, cx);
                }
            }
        }
        // A new terminal/document or an explicit tab activation leaves the review route.
        if changes && self.workbench.was_changes && !diff && active != self.workbench.last_active {
            self.side_panel.open = false;
            changes = false;
        }
        if !changes && self.workbench.was_changes && active == self.workbench.preview {
            if let Some(index) = self
                .workbench
                .return_tab
                .as_ref()
                .and_then(|id| self.tab_meta.iter().position(|meta| &meta.runtime_id == id))
                .filter(|index| self.sidebar_tab_indices().contains(index))
            {
                self.activate_tab(index, window, cx);
            }
        }
        self.workbench.was_changes = changes;
        self.workbench.last_active =
            self.tab_meta.get(self.active).map(|meta| meta.runtime_id.clone());
        if let Some(WorkspaceTab::Code { view, .. }) = self.tabs.get(self.active) {
            view.update(cx, |view, cx| view.set_hosted_diff(changes, cx));
        }
    }

    pub(super) fn select_workbench_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_turn_review();
        self.tabs_section_collapsed = false;
        if self.side_panel.view == PanelView::Git {
            self.side_panel.open = false;
        }
        self.sync_workbench_route(window, cx);
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn render_workbench_modes(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let changes = self.workbench_changes(cx);
        let sessions = self
            .sidebar_tab_indices()
            .iter()
            .filter(|&&index| self.tabs[index].is_terminal())
            .count();
        let files = self
            .side_panel
            .git_snapshot()
            .map(|git| (git.unstaged.len() + git.staged.len()).to_string())
            .unwrap_or_else(|| "—".to_owned());
        h_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .pb_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("workbench-sessions")
                    .ghost()
                    .small()
                    .flex_1()
                    .min_w_0()
                    .selected(!changes)
                    .h(px(34.0))
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(language.text(Message::ProjectSessions))
                            .child(
                                div()
                                    .px_1()
                                    .rounded_sm()
                                    .bg(cx.theme().muted)
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(sessions.to_string()),
                            ),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.select_workbench_sessions(window, cx)
                    })),
            )
            .child(
                Button::new("workbench-changes")
                    .ghost()
                    .small()
                    .flex_1()
                    .min_w_0()
                    .selected(changes)
                    .h(px(34.0))
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(language.text(Message::VcsChanges))
                            .child(
                                div()
                                    .px_1()
                                    .rounded_sm()
                                    .bg(cx.theme().muted)
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(files),
                            ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.select_side_panel_view(PanelView::Git, cx)
                    })),
            )
            .child(self.render_new_session_control(cx))
            .into_any_element()
    }

    fn render_new_session_control(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let target = Self::new_session_target(cx);
        let hint = language.format(
            Message::SessionTreeNewWith,
            &[("target", Self::new_session_target_label(target, language))],
        );
        let owner = cx.entity().downgrade();
        DropdownButton::new("workbench-new-session")
            .ghost()
            .xsmall()
            .flex_shrink_0()
            .tooltip(language.text(Message::SessionTreeDefault))
            .button(
                Button::new("workbench-new")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .aria_label(hint.clone())
                    .tooltip(hint)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.side_panel.view == PanelView::Git {
                            this.side_panel.open = false;
                        }
                        this.create_default_session(window, cx);
                    })),
            )
            .dropdown_menu(move |mut menu, _, cx| {
                let current = Self::new_session_target(cx);
                for target in nebula_settings::NewSessionTarget::ALL {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(language.format(
                            Message::SessionTreeNewWith,
                            &[("target", Self::new_session_target_label(target, language))],
                        ))
                        .on_click(move |_, window, cx| {
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |this, cx| {
                                    if this.side_panel.view == PanelView::Git {
                                        this.side_panel.open = false;
                                    }
                                    this.create_session(target, window, cx);
                                });
                            }
                        }),
                    );
                }
                menu = menu.separator().label(language.text(Message::SessionTreeDefault));
                for target in nebula_settings::NewSessionTarget::ALL {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(Self::new_session_target_label(target, language))
                            .checked(target == current)
                            .on_click(move |_, window, cx| {
                                if let Some(owner) = owner.upgrade() {
                                    owner.update(cx, |this, cx| {
                                        this.set_new_session_target(target, window, cx)
                                    });
                                }
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    pub(super) fn render_workbench_changes_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let switcher = self.render_project_switcher(cx).into_any_element();
        let modes = self.render_workbench_modes(cx);
        let git = self.render_git_tree(window, cx);
        v_flex()
            .w(px(self.sidebar_width))
            .h_full()
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .bg(cx.theme().background)
            .child(switcher)
            .child(modes)
            .child(div().flex_1().min_h_0().w_full().child(git))
            .child(self.render_usage_button(cx))
            .into_any_element()
    }

    pub(super) fn render_workbench_header(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if self.turn_review_visible() { return Some(self.render_turn_review_header(cx)); }
        let terminal = self.tabs.get(self.active).is_some_and(WorkspaceTab::is_terminal);
        if !self.project_tree_mode(cx)
            && !(terminal
                && self.tabs_position == nebula_settings::TabsPositionName::Sidebar
                && !self.settings_open
                && !self.reader_focus_active(cx))
        {
            return None;
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        if self.workbench_changes(cx) {
            if let Some(view) = self.workbench_diff(cx) {
                if let Some(location) = view.read(cx).review_location().cloned() {
                    let count = self.review_comment_count(&location, cx);
                    return Some(
                        view.update(cx, |view, cx| view.render_hosted_diff_toolbar(count, cx)),
                    );
                }
            }
        }
        let changes = self.workbench_changes(cx);
        let title = if changes {
            language.text(Message::ReviewQueueTitle).into()
        } else {
            self.tabs
                .get(self.active)
                .map(|_| self.tab_title(self.active, cx))
                .unwrap_or_else(|| language.text(Message::ProjectSessions).into())
        };
        let path =
            self.new_tab_project().map(|root| root.display().to_string()).unwrap_or_default();
        Some(
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
                        .child(div().text_sm().font_semibold().truncate().child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(path),
                        ),
                )
                .when(!changes, |row| {
                    row.when(terminal, |row| row.child(self.render_terminal_controls(cx))).child(
                        Button::new("workbench-review-changes")
                            .small()
                            .label(language.text(Message::WorkbenchViewChanges))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.select_side_panel_view(PanelView::Git, cx)
                            })),
                    )
                })
                .child(
                    Button::new("workbench-files")
                        .ghost()
                        .small()
                        .icon(IconName::Folder)
                        .tooltip(language.text(Message::WorkbenchFiles))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.select_side_panel_view(PanelView::Files, cx)
                        })),
                )
                .into_any_element(),
        )
    }

    fn render_terminal_controls(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let owner = cx.entity().downgrade();
        h_flex()
            .gap_1()
            .flex_shrink_0()
            .child(
                Button::new("workbench-new-shell")
                    .ghost()
                    .small()
                    .icon(IconName::SquareTerminal)
                    .label(language.text(Message::WorkbenchNewTerminal))
                    .on_click(cx.listener(|this, _, window, cx| this.add_terminal(window, cx))),
            )
            .child(
                DropdownButton::new("workbench-split-options")
                    .ghost()
                    .small()
                    .tooltip(language.text(Message::WorkbenchSplitTerminal))
                    .button(
                        Button::new("workbench-split-terminal")
                            .ghost()
                            .small()
                            .icon(IconName::PanelRight)
                            .label(language.text(Message::WorkbenchSplitTerminal))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.split_workbench_terminal(
                                    SplitDirection::LeftRight,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .dropdown_menu(move |mut menu, _, _| {
                        for (direction, message) in [
                            (SplitDirection::LeftRight, Message::TabMenuSplitLeftRight),
                            (SplitDirection::TopBottom, Message::TabMenuSplitTopBottom),
                        ] {
                            let owner = owner.clone();
                            menu = menu.item(PopupMenuItem::new(language.text(message)).on_click(
                                move |_, window, cx| {
                                    if let Some(owner) = owner.upgrade() {
                                        owner.update(cx, |this, cx| {
                                            this.split_workbench_terminal(direction, window, cx);
                                        });
                                    }
                                },
                            ));
                        }
                        menu
                    }),
            )
            .into_any_element()
    }

    fn split_workbench_terminal(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.split_focused(direction, window, cx) {
            Ok(_) => {
                let id = self.meta(self.active).runtime_id.clone();
                self.projects.expanded_sessions.insert(id);
                cx.notify();
            },
            Err(error) => window.push_notification(Notification::error(error.message), cx),
        }
    }

    pub(super) fn workbench_empty_review(&self, cx: &App) -> bool {
        self.workbench_changes(cx) && self.workbench_diff(cx).is_none()
    }

    pub(super) fn insert_workbench_preview(
        &mut self,
        view: Entity<CodeTabView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.workbench_changes(cx) {
            return false;
        }
        let previous = self.workbench.preview.as_ref().and_then(|id| {
            self.tab_meta.iter().position(|meta| {
                &meta.runtime_id == id && meta.custom_name.is_none() && meta.color.is_none()
            })
        });
        if let Some(index) = previous {
            if !matches!(&self.tabs[index], WorkspaceTab::Code { view, .. } if view.read(cx).review_location().is_some())
            {
                return false;
            }
            if !self.retain_review_before_close(index, window, cx) {
                return true;
            }
            if let WorkspaceTab::Code { view, .. } = &self.tabs[index] {
                if let Some(position) = view.read(cx).diff_position(cx) {
                    self.workbench.positions.retain(|old| !old.same_file(&position));
                    self.workbench.positions.push(position);
                    if self.workbench.positions.len() > 64 {
                        self.workbench.positions.remove(0);
                    }
                }
            }
            let project_root = self.new_tab_project();
            self.remove_tab_at(index);
            let subscription = cx.subscribe_in(&view, window, Self::on_code_tab_event);
            self.insert_tab_at(
                index,
                WorkspaceTab::Code { view: view.clone(), _subscription: subscription },
                TabMeta { project_root, ..TabMeta::default() },
            );
            self.active = index;
        } else {
            let subscription = cx.subscribe_in(&view, window, Self::on_code_tab_event);
            self.insert_new_tab(WorkspaceTab::Code {
                view: view.clone(),
                _subscription: subscription,
            });
        }
        self.restore_retained_review(&view, window, cx);
        if let Some(position) =
            self.workbench.positions.iter().find(|position| position.matches(view.read(cx)))
        {
            view.update(cx, |view, cx| view.restore_diff_position(position.clone(), cx));
        }
        self.workbench.preview = self.tab_meta.get(self.active).map(|meta| meta.runtime_id.clone());
        self.reveal_active_tab();
        self.focus_active(window, cx);
        cx.notify();
        true
    }
}
