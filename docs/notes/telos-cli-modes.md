# tel: standalone CLI modes (design note)

Status: design note. Decision: all-Rust native, single binary (option A).
Companion to tel-eport.md.

## Goal

`tel` is the ideal execution form of the product: a user types
`tel "prompt"` and the agent runs. Three entry modes share one binary:

1. `tel "<prompt>"` executor: run one turn, stream output to stdout, exit
   with a completion code. Suited to scripts and servers.
2. `tel` interactive REPL: human loop with thread selection and inline
   tool-approval prompts.
3. `tel --server` (default when the actus launch env is present): the
   current actus headless agent role.

The first two modes run without actus and without a python runtime.

## Facts (from the current code)

- The crates/telos main initializes the agent core inside a gpui App:
  providers, thread store, project, filesystem, and prompt store. The
  websocket sync service is an optional layer at the end of that init.
- The websocket service is transport only. An incoming chat_message
  flows to request_thread_creation in external_websocket_sync, which
  loads or creates an AcpThread and runs the turn. Events such as
  message_added and message_completed are emitted through a sync event
  sink that the websocket consumes.
- A deterministic fake provider exists behind TELOS_FAKE_BACKEND, and the
  real providers initialize from settings that actus injects today.

## Design

A common bootstrap replaces the actus-injected settings with local
resolution (LLM_API_KEY and friends from the environment or a config
file). Mode selection happens before app init:

- first positional argument that is not a flag: executor mode with that
  prompt;
- no arguments and no actus launch env: interactive REPL;
- actus launch env present (TELOS_WS_URL): server mode, unchanged.

## Implementation seam

The executor and REPL reuse the exact turn machinery the websocket path
uses. Instead of feeding chat_message over the wire, the CLI calls the
thread creation and turn entry in process and swaps the event sink from
the websocket outgoing channel to a stdout sink. The unknowns are the
public surface of request_thread_creation and the sink abstraction; both
are confined to external_websocket_sync and its AcpThread use, not to the
core agent crates.

## Gaps and sequencing

1. Executor mode spike: in-process turn driver, stdout sink, and config
   bootstrap.
2. REPL mode: thread list and resume, streaming render, inline approval
   for ask mode.
3. Mode arbitration and single-binary packaging documentation.
4. Context features such as at-mentions and file or rules search move
   into tel later; they are optional for the first version.

## Acceptance

- `tel "summarize this repository"` streams an answer and exits zero with
  a real key.
- The same binary still connects to actus in server mode; the actus
  smoke test is the regression gate.
- The standalone path never invokes python or an external runtime.

## Boundary

Everything above the facts section is design intent. The turn machinery
reuse is grounded in the code paths listed. The sink swap is a hypothesis
until the spike confirms the external_websocket_sync surface.
