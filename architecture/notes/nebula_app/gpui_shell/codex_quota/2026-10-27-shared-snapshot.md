# Shared official quota snapshot

## Status
Windows GPUI debug build passed on 2026-10-27; interaction acceptance pending.

Extended by [last successful data](2026-10-27-last-success-and-local-projection.md) for refresh failures and the Settings projection of local records.

## Context
The workspace queried Codex windows while Settings separately queried the full
report for capacity. Sharing a renderer did not share observation time, requests,
account invalidation or HTTP 429 backoff. Two surfaces could disagree temporarily.

## Evidence
`workspace/usage.rs` and `settings_pane/codex_usage/entitlements.rs` previously
called `ai_usage::read_codex` independently. The backend already returns the full
CodexReport and validates credentials; neither surface needs its own reader.

## Decision
One GPUI Codex Snapshot Entity owns the full report, observation time, current
request, generation and auth revision. A concrete application-global handle lets
all windows and Settings observe that same model; it is not a generic registry.
The existing workspace timer requests a freshness check every five seconds, with
a 60-second read interval and 300-second HTTP 429 backoff in the shared owner.
Opening a surface reuses a fresh result. Explicit refresh bypasses freshness but
not 429 backoff. Requests in progress are coalesced. The last observation stays
visible while refreshing, with its original timestamp.

Local-record collection remains workspace-owned and reports auth-file revision
changes to this model. Metadata observations carry monotonic capture times;
out-of-order local/credit results cannot roll the shared identity backwards.
Settings reports a credit read's auth observation before exposing its account
label, so a newly observed account cannot accompany the previous quota snapshot.
Auth changes invalidate the old report and request
generation. Start/end revision checks reject changed credentials during a read.
An invalidated blocking adapter read may finish in the background; its old UI task
cannot apply its result. Existing backend credential checks remain authoritative.

Settings still owns reset-credit reads, confirmation, redemption, durable receipt
and mutation lifetime. Its quota observer invalidates account-specific credit
state on revision change, while keeping an in-flight mutation owner. After a
redemption result, Settings invalidates and refreshes the shared quota model.

## Rejected alternatives
- Broadcasting cloned quota windows: preserves multiple mutable result owners.
- Backend process-wide cache of credentials: puts UI refresh/lifetime behavior
  into the adapter and broadens credential retention without a current need.
- A new general data-service framework: unnecessary for this one shared report.

## Consequences
Footer, flyout and Settings use the same official windows and observation time;
capacity remains a calibrated estimate in Settings. Local token/partial metrics
and native record budgets do not change. No dependency or persisted format changes.

## Validation
Initial source readback and local formatting were followed by a successful Windows
GPUI debug product build on 2026-10-27, logged in
`target/pebrel-lele-build-20261027-093914.log`. The matching experience executable
was launched and its main window was responsive. No automated tests, independent
live entitlement query, redemption or visual/interaction acceptance. Normal app
startup retains the existing automatic usage-refresh behavior.

## Supersedes
Independent Settings/workspace official-report request ownership; existing
entitlement mutation and local-record ownership contracts remain unchanged.

## Revisit when
The backend offers account-scoped streaming updates or additional quota providers.
