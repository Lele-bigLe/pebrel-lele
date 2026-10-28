# Window-lifetime review drafts

## Status
Windows GPUI debug build passed on 2026-10-27; interaction acceptance pending.

Partially superseded by [continuous review](../workspace/workbench/2026-10-27-continuous-review.md): preview replacement also detaches drafts, and known-current detached drafts may be validated without reopening.

## Context
Review comments and unadded text lived in each diff editor. Closing the editor
discarded them, even when the repository review dialog was the user's next step.

## Evidence
`workspace::finish_close_tab` removes and releases a CodeTab. `remove_tab_at` is
also used by reordering and cross-window operations, so archiving there would
duplicate content during moves. The review queue previously projected open tabs.

## Decision
On actual close, detach only review text, exact Git location/path/staged scope,
comment anchors, patch digest and comment IDs into the window's ReviewBatch.
The workspace also keeps the original project membership for queue-driven reopen.
Retain neither the editor Entity nor its background tasks, patch text or syntax
trees. Open tabs remain their sole content owner; reopening moves the detached
payload back into the editor and reloads the patch through the original reader.

The window keeps at most 64 detached records and 4 MiB of textual payload.
Over-budget close is refused visibly; no draft is silently evicted. A final diff
tab closes into the repository queue rather than closing the window. Moving the
last terminal also keeps a source window containing detached drafts alive.

The queue labels closed drafts. Added comments in closed drafts block batch
preview until reopened and checked; unadded text remains excluded. Restored
unadded text requires a new line selection. Existing patch-digest validation,
asynchronous draft equality checks, size limits and agent selection still apply.
Conflicting live content is never overwritten; detached records have window-local
IDs so independently created drafts for the same file can coexist safely.

## Rejected alternatives
- Keeping closed CodeTab Entities: retains editors, syntax state and tasks.
- Archiving from shared removal: duplicates or misattributes drafts during moves.
- Durable storage: not required by the current window-close scope; would need
  separate privacy, backup, conflict and migration contracts.

## Consequences
Drafts survive tab close within this window, not application restart/window exit.
The queue states that boundary. Existing settings and file editor save guards stay
in place. No new dependency, persistence file or Git write is introduced.

## Validation
Initial source readback and local formatting were followed by a successful Windows
GPUI debug product build on 2026-10-27. The build exposed a borrow conflict in the
stale-file warning: formatting the owned message before calling the mutable toast
API fixed it. Log: `target/pebrel-lele-build-20261027-093914.log`.
The matching experience package was launched and a responsive main window was
observed. No automated tests, real Git writes or visual/interaction acceptance.

## Supersedes
The open-diff-tab-only lifetime described by the earlier review implementation;
its snapshot anchoring and send validation remain authoritative.

## Revisit when
Users need restart recovery or cross-window repository-wide draft synchronization.
