use super::*;
use crate::ai_usage::codex_entitlements::{self, Outcome};

impl SettingsPane {
    pub(super) fn codex_entitlements_busy(&self, cx: &App) -> bool {
        self.codex_usage.reading.is_some()
            || self.codex_usage.consuming.is_some()
            || self.codex_usage.quota.read(cx).task.is_some()
    }

    pub(in crate::gpui_shell) fn invalidate_codex_entitlements(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.codex_usage;
        state.generation = state.generation.wrapping_add(1);
        state.info = None;
        state.reading = None;
        state.outcome = None;
        state.checked_at = None;
        // A mutation keeps its owner until the result is observed. The adapter
        // leaves a durable receipt on errors, including a change of account.
        cx.notify();
    }

    pub(super) fn refresh_codex_entitlements(&mut self, cx: &mut Context<Self>) {
        if self.codex_usage.reading.is_some() || self.codex_usage.consuming.is_some() {
            return;
        }
        self.codex_usage.quota.update(cx, |quota, cx| quota.refresh(false, cx));
        let generation = self.codex_usage.generation;
        self.codex_usage.checked_at = Some(cx.background_executor().now());
        let work = cx.background_executor().spawn(async {
            let revision = crate::ai_usage::auth_revision();
            let credits = codex_entitlements::read();
            let observed = std::time::Instant::now();
            let current = crate::ai_usage::auth_revision();
            let result = if revision == current { credits } else { Err(Issue::AuthChanged) };
            (current, observed, result)
        });
        self.codex_usage.reading = Some(cx.spawn(async move |this, cx| {
            let (revision, observed, result) = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.codex_usage.generation != generation {
                    return;
                }
                this.codex_usage.reading = None;
                let current = this
                    .codex_usage
                    .quota
                    .update(cx, |quota, cx| quota.observe_auth(revision, observed, cx));
                if !current {
                    this.invalidate_codex_entitlements(cx);
                    this.refresh_codex_entitlements(cx);
                    return;
                }
                this.sync_codex_account(cx);
                this.codex_usage.info = Some(result);
                this.codex_usage.checked_at = Some(cx.background_executor().now());
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn confirm_codex_reset(
        &mut self,
        account: String,
        credit: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.codex_entitlements_busy(cx) {
            return;
        }
        let Some(Ok(info)) = &self.codex_usage.info else { return };
        if info.account_key != account || (chrono::Local::now() - info.observed).num_seconds() > 90
        {
            self.refresh_codex_entitlements(cx);
            return;
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let expiry = credit
            .as_ref()
            .and_then(|id| info.credits.iter().find(|item| &item.id == id))
            .map(|item| {
                item.expires_at
                    .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| language.text(Message::UsageCreditNoExpiry).to_owned())
            })
            .unwrap_or_else(|| language.text(Message::UsageCreditRetry).to_owned());
        let body: SharedString = if credit.is_none() {
            language.format(
                Message::UsageCreditRetryConfirmBody,
                &[("account", &info.label), ("plan", &info.plan)],
            )
        } else {
            language.format(
                Message::UsageCreditConfirmBody,
                &[("account", &info.label), ("plan", &info.plan), ("expiry", &expiry)],
            )
        }
        .into();
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, _| {
            let owner = owner.clone();
            let account = account.clone();
            let credit = credit.clone();
            confirm_dialog(
                dialog,
                window,
                language.text(Message::UsageCreditConfirm),
                body.clone(),
                language.text(Message::UsageCreditUse),
                language.text(Message::CommonCancel),
                ButtonVariant::Danger,
            )
            .on_ok(move |_, _, cx| {
                if let Some(owner) = owner.upgrade() {
                    owner.update(cx, |this, cx| {
                        this.consume_codex_reset(account.clone(), credit.clone(), cx);
                    });
                }
                true
            })
        });
    }

    fn consume_codex_reset(
        &mut self,
        account: String,
        credit: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.codex_entitlements_busy(cx) {
            return;
        }
        let generation = self.codex_usage.generation;
        self.codex_usage.outcome = None;
        let work = cx
            .background_executor()
            .spawn(async move { codex_entitlements::consume(account, credit) });
        self.codex_usage.consuming = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.codex_usage.consuming = None;
                if this.codex_usage.generation == generation {
                    this.codex_usage.outcome = Some(result);
                }
                this.codex_usage.quota.update(cx, |quota, cx| {
                    quota.invalidate(cx);
                    quota.refresh(true, cx);
                });
                cx.emit(SettingsPaneEvent::CodexUsageChanged);
                this.refresh_codex_entitlements(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn render_codex_entitlements(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let state = &self.codex_usage;
        let muted = cx.theme().muted_foreground;
        let mut section =
            v_flex().gap_2().mt_2().pt_3().border_t_1().border_color(cx.theme().border).child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.text(Message::UsageCreditsTitle)),
            );
        if let Some(result) = &state.outcome {
            let text = match result {
                Ok(outcome) => language
                    .text(match outcome {
                        Outcome::Reset => Message::UsageCreditReset,
                        Outcome::AlreadyRedeemed => Message::UsageCreditRedeemed,
                        Outcome::NothingToReset => Message::UsageCreditNothing,
                        Outcome::NoCredit => Message::UsageCreditNone,
                    })
                    .to_owned(),
                Err(issue) => issue_text(issue, language),
            };
            section = section.child(div().text_sm().whitespace_normal().child(text));
        }
        match &state.info {
            Some(Ok(info)) => {
                section = section.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{}", info.observed.format("%m-%d %H:%M:%S"))),
                );
                section = section.child(
                    div().text_sm().child(
                        language.format(
                            Message::UsageCreditCount,
                            &[(
                                "count",
                                &info
                                    .available
                                    .map(|count| count.to_string())
                                    .unwrap_or_else(|| "—".to_owned()),
                            )],
                        ),
                    ),
                );
                if info.pending {
                    let account = info.account_key.clone();
                    section = section
                        .child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(cx.theme().warning)
                                .child(language.text(Message::UsageCreditPending)),
                        )
                        .child(
                            Button::new("codex-reset-retry")
                                .small()
                                .label(language.text(Message::UsageCreditRetry))
                                .disabled(self.codex_entitlements_busy(cx))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_codex_reset(account.clone(), None, window, cx);
                                })),
                        );
                } else {
                    if let Some(expiry) =
                        info.credits.iter().filter_map(|credit| credit.expires_at).min()
                    {
                        section = section.child(div().text_xs().text_color(muted).child(
                            language.format(
                                Message::UsageCreditNextExpiry,
                                &[("time", &expiry.format("%Y-%m-%d %H:%M").to_string())],
                            ),
                        ));
                    }
                    section = section.child(
                        details_button(
                            "codex-credit-details",
                            language.text(Message::UsageCreditDetails),
                            state.credits_details,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.codex_usage.credits_details = !this.codex_usage.credits_details;
                            cx.notify();
                        })),
                    );
                    if state.credits_details {
                        for (index, item) in info.credits.iter().enumerate() {
                            let account = info.account_key.clone();
                            let id = item.id.clone();
                            let expiry = item
                                .expires_at
                                .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_else(|| {
                                    language.text(Message::UsageCreditNoExpiry).to_owned()
                                });
                            let title = if item.title.is_empty() {
                                language.text(Message::UsageCreditItem).to_owned()
                            } else {
                                item.title.clone()
                            };
                            section = section.child(
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .min_w_0()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .child(div().truncate().child(title))
                                            .child(div().text_xs().text_color(muted).child(expiry)),
                                    )
                                    .child(
                                        Button::new(("codex-reset-credit", index))
                                            .small()
                                            .min_w(cx.theme().font_size * 7.0)
                                            .flex_shrink_0()
                                            .label(language.text(if index == 0 {
                                                Message::UsageCreditUseFirst
                                            } else {
                                                Message::UsageCreditUse
                                            }))
                                            .disabled(
                                                !info.complete
                                                    || self.codex_entitlements_busy(cx)
                                                    || (chrono::Local::now() - info.observed)
                                                        .num_seconds()
                                                        > 90
                                                    || item.expires_at.is_some_and(|time| {
                                                        time <= chrono::Local::now()
                                                    }),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.confirm_codex_reset(
                                                    account.clone(),
                                                    Some(id.clone()),
                                                    window,
                                                    cx,
                                                );
                                            })),
                                    ),
                            );
                        }
                    }
                    if !info.complete {
                        section = section.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(muted)
                                .child(language.text(Message::UsageCreditIncomplete)),
                        );
                    }
                }
            },
            Some(Err(issue)) => {
                section = section.child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(muted)
                        .child(issue_text(issue, language)),
                )
            },
            None => {
                section = section.child(
                    div().text_xs().text_color(muted).child(language.text(Message::UsageLoading)),
                )
            },
        }
        if state.consuming.is_some() {
            section =
                section.child(div().text_sm().child(language.text(Message::UsageCreditConsuming)));
        }
        section.into_any_element()
    }
}
