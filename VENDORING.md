# Zed Sync and Derivation

Telos is zed-derived: the agent graph under `crates/` started as upstream
zed code and now carries telos-owned divergences. This file is the context
record that lets an AI keep that code in sync. Sync is insight mapping, not
mechanical vendoring: an AI diffs from the pinned base, judges what applies,
ports it, and records what happened. There is no scheduled cadence and no
byte-identity requirement. Everything in this file exists so the next AI can
reconstruct the state without re-deriving it.

## Purpose

Telos is a zed-derived codebase that keeps only the agent graph and runs as a
headless server for actus. Upstream zed changes are brought in when they
matter: when a behavior telos depends on changes, or when telos work needs a
newer upstream. Each such port is performed by an AI that reads the upstream
diff and decides, per change, whether it applies to the telos cut. This file
records the derivation base, the known divergences, the layout, and the
verification gates so that decision can be made with full context.

The monthly tracking process in `docs/upstream-sync.md` handles small,
behavior-level observations. This file handles the structural sync: when a
zed release or a targeted commit changes the agent core, the zed-derived
crates are updated selectively and the headless layer is re-adapted.

## Derivation

The zed-derived crates under `crates/`, everything except the telos-owned
`crates/telos` and the helix-ported `crates/external_websocket_sync`, were
copied from upstream zed at this pinned base:

    zed-industries/zed @ e3adf43f37d7a2a9c165a78b255d293b0848d2d0
    main, 2026-08-28, "Show last recently used commands on top of the
    picker's list (#63388)"

Absorbed into telos on 2026-08-29 (telos `0a506ee`). At that time the copied
crates were byte-identical to the reference; spot checks were
`crates/text/src/text.rs` and `crates/sum_tree/src/sum_tree.rs`.

The pin is a diff baseline, not a promise of identity. Telos now diverges
from upstream on purpose (see Crate Map and Divergences). Before any port,
diff from the pin to see both upstream changes and telos drift:

    git -C <zed clone> diff e3adf43..<candidate> -- crates/<path>

Bump the pin at the end of every port to the reference commit that was
actually used. A stale pin silently widens the next diff.

## Divergences

Divergence is a first-class outcome of insight mapping, not an error. What
matters is traceability: the next AI must be able to tell which zed-derived
code differs from upstream and why. A divergence is findable when it is
recorded in at least one of these places:

- a commit touching a zed-derived path whose message states the reason,
- the Crate Map or the divergence notes in this file,
- a report in `docs/sync/`.

Current divergence: `crates/gpui` is being cut down to the runtime subset
the headless graph uses. UI subsystems are removed incrementally; the
measured surface, cut waves, and progress live in
`tooling/gpui-surface/README.md` and `tooling/gpui-surface/manifest.txt`.

## Boundary and Invariants

The following properties survive every port. A change that violates one of
them is a telos design decision, and must be recorded as one.

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
  binary is rooted at `crates/telos`, whose dependency closure defines the
  buildable graph.
- No UI surface in the graph. UI crates (`editor`, `workspace`, `collab`,
  `*_ui`, `ui_input`, `ui_macros`, `agent_ui`, `component`) are excluded
  from the workspace members. The render-only crates (`ui` stub,
  `markdown`, `mermaid_render`) were stripped once the headless graph
  stopped referencing their symbols; a port that re-adds them must
  re-exclude them in the strip step.
- Launch environment boundary. actus speaks the `TELOS_*` launch env
  contract. Zed-derived crates keep their upstream env literals (`ZED_*`,
  `HELIX_*`); the single translation point is `map_launch_env` in
  `crates/telos/src/main.rs`. Never rename those literals inside
  zed-derived crates. If upstream changes them, port them as-is and adjust
  the mapping. This is the invariant that keeps diffs readable.
- Delta hygiene. CI fails if a `TELOS_` identifier appears outside
  `crates/telos/`. This guards the launch contract, not byte purity:
  divergence is allowed, but telos-specific launch names do not leak into
  code that is compared against upstream.
- License. The zed-derived code keeps zed's license (`GPL-3.0-or-later`).
  Reports in `docs/sync/` record what changed and why, which keeps the
  derivation auditable.

## Current Layout

State as of the headless prune that removed the UI surface.

- 110 workspace members (109 crates plus `tooling/perf`);
  `default-members = ["crates/telos"]`.
- `crates/telos`: the only binary. Wires the agent core, gpui headless
  platform, project, the native agent server, and
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

The authoritative graph definition is `cargo tree -p telos`. When in doubt
about whether a crate belongs, ask whether the binary can reach it.

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
| `gpui` + platform crates | diverging | runtime subset cut in progress; keep only what the headless graph uses (App/Entity/Task/values), see `tooling/gpui-surface/`; port upstream gpui changes selectively |
| `theme`, `theme_settings`, `syntax_theme` | kept | consumed by the retained graph; authoritative closure is `cargo tree -p telos` |
| `icons` | kept, 28 variants | `IconName` only |
| `ui` | excluded | stripped once the graph stopped referencing widget symbols; never sync |
| `ui_input`, `ui_macros`, `component`, `*_ui`, `editor`, `workspace`, `collab` | excluded | never sync |
| `external_websocket_sync` | telos-native (helix) | the actus control layer; zed has no counterpart |

## Sync Procedure

Run by an AI when a sync is needed. There is no cadence; the trigger is
need.

1. Prepare the reference. Fetch the zed clone (the Derivation pin) to the
   candidate commit.
2. Diff scope. Diff from the pin over the paths that matter: `crates/agent`,
   `crates/acp_thread`, `crates/agent_servers`, `crates/language_model*`,
   `crates/project`, `crates/context_server`, `crates/acp_tools`,
   `crates/fs`, `crates/git`, `crates/text`, `crates/gpui`. Classify each
   change as port (applies to the telos cut), watch (protocol or contract
   risk), or ignore (UI, collab, unrelated). If nothing applies, stop; an
   upstream change elsewhere never forces a telos change by itself.
3. Port. For upstream-close crates, copy the changed upstream files, then
   re-apply the telos divergences that overlap them. For diverging crates
   (`gpui`), port selectively: apply only upstream changes that fit the
   telos cut, skip changes inside removed subsystems, and adapt when the
   upstream restructure conflicts with the cut. Never port blindly; the
   diff decides.
4. Adapt API drift. Expect a list like the previous round: `acp` schema
   paths, `SessionId::new`, `NoCertVerifier` in `http_client_tls`,
   `AssistantMessage::content_only`, `AgentConnection::wait_for_tools_ready`
   default, and the `close_session` rename. Record it in the report.
5. Re-strip the UI surface. Prune the workspace members to the graph
   closure: remove UI and render-only crates that the port re-added
   (`editor`, `workspace`, `collab`, `*_ui`, `ui`, `markdown`,
   `mermaid_render`, and so on) and re-trim `icons` to the referenced
   variants. The authoritative membership is `cargo tree -p telos` plus
   the explicit exclusions in the crate map.
6. Verify. Run the gates in the next section. The conformance suite is the
   acceptance decision.
7. Record. Commit the port, bump the Derivation pin to the reference commit
   used, update the Crate Map and divergence notes, and write
   `docs/sync/YYYY-MM.md` when the port is non-trivial. The record is how
   the next AI reconstructs context.

## Headless Adaptation

The control layer is the part zed does not have, so a port never overwrites
it wholesale. Re-check these after every port:

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
(`crates/telos`). Zed-derived crates keep their behavior suites in upstream
zed; telos CI verifies that they build here and that the telos-owned tests
pass. Behavior-level acceptance for ported code comes from the deterministic
conformance tier. During a port, run the upstream suites of the touched
crates locally (`cargo test -p acp_thread -p agent ...`) as a pre-commit
sanity check; they are not part of the telos CI gate.

Run in order. A gate that fails blocks the port.

1. Delta hygiene (CI, host step). No `TELOS_` identifier outside
   `crates/telos/`: `grep -rn "TELOS_" crates --include='*.rs'` must hit
   only `crates/telos/`. Re-verify that the `map_launch_env` pair table
   matches the actus `TELOS_*` launch contract.
2. Workspace check (CI, gate image). `cargo check --workspace` on
   ubuntu 24.04 with the pinned 1.97.1 toolchain, zero errors and no
   profile or patch warnings. This is the builds-in-telos gate for every
   zed-derived crate, diverging or not. Stale profile package specs and
   unused `[patch]` entries are removed as part of the strip.
3. Telos-owned tests (CI, gate image). `cargo test -p telos`: the launch
   env mapping tests in `crates/telos/src/main.rs` plus any other unit
   tests in the crate.
4. Release build (e2e-stub image). `cargo build --profile telos-release
   -p telos`; the binary lands at `target/telos-release/tel`, the path
   actus expects via `TELOS_BIN`.
5. Deterministic conformance (manual e2e-stub workflow). Runs the built
   `tel` against `cd ../actus && ./run.sh --scenarios-stub`. The stub
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

## AI Context Checklist

Before a sync, reconstruct state from these in order:

1. This file: Derivation pin, Divergences, Crate Map, exclusions.
2. `tooling/gpui-surface/` for the gpui cut: measured surface and wave plan.
3. `docs/sync/` for the most recent port or divergence records.
4. `git log` on the zed-derived paths for unreported telos-side commits.
5. The launch boundary in `crates/telos/src/main.rs` and the current
   `map_launch_env` pair table.

## License

Telos is zed-derived and distributed under `GPL-3.0-or-later`, matching the
absorbed upstream code. The actus contract documentation lives in the actus
repository and is not part of this license surface.
