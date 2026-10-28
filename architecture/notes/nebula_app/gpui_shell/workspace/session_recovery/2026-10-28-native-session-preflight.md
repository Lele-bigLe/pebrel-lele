# Native session ownership before cold restore

## Status

Partially superseded by [preserving unidentified agents](2026-10-28-preserve-unidentified-agents.md).

Implemented; compiled/native acceptance is tracked separately from source review.

## Context

Restoring a layout and resuming an AI conversation are separate settings. A local
snapshot with AI resumption disabled legitimately created shells. Some older
snapshots also lacked a conversation ID. A directory match cannot establish which
conversation was active or whether another terminal now owns it.

## Evidence

- `session/recovery.rs` prepares and prunes a saved layout before creating PTYs.
- `platform/ai_session_identity/recovery.rs` inspects native ownership evidence.
- Codex keeps empty `thread-writer-locks/<id>.lock` files after release; existence
  alone cannot establish occupancy. A nonblocking OS lock distinguishes a live
  writer from an unused file without deleting either.
- Claude's process registry associates session IDs with PIDs. Live process identity
  is checked, including Windows creation time against the registry file, rather
  than trusting stale PID files.

## Decision

The workspace owns a frozen pending snapshot while the background preflight runs.
Checkpoints and shutdown capture that snapshot until the plan is applied, so a
temporary empty workspace cannot replace the saved session. A normal close waits
for planning; application teardown can still preserve the frozen snapshot.

Only exact identities with available native ownership evidence are automatically
resumed. Busy, duplicate or unverifiable AI leaves are skipped; adjacent splits
collapse and surviving focus indexes are remapped. Ordinary shell leaves remain.
The native writer lock is released before starting Codex: retaining it across
launch would block the very conversation being resumed.

## Rejected alternatives

- Treating lock-file existence as occupancy: stale empty files are normal.
- Deleting a lock or killing its owner: would interfere with another terminal.
- Resuming Claude's latest conversation by directory when its ID is missing:
  cannot satisfy exact-identity or occupancy requirements.
- Creating all PTYs before checking: leaves misleading empty shells behind.

## Consequences

The preflight is a snapshot, not an atomic claim against external terminals. The
provider retains final ownership authority. Remote/WSL or unsupported native
ownership evidence is reported as unverifiable and skipped during cold AI restore.
Update-ticket restoration remains a separate, existing handoff path.

## Validation

Source inspection and native metadata sampling establish the input contracts.
Build results and interactive acceptance must be reported by the implementing run;
this note does not assert native UI or concurrency acceptance.

## Supersedes

None.

## Revisit when

Providers expose a supported atomic resume/attach API, remote ownership inspection
is added, or an installation does not publish the native metadata used here.
