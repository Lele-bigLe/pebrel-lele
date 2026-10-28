use super::*;
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pending {
    account: String,
    credit: String,
    key: String,
}

#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    Reset,
    AlreadyRedeemed,
    NothingToReset,
    NoCredit,
}

fn path(account: &str) -> Result<PathBuf, Issue> {
    if account.len() != 64 || !account.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err(Issue::InvalidAuth);
    }
    Ok(crate::platform::dirs::data_dir().join(format!("codex-reset-{account}.json")))
}

pub(super) fn pending(account: &str) -> Result<Option<Pending>, Issue> {
    let file = match std::fs::File::open(path(account)?) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Issue::Storage),
    };
    let mut bytes = Vec::new();
    file.take(8193).read_to_end(&mut bytes).map_err(|_| Issue::Storage)?;
    if bytes.len() > 8192 {
        return Err(Issue::Storage);
    }
    let pending: Option<Pending> = serde_json::from_slice(&bytes).map_err(|_| Issue::Storage)?;
    if pending.as_ref().is_some_and(|pending| {
        pending.account != account
            || pending.credit.is_empty()
            || pending.credit.len() > 512
            || pending.key.len() != 36
            || !pending.key.bytes().enumerate().all(|(i, value)| {
                if [8, 13, 18, 23].contains(&i) { value == b'-' } else { value.is_ascii_hexdigit() }
            })
    }) {
        return Err(Issue::Storage);
    }
    Ok(pending)
}

fn nonce() -> Result<String, Issue> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| Issue::Storage)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    Ok(format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..]))
}

pub(crate) fn consume(account: String, credit: Option<String>) -> Result<Outcome, Issue> {
    // A lifetime lock also excludes other windows/processes for the complete RPC.
    let lock_path = crate::platform::dirs::data_dir().join("codex-reset-operation");
    let _lock = crate::atomic_file::try_lifetime_lock(&lock_path)
        .map_err(|_| Issue::Storage)?
        .ok_or(Issue::OperationBusy)?;
    let guard = AuthGuard::capture()?;
    if guard.key() != account {
        return Err(Issue::AuthChanged);
    }
    rpc::run(async {
        let mut client = rpc::Client::start().await?;
        let result = async {
            let status = snapshot(&mut client, &guard).await?;
            let attempt = match pending(&account)? {
                Some(attempt) => {
                    if credit.as_ref().is_some_and(|id| *id != attempt.credit) {
                        return Err(Issue::ResetPending);
                    }
                    attempt
                },
                None => {
                    let credit = credit.ok_or(Issue::ResetPending)?;
                    if !status.complete || !status.credits.iter().any(|item| item.id == credit) {
                        return Err(Issue::InvalidQuota);
                    }
                    let attempt = Pending { account: account.clone(), credit, key: nonce()? };
                    guard.check()?;
                    crate::atomic_file::write_private(
                        &path(&account)?,
                        &serde_json::to_vec(&Some(&attempt)).map_err(|_| Issue::Storage)?,
                    )
                    .map_err(|_| Issue::Storage)?;
                    attempt
                },
            };
            guard.check()?;
            let response = client
                .call(
                    "account/rateLimitResetCredit/consume",
                    json!({"idempotencyKey":attempt.key,"creditId":attempt.credit}),
                )
                .await?;
            guard.check()?;
            let outcome = match response.get("outcome").and_then(Value::as_str) {
                Some("reset") => Outcome::Reset,
                Some("alreadyRedeemed") => Outcome::AlreadyRedeemed,
                Some("nothingToReset") => Outcome::NothingToReset,
                Some("noCredit") => Outcome::NoCredit,
                _ => return Err(Issue::ResetPending),
            };
            crate::atomic_file::write_private(&path(&account)?, b"null")
                .map_err(|_| Issue::Storage)?;
            Ok(outcome)
        }
        .await;
        client.close().await;
        result
    })
}
