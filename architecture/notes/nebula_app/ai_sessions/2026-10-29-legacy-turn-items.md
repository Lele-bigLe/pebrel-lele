# Read-only compatibility for older Codex item stores

## Status

Implemented; native UI acceptance remains separate from read-only protocol evidence.

## Context

The user reported repeated failures opening per-turn changes. The original reader
required item pagination for every persisted conversation.

## Evidence

A metadata-only inspection of 20 local conversations on Codex 0.165.0 found one
whose turn list succeeded but `thread/items/list` returned RPC -32601,
`thread/items/list is not supported yet`. `thread/read` with `includeTurns: true`
succeeded for that same conversation. Other inspected conversations supported
item pagination. No thread was resumed and no model turn was started.

## Decision

Keep pagination as the normal path. Only an unsupported first item page may use
the bounded legacy read. Verify the returned thread ID and select the exact turn
ID before reading completed file changes. Keep the existing response, edit-count,
patch-byte and operation-deadline bounds; an oversized history remains an error.
Do not mix a partially paged result with legacy items or replace errors with an
empty successful list. The dialog owns the task and the captured thread identity.

The transport retains a numeric RPC rejection code for diagnostics; it does not
expose raw provider responses, credentials or conversation content in the error.

## Rejected alternatives

- Resuming the thread to load history: viewing must not take its writer identity.
- Using the present Git diff: current worktree changes are not historical edits.
- Reading all history for every conversation: pagination bounds the usual cost.

## Consequences

Older stores can provide their native patches without a model invocation. Large
legacy histories can still exceed the explicit limit. Missing native patches are
still shown as missing, including writes performed by external tools.

## Validation

Read-only native RPC inspection established the compatibility gap and legacy read
availability. Build results and native UI acceptance are reported with each package.

## Supersedes

The pagination-only item-read decision in
[the original turn-change note](2026-10-28-codex-turn-changes.md).

## Revisit when

Codex removes the legacy read or all supported stores implement item pagination.
