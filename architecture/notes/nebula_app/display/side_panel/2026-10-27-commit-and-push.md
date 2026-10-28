# Commit actions and draft ownership

## Status
Built on Windows and related existing regressions passed on 2026-10-27.
Native visual and end-to-end commit/push acceptance remain pending.

## Context
The source-control panel scattered batch actions beside a single-line commit
field. GPUI cleared its draft on submission, including when a command failed.
The user requested a compact VS Code-style daily source-control flow.

## Evidence
SidePanel already owns the mutation slot, worker thread, captured working directory,
WSL command prefix, operation notice, and refresh flag. CommitInput owns the GPUI
draft; VcsList derives virtual rows from the existing GitInfo snapshot.

## Decision
Keep two explicit actions: commit the index, or commit the index and then push.
The main button and Ctrl/Cmd+Enter commit only; Enter can insert a new line.
Do not implicitly stage files, guess a remote, rewrite a commit, or force a push.

Extend the existing worker to run a command sequence under one mutation slot.
Every step uses the same captured program/root; a failure stops the sequence.
Single-command callers keep their existing entry points and execution path.

The worker's first-command ticket reports the actual commit exit result. GPUI
retains the message until this success is observed, the GitLocation still matches,
and the original draft has not been edited. A failed push receives a typed notice
that distinguishes it from a failed commit. The commit is not automatically retried.
The pending ticket belongs to CommitInput and is dropped with the view; no new
poller, configuration file, or persistent operation store is introduced.

The form and batch/menu actions check their originating repository before writing.
VcsList owns only the in-memory collapsed sections and projected headings/counts.
Bulk staging stays with the corresponding section; other remote actions stay in
the existing tools menu. SVN retains its own mutation semantics.

## Rejected alternatives
- A second UI command runner: would bypass the existing mutation slot and notices.
- Clearing a draft on queue acceptance: loses input on failure.
- Adding amend/sync or implicit staging to copy every reference menu item:
  expands Git semantics beyond this focused daily workflow.

## Consequences
Push failure does not create a false commit failure or require retyping a committed
message. Unstaged edits remain outside the commit unless the user stages them.
Git's own remote/authentication/upstream errors remain visible.

## Validation
Read the worker, callers, form/subscription, headings and snapshot projection.
Edited Rust modules and both locale catalogs parse. On 2026-10-27, the official
locked Windows debug product build and GPUI test-support test compilation passed,
along with the architecture and tracked diff whitespace checks.

Related existing suites passed: 531 tests, zero failures, and 12 existing ignored
tests. These include workspace, shared SidePanel, notice, and Git Runtime API
regressions. No tests were added. Tests used isolated profiles and temporary paths;
no commit or push was performed on the user's repository.

Results: `target/qa-20261027-161113/all-results.json`. The hash-checked application
and Hook are in `target/experience/build-20261027-161113/`. The application was not
launched in this build task; native layout and the complete commit/push interaction
still need experience acceptance.

## Supersedes
Only GPUI's clear-on-submit behavior and the Git panel's scattered action strip.

## Revisit when
Native feedback requires further commit actions, draft persistence per repository,
or cancellation of the underlying VCS worker.
