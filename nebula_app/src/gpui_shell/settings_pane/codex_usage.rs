use super::*;
use crate::ai_usage::{
    Issue,
    codex_entitlements::{self, Outcome, Status},
};
use crate::gpui_shell::usage_format::{issue_text, window_duration_label};
use crate::i18n::Message;
use gpui::FontWeight;

mod capacity;
mod entitlements;
mod local_records;

fn details_button(id: &'static str, label: &'static str, expanded: bool) -> Button {
    Button::new(id).ghost().small().w_full().min_w_0().px_0().aria_label(label).child(
        h_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(div().flex_1().min_w_0().text_left().truncate().child(label))
            .child(
                Icon::new(if expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                    .xsmall(),
            ),
    )
}

pub(super) struct State {
    info: Option<Result<Status, Issue>>,
    quota: Entity<crate::gpui_shell::codex_quota::Snapshot>,
    _quota_subscription: Subscription,
    account_revision: Option<Option<crate::ai_usage::AuthRevision>>,
    capacity_details: bool,
    credits_details: bool,
    local_details: bool,
    usage_owner: Option<gpui::WeakEntity<crate::gpui_shell::workspace::NebulaWorkspace>>,
    usage_subscription: Option<Subscription>,
    reading: Option<Task<()>>,
    consuming: Option<Task<()>>,
    outcome: Option<Result<Outcome, Issue>>,
    generation: u64,
    checked_at: Option<std::time::Instant>,
}

impl State {
    pub(super) fn new(cx: &mut Context<SettingsPane>) -> Self {
        let quota = crate::gpui_shell::codex_quota::shared(cx);
        let account_revision = quota.read(cx).revision;
        let subscription = cx.observe(&quota, |this, _, cx| {
            this.sync_codex_account(cx);
            cx.notify();
        });
        Self {
            info: None,
            quota,
            _quota_subscription: subscription,
            account_revision,
            capacity_details: false,
            credits_details: false,
            local_details: false,
            usage_owner: None,
            usage_subscription: None,
            reading: None,
            consuming: None,
            outcome: None,
            generation: 0,
            checked_at: None,
        }
    }
}

impl SettingsPane {
    fn sync_codex_account(&mut self, cx: &mut Context<Self>) {
        let revision = self.codex_usage.quota.read(cx).revision;
        if self.codex_usage.account_revision.is_some()
            && self.codex_usage.account_revision != revision
        {
            self.invalidate_codex_entitlements(cx);
        }
        self.codex_usage.account_revision = revision;
    }

    pub(in crate::gpui_shell) fn show_codex_usage(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_search_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.search_origin_section = None;
        self.active_section = USAGE_SECTION;
        self.about_sponsor_open = false;
        self.font_picker_open = false;
        self.bg_picker_open = false;
        self.refresh_codex_entitlements(cx);
        cx.notify();
    }

    pub(super) fn section_codex_usage(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        if self.codex_usage.checked_at.is_none_or(|last| {
            cx.background_executor().now().saturating_duration_since(last).as_secs() >= 60
        }) {
            self.refresh_codex_entitlements(cx);
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.codex_entitlements_busy(cx);
        v_flex()
            .w_full()
            .max_w(px(720.0))
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(language.text(Message::UsageTitle)),
                    )
                    .child(
                        Button::new("codex-usage-refresh")
                            .ghost()
                            .small()
                            .icon(IconName::Redo)
                            .label(language.text(Message::UsageRefresh))
                            .disabled(busy)
                            .loading(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.codex_usage
                                    .quota
                                    .update(cx, |quota, cx| quota.refresh(true, cx));
                                this.refresh_codex_entitlements(cx);
                            })),
                    ),
            )
            .child(self.render_codex_account(cx))
            .child(self.render_codex_windows(cx))
            .child(self.render_codex_capacity(cx))
            .child(self.render_codex_entitlements(cx))
            .child(self.render_local_records(cx))
    }

    fn render_codex_account(&self, cx: &App) -> gpui::AnyElement {
        let mut account = v_flex()
            .gap_1()
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Codex"));
        if let Some(Ok(info)) = &self.codex_usage.info {
            let label = format!("{} · {}", info.label, info.plan);
            let hint = label.clone();
            account = account.child(
                div()
                    .id("codex-usage-account")
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .child(label),
            );
        }
        account.into_any_element()
    }

    fn render_codex_windows(&self, cx: &App) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let mut section =
            v_flex().gap_3().pt_3().border_t_1().border_color(cx.theme().border).child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.text(Message::UsageOfficialTitle)),
            );
        let quota = self.codex_usage.quota.read(cx);
        match &quota.report {
            Some(Ok(report)) => {
                if report.windows.is_empty() {
                    section = section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(language.text(Message::UsageNoLimitedWindows)),
                    );
                }
                for (index, limit) in report.windows.iter().enumerate() {
                    section = section
                        .child(crate::gpui_shell::usage_view::quota_window(limit, index, cx));
                }
            },
            Some(Err(issue)) => {
                section = section.child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .text_color(cx.theme().muted_foreground)
                        .child(issue_text(issue, language)),
                );
            },
            None => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(language.text(Message::UsageLoading)),
                );
            },
        }
        if let Some(time) = &quota.checked_at {
            section = section.child(div().text_xs().text_color(cx.theme().muted_foreground).child(
                language.format(
                    Message::UsageLastSuccess,
                    &[("time", &time.format("%m-%d %H:%M:%S").to_string())],
                ),
            ));
        }
        if let Some(issue) = &quota.failure {
            section = section.child(
                div().text_xs().whitespace_normal().text_color(cx.theme().warning).child(
                    language.format(
                        Message::UsageStaleDetail,
                        &[("error", &issue_text(issue, language))],
                    ),
                ),
            );
        }
        section.into_any_element()
    }
}
