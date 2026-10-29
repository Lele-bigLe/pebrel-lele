# Preserve AI workspace panes without guessing conversations

## Status

Launch and selector policy is superseded by [explicit continuation](../../../ai_sessions/2026-10-29-deferred-session-continuation.md).

Implemented on 2026-10-28. Existing affected regressions passed; live agent
restoration remains a separate acceptance step.

## Context

The user wants the workspace saved on exit and restored on startup, including
Codex and Claude Code panes that have not acquired a native conversation ID.
The user also chose Codex's own display mode instead of forced terminal
scrollback or an additional fullscreen scrollbar.

## Evidence

- A current local snapshot contained ordinary shells and a Codex pane without an
  ID. The cold planner discarded the latter before creating its PTY.
- AI resumption was enabled in that profile, while Codex and Claude hooks were
  disabled. Hook installation is therefore not a prerequisite for saving a pane.
- `session/recovery.rs` already distinguishes free, busy and unverifiable native
  ownership. Failure to prove ownership does not prove a conversation is busy.
- Installed Codex 0.165.0 accepts `--no-daemon` on start, resume and fork. Its
  dedicated backend fits the Windows identity probe's verified process subtree.
- Native selectors are `codex resume` and `claude --resume`; neither requires
  guessing a conversation from the working directory.

## Decision

Reuse the existing workspace snapshot and frozen startup plan. Preserve local
Codex and Claude panes without IDs and launch their native selectors. An exact
ID with free ownership resumes automatically; confirmed busy or duplicate
conversations remain excluded. When ownership cannot be verified, preserve the
pane and its exact saved target but require native selection for this launch.
Retries retain that requirement. Selection acknowledgements must match the
saved provider before replacing the target.

Normal shutdown still attempts native identification first. A supported selector
is a durable recovery path, so an otherwise unidentified Codex or Claude pane
does not require the extra missing-ID warning. Unsupported identities retain the
existing warning. Save failures and unsaved-document checks remain authoritative.

Managed Codex commands use `--no-daemon` and no display override. Windows and
macOS native Codex probes use the verified shell PID even when Runtime execution
context is absent. Linux and WSL retain their existing execution-context checks.
No global CLI settings or hook opt-outs are changed.

## Rejected alternatives

- Dropping every unidentified pane: loses the tool while preserving only shells.
- Resuming the most recent conversation by directory: can attach the wrong one.
- Resuming an exact ID despite unknown ownership: can conflict with another CLI.
- Enabling hooks silently: overrides the profile's explicit configuration.
- Simulating a scrollbar for a CLI-owned viewport: has no reliable scroll range.

## Consequences

A pane can require a user choice after restart; its layout and provider survive.
The dedicated Codex backend no longer reuses a shared daemon; resource impact has
not been measured. Native ownership remains a preflight rather than an atomic
claim. Remote and unsupported ownership rules are unchanged. Snapshot schema v4
is unchanged, and no raw launch commands or conversation bodies are persisted.

## Validation

- Existing affected regressions: 453 passed, 0 failed, 6 ignored. No cases added.
- Installed CLI help accepted the generated Codex forms and Claude selector.
- Locale parsing, architecture checks and diff whitespace checks passed.
- Official Windows Release: `target/experience/release-20261028-162709/pebrel.exe`,
  with its matching hook; both copies match their build artifacts by SHA-256.
- Logs: `target/pebrel-lele-workspace-recovery-20261028-release.log` and
  `target/pebrel-lele-workspace-recovery-20261028-regression-final.log`.
- These checks do not establish a live AI conversation restart or concurrent
  ownership acceptance in the user's workspace.

## Supersedes

- [Native preflight](2026-10-28-native-session-preflight.md): only the policy of
  dropping unidentified local Codex/Claude panes and unverifiable ownership.
- [Managed Codex scrollback](../../../ai_agents/2026-10-28-managed-codex-scrollback.md):
  forced inline mode and agent-specific Auto scrollbar visibility were canceled
  by the user.

## Revisit when

Providers expose an atomic attach API, shared-daemon ownership becomes directly
queryable, or native selector and backend CLI contracts change.
