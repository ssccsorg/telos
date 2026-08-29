# Upstream Synchronization

## Purpose

Telos is an independent implementation, but it absorbs the zed agent core's
know-how. This document defines the monthly process that keeps that knowledge
current without merging zed code: an AI agent diffs the relevant upstream
changes, classifies them, and translates the insights into Telos's own
structure.

The process exists because zed's agent evolves quickly and Telos should track
the behavior, not the expression. The actus contract is the stable boundary:
as long as the eight events and three commands hold, Telos internals can
change freely without touching actus.

## Crate to Module Mapping

The mapping is what makes the sync tractable. Every upstream change lands in a
known Telos location.

| zed crate (upstream) | telos module | sync interest |
|---|---|---|
| `agent` | `telos-core` agent loop, tool executor | loop patterns, tool behavior changes |
| `acp_thread`, `agent-client-protocol` | `telos-protocol` | protocol version drift |
| `agent_servers` | `telos-core` session lifecycle | session resume, auth flows |
| `project`, `language`, `language_core`, `lsp` | `telos-tools` (planned) | MCP, LSP behavior |
| `context_server` | `telos-tools` MCP client | MCP protocol changes |
| `fs`, `git`, `text`, `streaming_diff` | `telos-tools` | file and diff semantics |
| `gpui` | tokio runtime (intentional divergence) | none, by design |
| UI crates (`*_ui`, `editor`, `collab`) | excluded | never synced |

## Monthly Process

1. Fetch upstream: `git fetch` in the zed reference clone, then
   `git log --since=<1 month> -- crates/agent crates/acp_thread crates/agent_servers crates/project crates/context_server crates/acp_tools`.
2. Classify each change into one of three buckets.
   - Absorb: behavioral changes, new patterns, bug fixes. Record in the sync
     report and reflect in Telos as independent code.
   - Watch: protocol changes (`agent-client-protocol` versions, ACP wire
     changes). Check whether the actus contract is affected.
   - Ignore: UI, collaboration, or unrelated changes.
3. Produce a sync report committed to `docs/sync/YYYY-MM.md` listing absorbed
   items, the Telos changes made, and watch items.
4. Gate: run the actus conformance suite (`run.sh --test` with the telos
   binary attached) and require it green before closing the sync.

## Rules

- Absorb behavior and design intent, never code expression. The license
  boundary is the reason the reports record what changed, not how the code
  looks.
- Keep the mapping table current. If Telos diverges architecturally from zed,
  update the table as part of the same sync, otherwise the next sync loses
  precision.
- Protocol drift is the only cross-cutting risk. A `watch` item that touches
  the actus contract becomes a Telos protocol change plus an actus-side note,
  and the conformance suite decides acceptance.
