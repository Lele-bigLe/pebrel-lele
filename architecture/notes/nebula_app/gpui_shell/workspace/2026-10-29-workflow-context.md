# Workspace navigation, repository drafts and turn review ownership

## Status

Implemented locally. Native interaction acceptance remains pending.

## Context

Existing project, terminal, Git and turn-history capabilities had separate entry
points. Finding another project's session required changing filters; the commit
input carried one draft between repositories; turn patches opened in a modal.

## Evidence

- `workspace/project_tree/overview.rs` projects the existing tab/pane identities.
- `workspace/projects/navigation.rs` validates the return target against live panes.
- `workspace/vcs_panel/commit_input.rs` owns repository drafts and commit receipts.
- `workspace/agents/changes.rs` owns immutable conversation history targets.
- `code_tab/diff/historical.rs` renders recorded patches without issuing a live
  Git staging or annotation token.

## Decision

- Keep the project library and native tabs as the authority. The all-project view
  flattens their tree for a virtual list; documents remain a separate section.
  Its ordering is stable rather than following the recent-project menu.
- A window-local return target stores a live tab ID and pane ID. Notification
  jumps use that same navigation path. Closed targets cannot address a new tab
  occupying the old index. This is navigation history, not session persistence.
- Retain commit drafts per execution location within the window. The commit
  receipt clears only the matching original draft; switching repositories or
  editing the message cannot clear an unrelated draft. Preserve SVN support.
- Derive the main Git action from the current repository snapshot. Clean branches
  offer publication, push or fast-forward pull as appropriate. Fetch is explicit;
  an idle local tracking snapshot does not claim fresh knowledge of the remote.
- The workbench owns at most one cached turn-review view, bound to tab, pane,
  conversation ID and root. Returning to the same target retains its selection.
  Switching tabs or choosing Git leaves the review route. Review owns keyboard
  focus; closing the displayed review returns to the source terminal.
- Continue reading native Codex patches. Running turns, absent patches, unknown
  states and read failures remain distinct. No checkpoint store or fallback to
  current working-tree differences is introduced.

## Rejected alternatives

- A second project/session database or a new notification center duplicates
  existing lifecycle owners.
- Automatic Git synchronization could introduce an unexpected merge or push.
- Replacing historical patches with current Git differences would misattribute
  later edits. Recording new checkpoints requires a separate turn-boundary and
  concurrent-write design.
- Hosting turn history as a new persistent tab kind would change snapshot and
  runtime contracts for a window-local read-only surface.

## Consequences

No new dependencies, credential stores or persisted session fields. Focus return,
commit drafts and the single cached review last only for the window lifetime.
Historical coverage still depends on native records. CC Switch continues owning
provider configuration, routing, MCP and skills.

## Validation

Source ownership and diff reviewed; English and Chinese catalogs parsed.
Windows product and installer build results are recorded in project memory.
No live Git mutation or native visual acceptance is implied by a product build.

## Supersedes

None. This changes presentation ownership without changing native history data.

## Revisit when

Cross-window focus return, persistent commit drafts, multiple cached histories or
checkpoint capture becomes an explicitly accepted requirement.
