//! Shared official-window presentation for the workspace summary and Settings.

use super::prelude::*;
use super::usage_format::window_duration_label;
use crate::ai_usage::QuotaWindow;
use crate::i18n::Message;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, relative,
};

pub(super) fn quota_window(limit: &QuotaWindow, index: usize, cx: &App) -> gpui::AnyElement {
    let language = crate::gpui_shell::config::ui_language(cx);
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let pending = limit.resets_at.is_some_and(|reset| reset <= chrono::Local::now());
    let used = 100.0 - limit.remaining_percent;
    let value = if pending {
        language.text(Message::UsagePending).to_owned()
    } else {
        format!("{used:.0}%")
    };
    let color = if limit.remaining_percent <= 10.0 {
        theme.danger
    } else if limit.remaining_percent <= 25.0 {
        theme.warning
    } else {
        theme.primary
    };
    let mut detail = if pending {
        language.text(Message::UsageResetPending).to_owned()
    } else {
        language.format(
            Message::UsageRemaining,
            &[("percent", &format!("{:.0}", limit.remaining_percent))],
        )
    };
    if let Some(reset) = &limit.resets_at {
        detail.push_str(&format!(
            "\n{}",
            language.format(
                Message::UsageResets,
                &[("time", &reset.format("%m-%d %H:%M").to_string())]
            )
        ));
    }
    let mut row = v_flex()
        .id(("usage-window", index))
        .w_full()
        .gap_2()
        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .gap_2()
                .child(div().text_sm().child(window_duration_label(
                    &limit.id,
                    limit.seconds,
                    language,
                )))
                .child(
                    h_flex()
                        .gap_2()
                        .items_baseline()
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(language.text(Message::UsageUsedLabel)),
                        )
                        .child(
                            div()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(value),
                        ),
                ),
        )
        .child(div().h(px(5.0)).w_full().rounded_full().bg(theme.muted).when(!pending, |track| {
            track.child(div().h_full().w(relative((used / 100.0) as f32)).rounded_full().bg(color))
        }));
    if let Some(reset) = &limit.resets_at {
        row =
            row.child(div().text_xs().text_color(muted).child(language.format(
                Message::UsageResets,
                &[("time", &reset.format("%m-%d %H:%M").to_string())],
            )));
    }
    row.into_any_element()
}
