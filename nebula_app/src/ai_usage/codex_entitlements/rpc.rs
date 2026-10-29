//! Quota-specific errors and startup policy over the shared CLI transport.
use super::super::Issue;
use crate::ai_hook::integrations::codex_rpc::{self, Purpose};
use serde_json::Value;

pub(super) use codex_rpc::run;

impl From<codex_rpc::Error> for Issue {
    fn from(error: codex_rpc::Error) -> Self {
        match error {
            codex_rpc::Error::Unavailable => Self::CodexUnavailable,
            codex_rpc::Error::MissingHome => Self::MissingAuth,
            codex_rpc::Error::Transport => Self::Network,
            codex_rpc::Error::Unsupported => Self::CodexUnsupported,
            codex_rpc::Error::InvalidResponse
            | codex_rpc::Error::Rejected(_)
            | codex_rpc::Error::TooLarge => Self::InvalidQuota,
        }
    }
}

pub(super) struct Client(codex_rpc::Client);

impl Client {
    pub(super) async fn start() -> Result<Self, Issue> {
        Ok(Self(codex_rpc::Client::start(Purpose::Entitlements).await?))
    }

    pub(super) async fn call(&mut self, method: &str, params: Value) -> Result<Value, Issue> {
        self.0.call(method, params).await.map_err(Into::into)
    }

    pub(super) async fn close(&mut self) {
        self.0.close().await;
    }
}
