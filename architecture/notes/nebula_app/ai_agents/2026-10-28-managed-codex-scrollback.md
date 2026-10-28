# Managed Codex terminal scrollback

## Status

Superseded by [preserving unidentified agents](../gpui_shell/workspace/session_recovery/2026-10-28-preserve-unidentified-agents.md); the user canceled the display override and additional agent scrollbar.

Implemented on 2026-10-28; Windows product build and existing regressions passed.
Live agent scrollbar interaction remains for user acceptance.

## Context

Codex alternate-screen mode owns its viewport and does not provide normal terminal
scrollback. Separately, the default Auto scrollbar hides its thumb at the bottom,
including in identified Codex and Claude Code sessions that already have history.

## Evidence

- Installed Codex CLI 0.165.0 documents `--no-alt-screen` for start, resume and fork.
- The official CLI reference defines this flag as retaining terminal scrollback.
- `TerminalView::scrollbar_geometry` correctly requires real terminal history.
- `runtime_chat_agent_kind` already identifies an active agent from cached state.

## Decision

`AgentKind` adds `--no-alt-screen` to Pebrel-generated Codex start, resume and fork
commands. Auto visibility also shows the thumb for an identified running agent,
using the existing cached identity helper. Geometry, gutter and drag handling stay
owned by the terminal view and shared scrollbar implementation.

## Rejected alternatives

- Global Codex configuration would change terminals outside Pebrel.
- Rewriting arbitrary commands would override user-owned launch choices.
- A fake thumb for an alternate-screen application would misrepresent history.
- Always showing Auto scrollbars in ordinary shells would change existing settings.

## Consequences

The launch flag applies to new and recovered managed Codex sessions; an existing
process keeps its launch mode. Claude Code receives no undocumented option.
Explicit Hover and Always modes retain their behavior. No process or disk lookup
is added to painting. A thumb still requires nonempty terminal history.

## Validation

- Codex 0.165.0 accepted all three generated launch forms with `--help`.
- Existing affected regressions: 461 passed, 7 ignored. The separate public update
  metadata case and two SSH loopback cases also passed. No test cases were added.
- The old Claude `/plan` negative fixture now includes task text, matching the
  existing exact-only local command rule and retaining the submit barrier assertion.
- Official Windows Release: `target/experience/release-20261028-151807/pebrel.exe`.
  An isolated instance exposed one window through Runtime and closed normally.
- These checks do not establish live model generation or visual scrollbar acceptance.

## Supersedes

No previous decision is superseded; ordinary-shell Auto semantics are preserved.

## Revisit when

An agent exposes supported scrollback integration for a viewport it owns, or the
Codex CLI removes or changes the supported inline-mode flag.
