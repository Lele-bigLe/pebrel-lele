# Continuous native repository review

## Status
Windows GPUI debug build passed on 2026-10-27; native workflow/visual acceptance pending.

## Context
The approved reference has session/change navigation and one preview position.
Reusing an editor before detaching its comments would lose drafts. The old queue
required reopening every detached file and then opened a second recipient dialog.

## Evidence
`workspace/workbench.rs` derives the review route from the existing SidePanel.
`workspace/documents.rs` already reuses a matching native diff tab. Native removal
is shared with transfer/reorder, so the retention hook cannot live in that helper.
`code_tab/review.rs` anchors comments to exact patch digests and patch byte ranges.

## Decision
The local project-sidebar workbench uses the existing SidePanel for Git visibility,
snapshots, polling and writes. It moves that same Git body to the left in review
mode. Other contexts retain their native drawer/navigation; Files keeps its drawer.

A native TabId identifies one reusable preview. Before replacing it, detach only
the existing text payload and cache bounded viewport/expansion data. Release the
old editor/subscription/tasks and insert the new view with native tab helpers and
a new runtime identity. Named/colored independent tabs are not commandeered.
Restore viewport data only after a matching patch digest. Observe active-tab
transitions and explicit activation so returning to a terminal exits review;
returning to review reopens a same-project preview without replacing the task.

Open editors and detached records remain the sole owners of comment content.
The queue projects both, exposes individual comments, and uses original ranges
for location only while digests match. An unadded input is never batch feedback.
Known-current detached comments can enter validation directly; known-stale ones
still require review. Every preview/send rereads each patch on the worker, then
compares the complete sorted batch against the live owners before applying it.
Send additionally checks the displayed preview at click time, binds the selected
recipient, and delivers the exact previewed message. The dialog owns its task;
closing it cancels UI delivery. No successful send clears source drafts.

Quotes and review feedback share one native recipient builder and delivery guard.
Remote targets retain their badge and explicit-user origin; new CLIs retain their
manual pending-send flow. No timer enters content into login/trust prompts.
Child dialog construction receives sources and picker from the leased workspace;
it never reads that workspace in its constructor.

## Rejected alternatives
- Keep one editor per visited file: retains patches, syntax models and tasks.
- Introduce a second repository comment store: duplicates counts and stale state.
- Restore comments solely by displayed row: row numbers are presentation data.
- Send a newly collected batch without checking the displayed preview: may send
  comments the user has not reviewed after a concurrent change.

## Consequences
Drafts remain window-scoped, bounded at 64 detached records / 4 MiB. No text is
silently evicted at the limit; a blocked replacement keeps the old preview.
Viewport-only cache eviction is safe and capped at 64 files. Reopening retains
original project membership. Existing Git queues, exact hunk validation, editable
file save guards and window/transfer boundaries remain authoritative.

## Validation
Source checks were followed by the successful `132844` Windows product build.
Related workspace/settings/widget/usage/Git suites passed 344 tests with 10 existing
conditional skips. Tests used isolated configuration/user directories and Git
fixtures. The packaged application started with a responsive main window. See
`plans/2026-10-26-workbench-reorganization.md` for logs and artifact identity.
This is not full-suite or prototype/workflow acceptance. Native geometry, focus,
scrolling, concurrent invalidation, target lifecycle and actual delivery remain
product acceptance items; no actual Agent feedback was sent by this validation.

## Supersedes
The close-only detachment trigger and mandatory reopen-before-batch portion of
[window review retention](../../code_tab/2026-10-27-window-review-retention.md).
Its text budget, window lifetime and non-destructive transfer contracts remain.

## Revisit when
Restart recovery, cross-window drafts, or multiple pinned review workspaces are required.
