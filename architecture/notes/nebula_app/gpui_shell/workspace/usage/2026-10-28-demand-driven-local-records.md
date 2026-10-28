# Demand-driven local usage reads

## Status
Implemented and checked with isolated optimized-build samples.

## Context
The sidebar now displays official quota only. Native local usage is consumed by
the usage/settings surfaces and identified AI sessions. An ordinary idle Shell
does not display these records, but the five-second poll still backfilled them.

## Evidence
An isolated, visible Release instance with one idle Shell and closed usage/settings
read 80.81 MiB in 30.09 seconds. CPU was 8.78% of one logical core, or 0.73% of the
12-logical-processor host. This is a local sample, not a general performance bound.
Artifacts are under `tmp/qa-release-20261028-130459/`.

## Decision
Keep the existing poll, worker owner and shared quota snapshot. Only call the
native record reader when usage/settings is open or a local AI session requests
metrics. Opening Settings requests a read immediately. Retain recorded timestamps
when no records were read; a cheap authentication observation is not a statistics
refresh.

## Rejected alternatives
Stopping the entire poll would remove the cheap authentication revision check that
invalidates a stale quota after CC Switch changes accounts. Increasing the interval
would delay live AI metrics. A second scanner/cache is unnecessary.

## Consequences
An idle Shell avoids history backfill. Opening a consumer can show its existing
loading state until the requested read completes. AI metrics retain their existing
five-second cadence and official quota keeps its shared refresh/backoff rules.

## Validation
Both Release builds used the official Windows product script. In matching
30-second idle-Shell samples, process read traffic fell from 80.81 MiB to below
0.01 MiB and host-normalized CPU from 0.73% to 0.38%. Main-process private memory
was 122.3 MiB before and 116.0 MiB after; working-set residency varied and is not
used as evidence of a memory reduction. These are local samples, not guarantees.

Four panes concurrently consumed 19,457,504 fixture bytes in 5.53–6.12 seconds,
all returning exit code 0. Their observed history stayed at or below 10,000 lines.
A subsequent 30-second idle sample used 0.77% host-normalized CPU and ended at
223.3 MiB main-process private memory, excluding child shells. Existing navigation,
restore, Git, quota and runtime regressions passed. Native button/visual acceptance
and real model-generation metrics remain separate from these checks.

## Supersedes
None.

## Revisit when
Background consumers need local records independently of these surfaces, or
measurements show that identified AI sessions need a narrower native read path.
