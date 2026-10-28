# Selective upstream Runtime maintenance

## Status
Ported production behavior; Release, existing regressions and native checks passed.

## Context
Upstream `54548685` is 80 first-parent commits ahead of local `4e4ea27d` (573
including merged branch history; 630 changed files). Local workbench, Git, quota
and session changes remain uncommitted. A full source replacement is unsuitable.

## Evidence
- `f30aa226` (#759): `to_writer(TcpStream, ...)` writes individual JSON fragments.
- `486f5cf1` (#721): a resident's old literal endpoint stays pinned in child env.
- `278a0650` (#760): shell dispatch wakes only on its 120 ms timer; its port is
  recorded beside the workspace owner.

## Decision
Buffer each JSON line once, append its existing newline and use `write_all` on
the original TcpStream interface. No generic test writer or additional protocol.

Hosted endpoint state records whether it publishes shared discovery. Its children
receive `resident` when shared, and the original explicit address/token when
private. The client resolves resident discovery anew; invalid explicit endpoints
still fail without selecting another instance. In-process requests keep their
hosted endpoint. Reuse the existing port-file owner lock, ACL and drop guard.

## Rejected alternatives
Whole-tree merge/replacement would mix unrelated features and local UI changes.
The large renderer/activity-cache and native-result workspace changes need their
own integration and real-agent acceptance. The sidebar release fix depends on
upstream tab capture code absent locally; importing it alone is incomplete.
Retrying an explicit private endpoint against a shared resident could misroute
control. Socket buffering is not an atomic-message guarantee.

## Consequences
Desktop and mobile responses retain their JSON bytes and framing with fewer
fragment writes. Shared clients follow endpoint rotation after restart; private
and legacy explicit clients keep their isolation contract. No settings schema,
provider configuration, dependency, branch or release-version change.

## Validation
Official Release and 92 existing Runtime tests passed. An old unsupported-launch
fixture named Gemini although the existing shared command contract supports it;
the fixture now uses unsupported Aider, preserving the same rejection assertion.

In two isolated native launches, a child confirmed resident mode, the same mode
resolved the second process after restart, an invalid explicit endpoint returned
`runtime_unavailable`, and a four-second command returned exit code 0 in 4.41 s.
Both owned instances acknowledged close and exited. Full-viewport reads confirmed
the child probe; a short bottom-of-buffer tail initially missed its top-screen
output. No command was resubmitted for that observation issue.

Source preimages and reports are under `tmp/qa-release-20261028-130459/`. The
checked artifact is `target/experience/release-20261028-141556/`, before the later
requested terminal-scrollbar and fork-update-channel work.

## Supersedes
None.

## Revisit when
The fork adopts a later complete rendering/session baseline, or Runtime discovery
and client compatibility requirements change.
