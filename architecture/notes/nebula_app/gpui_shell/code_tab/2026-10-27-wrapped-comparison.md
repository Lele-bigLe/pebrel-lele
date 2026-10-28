# Resizable, wrapped Git comparison

## Status
Windows GPUI debug build passed; native drag, scroll and text-layout acceptance pending.

## Context
The previous comparison used two fixed-height virtual lists and a decorative
one-pixel divider. It could neither resize the columns nor wrap long lines.
Wrapping the two lists independently would misalign old and new line numbers.

## Evidence
The supplied 2026-10-27 screenshot marks the inert divider and clipped long lines.
The existing GPUI dependencies provide ResizableState, variable-height ListState,
native text wrapping and horizontal ScrollHandles.

## Decision
Render each aligned before/after pair in one variable-height virtual-list row.
The taller wrapped side determines the height shared by both cells. Only visible
rows are laid out; syntax preparation and byte anchors keep their existing owner.
Native transparent resizable panels overlay only the full-height drag handle;
their authoritative sizes determine both text-column proportions. No custom
mouse-capture state machine or second diff parser is added.

Automatic wrapping is initially enabled and can be toggled in the diff toolbar.
Unwrapped columns keep independent horizontal controls. List remeasurement follows
width, font-size and wrap changes. DiffPosition retains the logical vertical
anchor, horizontal offsets, wrap flag and split ratio under the existing exact
patch-digest restoration guard. Changes to presentation never alter patch bytes.

## Rejected alternatives
- Independent wrapped list heights: loses line alignment and reliable y syncing.
- Expanding and shaping every source line on each resize: increases UI-thread
  work with total file length instead of the visible viewport.
- Replacing the diff backend/editor: unnecessary; existing selection, comment
  validation, raw preview and staging boundaries remain in use.

## Consequences
Vertical synchronization is structural rather than copying two scroll offsets.
The raw patch view remains the original code editor. No new dependency, durable
preference or Git write behavior is introduced.

## Validation
Read the native list/resizable APIs, formatted and reread affected source.
The 2026-10-27 product build caught an App TextSystem call to `shape_line`, which
belongs to WindowTextSystem. Pass the existing render Window through the diff
renderer and use its text-system cache for horizontal extent measurement.
The corrected product build and 484 related existing tests passed, including 3
diff-view tests. The new experience process exposed a responsive main window;
these checks do not establish native drag, wrapping or visual acceptance.

## Supersedes
Only fixed-height paired comparison rendering described by the earlier diff note.

## Revisit when
Real native feedback exposes resize hit-testing, wrapped-line alignment or scroll
restoration problems, or additional durable per-project display preferences are requested.
