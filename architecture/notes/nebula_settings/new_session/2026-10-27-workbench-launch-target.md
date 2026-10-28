# Workbench new-session launch target

## Status

Compiled for Windows GPUI debug. Native launch-choice and visual acceptance pending.

## Context

The workbench needs a remembered Agent choice for its new-session button while
retaining plain terminals and the existing shell configuration.

## Evidence

- `nebula_settings::RuntimeSettings` owns parsing, defaults, and reset keys.
- GPUI `config::Settings` caches runtime preferences for rendering.
- `workspace::add_terminal_at` already queues a startup command in the native terminal.
- `AgentKind::start_command` is the existing command registry. Gemini's interactive
  command is documented at <https://geminicli.com/docs/cli/cli-reference/>.

## Decision

Persist the application-wide `new_session_target` key with the existing settings
writer. Accept `shell`, `codex`, `claude`, `gemini`, and `opencode`; missing, empty,
or invalid values resolve to `shell`. Restore-defaults removes the key.

The split button's menu only saves the preference. Its main `+` button creates a
session using the configured shell and current project directory, then queues the
selected Agent's registered command. Plain-terminal selection retains the existing
`add_terminal` path. Render reads the cached preference; save failures are visible.

The preference applies to the workbench control. Keyboard terminal creation,
session restore, splits, and remote launch behavior retain their existing owners.
No arbitrary command, authentication, installation, or provider configuration is
stored in this setting.

## Rejected alternatives

- Storing the preference in project/session metadata would give it a second owner.
- Changing the configured shell would mix terminal selection with Agent startup.

## Consequences

Old settings keep creating plain terminals until the user selects an Agent.
Agent installation and authentication remain prerequisites of native CLI startup.

## Validation

Read back the settings parser, reset key, cached configuration, and native launch
callers. Locale catalogs parse and the edited Rust files pass syntax formatting.
The existing Gemini command assertion matches its supported cold-start command.
On 2026-10-27 the product build and 484 related existing tests passed, with 10
ignored. This includes 99 settings-core and 38 Agent-command tests; it does not
establish native default-selection persistence or real CLI startup acceptance.
The experience package is `target/experience/build-20261027-144433/`; its main
window was observed responsive. See the workbench plan for hashes and logs.

## Supersedes

None.

## Revisit when

The supported Agent set or an explicitly requested preference scope changes.
