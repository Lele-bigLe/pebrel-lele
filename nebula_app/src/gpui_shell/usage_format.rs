use crate::ai_usage::Issue;
use crate::display::UiLanguage;
use crate::i18n::Message;

/// Truncate compact values so an incomplete-record lower bound never rounds upward.
pub(super) fn recorded_tokens(tokens: i64, partial: bool) -> String {
    if tokens < 0 || (partial && tokens == 0) {
        return "—".into();
    }
    let amount = [(1_000_000_000_000, "T"), (1_000_000_000, "B"), (1_000_000, "M"), (1_000, "K")]
        .into_iter()
        .find(|(unit, _)| tokens >= *unit)
        .map(|(unit, suffix)| {
            format!("{}.{:02}{suffix}", tokens / unit, tokens % unit * 100 / unit)
        })
        .unwrap_or_else(|| tokens.to_string());
    if partial { format!("≥{amount}") } else { amount }
}

pub(super) fn issue_text(issue: &Issue, language: UiLanguage) -> String {
    let message = match issue {
        Issue::MissingAuth => Message::UsageMissingAuth,
        Issue::ApiKeyAuth => Message::UsageApiKeyAuth,
        Issue::InvalidAuth => Message::UsageInvalidAuth,
        Issue::AuthChanged => Message::UsageAuthChanged,
        Issue::Authentication => Message::UsageAuthentication,
        Issue::Http(429) => Message::UsageThrottled,
        Issue::Http(status) => {
            return language.format(Message::UsageHttp, &[("status", &status.to_string())]);
        },
        Issue::Network => Message::UsageNetwork,
        Issue::InvalidQuota => Message::UsageInvalidQuota,
        Issue::MissingDatabase => Message::UsageMissingDatabase,
        Issue::Database => Message::UsageDatabase,
        Issue::DateBoundary => Message::UsageDateBoundary,
        Issue::CodexUnavailable => Message::UsageCodexUnavailable,
        Issue::CodexUnsupported => Message::UsageCodexUnsupported,
        Issue::OperationBusy => Message::UsageOperationBusy,
        Issue::Storage => Message::UsageStorage,
        Issue::ResetPending => Message::UsageCreditPending,
    };
    language.text(message).to_owned()
}

pub(super) fn window_duration_label(
    id: &str,
    seconds: Option<u64>,
    language: UiLanguage,
) -> String {
    let (message, amount) = match seconds {
        Some(seconds) if seconds % 86400 == 0 => (Message::UsageDays, seconds / 86400),
        Some(seconds) if seconds % 3600 == 0 => (Message::UsageHours, seconds / 3600),
        Some(seconds) if seconds % 60 == 0 => (Message::UsageMinutes, seconds / 60),
        Some(seconds) => (Message::UsageSeconds, seconds),
        None => return id.to_owned(),
    };
    language.format(message, &[("count", &amount.to_string())])
}
