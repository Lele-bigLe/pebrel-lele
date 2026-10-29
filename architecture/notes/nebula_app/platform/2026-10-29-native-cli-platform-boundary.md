# Native CLI platform ownership

## Status

Implemented.

## Context

Native Codex package discovery, PowerShell launchers and Windows project path
identity introduced six platform conditionals outside the platform owner.

## Evidence

`check_platform_cfg.py` reported 465 sites against the unchanged 459 budget.
The new sites were in `codex_rpc.rs`, `updates.rs` and `project_library.rs`.

## Decision

Keep CLI install layout and launcher conventions in `platform/cli.rs`, local
path identity in `platform/local_paths.rs`, and use the existing hidden-command
adapter through Tokio's `as_std_mut`. Protocol and update policy stay with their
existing integration owners. Platform code depends only on the standard library.

## Rejected alternatives

- Raising or excluding the budget would retain the ownership problem.
- A second process abstraction is unnecessary with the existing adapter.

## Consequences

The budget remains 459. Executable search, quoting, package prefix selection and
local versus guest path comparison preserve their existing semantics.

## Validation

The platform guard reports 459 sites. Product compilation and the existing
architecture contracts verify callers; package installation remains a separate
acceptance step.

## Supersedes

None.

## Revisit when

Native CLI package layouts change or a platform requires another launcher host.
