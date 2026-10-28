//! Local CLI versions and opt-in upgrades; all I/O belongs on a background executor.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::ai_agents::AgentKind;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Version {
    text: String,
    numbers: [u64; 3],
    prerelease: bool,
}

impl Version {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('v');
        if text.len() > 128
            || !text.bytes().all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c))
        {
            return None;
        }
        let core = text.split(['-', '+']).next()?;
        let parts = core.split('.').map(str::parse::<u64>).collect::<Result<Vec<_>, _>>().ok()?;
        let numbers = parts.try_into().ok()?;
        Some(Self { text: text.to_owned(), numbers, prerelease: text.contains('-') })
    }
}

#[derive(Clone, Debug)]
enum Installer {
    Npm { executable: PathBuf, prefix: PathBuf },
    ClaudeNative,
    CodexNative,
    Manual,
}

#[derive(Clone, Debug)]
pub(crate) struct Check {
    pub kind: AgentKind,
    pub executable: PathBuf,
    current: Version,
    stable: Version,
    installer: Installer,
}

impl Check {
    pub fn current(&self) -> &str {
        &self.current.text
    }
    pub fn stable(&self) -> &str {
        &self.stable.text
    }
    pub fn comparison(&self) -> Ordering {
        self.current
            .numbers
            .cmp(&self.stable.numbers)
            .then_with(|| self.stable.prerelease.cmp(&self.current.prerelease))
    }
    pub fn can_upgrade(&self) -> bool {
        !matches!(self.installer, Installer::Manual)
    }
}

#[derive(Debug)]
pub(crate) enum Error {
    NotFound,
    Version(String),
    Registry(String),
    Unsupported,
    Changed,
    Upgrade(String),
    Verify { expected: String, actual: String },
}

fn package(kind: AgentKind) -> Result<&'static str, Error> {
    match kind {
        AgentKind::Codex => Ok("@openai/codex"),
        AgentKind::Claude => Ok("@anthropic-ai/claude-code"),
        _ => Err(Error::Unsupported),
    }
}

pub(crate) fn check(kind: AgentKind) -> Result<Check, Error> {
    let package = package(kind)?;
    let executable = super::executable(kind).ok_or(Error::NotFound)?;
    let current = installed_version(&executable)?;
    // Claude's stable channel differs from latest. Codex publishes stable as latest.
    let url = format!("https://registry.npmjs.org/-/package/{package}/dist-tags");
    let mut response = crate::update_proxy::agent(&url, Duration::from_secs(30))
        .get(&url)
        .call()
        .map_err(|error| Error::Registry(error.to_string()))?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_to_vec()
        .map_err(|error| Error::Registry(error.to_string()))?;
    let tags: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| Error::Registry(error.to_string()))?;
    let tag = if kind == AgentKind::Claude { "stable" } else { "latest" };
    let stable = tags
        .get(tag)
        .and_then(serde_json::Value::as_str)
        .and_then(Version::parse)
        .filter(|version| !version.prerelease)
        .ok_or_else(|| Error::Registry("Invalid stable version".to_owned()))?;
    let installer = installer(kind, &executable, package);
    Ok(Check { kind, executable, current, stable, installer })
}

pub(crate) fn upgrade(approved: &Check) -> Result<Check, Error> {
    // A changed PATH, auto-updated CLI or newly published release needs fresh consent.
    let fresh = check(approved.kind)?;
    if fresh.executable != approved.executable
        || fresh.current != approved.current
        || fresh.stable != approved.stable
    {
        return Err(Error::Changed);
    }
    if fresh.comparison() != Ordering::Less {
        return Err(Error::Changed);
    }
    let mut command = match &fresh.installer {
        Installer::Npm { executable, prefix } => {
            let mut command = command(executable);
            command
                .arg("--prefix")
                .arg(prefix)
                .args(["install", "--global"])
                .arg(format!("{}@{}", package(fresh.kind)?, fresh.stable()))
                .args(["--registry=https://registry.npmjs.org", "--no-audit", "--no-fund"]);
            command
        },
        Installer::ClaudeNative => {
            let mut command = command(&fresh.executable);
            command.args(["install", fresh.stable()]);
            command
        },
        Installer::CodexNative => {
            let mut command = command(&fresh.executable);
            command.arg("update");
            command
        },
        Installer::Manual => return Err(Error::Unsupported),
    };
    // Never terminate existing sessions or inherit an interactive prompt.
    command.env("CI", "1");
    output(command, Duration::from_secs(600), 2 * 1024 * 1024).map_err(Error::Upgrade)?;
    let actual = installed_version(&fresh.executable)?;
    if actual != fresh.stable {
        return Err(Error::Verify { expected: fresh.stable.text, actual: actual.text });
    }
    Ok(Check { current: actual, ..fresh })
}

fn installed_version(executable: &Path) -> Result<Version, Error> {
    let mut command = command(executable);
    command.arg("--version");
    let output = output(command, Duration::from_secs(20), 8192).map_err(Error::Version)?;
    output
        .split_whitespace()
        .find_map(Version::parse)
        .ok_or_else(|| Error::Version("Unrecognized CLI version".to_owned()))
}

fn installer(kind: AgentKind, executable: &Path, package: &str) -> Installer {
    let canonical = executable.canonicalize().unwrap_or_else(|_| executable.to_owned());
    for parent in
        executable.ancestors().skip(1).take(8).chain(canonical.ancestors().skip(1).take(8))
    {
        let root = parent.join("node_modules").join(package);
        let shim = executable
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| ["cmd", "bat", "ps1"].iter().any(|ext| s.eq_ignore_ascii_case(ext)));
        if !(canonical.starts_with(&root) || shim && executable.parent() == Some(parent)) {
            continue;
        }
        let manifest = std::fs::File::open(root.join("package.json")).ok().and_then(|file| {
            use std::io::Read as _;
            serde_json::from_reader::<_, serde_json::Value>(file.take(64 * 1024)).ok()
        });
        if manifest.as_ref().and_then(|value| value.get("name")).and_then(|value| value.as_str())
            != Some(package)
        {
            continue;
        }
        let npm = super::executable_directories()
            .into_iter()
            .flat_map(|dir| ["npm.exe", "npm.cmd", "npm"].map(|name| dir.join(name)))
            .find(|path| path.is_file());
        if let Some(executable) = npm {
            let prefix = if !cfg!(windows) && parent.file_name().is_some_and(|name| name == "lib") {
                parent.parent().unwrap_or(parent)
            } else {
                parent
            };
            return Installer::Npm { executable, prefix: prefix.to_owned() };
        }
    }
    // Native Claude installs live here; don't migrate Homebrew/WinGet installs.
    if kind == AgentKind::Claude
        && crate::platform::dirs::home_dir().is_some_and(|home| {
            executable.starts_with(home.join(".local/bin"))
                || canonical.starts_with(home.join(".local/share/claude"))
        })
    {
        return Installer::ClaudeNative;
    }
    if kind == AgentKind::Codex {
        // Older Codex treats unknown subcommands as prompts; probe help first.
        let mut command = command(executable);
        command.arg("--help");
        if output(command, Duration::from_secs(20), 64 * 1024).is_ok_and(|help| {
            help.lines().any(|line| line.trim_start().split_whitespace().next() == Some("update"))
        }) {
            return Installer::CodexNative;
        }
    }
    Installer::Manual
}

fn command(executable: &Path) -> Command {
    // Rust handles .cmd/.bat quoting; PowerShell scripts require an explicit host.
    if cfg!(windows)
        && executable
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
    {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"]).arg(executable);
        command
    } else {
        Command::new(executable)
    }
}

fn output(command: Command, timeout: Duration, limit: usize) -> Result<String, String> {
    crate::platform::process_output::read_cancellable(command, timeout, limit, &|| false)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| error.to_string())
}
