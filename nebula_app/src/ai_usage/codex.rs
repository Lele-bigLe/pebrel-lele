use std::io::Read as _;
use std::time::Duration;

use base64::Engine as _;
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use zeroize::Zeroizing;

use super::{Issue, QuotaWindow};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const MAX_JSON_BYTES: u64 = 1024 * 1024;

pub(crate) type AuthRevision = (std::time::SystemTime, u64);

/// Cache invalidation only: credentials never cross the adapter boundary.
pub(crate) fn auth_revision() -> Option<AuthRevision> {
    let path = crate::codex_config::codex_home()?.join("auth.json");
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

struct Credentials {
    token: Zeroizing<String>,
    account: Zeroizing<String>,
}

pub(super) struct AuthGuard(Credentials);

impl AuthGuard {
    pub(super) fn capture() -> Result<Self, Issue> {
        credentials().map(Self)
    }

    pub(super) fn key(&self) -> String {
        use sha2::{Digest as _, Sha256};
        Sha256::digest(self.0.account.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    pub(super) fn check(&self) -> Result<(), Issue> {
        let current = credentials().map_err(|_| Issue::AuthChanged)?;
        if current.token.as_str() != self.0.token.as_str()
            || current.account.as_str() != self.0.account.as_str()
        {
            return Err(Issue::AuthChanged);
        }
        Ok(())
    }

    pub(super) fn check_account(&self, response: &Value) -> Result<(), Issue> {
        if response
            .get("accountId")
            .or_else(|| response.get("account_id"))
            .and_then(Value::as_str)
            .is_some_and(|id| id != self.0.account.as_str())
        {
            return Err(Issue::AuthChanged);
        }
        self.check()
    }
}

fn credentials() -> Result<Credentials, Issue> {
    let path = crate::codex_config::codex_home().ok_or(Issue::MissingAuth)?.join("auth.json");
    let file = std::fs::File::open(path).map_err(|_| Issue::MissingAuth)?;
    let mut text = Zeroizing::new(String::new());
    file.take(MAX_JSON_BYTES + 1).read_to_string(&mut text).map_err(|_| Issue::InvalidAuth)?;
    if text.len() as u64 > MAX_JSON_BYTES {
        return Err(Issue::InvalidAuth);
    }
    let auth: Value = serde_json::from_str(&text).map_err(|_| Issue::InvalidAuth)?;
    let mode = auth.get("auth_mode").or_else(|| auth.get("authMode")).and_then(Value::as_str);
    if mode.is_some_and(|mode| mode != "chatgpt")
        || (mode.is_none()
            && auth
                .get("OPENAI_API_KEY")
                .and_then(Value::as_str)
                .is_some_and(|key| !key.is_empty()))
    {
        return Err(Issue::ApiKeyAuth);
    }
    let tokens = auth.get("tokens").ok_or(Issue::InvalidAuth)?;
    let token = tokens
        .get("access_token")
        .or_else(|| tokens.get("accessToken"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(Issue::InvalidAuth)?;
    let claims = token.split('.').nth(1).and_then(|payload| {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
        serde_json::from_slice::<Value>(&bytes).ok()
    });
    let token_account = claims
        .as_ref()
        .and_then(|claims| claims.get("https://api.openai.com/auth"))
        .and_then(|claims| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str);
    let account = tokens
        .get("account_id")
        .or_else(|| tokens.get("accountId"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or(token_account)
        .ok_or(Issue::InvalidAuth)?;
    if token_account.is_some_and(|expected| expected != account) {
        return Err(Issue::InvalidAuth);
    }
    Ok(Credentials {
        token: Zeroizing::new(token.to_owned()),
        account: Zeroizing::new(account.to_owned()),
    })
}

pub(crate) fn read() -> Result<super::CodexReport, Issue> {
    let guard = AuthGuard::capture()?;
    let result = read_guarded(&guard);
    if result.is_err() {
        super::capacity::break_chain(&guard);
    }
    result
}

fn read_guarded(guard: &AuthGuard) -> Result<super::CodexReport, Issue> {
    let auth = &guard.0;
    let agent = ureq::config::Config::builder()
        .proxy(crate::update_proxy::resolve(USAGE_URL))
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .build()
        .new_agent();
    let bearer = Zeroizing::new(format!("Bearer {}", auth.token.as_str()));
    let mut response = agent
        .get(USAGE_URL)
        .header("Authorization", bearer.as_str())
        .header("ChatGPT-Account-Id", auth.account.as_str())
        .header("User-Agent", "codex-cli")
        .header("Accept", "application/json")
        .header("Cache-Control", "no-cache")
        .call()
        .map_err(|_| Issue::Network)?;
    match response.status().as_u16() {
        200 => {},
        401 | 403 => return Err(Issue::Authentication),
        status => return Err(Issue::Http(status)),
    }
    let body: Value = response
        .body_mut()
        .with_config()
        .limit(MAX_JSON_BYTES)
        .read_json()
        .map_err(|_| Issue::InvalidQuota)?;
    // Include the entire response download in the account-change guard.
    guard.check_account(&body)?;
    let windows = quota_windows(&body, Utc::now())?;
    let plan = body
        .get("plan_type")
        .or_else(|| body.get("planType"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    guard.check()?;
    let capacity = super::capacity::observe(guard, plan, &windows);
    if capacity.is_err() {
        super::capacity::break_chain(guard);
    }
    guard.check()?;
    Ok(super::CodexReport { plan: (!plan.is_empty()).then(|| plan.to_owned()), windows, capacity })
}

fn quota_windows(body: &Value, observed: DateTime<Utc>) -> Result<Vec<QuotaWindow>, Issue> {
    let rate_limit = body
        .get("rate_limit")
        .or_else(|| body.get("rateLimit"))
        .and_then(Value::as_object)
        .ok_or(Issue::InvalidQuota)?;
    let mut windows = Vec::new();
    for (key, value) in rate_limit {
        let Some(id) = key.strip_suffix("_window").or_else(|| key.strip_suffix("Window")) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let used = number(value.get("used_percent").or_else(|| value.get("usedPercent")))
            .filter(|used| (0.0..=100.0).contains(used))
            .ok_or(Issue::InvalidQuota)?;
        let seconds =
            number(value.get("limit_window_seconds").or_else(|| value.get("limitWindowSeconds")))
                .filter(|value| *value >= 1.0 && *value <= u32::MAX as f64 && value.fract() == 0.0)
                .map(|value| value as u64);
        let reset = number(
            value
                .get("reset_at")
                .or_else(|| value.get("resetAt"))
                .or_else(|| value.get("resets_at"))
                .or_else(|| value.get("resetsAt")),
        )
        .and_then(|epoch| {
            if epoch >= 1_000_000_000_000.0 {
                DateTime::<Utc>::from_timestamp_millis(epoch as i64)
            } else {
                DateTime::<Utc>::from_timestamp(epoch as i64, 0)
            }
        })
        .or_else(|| {
            number(value.get("reset_after_seconds").or_else(|| value.get("resetAfterSeconds")))
                .filter(|seconds| *seconds <= i32::MAX as f64)
                .and_then(|seconds| {
                    observed.checked_add_signed(chrono::Duration::seconds(seconds as i64))
                })
        });
        windows.push(QuotaWindow {
            id: id.to_owned(),
            seconds,
            remaining_percent: 100.0 - used,
            resets_at: reset.map(|time| time.with_timezone(&Local)),
        });
    }
    if windows.is_empty()
        && !["primary_window", "secondary_window", "primaryWindow", "secondaryWindow"]
            .iter()
            .any(|key| rate_limit.get(*key).is_some_and(Value::is_null))
    {
        return Err(Issue::InvalidQuota);
    }
    windows.sort_by_key(|window| window.seconds.unwrap_or(u64::MAX));
    Ok(windows)
}

fn number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quota_windows_preserve_server_periods_and_reset_times() {
        let observed = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let body = json!({"rate_limit": {
            "secondary_window": {
                "used_percent": 100, "limit_window_seconds": 604800,
                "reset_after_seconds": 60
            },
            "primary_window": {
                "used_percent": 25, "limit_window_seconds": 18000,
                "reset_at": 1700000900000_u64
            }
        }});
        let windows = quota_windows(&body, observed).unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].seconds, Some(18000));
        assert_eq!(windows[0].remaining_percent, 75.0);
        assert_eq!(windows[0].resets_at.as_ref().unwrap().timestamp(), 1_700_000_900);
        assert_eq!(windows[1].remaining_percent, 0.0);
        assert_eq!(windows[1].resets_at.as_ref().unwrap().timestamp(), 1_700_000_060);
    }

    #[test]
    fn malformed_quota_never_becomes_an_available_balance() {
        let observed = Utc::now();
        for used in [json!(null), json!(-1), json!(101), json!("NaN")] {
            let body = json!({"rate_limit": {"primary_window": {"used_percent": used}}});
            assert!(quota_windows(&body, observed).is_err());
        }
        assert!(quota_windows(&json!({"rate_limit": {}}), observed).is_err());
    }

    #[test]
    #[ignore = "requires the local ChatGPT login and a live quota request"]
    fn local_chatgpt_quota_request() {
        let windows =
            read().expect("live quota request failed (credentials are never printed)").windows;
        assert!(!windows.is_empty());
        assert!(windows.iter().all(|window| (0.0..=100.0).contains(&window.remaining_percent)));
        println!("Live quota response: {} valid windows", windows.len());
    }
}
