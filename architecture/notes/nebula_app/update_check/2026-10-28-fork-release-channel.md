# Fork-owned application update channel

## Status

Implemented on 2026-10-28; Windows product build, existing regressions and live
fork metadata checks passed.

## Context

The customized Pebrel executable must discover and install its own releases rather
than replace itself with an upstream build. The fork currently has no Releases.

## Evidence

- The checkout origin is `Lele-bigLe/pebrel-lele`; upstream is `Kuddev/pebrel`.
- Discovery used upstream REST, public latest-release redirects and SHA256SUMS.
- Download validation also accepted the historical `Kuddev/nebula` repository.
- GitHub currently redirects the fork's `/releases/latest` to `/releases` (HTTP 200).
- Cached downloads and failed handoffs already call the shared asset validator.

## Decision

REST discovery, the user-facing Releases page, fallback tag discovery and checksum
fetches use `Lele-bigLe/pebrel-lele`. Asset generation and download validation share
one fork download prefix. Old upstream origins are rejected, including when cache
or failed-handoff metadata is restored. Prompt suppression uses a fork-specific
state filename without deleting or migrating existing files.

`check_now` returns `Ok(None)` when there is no published release. REST 404 and a
fallback redirect to the exact channel Releases list are normal absence states;
the settings page displays a typed localized message. Network and malformed
metadata failures remain errors.

## Rejected alternatives

- Changing only the visible link would retain upstream discovery and downloads.
- Keeping upstream origins trusted could install a build without custom changes.
- A fabricated latest version would incorrectly report an unpublished fork as current.
- Falling back to upstream would cross release-channel ownership.

## Consequences

Future self-updates require a published fork Release and existing verified native
installer contracts. Asset naming compatibility, exact version matching, HTTPS,
size, executable type and SHA-256 checks remain in force. Git upstream remotes and
Codex/Claude Code CLI vendor update sources are unaffected. No Release is published.

## Validation

- Existing update-check regressions: 20 passed; download validation: 13 passed.
- The existing public-network case was explicitly run: primary discovery and the
  independent fallback both returned no published release for this fork.
- Full affected selection: 461 passed, 7 ignored; the public-network case was one
  of those ignored cases and passed separately. No test cases were added.
- Both locale JSON catalogs parsed with matching update keys. Architecture and
  changed-source whitespace checks passed.
- Official Release `target/experience/release-20261028-151807` contains matching
  source/copy hashes for the executable and hook. Isolated startup/close passed.
- No fork installer was downloaded or executed. Installed-upgrade acceptance
  remains unverified until a fork installer is published and exercised.

## Supersedes

The default upstream channel applies to the original project; this fork owns its
application release discovery and trusted package origin.

## Revisit when

The fork changes repository or release packaging, or adds a separate managed
distribution channel with explicit installation ownership.
