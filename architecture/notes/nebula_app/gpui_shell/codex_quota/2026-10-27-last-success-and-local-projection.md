# Last successful quota data and local record projection

## Status
Windows GPUI debug build passed on 2026-10-27; full native acceptance pending.

## Context
A failed refresh replaced the successful shared report and advanced its timestamp.
Retaining old values without a failure marker would also misrepresent freshness.
Moving local details into Settings must not introduce another record scanner.

## Evidence
`codex_quota.rs` is already the application-global owner of report/task/revision.
`workspace/usage.rs` owns native local records, period, partial state and metrics.
`settings_pane/codex_usage/entitlements.rs` owns redemption and its durable receipt.

## Decision
The shared quota owner keeps an optional failure alongside its last successful
report. Ordinary read failure leaves successful values/time intact and exposes
the failure. Success replaces values/time and clears the failure. No-success
failure remains an error with no successful time. Account revision changes and
authentication failures clear old values rather than present another identity's
data. HTTP 429 backoff examines the retained failure as well as initial errors;
explicit refresh still cannot bypass the backoff.

Footer, flyout and Settings project that same report. Stale data is labelled and
its last success remains visible; the UI never substitutes zero usage. Capacity
continues to use the report's existing account/model/window sampling contract.

Settings observes its owning workspace through a weak UI reference and borrows
the existing local-record snapshot. Period changes return as a Settings event to
the workspace owner, which rejects switches during its current scan. There is no
second scanner, statistics database, mutable record copy or new global service.
Settings gets the reference after construction, without reading a leased parent.
Closing the window releases subscriptions; the weak reference cannot keep it alive.
Records remain separate from official percentage usage, including partial markers.

Credit count and nearest expiry remain visible; card details are expanded for
redemption. Pending redemption/retry and incomplete-card warnings remain visible
even while details are collapsed. Original confirmation and mutation ownership stay.

## Rejected alternatives
- Replace errors with empty windows/zero: invents account usage.
- Advance the success timestamp on failure: labels stale values as new data.
- Keep data across account/auth changes: risks mismatched identity and quota.
- Scan local records from Settings: duplicates I/O and creates conflicting periods.

## Consequences
The flyout is an official summary leading to Settings. Detailed local tokens are
available on demand, without cluttering the footer or adding another data owner.
No dependency, persisted format or real reset operation is added by this change.

## Validation
The `132844` Windows product build passed. Related tests passed 344 cases with 10
existing conditional skips, including isolated quota-flyout controls and Settings
navigation. The packaged application started with a responsive main window.
No independent live quota, reset-credit redemption or prototype visual acceptance
was performed; normal app startup retains automatic refresh behavior. Failure/
backoff, auth races, cross-window consistency and record expansion still require
native acceptance. Artifact/log details live in the workbench plan.

## Supersedes
The error-replaces-report timestamp behavior of [shared snapshot](2026-10-27-shared-snapshot.md).
Shared request ownership, generation protection and redemption separation remain.

## Revisit when
Local records need independent application-level lifetime or additional quota providers.
