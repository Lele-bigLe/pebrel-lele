//! Codex owns authentication and entitlements; Pebrel stores only local operation receipts.

use super::codex::AuthGuard;
use super::{Issue, QuotaWindow};
use chrono::{DateTime, Local, Utc};
use serde_json::{Value, json};

mod reset;
mod rpc;
pub(crate) use reset::{Outcome, consume};

#[derive(Clone)]
pub(crate) struct Credit {
    pub id: String,
    pub title: String,
    pub expires_at: Option<DateTime<Local>>,
}

pub(crate) struct Status {
    pub account_key: String,
    pub label: String,
    pub plan: String,
    pub windows: Vec<QuotaWindow>,
    pub available: Option<u64>,
    pub credits: Vec<Credit>,
    pub complete: bool,
    pub pending: bool,
    pub observed: DateTime<Local>,
}

fn date(value: &Value) -> Option<DateTime<Local>> {
    if let Some(epoch) = value.as_i64() {
        let time = if epoch >= 1_000_000_000_000 {
            DateTime::<Utc>::from_timestamp_millis(epoch)
        } else {
            DateTime::<Utc>::from_timestamp(epoch, 0)
        }?;
        Some(time.with_timezone(&Local))
    } else {
        DateTime::parse_from_rfc3339(value.as_str()?).ok().map(|time| time.with_timezone(&Local))
    }
}

async fn snapshot(client: &mut rpc::Client, guard: &AuthGuard) -> Result<Status, Issue> {
    guard.check()?;
    let identity = client.call("account/read", json!({"refreshToken":false})).await?;
    let account = identity.get("account").ok_or(Issue::InvalidAuth)?;
    if account.get("type").and_then(Value::as_str) != Some("chatgpt") {
        return Err(Issue::ApiKeyAuth);
    }
    let label = account
        .get("email")
        .and_then(Value::as_str)
        .filter(|label| !label.trim().is_empty())
        .ok_or(Issue::InvalidAuth)?;
    let body = client.call("account/rateLimits/read", json!({})).await?;
    guard.check_account(&body)?;
    let bucket = body
        .pointer("/rateLimitsByLimitId/codex")
        .filter(|value| value.is_object())
        .or_else(|| body.get("rateLimits"))
        .ok_or(Issue::InvalidQuota)?;
    if bucket.get("limitId").and_then(Value::as_str).is_some_and(|id| id != "codex") {
        return Err(Issue::InvalidQuota);
    }
    let mut windows = Vec::new();
    for id in ["primary", "secondary"] {
        let Some(window) = bucket.get(id).filter(|value| !value.is_null()) else { continue };
        let used = window
            .get("usedPercent")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
            .ok_or(Issue::InvalidQuota)?;
        let minutes = window
            .get("windowDurationMins")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0 && *value <= u32::MAX as u64)
            .ok_or(Issue::InvalidQuota)?;
        windows.push(QuotaWindow {
            id: id.to_owned(),
            seconds: minutes.checked_mul(60),
            remaining_percent: 100.0 - used,
            resets_at: window.get("resetsAt").and_then(date),
        });
    }
    let reset = body.get("rateLimitResetCredits");
    let available = reset.and_then(|value| value.get("availableCount")).and_then(Value::as_u64);
    let rows = reset.and_then(|value| value.get("credits")).and_then(Value::as_array);
    let mut complete = rows.is_some();
    let mut available_ids = std::collections::HashSet::new();
    let mut credits = Vec::new();
    if let Some(rows) = rows {
        if rows.len() > 1000 {
            return Err(Issue::InvalidQuota);
        }
        for row in rows {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 512);
            let status = row.get("status").and_then(Value::as_str);
            let expiry = row.get("expiresAt");
            let valid = id.is_some()
                && status.is_some()
                && expiry.is_some()
                && row.get("resetType").and_then(Value::as_str) == Some("codexRateLimits")
                && expiry.is_some_and(|value| value.is_null() || date(value).is_some());
            complete &= valid;
            if !valid || status != Some("available") {
                continue;
            }
            let id = id.unwrap();
            if !available_ids.insert(id.to_owned()) {
                complete = false;
                continue;
            }
            let expires_at = expiry.and_then(date);
            if expires_at.is_some_and(|expiry| expiry <= Local::now()) {
                continue;
            }
            credits.push(Credit {
                id: id.to_owned(),
                title: row
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(128)
                    .collect(),
                expires_at,
            });
        }
    }
    complete &= available == Some(available_ids.len() as u64);
    // Non-expiring credits come last; use expiring entitlements first.
    credits.sort_by_key(|credit| (credit.expires_at.is_none(), credit.expires_at));
    guard.check()?;
    let account_key = guard.key();
    Ok(Status {
        pending: reset::pending(&account_key)?.is_some(),
        account_key,
        label: label.chars().take(160).collect(),
        plan: bucket
            .get("planType")
            .or_else(|| account.get("planType"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect(),
        windows,
        available,
        credits,
        complete,
        observed: Local::now(),
    })
}

pub(crate) fn read() -> Result<Status, Issue> {
    let guard = AuthGuard::capture()?;
    rpc::run(async {
        let mut client = rpc::Client::start().await?;
        let result = snapshot(&mut client, &guard).await;
        client.close().await;
        result
    })
}
