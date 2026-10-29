//! Local-record snapshots and a flyout observing the shared official quota report.
//! Project filtering never changes billing scope.

use super::*;
use crate::ai_usage::{AppUsage, Issue, Period, QuotaWindow, SessionMetrics};
use crate::gpui_shell::usage_format::{issue_text, window_duration_label};
use crate::i18n::Message;

const METRICS_INTERVAL: u64 = 5;
const STATS_INTERVAL: u64 = 5;
const QUOTA_INTERVAL: u64 = crate::gpui_shell::codex_quota::INTERVAL;

fn auto_refresh_text(language: crate::display::UiLanguage) -> String {
    language.format(
        Message::UsageAutoRefresh,
        &[
            ("metrics", &METRICS_INTERVAL.to_string()),
            ("stats", &STATS_INTERVAL.to_string()),
            ("quota", &QUOTA_INTERVAL.to_string()),
        ],
    )
}

#[derive(Default)]
pub(super) struct UsageState {
    open: bool,
    period: Period,
    quota: Option<Entity<crate::gpui_shell::codex_quota::Snapshot>>,
    quota_subscription: Option<Subscription>,
    local: Option<Result<Vec<AppUsage>, Issue>>,
    local_task: Option<gpui::Task<()>>,
    metrics: Option<Result<Vec<SessionMetrics>, Issue>>,
    local_partial: bool,
    polling: Option<gpui::Task<()>>,
    stats_attempt: Option<std::time::Instant>,
    local_checked_at: Option<chrono::DateTime<chrono::Local>>,
    focus: Option<gpui::FocusHandle>,
    focus_subscription: Option<Subscription>,
    scroll: gpui::ScrollHandle,
}

/// Settings borrows the existing workspace snapshot; it never starts a second scanner.
pub(in crate::gpui_shell) struct LocalUsageView<'a> {
    pub period: Period,
    pub local: Option<&'a Result<Vec<AppUsage>, Issue>>,
    pub partial: bool,
    pub busy: bool,
    pub checked_at: Option<&'a chrono::DateTime<chrono::Local>>,
}

fn window_label(window: &QuotaWindow, language: crate::display::UiLanguage) -> String {
    window_duration_label(&window.id, window.seconds, language)
}

impl NebulaWorkspace {
    pub(in crate::gpui_shell) fn local_usage(&self) -> LocalUsageView<'_> {
        LocalUsageView {
            period: self.usage.period,
            local: self.usage.local.as_ref(),
            partial: self.usage.local_partial,
            busy: self.usage.local_task.is_some(),
            checked_at: self.usage.local_checked_at.as_ref(),
        }
    }

    pub(super) fn set_usage_period(&mut self, period: Period, cx: &mut Context<Self>) {
        if self.usage.period != period && self.usage.local_task.is_none() {
            self.usage.period = period;
            self.usage.local = None;
            self.usage.local_checked_at = None;
            self.refresh_local_usage(cx);
        }
    }
    pub(super) fn start_usage_polling(&mut self, cx: &mut Context<Self>) {
        if self.usage.polling.is_some() {
            return;
        }
        let quota = crate::gpui_shell::codex_quota::shared(cx);
        self.usage.quota_subscription = Some(cx.observe(&quota, |_, _, cx| cx.notify()));
        self.usage.quota = Some(quota);
        self.refresh_usage(cx);
        self.usage.polling = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(METRICS_INTERVAL)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.window_hidden {
                            return;
                        }
                        let now = cx.background_executor().now();
                        if let Some(quota) = &this.usage.quota {
                            quota.update(cx, |quota, cx| quota.refresh(false, cx));
                        }
                        if this.usage.stats_attempt.is_none_or(|last| {
                            now.saturating_duration_since(last).as_secs() >= STATS_INTERVAL
                        }) {
                            this.refresh_local_usage(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(super) fn refresh_local_usage(&mut self, cx: &mut Context<Self>) {
        if self.usage.local_task.is_some() {
            return;
        }
        let mut sessions = std::collections::BTreeSet::new();
        for tab in &self.tabs {
            if let WorkspaceTab::Terminal { panes, .. } = tab {
                for pane in panes {
                    let view = pane.view.read(cx);
                    if view.ssh_destination.is_some() || view.saved_agent_pending() {
                        continue;
                    }
                    if let Some(agent) = view.session_agent()
                        && let Some(id) = agent.session_id
                        && matches!(
                            agent.source.as_str(),
                            "codex" | "claude" | "gemini" | "opencode"
                        )
                    {
                        sessions.insert((agent.source, id));
                    }
                }
            }
        }
        let read_records = self.usage.open || self.settings_open || !sessions.is_empty();
        let period = self.usage.period;
        self.usage.stats_attempt = Some(cx.background_executor().now());
        let worker = cx.background_executor().spawn(async move {
            let result = read_records
                .then(|| crate::ai_usage::read_native(period, sessions.into_iter().collect()));
            let observed = std::time::Instant::now();
            (result, crate::ai_usage::auth_revision(), observed)
        });
        self.usage.local_task = Some(cx.spawn(async move |this, cx| {
            let (result, revision, observed) = worker.await;
            let _ = this.update(cx, |this, cx| {
                this.usage.local_task = None;
                match result {
                    Some(Ok(report)) => {
                        this.usage.local_partial = report.partial;
                        this.usage.local = Some(Ok(report.apps));
                        this.usage.metrics = Some(Ok(report.metrics));
                    },
                    Some(Err(issue)) => {
                        this.usage.local = Some(Err(issue));
                        this.usage.metrics = None;
                    },
                    None => {},
                }
                if read_records {
                    this.usage.local_checked_at = Some(chrono::Local::now());
                }
                // Account switches must still invalidate quota while no records are displayed.
                if let Some(quota) = &this.usage.quota {
                    quota.update(cx, |quota, cx| quota.observe_auth(revision, observed, cx));
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn usage_summary(&self, language: crate::display::UiLanguage, cx: &App) -> Vec<String> {
        let mut lines = Vec::new();
        let quota = self.usage.quota.as_ref().map(|quota| quota.read(cx));
        if let Some(Ok(report)) = quota.and_then(|quota| quota.report.as_ref()) {
            let windows = &report.windows;
            if windows.is_empty() {
                lines.push(language.text(Message::UsageNoLimitedWindows).to_owned());
            }
            if let Some(limit) =
                windows.iter().max_by_key(|limit| limit.seconds.unwrap_or_default())
            {
                lines.push(if limit.resets_at.is_some_and(|reset| reset <= chrono::Local::now()) {
                    language.text(Message::UsageResetPending).to_owned()
                } else {
                    language.format(
                        Message::UsageUsed,
                        &[
                            ("window", &window_label(limit, language)),
                            ("percent", &format!("{:.0}", 100.0 - limit.remaining_percent)),
                        ],
                    )
                });
            }
        }
        if lines.is_empty() {
            lines.push(
                language
                    .text(if quota.is_some_and(|quota| quota.task.is_some()) {
                        Message::UsageLoading
                    } else {
                        Message::UsageQuotaUnavailable
                    })
                    .to_owned(),
            );
        }
        if quota.is_some_and(|quota| quota.failure.is_some()) {
            if let Some(line) = lines.first_mut() {
                line.push_str(&format!(" · {}", language.text(Message::UsageStale)));
            }
        }
        lines
    }

    fn usage_hint(&self, language: crate::display::UiLanguage, cx: &App) -> String {
        let mut parts = vec![
            language.text(Message::UsageTitle).to_owned(),
            auto_refresh_text(language),
            language.text(Message::UsageSource).to_owned(),
        ];
        let quota = self.usage.quota.as_ref().map(|quota| quota.read(cx));
        if let Some(Err(issue)) = quota.and_then(|quota| quota.report.as_ref()) {
            parts.push(issue_text(issue, language));
        }
        if let Some(issue) = quota.and_then(|quota| quota.failure.as_ref()) {
            parts.push(
                language
                    .format(Message::UsageStaleDetail, &[("error", &issue_text(issue, language))]),
            );
        }
        if let Some(Err(issue)) = &self.usage.local {
            parts.push(issue_text(issue, language));
        }
        for (source, time) in [
            ("Codex", quota.and_then(|quota| quota.checked_at.as_ref())),
            (language.text(Message::UsageLocalTitle), self.usage.local_checked_at.as_ref()),
        ] {
            if let Some(time) = time {
                parts.push(language.format(
                    Message::UsageUpdated,
                    &[("source", source), ("time", &time.format("%H:%M:%S").to_string())],
                ));
            }
        }
        parts.join("\n")
    }

    pub(super) fn render_session_metrics(
        &self,
        view: &Entity<TerminalView>,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if view.read(cx).saved_agent_pending() || view.read(cx).can_choose_recovery_session() {
            return self.render_saved_agent(view, cx);
        }
        let shortcuts = self.render_agent_shortcuts(view, cx);
        let view = view.read(cx);
        if view.ssh_destination.is_some() {
            return None;
        }
        let agent = view.session_agent()?;
        if !matches!(agent.source.as_str(), "codex" | "claude" | "gemini" | "opencode") {
            return None;
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let theme = cx.theme();
        let metric =
            self.usage.metrics.as_ref().and_then(|result| result.as_ref().ok()).and_then(|rows| {
                rows.iter().find(|row| {
                    row.app == agent.source && Some(&row.session_id) == agent.session_id.as_ref()
                })
            });
        let mut hint = language
            .format(Message::UsageMetricsSource, &[("seconds", &METRICS_INTERVAL.to_string())]);
        if let Some(metric) = metric {
            let time = chrono::DateTime::from_timestamp(metric.created_at, 0)
                .map(|time| time.with_timezone(&chrono::Local).format("%m-%d %H:%M:%S").to_string())
                .unwrap_or_default();
            hint.push_str(&format!(
                "\n{}",
                language.format(
                    Message::UsageMetricsDetail,
                    &[
                        ("model", &metric.model),
                        ("time", &time),
                        ("tokens", &metric.output_tokens.to_string())
                    ]
                )
            ));
        } else if let Some(Err(issue)) = &self.usage.metrics {
            hint.push_str(&format!("\n{}", issue_text(issue, language)));
        }
        let speed = metric
            .and_then(|m| m.tokens_per_second)
            .map(|speed| {
                format!(
                    "{}{speed:.1} tok/s",
                    if metric.is_some_and(|m| m.estimated) { "≈ " } else { "" }
                )
            })
            .unwrap_or_else(|| {
                language
                    .text(
                        if matches!(
                            view.runtime_task_state(),
                            crate::runtime_api::RuntimeTaskState::Running
                        ) {
                            Message::UsageMetricsSampling
                        } else {
                            Message::UsageMetricsUnavailable
                        },
                    )
                    .to_owned()
            });
        let first = metric
            .and_then(|m| m.first_token_ms)
            .map(|ms| format!("{:.2}s", ms as f64 / 1000.0))
            .unwrap_or_else(|| "—".to_owned());
        let name = crate::ai_agents::AgentKind::parse(&agent.source)
            .map_or(agent.source.as_str(), |kind| kind.display_name());
        let details_hint = hint.clone();
        Some(
            h_flex()
                .id(SharedString::from(format!("agent-metrics-{}", view.pane_id)))
                .h(px(30.0))
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .gap_2()
                .px_2()
                .items_center()
                .border_t_1()
                .border_color(theme.border)
                .bg(theme.background)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .id(SharedString::from(format!("agent-metrics-label-{}", view.pane_id)))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                        .child(language.format(
                            if metric.is_some_and(|metric| metric.in_progress)
                                && matches!(
                                    view.runtime_task_state(),
                                    crate::runtime_api::RuntimeTaskState::Running
                                )
                            {
                                Message::UsageMetricsLive
                            } else {
                                Message::UsageMetricsLatest
                            },
                            &[("agent", name)],
                        )),
                )
                .children(shortcuts)
                .child(
                    h_flex()
                        .id(SharedString::from(format!("agent-metrics-details-{}", view.pane_id)))
                        .min_w_0()
                        .gap_2()
                        .tooltip(move |window, cx| {
                            Tooltip::new(details_hint.clone()).build(window, cx)
                        })
                        .child(div().min_w_0().truncate().child(speed))
                        .child(
                            div().min_w_0().truncate().child(
                                language.format(Message::UsageFirstToken, &[("time", &first)]),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn usage_panel_open(&self) -> bool {
        self.usage.open
    }

    pub(super) fn dismiss_usage(&mut self) {
        self.usage.open = false;
        self.usage.focus = None;
        self.usage.focus_subscription = None;
    }

    fn close_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_usage();
        self.focus_active(window, cx);
        cx.notify();
    }

    fn toggle_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.usage.open {
            self.close_usage(window, cx);
            return;
        }
        self.dismiss_palette_state();
        self.command_manager_open = false;
        self.usage.open = true;
        let focus = cx.focus_handle();
        self.usage.focus_subscription =
            Some(cx.on_focus_out(&focus, window, |this, _, window, cx| {
                if this
                    .usage
                    .focus
                    .as_ref()
                    .is_some_and(|focus| !focus.contains_focused(window, cx))
                {
                    this.dismiss_usage();
                    cx.notify();
                }
            }));
        focus.focus(window, cx);
        self.usage.focus = Some(focus);
        self.refresh_usage(cx);
    }

    fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        if let Some(quota) = &self.usage.quota {
            quota.update(cx, |quota, cx| quota.refresh(false, cx));
        }
        self.refresh_local_usage(cx);
    }

    pub(super) fn refresh_codex_usage(&mut self, cx: &mut Context<Self>) {
        if let Some(quota) = &self.usage.quota {
            quota.update(cx, |quota, cx| quota.refresh(true, cx));
        }
    }

    pub(super) fn render_usage_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let line = self.usage_summary(language, cx).pop().unwrap_or_default();
        h_flex()
            .w_full()
            .min_w_0()
            .h(px(48.0))
            .flex_shrink_0()
            .gap_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("workspace-usage")
                    .debug_selector(|| "workspace-usage".into())
                    .ghost()
                    .small()
                    .flex_1()
                    .min_w_0()
                    .icon(Icon::new(Icon::empty()).path("icons/nebula-agent-openai.svg"))
                    .h(px(32.0))
                    .aria_label(language.text(Message::UsageTitle))
                    .tooltip(SharedString::from(self.usage_hint(language, cx)))
                    .child(div().flex_1().min_w_0().truncate().text_xs().text_left().child(line))
                    .selected(self.usage.open)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_usage(window, cx))),
            )
            .child(
                Button::new("workspace-usage-settings")
                    .ghost()
                    .small()
                    .icon(IconName::Settings)
                    .tooltip(language.text(Message::CommonSettings))
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
            )
    }

    pub(super) fn render_usage_toolbar_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        Button::new("workspace-usage-toolbar")
            .ghost()
            .small()
            .min_w_0()
            .icon(IconName::Bot)
            .selected(self.usage.open)
            .aria_label(language.text(Message::UsageTitle))
            .tooltip(SharedString::from(self.usage_hint(language, cx)))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .child(self.usage_summary(language, cx).join(" · ")),
            )
            .on_click(cx.listener(|this, _, window, cx| this.toggle_usage(window, cx)))
    }

    pub(super) fn render_usage_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.usage.open {
            return None;
        }
        let focus = self.usage.focus.as_ref()?;
        let language = crate::gpui_shell::config::ui_language(cx);
        let theme = cx.theme();
        let background = theme.popover;
        let muted = theme.muted_foreground;
        let border = theme.border;
        let quota = self.usage.quota.as_ref().map(|quota| quota.read(cx));
        let busy = quota.is_some_and(|quota| quota.task.is_some());
        let title_height = window_titlebar::effective_title_bar_height(
            self.density,
            crate::platform::window_chrome::layout(window),
        );
        let available_height =
            (f32::from(window.viewport_size().height) - title_height - 60.0).max(0.0);
        let width = (f32::from(window.viewport_size().width) - 24.0).clamp(0.0, 300.0);
        let plan = quota
            .and_then(|quota| quota.report.as_ref())
            .and_then(|report| report.as_ref().ok())
            .and_then(|report| report.plan.as_deref());
        let plan = plan.map(|plan| match plan {
            "plus" => "Plus",
            "pro" => "Pro",
            "free" => "Free",
            "team" => "Team",
            "business" => "Business",
            "enterprise" => "Enterprise",
            _ => plan,
        });
        let title = match plan {
            Some(plan) => format!("Codex · ChatGPT {plan}"),
            None => "Codex · ChatGPT".to_owned(),
        };
        let mut body = v_flex().w_full().min_w_0().gap(px(24.0));
        body = body.child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    Button::new("usage-refresh")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Redo)
                        .tooltip(language.text(Message::UsageRefresh))
                        .disabled(busy)
                        .loading(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_codex_usage(cx))),
                )
                .child(
                    Button::new("usage-close")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .aria_label(language.text(Message::CommonClose))
                        .on_click(cx.listener(|this, _, window, cx| this.close_usage(window, cx))),
                ),
        );
        if let Some(result) = quota.and_then(|quota| quota.report.as_ref()) {
            match result {
                Ok(report) => {
                    let windows = &report.windows;
                    if windows.is_empty() {
                        body = body.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(language.text(Message::UsageNoLimitedWindows)),
                        );
                    }
                    for (index, limit) in windows.iter().enumerate() {
                        body = body
                            .child(crate::gpui_shell::usage_view::quota_window(limit, index, cx));
                    }
                },
                Err(issue) => {
                    body = body
                        .child(div().text_sm().text_color(muted).child(issue_text(issue, language)))
                },
            }
        } else {
            body = body.child(
                div().text_sm().text_color(muted).child(language.text(Message::UsageLoading)),
            );
        }
        if let Some(issue) = quota.and_then(|quota| quota.failure.as_ref()) {
            body =
                body.child(div().text_xs().whitespace_normal().text_color(theme.warning).child(
                    language.format(
                        Message::UsageStaleDetail,
                        &[("error", &issue_text(issue, language))],
                    ),
                ));
        }
        let mut updated = div().flex_1().min_w_0().text_xs().text_color(muted);
        if let Some(time) = quota.and_then(|quota| quota.checked_at.as_ref()) {
            updated = updated.child(div().child(language.format(
                Message::UsagePeekUpdated,
                &[("time", &time.format("%H:%M").to_string())],
            )));
        }
        body = body.child(
            h_flex()
                .pt_2()
                .gap_2()
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(border)
                .child(updated)
                .child(
                    Button::new("usage-details")
                        .ghost()
                        .small()
                        .text_color(theme.link)
                        .label(language.text(Message::UsageDetails))
                        .icon(IconName::ArrowRight)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_usage(window, cx);
                            this.open_settings(window, cx);
                            if let Some((view, _)) = this.settings_surface.as_ref() {
                                view.update(cx, |view, cx| view.show_codex_usage(window, cx));
                            }
                        })),
                ),
        );
        Some(
            div()
                .id("workspace-usage-panel")
                .debug_selector(|| "workspace-usage-panel".into())
                .role(gpui::Role::Dialog)
                .aria_label(language.text(Message::UsageTitle))
                .absolute()
                .inset_0()
                .occlude()
                .track_focus(focus)
                .key_context("PebrelUsage")
                .on_action(cx.listener(|this, _: &CloseActiveTerminal, window, cx| {
                    this.close_usage(window, cx);
                    cx.stop_propagation();
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.close_usage(window, cx);
                        cx.stop_propagation();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_usage(window, cx)),
                )
                .child(
                    v_flex()
                        .id("usage-content")
                        .debug_selector(|| "usage-content".into())
                        .absolute()
                        .left(px(12.0))
                        .bottom(px(56.0))
                        .w(px(width))
                        .max_h(px(available_height.min(520.0)))
                        .min_h_0()
                        .bg(background)
                        .border_1()
                        .border_color(border)
                        .rounded_lg()
                        .shadow_lg()
                        .overflow_y_scroll()
                        .track_scroll(&self.usage.scroll)
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        // Keep intrinsic height in flow; percentage-height wrappers collapse this card.
                        .child(
                            div()
                                .id("usage-body")
                                .flex_shrink_0()
                                .p(px(16.0))
                                .child(body),
                        )
                        .vertical_scrollbar(&self.usage.scroll),
                )
                .into_any_element(),
        )
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, point};

    #[gpui::test]
    fn loaded_and_error_flyouts_fit_and_close_through_real_input(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::gpui_shell::math_view::register(cx);
            crate::gpui_shell::file_editor::init(cx);
            super::super::init(cx);
            windowing::initialize(cx, crate::runtime_api::RuntimeHub::new());
            cx.set_reduce_motion(true);
        });
        let mut owner = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| {
                NebulaWorkspace::new(
                    window,
                    None,
                    None,
                    1,
                    crate::runtime_api::RuntimeHub::new(),
                    windowing::WorkspaceStartup::Empty,
                    windowing::WindowRole::Regular,
                    cx,
                )
            });
            owner = Some(workspace.clone());
            Root::new(workspace, window, cx)
        });
        let owner = owner.unwrap();
        for error in [false, true] {
            cx.update(|window, cx| {
                owner.update(cx, |this, cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    this.usage.open = true;
                    this.usage.focus = Some(focus);
                    let quota = crate::gpui_shell::codex_quota::shared(cx);
                    this.usage.quota = Some(quota.clone());
                    quota.update(cx, |quota, cx| {
                        quota.invalidate(cx);
                        quota.report = Some(if error {
                            Err(Issue::Network)
                        } else {
                            Ok(crate::ai_usage::CodexReport {
                                plan: Some("Plus".into()),
                                windows: vec![QuotaWindow {
                                    id: "primary".into(),
                                    seconds: Some(18000),
                                    remaining_percent: 75.0,
                                    resets_at: None,
                                }],
                                capacity: Err(Issue::MissingAuth),
                            })
                        });
                        cx.notify();
                    });
                    this.usage.local = Some(if error {
                        Err(Issue::MissingDatabase)
                    } else {
                        Ok(["codex", "claude", "gemini", "opencode"]
                            .into_iter()
                            .map(|app| AppUsage { app: app.into(), requests: 1, tokens: 100 })
                            .collect())
                    });
                    cx.notify();
                });
            });
            cx.run_until_parked();
            let panel = cx.debug_bounds("usage-content").expect("flyout content");
            let overlay = cx.debug_bounds("workspace-usage-panel").expect("flyout overlay");
            assert!(panel.right() <= overlay.right());
            assert!(panel.bottom() <= overlay.bottom());
            cx.simulate_click(panel.origin + point(px(12.0), px(12.0)), Modifiers::default());
            assert!(
                owner.read_with(cx, |this, _| this.usage.open),
                "panel={panel:?}, overlay={overlay:?}"
            );
            if error {
                cx.simulate_keystrokes("escape");
            } else {
                cx.simulate_click(overlay.origin + point(px(2.0), px(2.0)), Modifiers::default());
            }
            assert!(!owner.read_with(cx, |this, _| this.usage.open));
        }
    }
}
