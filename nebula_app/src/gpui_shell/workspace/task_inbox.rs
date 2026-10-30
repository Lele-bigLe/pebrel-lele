//! Bounded, window-owned notification history over existing pane lifecycle facts.

use super::*;
use crate::i18n::Message;
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    Unread,
    All,
    Attention,
    Running,
    Errors,
}

struct Entry {
    id: u64,
    pane: u64,
    project: Option<PathBuf>,
    title: String,
    body: String,
    at: chrono::DateTime<chrono::Local>,
    failure: bool,
    read: bool,
}

#[derive(Default)]
pub(super) struct TaskInbox {
    open: bool,
    filter: Filter,
    all_projects: bool,
    entries: VecDeque<Entry>,
    next_id: u64,
    focus: Option<gpui::FocusHandle>,
    focus_subscription: Option<Subscription>,
}

struct Row {
    key: SharedString,
    notification: Option<u64>,
    pane: u64,
    title: String,
    body: String,
    detail: String,
    unread: bool,
    open: bool,
}

impl NebulaWorkspace {
    pub(super) fn record_task_notification(
        &mut self,
        pane: u64,
        notification: &crate::notify::Notification,
        visible: bool,
        cx: &App,
    ) {
        let visible = visible
            && !self.task_inbox.open
            && !self.command_palette_open
            && !self.command_manager_open
            && !self.usage_panel_open()
            && !self.turn_review_visible();
        let Some(tab) = self.tab_of_pane(pane) else { return };
        let project = self.tab_meta.get(tab).and_then(|meta| meta.project_root.clone());
        let (title, body) = notification.raw_toast_text();
        let source = self.tab_title(tab, cx);
        let title: String = format!("{source} · {title}").chars().take(200).collect();
        let body: String = body.chars().take(800).collect();
        let at = chrono::Local::now();
        let failure = notification.is_failure();
        // Coalesce retry cards without changing notification delivery or agent state.
        if failure
            && let Some(last) = self.task_inbox.entries.iter_mut().find(|entry| entry.pane == pane)
            && last.failure
            && last.title == title
            && last.body == body
            && (at - last.at).num_seconds() < 30
        {
            last.at = at;
            last.read = visible;
            return;
        }
        self.task_inbox.next_id = self.task_inbox.next_id.wrapping_add(1);
        self.task_inbox.entries.push_front(Entry {
            id: self.task_inbox.next_id,
            pane,
            project,
            title,
            body,
            at,
            failure,
            read: visible,
        });
        self.task_inbox.entries.truncate(256);
    }

    pub(super) fn mark_visible_task_read(&mut self, window: &Window, cx: &App) {
        if !window.is_window_active()
            || self.window_hidden
            || self.settings_open
            || self.command_palette_open
            || self.command_manager_open
            || self.task_inbox.open
            || self.usage_panel_open()
            || self.turn_review_visible()
        {
            return;
        }
        let Some(WorkspaceTab::Terminal { focused, panes, .. }) = self.tabs.get(self.active) else {
            return;
        };
        if panes
            .iter()
            .find(|pane| pane.id == *focused)
            .is_some_and(|pane| pane.view.read(cx).is_reading_answer())
        {
            return;
        }
        for entry in &mut self.task_inbox.entries {
            if entry.pane == *focused {
                entry.read = true;
            }
        }
    }

    fn task_rows(&self, cx: &App) -> Vec<Row> {
        let language = crate::gpui_shell::config::ui_language(cx);
        let scoped = |project: Option<&PathBuf>| {
            self.task_inbox.all_projects || self.projects.filter.matches(project)
        };
        let mut rows = Vec::new();
        if matches!(self.task_inbox.filter, Filter::Attention | Filter::Running | Filter::Errors) {
            for (index, tab) in self.tabs.iter().enumerate() {
                let project = self.tab_meta.get(index).and_then(|meta| meta.project_root.as_ref());
                if !scoped(project) {
                    continue;
                }
                let WorkspaceTab::Terminal { panes, .. } = tab else { continue };
                for pane in panes {
                    let view = pane.view.read(cx);
                    let state = view.sidebar_activity();
                    let label = match (self.task_inbox.filter, state) {
                        (
                            Filter::Attention,
                            SidebarActivity::Attention | SidebarActivity::WaitingInput,
                        ) => Message::TasksAttention,
                        (Filter::Running, SidebarActivity::Running | SidebarActivity::Paused) => {
                            Message::TasksRunning
                        },
                        (
                            Filter::Errors,
                            SidebarActivity::CommandFailed | SidebarActivity::Failed,
                        ) => Message::TasksErrors,
                        _ => continue,
                    };
                    rows.push(Row {
                        key: format!("task-live-{}", pane.id).into(),
                        notification: None,
                        pane: pane.id,
                        title: pane.custom_name.clone().unwrap_or_else(|| {
                            let title = self.tab_title(index, cx);
                            if panes.len() > 1 {
                                format!("{title} · #{}", pane.id)
                            } else {
                                title.to_string()
                            }
                        }),
                        body: language.text(label).to_owned(),
                        detail: project.map(|p| p.display().to_string()).unwrap_or_default(),
                        unread: false,
                        open: true,
                    });
                }
            }
            if self.task_inbox.filter != Filter::Errors {
                return rows;
            }
        }
        let failed_panes: Vec<_> = rows.iter().map(|row| row.pane).collect();
        rows.extend(self.task_inbox.entries.iter().filter_map(|entry| {
            let tab = self.tab_of_pane(entry.pane);
            let project = entry.project.as_ref();
            if !scoped(project)
                || (self.task_inbox.filter == Filter::Unread && entry.read)
                || (self.task_inbox.filter == Filter::Errors && !entry.failure)
                || (self.task_inbox.filter == Filter::Errors && failed_panes.contains(&entry.pane))
            {
                return None;
            }
            let status = if tab.is_none() { language.text(Message::TasksClosed) } else { "" };
            Some(Row {
                key: format!("task-notification-{}", entry.id).into(),
                notification: Some(entry.id),
                pane: entry.pane,
                title: entry.title.clone(),
                body: entry.body.clone(),
                detail: format!(
                    "{} · {} {status}",
                    entry.at.format("%m-%d %H:%M"),
                    project.map(|p| p.display().to_string()).unwrap_or_default()
                ),
                unread: !entry.read,
                open: tab.is_some(),
            })
        }));
        rows
    }

    fn dismiss_task_inbox(&mut self) {
        self.task_inbox.open = false;
        self.task_inbox.focus = None;
        self.task_inbox.focus_subscription = None;
    }

    fn close_task_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_task_inbox();
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_task_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.task_inbox.open {
            self.close_task_inbox(window, cx);
            return;
        }
        self.dismiss_usage();
        self.dismiss_palette_state();
        self.command_manager_open = false;
        self.task_inbox.open = true;
        let focus = cx.focus_handle();
        self.task_inbox.focus_subscription =
            Some(cx.on_focus_out(&focus, window, |this, _, window, cx| {
                if this
                    .task_inbox
                    .focus
                    .as_ref()
                    .is_some_and(|focus| !focus.contains_focused(window, cx))
                {
                    this.dismiss_task_inbox();
                    cx.notify();
                }
            }));
        focus.focus(window, cx);
        self.task_inbox.focus = Some(focus);
        cx.notify();
    }

    pub(super) fn render_task_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let unread = self.task_inbox.entries.iter().filter(|entry| !entry.read).count();
        h_flex()
            .gap_0()
            .child(
                Button::new("workspace-tasks")
                    .ghost()
                    .small()
                    .icon(IconName::Bell)
                    .selected(self.task_inbox.open)
                    .aria_label(language.text(Message::TasksTitle))
                    .tooltip(language.text(Message::TasksHint))
                    .when(unread > 0, |button| {
                        button.label(if unread > 99 { "99+".into() } else { unread.to_string() })
                    })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.toggle_task_inbox(window, cx)),
                    ),
            )
            .when(unread > 0, |row| {
                row.child(
                    Button::new("workspace-next-unread")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronRight)
                        .tooltip(language.text(Message::TasksNextUnread))
                        .disabled(
                            !self
                                .task_inbox
                                .entries
                                .iter()
                                .any(|entry| !entry.read && self.tab_of_pane(entry.pane).is_some()),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            let pane = this
                                .task_inbox
                                .entries
                                .iter()
                                .find(|entry| !entry.read && this.tab_of_pane(entry.pane).is_some())
                                .map(|entry| entry.pane);
                            if let Some(pane) = pane
                                && let Some(tab) = this.tab_of_pane(pane)
                            {
                                this.dismiss_task_inbox();
                                this.reveal_session_pane(tab, pane, window, cx);
                            }
                        })),
                )
            })
    }

    pub(super) fn render_task_inbox(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.task_inbox.open {
            return None;
        }
        let focus = self.task_inbox.focus.as_ref()?;
        let language = crate::gpui_shell::config::ui_language(cx);
        let rows = self.task_rows(cx);
        let has_unread = rows.iter().any(|row| row.unread);
        let ids: Vec<_> = rows.iter().filter_map(|row| row.notification).collect();
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let background = cx.theme().popover;
        let width = (f32::from(window.viewport_size().width) - 32.0).min(540.0).max(0.0);
        let height = (f32::from(window.viewport_size().height) - 80.0).max(0.0);
        let filters = [
            (Filter::Unread, Message::TasksUnread),
            (Filter::All, Message::TasksAll),
            (Filter::Attention, Message::TasksAttention),
            (Filter::Running, Message::TasksRunning),
            (Filter::Errors, Message::TasksErrors),
        ];
        let mut list = v_flex().gap_2();
        if rows.is_empty() {
            list = list.child(
                div().p_3().text_sm().text_color(muted).child(language.text(Message::TasksEmpty)),
            );
        }
        for row in rows {
            let pane = row.pane;
            let hint = row.detail.clone();
            list = list.child(
                div()
                    .id(row.key)
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .when(row.unread, |card| card.bg(cx.theme().accent.opacity(0.08)))
                    .child(
                        h_flex().gap_2().child(
                            Button::new(("task-open", pane))
                                .ghost()
                                .small()
                                .flex_1()
                                .min_w_0()
                                .label(row.title)
                                .disabled(!row.open)
                                .tooltip(language.text(if row.open {
                                    Message::TasksOpen
                                } else {
                                    Message::TasksClosed
                                }))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(tab) = this.tab_of_pane(pane) {
                                        this.dismiss_task_inbox();
                                        this.reveal_session_pane(tab, pane, window, cx);
                                        for entry in &mut this.task_inbox.entries {
                                            if entry.pane == pane {
                                                entry.read = true;
                                            }
                                        }
                                        cx.notify();
                                    }
                                })),
                        ),
                    )
                    .child(div().text_sm().whitespace_normal().child(row.body))
                    .child(
                        div()
                            .id(("task-detail", pane))
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                            .child(row.detail),
                    ),
            );
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .track_focus(focus)
                .key_context("PebrelTasks")
                .on_action(cx.listener(|this, _: &CloseActiveTerminal, window, cx| {
                    this.close_task_inbox(window, cx);
                    cx.stop_propagation();
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.close_task_inbox(window, cx);
                        cx.stop_propagation();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_task_inbox(window, cx)),
                )
                .child(
                    v_flex()
                        .absolute()
                        .top(px(48.0))
                        .right(px(16.0))
                        .w(px(width))
                        .max_h(px(height))
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .bg(background)
                        .shadow_lg()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(language.text(Message::TasksTitle)),
                                )
                                .child(
                                    Button::new("tasks-mark-read")
                                        .ghost()
                                        .xsmall()
                                        .label(language.text(Message::TasksMarkRead))
                                        .disabled(!has_unread)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            for entry in &mut this.task_inbox.entries {
                                                if ids.contains(&entry.id) {
                                                    entry.read = true;
                                                }
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("tasks-close")
                                        .ghost()
                                        .small()
                                        .icon(IconName::Close)
                                        .aria_label(language.text(Message::CommonClose))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.close_task_inbox(window, cx)
                                        })),
                                ),
                        )
                        .child(h_flex().px_3().pb_2().gap_1().flex_wrap().children(
                            filters.into_iter().enumerate().map(|(index, (filter, label))| {
                                Button::new(("task-filter", index))
                                    .ghost()
                                    .xsmall()
                                    .selected(self.task_inbox.filter == filter)
                                    .label(language.text(label))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.task_inbox.filter = filter;
                                        cx.notify();
                                    }))
                            }),
                        ))
                        .child(
                            Button::new("tasks-project-scope")
                                .ghost()
                                .xsmall()
                                .selected(self.task_inbox.all_projects)
                                .label(language.text(Message::TasksAllProjects))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.task_inbox.all_projects = !this.task_inbox.all_projects;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("task-inbox-scroll")
                                .p_3()
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scrollbar()
                                .child(list),
                        ),
                )
                .into_any_element(),
        )
    }
}
