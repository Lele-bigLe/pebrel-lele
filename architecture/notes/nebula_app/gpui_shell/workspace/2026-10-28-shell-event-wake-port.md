# Wake the shell event dispatcher on arrival

## Status
Ported from upstream merge `278a0650` (#760); Release and native checks passed.

## Context
The initial workspace owns the process-wide shell receiver. The dispatcher handles
desktop/phone Runtime requests and publishes periodic snapshots for all windows.
Waiting only for its 120 ms tick adds a full tick to each serial control request.

## Evidence
The local `start_shell_event_pump` drained the standard receiver only after its
timer. The initial workspace is the sole caller with a nonempty receiver. Upstream
#760 preserves the dispatcher while making arrival wake the async task.

## Decision
Port the standard-receiver to futures-channel forwarder. It owns no GPUI/window
references and lives with the process receiver. The existing GPUI task dispatches
on event arrival or the original idle tick. A closed async stream falls back to
the timer so it cannot spin. No new polling interval or dispatch implementation.

## Rejected alternatives
Shortening the poll increases idle wakeups. Moving UI dispatch onto the receiver
thread violates GPUI ownership. Replacing the entire upstream workspace would
overwrite unrelated local workbench and session behavior.

## Consequences
Queued controls no longer wait solely for the next tick. Snapshot publication and
the existing process-level routing remain intact. The forwarder ends on sender
closure or on a failed send after its async consumer closes; it is process-owned.

## Validation
Official Release build and 92 existing Runtime regressions passed. Two successive
isolated native launches accepted controls, closed normally, and followed the same
resident discovery mode across restart. A four-second native command returned its
actual exit code after 4.41 seconds. This is not a frame-rate or visual guarantee.

## Supersedes
None.

## Revisit when
Shell event producers can use the async channel directly, or the process lifetime
and dispatcher owner change.
