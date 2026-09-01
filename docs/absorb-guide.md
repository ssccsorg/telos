# Zed Absorption Guide

## Purpose

Telos is a zed-derived codebase that keeps only the agent graph and runs as a
headless server for actus. Every upstream zed upgrade that touches the agent
core must be absorbed through a standard procedure. This guide is that
procedure: it records the boundary, the current layout, the crate map, and
the step-by-step absorb workflow. It is the skeleton and blueprint for the
next zed update, so the absorb stays mechanical and the verification stays
reproducible.

The monthly tracking process in `upstream-sync.md` handles small ongoing
changes. This guide handles the version-level absorb: when a new zed release
changes the agent core, the vendored crates are replaced, the headless layer
is re-adapted, the UI surface is re-stripped, and the conformance suite
decides acceptance.

## Boundary and Invariants

The following properties must survive every absorb. A change that violates
one of them is a design decision, not an absorb.

- The actus contract is the stable boundary. It pins eight events
  (`agent_ready`, `thread_created`, `thread_title_changed`, `message_added`,
  `message_completed`, `chat_response_error`, `turn_cancelled`,
  `tool_call_authorization_requested`) and three commands (`chat_message`,
  `cancel_current_turn`, `resolve_tool_call_authorization`) over a WebSocket
  client connection. The full semantics live in the actus repository:
  `docs/devlogs/2026-08-29-nyx-integration-contract.md`. Actus owns session
  mapping, thread persistence, and context injection; telos only knows
  `acp_thread_id`.
- Headless only. No window, no editor, no workspace, no collab UI. The
  binary is rooted at `crates/telos-headless`, whose dependency closure
  defines the buildable graph.
- No full UI crates in the graph. `crates/ui` is a stub that provides only
  the symbols the graph compiles against. UI crates (`editor`, `workspace`,
  `collab`, `*_ui`, `ui_input`, `ui_macros`, `agent_ui`, `component`) are
  excluded from the workspace members.
- License. The absorbed zed code keeps zed's license
  (`GPL-3.0-or-later`). Reports in `docs/sync/` record what changed and why,
  which is what keeps the derivation auditable.

## Current Layout

State as of the `58148f5` absorb round.

- 116 workspace members; `default-members = ["crates/telos-headless"]`.
- `crates/telos-headless`: the only binary. Wires the agent core, gpui
  headless platform, project, the native agent server, and
  `external_websocket_sync`. Handles `--printenv` before app init so a shell
  environment probe never spawns a second agent.
- `crates/external_websocket_sync`: the control layer, ported from helix, not
  zed. Implements the actus command intake, event emission, thread sync, MCP
  server management, and reconnect/resend behavior.
- `crates/ui`: the stub, about 36 files. Keeps `App`/`SharedString` gpui
  re-exports, `IconName` via `icons`, and the widget closure the graph
  compiles against: `Icon`, `IconSize`, `Label`, `LabelSize`, `Checkbox`,
  `CopyButton`, `ScrollAxes`, `Scrollbars`, `WithScrollbar`, `Tooltip`,
  `ButtonStyle`, `TintColor`, `Color`, `IconButton`, `KeyBinding`,
  `Indicator`, `h_flex`, `v_flex`, plus the `prelude`.
- `crates/icons`: 28 `IconName` variants, the exact set referenced by
  compiled code.
- `crates/` also holds orphaned directories that are not members: the
  full zed UI crates and the `telos-core`/`telos-protocol`/`telos` prototype
  dirs. They are excluded from the build and are candidates for deletion.

The authoritative graph definition is `cargo tree -p telos-headless`. When in
doubt about whether a crate belongs, ask whether the binary can reach it.

## Crate Map

| upstream zed crate | telos status | sync interest |
|---|---|---|
| `acp_thread`, `agent`, `agent_servers` | kept, adapted | the agent loop, tool execution, session lifecycle, auth flows |
| `agent-client-protocol` (external) | pinned `=2.0.0` | ACP wire drift; the highest-risk dependency |
| `language_model`, `language_model_core`, `language_models` | kept, adapted | provider registry, API key state, model requests |
| `project`, `language`, `language_core`, `lsp` | kept | MCP, LSP behavior, worktree handling |
| `context_server`, `fs`, `git`, `text`, `streaming_diff` | kept | MCP client, file and diff semantics |
| `markdown`, `mermaid_render`, `html_to_markdown` | kept, ui-stripped | message content storage and rendering |
| `gpui` + platform crates | kept | application framework: `App`, `Entity`, `Task`, `SharedString`; intentional long-term divergence toward tokio |
| `theme`, `theme_settings`, `syntax_theme` | kept | required by the retained ui stub and markdown |
| `icons` | kept, 28 variants | `IconName` only |
| `ui` | stubbed | the widget closure above; never sync the full component set |
| `ui_input`, `ui_macros`, `component`, `*_ui`, `editor`, `workspace`, `collab` | excluded | never sync |
| `external_websocket_sync` | telos-native (helix) | the actus control layer; zed has no counterpart |

## Absorb Workflow

1. Prepare the upstream reference. Keep a zed clone at a known commit. The
   absorb targets the agent core paths: `crates/agent`, `crates/acp_thread`,
   `crates/agent_servers`, `crates/language_model*`, `crates/project`,
   `crates/context_server`, `crates/acp_tools`, `crates/fs`, `crates/git`,
   `crates/text`, `crates/gpui`.
2. Diff scope. Run the monthly sync diff first to know which of those paths
   moved and how. Classify each change as absorb, watch, or ignore using the
   buckets in `upstream-sync.md`.
3. Replace the vendored crates. Copy the changed upstream crates over the
   telos copies, then restore the telos-specific edits that the copies
   overwrote. Known telos deltas to re-apply: the headless entry in
   `telos-headless`, the `external_websocket_sync` wiring, the ui-stripped
   `markdown` (mermaid tab buttons are plain gpui elements), and the
   `language_models` headless providers whose `settings_view` returns `None`.
4. Adapt API drift. The previous absorb had to adjust for `acp` v1 schema
   paths, `SessionId::new`, `NoCertVerifier` in `http_client_tls`,
   `AssistantMessage::content_only`, `AgentConnection::wait_for_tools_ready`
   default, and the `close_session` rename. Expect a similar list each time;
   record it in the sync report.
5. Re-strip the UI surface. Prune the workspace members to the graph
   closure: remove UI crates that the absorb re-added, re-apply the ui stub
   closure, and re-trim `icons` to the referenced variants. The stub keeps
   the widget list in the crate map; everything else in the component set
   goes.
6. Rebuild and verify. Run the gates in the next section. The conformance
   suite is the acceptance decision.
7. Commit and report. Commit the absorb on the working branch, then write
   `docs/sync/YYYY-MM.md` listing the replaced crates, the API adaptations,
   the strip actions, and the watch items. Keep the crate map table current
   in the same commit.

## Headless Adaptation

The control layer is the part zed does not have, so an absorb never overwrites
it wholesale. Re-check these after every absorb:

- `external_websocket_sync` compiles against the new `acp_thread` and
  `agent_servers` APIs. Its `request_thread_creation` path sends the user
  message as `ContentBlock::Text`; mention URIs are parsed downstream by
  `acp_thread::mention::MentionUri`.
- `telos-headless` initializes the graph in order: release channel, settings,
  feature flags, trusted worktrees, HTTP client, client, language registry,
  node runtime, agent, agent servers, and the websocket sync.
- The release profile is `telos-release` (`debug = false`, `strip =
  "symbols"`). The binary lands at `target/telos-release/telos-headless`;
  actus's `run.sh` expects exactly that path.
- The `--printenv` short-circuit stays first in `main`, before any app
  initialization.

## Verification Gates

Run in order. A gate that fails blocks the absorb.

1. `cargo check --workspace` with zero errors and no profile or patch
   warnings. Stale profile package specs and unused `[patch]` entries are
   removed as part of the strip.
2. `cargo test` on the graph crates: `acp_thread`, `agent`,
   `agent_servers`, `language_models`, `markdown`, `ui`, `icons`,
   `external_websocket_sync`. The `acp_thread` suite includes the mention
   parsing tests, including `test_parse_thread_uri` for
   `zed:///agent/thread/{id}?name=...`.
3. `cargo build --profile telos-release -p telos-headless --target-dir
   target/telos-release`, then copy the binary to
   `target/telos-release/telos-headless`.
4. The actus live suite: `cd ../actus && ./run.sh --scenarios`. It must pass
   all checks, including the file mention probe (S0) and the thread mention
   probe (S8) that seeds a thread and references it via its
   `zed:///agent/thread/{id}?name=...` URI. LSP diagnostics mentions are
   intentionally out of scope for the probes.
5. `./run.sh --test` for the full actus integration surface when the
   scenario suite passes.

## Sync Reporting

Each absorb writes `docs/sync/YYYY-MM.md` with four sections: replaced
crates, API adaptations, strip actions, and watch items. Watch items that
touch the actus contract become a telos protocol change plus an actus-side
note, and the conformance suite decides acceptance. The report is the audit
trail for the license boundary: it records what changed and why, so the
derivation stays traceable to upstream.

## License

Telos is zed-derived and distributed under `GPL-3.0-or-later`, matching the
absorbed upstream code. The actus contract documentation lives in the actus
repository and is not part of this license surface.
