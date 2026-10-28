//! Agent commands use the guarded paste and native Enter submission path.

use super::{InputOrigin, TerminalView};
use crate::ai_agents::AgentKind;
use crate::runtime_api::{ApiError, RuntimeAgentStateSource, RuntimeTaskState};
use gpui::Context;
use nebula_terminal::term::TermMode;

#[derive(Clone, Copy)]
pub(in crate::gpui_shell) enum AgentShortcut {
    Model,
    Usage,
    Presets,
    Status,
    Plan,
    Permissions,
    Context,
    Compact,
    Diff,
}

impl AgentShortcut {
    pub(in crate::gpui_shell) fn command(self, kind: AgentKind) -> Option<&'static str> {
        match (kind, self) {
            (AgentKind::Codex | AgentKind::Claude, Self::Model) => Some("/model"),
            (AgentKind::Codex | AgentKind::Claude, Self::Usage) => Some("/usage"),
            // Codex 0.164+ provides numbered reusable prompt/command presets.
            (AgentKind::Codex, Self::Presets) => Some("/presets"),
            (AgentKind::Codex | AgentKind::Claude, Self::Status) => Some("/status"),
            (AgentKind::Codex | AgentKind::Claude, Self::Plan) => Some("/plan"),
            (AgentKind::Codex | AgentKind::Claude, Self::Permissions) => Some("/permissions"),
            (AgentKind::Claude, Self::Context) => Some("/context"),
            (AgentKind::Codex | AgentKind::Claude, Self::Compact) => Some("/compact"),
            (AgentKind::Codex | AgentKind::Claude, Self::Diff) => Some("/diff"),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::gpui_shell) struct AgentShortcutTarget {
    pub kind: AgentKind,
    session_id: Option<String>,
    epoch: u64,
}

impl TerminalView {
    pub(in crate::gpui_shell) fn agent_shortcut_target(&self) -> Option<AgentShortcutTarget> {
        if self.ssh_destination.is_some() || self.exited.is_some() {
            return None;
        }
        let agent = self.runtime_chat_agent()?;
        let kind = AgentKind::parse(&agent.kind)?;
        matches!(kind, AgentKind::Codex | AgentKind::Claude).then_some(AgentShortcutTarget {
            kind,
            session_id: agent.session_id,
            epoch: self.ai_session_probe_epoch,
        })
    }

    pub(in crate::gpui_shell) fn agent_shortcuts_ready(&self) -> bool {
        let Some(agent) = self.runtime_chat_agent() else { return false };
        let state = self.runtime_task_state();
        // A foreground process alone does not establish an in-flight Agent turn.
        let input_state = matches!(state, RuntimeTaskState::Idle | RuntimeTaskState::Finished)
            || (state == RuntimeTaskState::Running
                && agent.state_source == RuntimeAgentStateSource::Process
                && !agent.hook_seen);
        input_state
            && self.exited.is_none()
            && self.error.is_none()
            && self.answer_reader.is_none()
            && self.marked_text.is_none()
            && self.pending_runtime_submit.is_none()
            && self.term_mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub(in crate::gpui_shell) fn run_agent_shortcut(
        &mut self,
        expected: &AgentShortcutTarget,
        shortcut: AgentShortcut,
        cx: &mut Context<Self>,
    ) -> Result<&'static str, ApiError> {
        if self.agent_shortcut_target().as_ref() != Some(expected) {
            return Err(ApiError::new("stale_target", "the foreground Agent has changed"));
        }
        if !self.agent_shortcuts_ready() {
            return Err(ApiError::new("invalid_state", "the Agent is not ready for command input"));
        }
        if !self.suggest.line_buf.is_empty() {
            return Err(ApiError::new(
                "input_not_empty",
                "finish the existing input before running a shortcut",
            ));
        }
        let command = shortcut.command(expected.kind).ok_or_else(|| {
            ApiError::new("unsupported_command", "the Agent does not support this shortcut")
        })?;
        self.runtime_agent_paste(command.to_owned(), true, InputOrigin::User, cx)?;
        Ok(command)
    }
}
