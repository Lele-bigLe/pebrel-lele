# Deferred native conversation continuation

## Status

Implemented; native UI acceptance remains separate from compilation.

## Context

The user chose to retain Codex and Claude conversations in the sidebar and
continue them explicitly, rather than launching a provider's resume chooser.
Turn metrics also remained unavailable when native conversation identity was lost.

## Evidence

The old recovery path invoked the native chooser for missing IDs. Codex's owned
rollout parser accepted only `source=cli`, while recent local user records retain
`source=vscode`, including records with native output and first-token timing.

## Decision

Persist the existing AgentSession identity and mark cold Codex/Claude panes
deferred. Continue performs an asynchronous occupancy check before the existing
exact-ID command. Missing IDs use the existing Pebrel history picker, targeting
the same pane. A failed resume retains the saved target without an automatic CLI
chooser. Dormant sessions do not schedule usage scans.

Accept CLI and IDE-origin user rollouts only behind the existing verified
process-owner and unambiguous rollout-ID checks. Keep the last completed turn
metric until the next turn emits usage. Native first-token timing is not inferred
from file timestamps; in-progress speed retains its estimated turn-average label.

## Rejected alternatives

- Starting `codex resume` without an ID opens an unwanted native chooser.
- Guessing from the newest project transcript can select another terminal's work.
- Synthesizing first-token latency or empty successful metrics would hide missing data.

## Consequences

Restart does not start saved Codex/Claude agents. Other tools keep their existing
recovery policy. Occupied or unverified targets remain in the sidebar. No undo,
process takeover or separate conversation database is introduced.

## Validation

Readonly native record inspection confirmed IDE-origin user metadata and native
completion timing fields. Existing recovery, identity and usage regressions and
the Windows build check the implementation. No resident runtime was reachable
during diagnosis, so live model streaming and restart UI require user acceptance.

## Supersedes

The automatic native chooser policy in
[preserving unidentified agents](../gpui_shell/workspace/session_recovery/2026-10-28-preserve-unidentified-agents.md)
and cold Codex/Claude launch timing in
[native session preflight](../gpui_shell/workspace/session_recovery/2026-10-28-native-session-preflight.md).

## Revisit when

Provider resume commands, rollout metadata or occupancy ownership contracts change.
