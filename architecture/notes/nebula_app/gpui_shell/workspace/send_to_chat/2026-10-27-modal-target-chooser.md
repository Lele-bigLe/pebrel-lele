# Modal Agent target chooser

## Status
Implemented in source; native layout and keyboard acceptance pending.

## Context
The repository feedback dialog showed Agent options behind its own surface.
Giving the trigger a height does not fix this paint-order conflict.

## Evidence
The workspace defers its modal layer at priority 10, after theme overlays.
The pinned component Select defers its menu at priority 1. GPUI prepaints nested
deferred draws but globally sorts them by priority when painting. The supplied
2026-10-27 screenshot shows menu rows visible only below the dialog.

## Decision
The existing ChatPicker renders a bounded, in-flow option list in both feedback
and quoted-text dialogs. Its native SelectState remains the sole selected-target
authority. Existing target discovery, Codex default, remote badges, search and
delivery guards are retained. The list closes after selection or Escape; focus
returns to the chooser. Its query and expansion live only with the dialog picker.
Feedback uses DialogContent with bounded scrolling instead of an unconstrained
percentage-height body. A prepared feedback preview appears only after validation.

## Rejected alternatives
- Raising trigger height: does not change the menu's deferred priority.
- Lowering the global modal layer: would put existing theme overlays above dialogs.
- Editing the Cargo dependency cache or adding another popup framework: creates
  an untracked fix or a new dependency for a local presentation problem.

## Consequences
Opening options takes space within the dialog. Scroll bounds keep the choices
and form reachable without extending an occluded popup beyond the dialog.
Selection still does not authorize delivery; the existing explicit send path does.

## Validation
Read current host and pinned component paint ordering; formatted and reread source.
No new build, tests, real Agent delivery or native visual acceptance in this pass.

## Supersedes
Only the Select presentation in these two ChatPicker dialogs.

## Revisit when
The supported component API permits modal-relative menu layering, or the host's
global layer policy changes. Other unrelated Select dialogs are outside this fix.
