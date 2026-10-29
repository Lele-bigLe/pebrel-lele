//! Shared short-lived App Server transport. Callers own the permitted operations.

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Error {
    Unavailable,
    MissingHome,
    Transport,
    Unsupported,
    InvalidResponse,
    TooLarge,
}

pub(crate) enum Purpose {
    Entitlements,
    History,
}

fn executable() -> Result<PathBuf, Error> {
    let launcher = crate::ai_hook::integrations::executable(crate::ai_agents::AgentKind::Codex)
        .ok_or(Error::Unavailable)?;
    crate::platform::cli::codex_executable(launcher).ok_or(Error::Unavailable)
}

pub(crate) struct Client {
    child: tokio::process::Child,
    output: BufReader<tokio::process::ChildStdout>,
    input: tokio::process::ChildStdin,
    sequence: u64,
    max_line: u64,
}

impl Client {
    pub(crate) async fn start(purpose: Purpose) -> Result<Self, Error> {
        let mut command = tokio::process::Command::new(executable()?);
        command.arg("app-server");
        if matches!(purpose, Purpose::Entitlements) {
            command.args([
                "-c",
                "cli_auth_credentials_store=\"file\"",
                "-c",
                "model_provider=\"openai\"",
            ]);
        }
        for option in [
            "hooks",
            "codex_hooks",
            "apps",
            "shell_tool",
            "unified_exec",
            "multi_agent",
            "browser_use",
            "computer_use",
            "code_mode.enabled",
        ] {
            command.arg("-c").arg(format!("features.{option}=false"));
        }
        command
            .args(["-c", "web_search=\"disabled\""])
            .env("CODEX_HOME", crate::codex_config::codex_home().ok_or(Error::MissingHome)?)
            .current_dir(crate::platform::dirs::data_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        crate::platform::process::hidden_command(command.as_std_mut());
        let mut child = command.spawn().map_err(|_| Error::Unavailable)?;
        let input = child.stdin.take().ok_or(Error::Transport)?;
        let output = BufReader::new(child.stdout.take().ok_or(Error::Transport)?);
        let max_line = match purpose {
            Purpose::Entitlements => 1024 * 1024,
            Purpose::History => 16 * 1024 * 1024,
        };
        let mut client = Self { child, input, output, sequence: 0, max_line };
        client.call("initialize", json!({"clientInfo":{"name":"pebrel","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        client.send(json!({"method":"initialized"})).await?;
        Ok(client)
    }

    async fn send(&mut self, value: Value) -> Result<(), Error> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| Error::InvalidResponse)?;
        bytes.push(b'\n');
        self.input.write_all(&bytes).await.map_err(|_| Error::Transport)?;
        self.input.flush().await.map_err(|_| Error::Transport)
    }

    pub(crate) async fn call(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        self.sequence += 1;
        let id = self.sequence;
        self.send(json!({"id":id,"method":method,"params":params})).await?;
        // Bound both response size and unsolicited notifications.
        for _ in 0..128 {
            let mut line = String::new();
            (&mut self.output)
                .take(self.max_line + 1)
                .read_line(&mut line)
                .await
                .map_err(|_| Error::Transport)?;
            if line.is_empty() {
                return Err(Error::Transport);
            }
            if line.len() as u64 > self.max_line {
                return Err(Error::TooLarge);
            }
            let message: Value = serde_json::from_str(&line).map_err(|_| Error::InvalidResponse)?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(if error.get("code").and_then(Value::as_i64) == Some(-32601) {
                    Error::Unsupported
                } else {
                    Error::InvalidResponse
                });
            }
            return message.get("result").cloned().ok_or(Error::InvalidResponse);
        }
        Err(Error::InvalidResponse)
    }

    pub(crate) async fn close(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

pub(crate) fn run<T, E: From<Error>>(
    action: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, E> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| E::from(Error::Transport))?
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(25), action)
                .await
                .map_err(|_| E::from(Error::Transport))?
        })
}
