//! A read-only Settings projection of the window's native record scanner.
use super::*;
use crate::ai_usage::Period;
use crate::gpui_shell::usage_format::recorded_tokens;
use crate::gpui_shell::workspace::NebulaWorkspace;

impl SettingsPane {
    pub(in crate::gpui_shell) fn attach_usage_owner(
        &mut self,
        owner: gpui::WeakEntity<NebulaWorkspace>,
        cx: &mut Context<Self>,
    ) {
        if self.codex_usage.usage_owner.as_ref() == Some(&owner) {
            return;
        }
        self.codex_usage.usage_subscription =
            owner.upgrade().map(|owner| cx.observe(&owner, |_, _, cx| cx.notify()));
        self.codex_usage.usage_owner = Some(owner);
    }

    pub(super) fn render_local_records(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let muted = cx.theme().muted_foreground;
        let mut section = v_flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                details_button(
                    "usage-local-details",
                    language.text(Message::UsageLocalTitle),
                    self.codex_usage.local_details,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.codex_usage.local_details = !this.codex_usage.local_details;
                    cx.notify();
                })),
            );
        if !self.codex_usage.local_details {
            return section.into_any_element();
        }
        let Some(owner) = self.codex_usage.usage_owner.as_ref().and_then(|owner| owner.upgrade())
        else {
            return section
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(language.text(Message::UsageLocalUnavailable)),
                )
                .into_any_element();
        };
        let usage = owner.read(cx).local_usage();
        section = section.child(h_flex().gap_1().children(
            [Period::Today, Period::Month].into_iter().map(|period| {
                Button::new(if period == Period::Today {
                    "settings-usage-today"
                } else {
                    "settings-usage-month"
                })
                .ghost()
                .small()
                .disabled(usage.busy)
                .selected(usage.period == period)
                .label(language.text(if period == Period::Today {
                    Message::UsageToday
                } else {
                    Message::UsageMonth
                }))
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(SettingsPaneEvent::UsagePeriodChanged(period))
                }))
            }),
        ));
        match usage.local {
            Some(Ok(rows)) if rows.is_empty() => {
                section = section.child(div().text_sm().text_color(muted).child(language.text(
                    if usage.partial { Message::UsageReadingRecords } else { Message::UsageEmpty },
                )));
            },
            Some(Ok(rows)) => {
                for row in rows {
                    let name = match row.app.as_str() {
                        "codex" => "Codex",
                        "claude" => "Claude Code",
                        "gemini" => "Gemini CLI",
                        "opencode" => "OpenCode",
                        other => other,
                    };
                    section = section.child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_3()
                            .justify_between()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(div().truncate().child(name.to_owned()))
                                    .child(div().text_xs().text_color(muted).child(
                                        language.format(
                                            Message::UsageRequests,
                                            &[("count", &row.requests.to_string())],
                                        ),
                                    )),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .child(format!(
                                        "{} tokens",
                                        recorded_tokens(row.tokens, usage.partial)
                                    )),
                            ),
                    );
                }
            },
            Some(Err(issue)) => {
                section = section.child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .text_color(muted)
                        .child(issue_text(issue, language)),
                )
            },
            None => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(language.text(Message::UsageReadingRecords)),
                )
            },
        }
        if usage.partial && matches!(usage.local, Some(Ok(_))) {
            section = section.child(
                div()
                    .text_xs()
                    .whitespace_normal()
                    .text_color(muted)
                    .child(language.text(Message::UsagePartial)),
            );
        }
        if let Some(time) = usage.checked_at {
            section = section.child(div().text_xs().text_color(muted).child(language.format(
                Message::UsageUpdated,
                &[
                    ("source", language.text(Message::UsageLocalTitle)),
                    ("time", &time.format("%m-%d %H:%M:%S").to_string()),
                ],
            )));
        }
        section
            .child(
                div()
                    .text_xs()
                    .whitespace_normal()
                    .text_color(muted)
                    .child(language.text(Message::UsageSource)),
            )
            .into_any_element()
    }
}
