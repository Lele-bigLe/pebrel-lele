//! Native CLI usage records and official Codex account entitlements.

use chrono::{DateTime, Local};

pub(crate) mod capacity;
mod codex;
pub(crate) mod codex_entitlements;
mod native;
pub(crate) use codex::read as read_codex;
pub(crate) use codex::{AuthRevision, auth_revision};
pub(crate) use native::read as read_native;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Period {
    #[default]
    Today,
    Month,
}

#[derive(Debug)]
pub(crate) enum Issue {
    MissingAuth,
    ApiKeyAuth,
    InvalidAuth,
    AuthChanged,
    Authentication,
    Http(u16),
    Network,
    InvalidQuota,
    MissingDatabase,
    Database,
    DateBoundary,
    CodexUnavailable,
    CodexUnsupported,
    OperationBusy,
    Storage,
    ResetPending,
}

#[derive(Clone)]
pub(crate) struct QuotaWindow {
    pub id: String,
    pub seconds: Option<u64>,
    pub remaining_percent: f64,
    pub resets_at: Option<DateTime<Local>>,
}

pub(crate) struct CodexReport {
    pub plan: Option<String>,
    pub windows: Vec<QuotaWindow>,
    pub capacity: Result<capacity::Report, Issue>,
}

pub(crate) struct AppUsage {
    pub app: String,
    pub requests: i64,
    pub tokens: i64,
}

pub(crate) struct NativeReport {
    pub apps: Vec<AppUsage>,
    pub metrics: Vec<SessionMetrics>,
    pub partial: bool,
}

#[derive(Clone)]
pub(crate) struct SessionMetrics {
    pub app: String,
    pub session_id: String,
    pub model: String,
    pub created_at: i64,
    pub output_tokens: i64,
    pub first_token_ms: Option<i64>,
    pub tokens_per_second: Option<f64>,
    pub estimated: bool,
    pub in_progress: bool,
}
