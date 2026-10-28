use super::*;

impl SettingsPane {
    pub(super) fn render_codex_capacity(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let muted = cx.theme().muted_foreground;
        let details = self.codex_usage.capacity_details;
        let mut section =
            v_flex().gap_2().mt_2().pt_3().border_t_1().border_color(cx.theme().border).child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.text(Message::UsageCapacityTitle)),
                    )
                    .child(
                        Button::new("codex-capacity-details")
                            .ghost()
                            .small()
                            .selected(details)
                            .icon(if details {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .label(language.text(Message::UsageCapacityDetails))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.codex_usage.capacity_details =
                                    !this.codex_usage.capacity_details;
                                cx.notify();
                            })),
                    ),
            );
        let quota = self.codex_usage.quota.read(cx);
        let report = quota
            .report
            .as_ref()
            .map(|result| result.as_ref().and_then(|report| report.capacity.as_ref()));
        match report {
            Some(Ok(report)) if !report.estimates.is_empty() => {
                for estimate in &report.estimates {
                    let range = |(low, high): (f64, f64)| format!("{low:.0}–{high:.0}");
                    section = section.child(
                        v_flex()
                            .gap_1()
                            .child(div().min_w_0().truncate().child(format!(
                                "{} · {} · {} · {}",
                                estimate.model,
                                estimate.effort,
                                estimate.tier,
                                window_duration_label(&estimate.window, estimate.seconds, language)
                            )))
                            .child(div().text_sm().child(language.format(
                                Message::UsageCapacityTotal,
                                &[("range", &range(estimate.total))],
                            )))
                            .child(div().text_sm().child(language.format(
                                Message::UsageCapacityRemaining,
                                &[("range", &range(estimate.remaining))],
                            )))
                            .when(details, |row| {
                                row.child(
                                    div().text_xs().whitespace_normal().text_color(muted).child(
                                        language.format(
                                            Message::UsageCapacitySample,
                                            &[
                                                ("requests", &estimate.requests.to_string()),
                                                ("points", &format!("{:.1}", estimate.points)),
                                                (
                                                    "cache",
                                                    &format!(
                                                        "{:.0}",
                                                        estimate.cached_ratio * 100.0
                                                    ),
                                                ),
                                            ],
                                        ),
                                    ),
                                )
                            }),
                    );
                }
            },
            Some(Ok(report)) => {
                use crate::ai_usage::capacity::SamplingIssue;
                let message = match report.blocked {
                    Some(SamplingIssue::Source) => Message::CapacitySource,
                    Some(SamplingIssue::Incomplete) => Message::CapacityIncomplete,
                    Some(SamplingIssue::Model) => Message::CapacityModel,
                    Some(SamplingIssue::Mixed) => Message::CapacityMixed,
                    Some(SamplingIssue::Interrupted) => Message::CapacityInterrupted,
                    None => Message::UsageCapacityPending,
                };
                section = section
                    .child(div().text_sm().whitespace_normal().child(language.text(message)));
                for progress in report.progress.iter().filter(|_| details) {
                    section =
                        section.child(div().text_xs().whitespace_normal().text_color(muted).child(
                            language.format(
                                Message::CapacityProgress,
                                &[
                                    (
                                        "model",
                                        &format!(
                                            "{} · {}",
                                            progress.model,
                                            window_duration_label(
                                                &progress.window,
                                                progress.seconds,
                                                language
                                            )
                                        ),
                                    ),
                                    ("requests", &progress.requests.to_string()),
                                    ("points", &format!("{:.1}", progress.points)),
                                ],
                            ),
                        ));
                }
                section = section.when(details, |section| {
                    section.child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(muted)
                            .child(language.text(Message::CapacityThreshold)),
                    )
                });
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
            _ => {
                section =
                    section.child(div().text_xs().whitespace_normal().text_color(muted).child(
                        language.text(if self.codex_usage.reading.is_some() {
                            Message::UsageLoading
                        } else {
                            Message::UsageCapacityPending
                        }),
                    ))
            },
        }
        section
            .when(details, |section| {
                section.child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(muted)
                        .child(language.text(Message::UsageCapacityScope)),
                )
            })
            .into_any_element()
    }
}
