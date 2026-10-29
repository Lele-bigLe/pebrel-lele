# Codex turn change viewing

## Status

Implemented. Native UI acceptance is separate from build and schema checks.

## Context

The user requested read-only changes for each Codex turn, without undo or editor
extension hosting.

## Evidence

`ai_sessions/changes.rs` reads completed native file changes; the shared
`ai_hook/integrations/codex_rpc.rs` transport performs bounded App Server reads.

## Decision

The terminal footer captures one local Codex native thread identity. Its dialog
owns turn pagination, selected-turn item pagination and frozen patch views. It
only calls `thread/turns/list` and `thread/items/list`; reading never resumes a
thread, takes its writer lock or starts a model turn. WSL/SSH are not mapped to
the host's session store.

`ai_hook::integrations::codex_rpc` owns executable discovery, the bounded JSONL
transport and the short-lived child process. Quota operations retain their
existing startup overrides and errors through their adapter. History uses the
configured provider and a bounded larger response allowance. One operation has
a 25-second deadline; pagination can continue explicitly. Closing a dialog
drops its UI task; any already-started background read remains deadline-bound.

Native completed `fileChange` items are the source of edits. Shell/external writes
without native patches are not attributed from the current Git working tree.
Files with repeated edits keep their chronological steps, rather than treating
concatenated intermediate patches as a net diff. Add/delete native content is
converted to hunks for the shared renderer. Missing context stays omitted.

Historical diffs reuse the existing comparison UI but have an explicit read-only
source flag and no live Git/review snapshot token. Live reload, staging and
review entry points reject historical views. Later working-tree changes cannot
replace their captured patch. No undo or extension hosting is introduced.

## Rejected alternatives

- Inferring historical edits from the current Git patch: it is a different snapshot.
- Resuming a thread to read it: viewing must not claim its writer lock.

## Consequences

Only native persisted patches are available. Missing patches are not fabricated.
Protocol errors remain visible and cannot become an empty successful turn.

## Validation

The initial implementation checked native pagination and patch fields against the
installed Codex 0.165.0 and was included in Windows product builds. These facts
do not establish every native UI interaction or future protocol compatibility.

## Supersedes

None.

## Revisit when

Codex changes its persisted patch schema, read-only paging contract or native
file-change coverage.
