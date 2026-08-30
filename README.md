# Telos

General agent execution layer of the SSCCS stack. Actus plans, Telos
executes: it receives commands from actus and converts them into system
calls, tool invocations, and protocol traffic over files, processes, and
networks.

The working thesis is that an agent does not need to traverse trees to act.
Work is addressed by coordinate and reached by teleport: no path walks, no
directory descent, no sequential scans. The agent moves between work
locations in space, and the execution layer materializes that movement as
concrete system effects.

The final interface is a contract, kept 1:1 with actus. The bootstrap
starts at the boundary: a mock agent emits the exact event sequences actus
depends on, the conformance suite gates every change, and the internals are
filled in behind that stable surface.

Status: bootstrap. A self-contained agent core (`telos-core`) implements
the contract end to end: an LLM backend (OpenAI-compatible, deepseek
default) with a tool registry and a tool-call loop. Without an API key the
loop falls back to a canned backend, so the actus chat flow works with
this binary attached either way.

## Position in the stack

| Layer | Project | Role |
|:---|:---|:---|
| Physical foundation | Chton (tagma) | coordinates over disk, RAM, network |
| Data hub | Nexus | logs, snapshots, state over the coordinate space |
| Verification | EV and peers | mathematical verification of coordinate operations |
| Intelligence | Actus | planning with the LLM, domain knowledge |
| Execution | Telos | command intake, system call conversion, ACP/MCP execution |

## Layout

- `crates/telos-protocol`: wire types for the actus contract
- `crates/telos-core`: self-contained agent core (LLM backend, tools, loop)
- `crates/telos`: the headless binary over the core
- `docs/upstream-sync.md`: upstream knowledge synchronization process
- `index.qmd`: project documentation at the SSCCS index level

## Run

Build the minimal headless agent and attach it to actus:

```sh
cargo build --profile telos-release -p telos-headless
ZED_BIN=/path/to/this/repo/target/telos-release/telos-headless LLM_API_KEY=dummy ./run.sh --test
```

The headless binary (`crates/telos-headless`) wires the agent core directly
(gpui headless + project + native agent + external websocket sync) without
the editor, workspace, agent_ui, or collab UI stack. It speaks the actus
WebSocket contract: connect, agent_ready, chat, tools, completion.

The build disables `language_models/extension-support`, so wasm extension
hosted LLM providers (and the wasmtime/cranelift chain they drag in) are
compiled out; only the built-in anthropic, open_ai, and deepseek providers
stay. Re-enable the feature in the workspace root if extension providers
are ever needed.

The repository is private. Licensing is TBD and is decided only if and when
distribution is planned.
