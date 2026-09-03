# Zed Vendoring

Telos vendors the zed agent graph: `crates/` holds byte-identical copies of
upstream zed crates plus a thin telos-owned layer. This file records the
derivation (which zed commit the vendored tree matches), the invariants
that survive every update, and the version-level absorb procedure.

## Purpose

Telos is a zed-derived codebase that keeps only the agent graph and runs as a
headless server for actus. Every upstream zed upgrade that touches the agent
core must be absorbed through a standard procedure. This file is that
procedure: it records the derivation commit, the boundary, the current
layout, the crate map, and the step-by-step absorb workflow. It is the
skeleton and blueprint for the next zed update, so the absorb stays
mechanical and the verification stays reproducible.

Absorb is the standing process: telos never freezes against upstream, and
every upstream zed change that touches the agent graph lands through this
procedure.

The monthly tracking process in `docs/upstream-sync.md` handles small
ongoing changes. This file handles the version-level absorb: when a new zed
release changes the agent core, the vendored crates are replaced, the
headless layer is re-adapted, the UI surface is re-stripped, and the
conformance suite decides acceptance.

## Derivation

The vendored crates under `crates/`, everything except the telos-owned
`crates/telos` and the helix-ported `crates/external_websocket_sync`, are
copied from upstream zed at one pinned commit:

    zed-industries/zed @ e3adf43f37d7a2a9c165a78b255d293b0848d2d0
    main, 2026-08-28, "Show last recently used commands on top of the
    picker's list (#63388)"

Absorbed into telos on 2026-08-29 (telos `0a506ee`). Byte-identical spot
checks against the reference clone: `crates/text/src/text.rs`,
`crates/sum_tree/src/sum_tree.rs`.

The pin makes a zed update a scoped diff: `git diff e3adf43..<new> --
crates/agent crates/acp_thread ...`. Upstream changes outside the telos
graph need no telos work, and the diff shows that before any vendored copy
is touched. Bump the pin at the end of every absorb (step 7) to the
reference commit that was actually merged; a stale pin silently widens the
next diff.

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
  binary is rooted at `crates/telos`, whose dependency closure
  defines the buildable graph.
- No UI surface in the graph. UI crates (`editor`, `workspace`, `collab`,
  `*_ui`, `ui_input`, `ui_macros`, `agent_ui`, `component`) are excluded
  from the workspace members. The render-only crates (`ui` stub,
  `markdown`, `mermaid_render`) were stripped once the headless graph
  stopped referencing their symbols; an absorb that re-adds them must
  re-exclude them in the strip step.
- License. The absorbed zed code keeps zed's license
  (`GPL-3.0-or-later`). Reports in `docs/sync/` record what changed and why,
  which is what keeps the derivation auditable.
- Upstream absorb is mandatory and continuous. A telos change that
diverges from upstream outside the recorded delta set is a design
decision, not an absorb, and must be reviewed as one.
- Launch environment boundary. actus speaks the `TELOS_*` launch env
contract. Vendored crates keep their upstream env literals (`ZED_*`,
`HELIX_*`); the single translation point is `map_launch_env` in
`crates/telos/src/main.rs`. Never rename those literals inside vendored
crates. If upstream changes them, absorb them as-is and adjust the
mapping.
- Vendored purity. After the replace step, vendored crates differ from
upstream only by the recorded telos delta set (ui strip, headless
providers, external sync wiring). A telos-only identifier found inside a
vendored crate fails the delta-integrity gate.

## Current Layout

State as of the headless prune that removed the UI surface.

- 110 workspace members (109 crates plus `tooling/perf`);
  `default-members = ["crates/telos"]`.
- `crates/telos`: the only binary. Wires the agent core, gpui
  headless platform, project, the native agent server, and
  `external_websocket_sync`. Handles `--printenv` before app init so a shell
  environment probe never spawns a second agent. This is the only
  telos-owned crate; CI unit-tests it and no other.
- `crates/external_websocket_sync`: the control layer, ported from helix, not
  zed. Implements the actus command intake, event emission, thread sync, MCP
  server management, and reconnect/resend behavior.
- `crates/icons`: 28 `IconName` variants, the exact set referenced by
  compiled code.
- `crates/` also holds orphaned directories that are not members (full
  zed UI crates, `ui`, `markdown`, `mermaid_render`, and others). They are
  excluded from the build and are candidates for deletion.

The authoritative graph definition is `cargo tree -p telos`. When in
doubt about whether a crate belongs, ask whether the binary can reach it.

## Crate Map

| upstream zed crate | telos status | sync interest |
|---|---|---|
| `acp_thread`, `agent`, `agent_servers` | kept, adapted | the agent loop, tool execution, session lifecycle, auth flows |
| `agent-client-protocol` (external) | pinned `=2.0.0` | ACP wire drift; the highest-risk dependency |
| `language_model`, `language_model_core`, `language_models` | kept, adapted | provider registry, API key state, model requests |
| `project`, `language`, `language_core`, `lsp` | kept | MCP, LSP behavior, worktree handling |
| `context_server`, `fs`, `git`, `text`, `streaming_diff` | kept | MCP client, file and diff semantics |
| `html_to_markdown` | kept | HTML to markdown conversion used by message content |
| `markdown`, `mermaid_render` | excluded | render-only stack, stripped with the UI surface |
| `gpui` + platform crates | kept | application framework: `App`, `Entity`, `Task`, `SharedString`; intentional long-term divergence toward tokio |
| `theme`, `theme_settings`, `syntax_theme` | kept | consumed by the retained graph; authoritative closure is `cargo tree -p telos` |
| `icons` | kept, 28 variants | `IconName` only |
| `ui` | excluded | stripped once the graph stopped referencing widget symbols; never sync |
| `ui_input`, `ui_macros`, `component`, `*_ui`, `editor`, `workspace`, `collab` | excluded | never sync |
| `external_websocket_sync` | telos-native (helix) | the actus control layer; zed has no counterpart |

## Absorb Workflow

1. Prepare the upstream reference. Use the pinned zed clone from the
   Derivation section and fetch it to the candidate commit. The absorb
   targets the agent core paths: `crates/agent`, `crates/acp_thread`,
   `crates/agent_servers`, `crates/language_model*`, `crates/project`,
   `crates/context_server`, `crates/acp_tools`, `crates/fs`, `crates/git`,
   `crates/text`, `crates/gpui`.
2. Diff scope. Diff from the pinned base: `git -C <zed clone> diff
   e3adf43..<candidate> -- <target paths>`. Run the monthly sync diff
   first to know which of those paths moved and how. Classify each change
   as absorb, watch, or ignore using the buckets in
   `docs/upstream-sync.md`. If nothing in the target paths moved, record a
   no-op absorb and stop: an upstream change elsewhere never forces a
   telos change by itself.
3. Replace the vendored crates. Copy the changed upstream crates over the
   telos copies, then restore the telos-specific edits that the copies
   overwrote. Known telos deltas to re-apply: the `TELOS_*` to `ZED_*`
   launch-env mapping in `crates/telos/src/main.rs`, the entry in
   `telos`, the `external_websocket_sync` wiring, and the
   `language_models` headless providers whose `settings_view` returns
   `None`. The ui-stripped `markdown` no longer exists; if the absorb
   re-adds it, the strip step must re-exclude it.
4. Adapt API drift. The previous absorb had to adjust for `acp` v1 schema
   paths, `SessionId::new`, `NoCertVerifier` in `http_client_tls`,
   `AssistantMessage::content_only`, `AgentConnection::wait_for_tools_ready`
   default, and the `close_session` rename. Expect a similar list each time;
   record it in the sync report.
5. Re-strip the UI surface. Prune the workspace members to the graph
   closure: remove UI and render-only crates that the absorb re-added
   (`editor`, `workspace`, `collab`, `*_ui`, `ui`, `markdown`,
   `mermaid_render`, and so on) and re-trim `icons` to the referenced
   variants. The authoritative membership is `cargo tree -p telos` plus
   the explicit exclusions in the crate map; everything outside the graph
   closure goes.
6. Rebuild and verify. Run the gates in the next section. The conformance
   suite is the acceptance decision.
7. Commit and report. Commit the absorb on the working branch, then write
   `docs/sync/YYYY-MM.md` listing the replaced crates, the API adaptations,
   the strip actions, and the watch items. Bump the Derivation pin to the
   reference commit that was merged and keep the crate map table current in
   the same commit.

## Headless Adaptation

The control layer is the part zed does not have, so an absorb never overwrites
it wholesale. Re-check these after every absorb:

- `external_websocket_sync` compiles against the new `acp_thread` and
  `agent_servers` APIs. Its `request_thread_creation` path sends the user
  message as `ContentBlock::Text`; mention URIs are parsed downstream by
  `acp_thread::mention::MentionUri`.
- `telos` initializes the graph in order: release channel, settings,
  feature flags, trusted worktrees, HTTP client, client, language registry,
  node runtime, agent, agent servers, and the websocket sync.
- The release profile is `telos-release` (`debug = false`, `strip =
  "symbols"`). The binary lands at `target/telos-release/tel`; actus
  expects exactly that path via `TELOS_BIN`.
- The `--printenv` short-circuit stays first in `main`, before any app
  initialization.

## Verification Gates

Coverage philosophy: CI unit-tests only the telos-owned crate
(`crates/telos`). Synced crates (zed upstream, and helix for the control
layer) keep their behavior suites in their upstream projects; telos CI
verifies that they build here, that the recorded delta set is intact, and
that the telos-owned tests pass. Behavior-level acceptance for synced code
comes from the deterministic conformance tier. During an absorb, run the
upstream suites of the replaced crates locally (`cargo test -p acp_thread
-p agent -p external_websocket_sync ...`) as a pre-commit sanity check;
they are not part of the telos CI gate.

Run in order. A gate that fails blocks the absorb.

1. Delta integrity (CI, host step). Vendored crates carry no telos-only
   identifiers: `grep -rn "TELOS_" crates --include='*.rs'` must hit only
   `crates/telos/`. Re-verify that the `map_launch_env` pair table
   matches the actus `TELOS_*` launch contract.
2. Workspace check (CI, gate image). `cargo check --workspace` on
   ubuntu 24.04 with the pinned 1.97.1 toolchain, zero errors and no
   profile or patch warnings. This is the builds-in-telos gate for every
   synced crate. Stale profile package specs and unused `[patch]`
   entries are removed as part of the strip.
3. Telos-owned tests (CI, gate image). `cargo test -p telos`: the launch
   env mapping tests in `crates/telos/src/main.rs` plus any other unit
   tests in the crate.
4. Release build (e2e-fake image). `cargo build --profile telos-release
   -p telos`; the binary lands at `target/telos-release/tel`, the path
   actus expects via `TELOS_BIN`.
5. Deterministic conformance (manual e2e-fake workflow). Runs the built
   `tel` against `cd ../actus && ./run.sh --scenarios-fake`. The fake
   backend answers every prompt, no API key is used, and the tier must
   pass fully.
6. Live suite, opt-in only because it consumes LLM tokens: `cd ../actus
   && ./run.sh --scenarios-llm`. It must pass
   all checks, including the file mention probe (S0) and the thread mention
   probe (S8) that seeds a thread and references it via its
   `zed:///agent/thread/{id}?name=...` URI. LSP diagnostics mentions are
   intentionally out of scope for the probes.
7. `./run.sh --test` for the full actus integration surface when the
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
