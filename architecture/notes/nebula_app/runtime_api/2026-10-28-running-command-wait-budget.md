# Preserve the execution wait after CommandStart

## Status
Implemented and checked with existing regressions and native commands.

## Context
The phased run waiter uses a short grace period to detect submitted commands that
never reach CommandStart. The caller separately provides the full execution wait.

## Evidence
An owned Release terminal received a 4,864,376-byte ANSI/CJK output fixture through
`pane.run` with a 30-second timeout. The response reported a started-phase timeout
after about three seconds. A subsequent native snapshot reported the same run as
finished with exit code 0 and supported exit-code capability.

## Decision
When the first grace wait times out after CommandStart, wait the remaining caller
budget for the same run. Do not inspect quiet-screen evidence or resubmit input.
Commands completed during the grace period and true execution deadlines retain
their existing results. Submitted-without-start detection remains unchanged.

## Rejected alternatives
Increasing the global grace would delay reporting blocked submissions. Resending
the command could duplicate an already running operation. Treating a timeout as
success would lose the real exit status.

## Consequences
Commands lasting longer than the start grace can complete within the caller's
budget. The terminal is left untouched when the deadline expires.

## Validation
The four existing run-related runtime tests passed, including swallowed-submission
detection. In an isolated rebuilt Release, two commands containing a four-second
sleep completed in 4.33 seconds each and returned their actual exit codes 0 and 7.
Four concurrent 4,864,376-byte output commands also completed normally after
5.53–6.12 seconds. Reports are under `tmp/qa-release-20261028-130459/`.

## Supersedes
None.

## Revisit when
Run phases or the meaning of the caller's execution timeout change.
