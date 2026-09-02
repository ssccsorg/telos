# tel-eport: coordinate-addressed remote dispatch

Status: idea note, unvetted. Private working note inside the telos
repository. It records a concept for technical evaluation, not a design
commitment.

## Premise

Telos is the execution layer of the SSCCS stack. actus plans and telos
executes. Work is addressed by tagma coordinates instead of path
traversal, and the shipped binary is `tel`. neXus is the data hub, the
tagma/Chton layer is the physical foundation, actus is the intelligence
layer, and verification sits with EV and peers. kineTics is referenced in
this note as the presumed remote orchestration layer; it is not defined in
the stack documentation available at the time of writing, so every
statement that depends on it is conjecture.

## The idea

A local invocation carries a goal to the local agent. The proposed
extension carries the same goal to an agent on another node:

`tel "analyze data"` sends a goal to the local agent.
`tel --eport <coord> "analyze data"` sends the goal to the node whose
tagma coordinate is `<coord>`.

The name plays on teleport: the goal moves through the coordinate space
instead of walking a tree. This is consistent with the teleport thesis of
the stack.

## Placement in the stack (proposed framing)

The proposer describes SSCCS as pillar axes in which two management
planes mirror each other over addressable substrates:

- nexus over chhton: the state plane over physical coordinates. chhton
  provides coordinates over disk, RAM, and network; nexus records logs,
  snapshots, and state over that space.
- kineTics over actus and telos: the movement plane over live agents.
  actus plans and drives N telos agents; kineTics (not yet implemented)
  would resolve coordinates to nodes and orchestrate the remote actus
  and telos pair, the way nexus addresses chhton coordinates.

In naval terms the shape is a carrier group: the carrier is the hub and
the destroyers sit directly under it. The claim is that the nexus to
chhton hierarchy and the kineTics to actus and telos hierarchy coincide
structurally.

Two differences keep the mirror from being exact. nexus manages state
over passive coordinates, while kineTics would manage live agent
execution, which adds authentication, lifecycle, and consent that
passive state does not have. nexus exists today, while kineTics is a
placeholder until its interface is specified.

## Analysis

### What exists today

- `tel` is the agent process binary. actus launches it and speaks a
  WebSocket contract to it (connect, agent_ready, chat, tools,
  completion).
- The interactive entrypoint today is the actus CLI, not `tel` itself.
- Coordinates are the addressing thesis of the stack. Coordinate-based
  resolution and indexing are listed as the next steps for telos, not as
  shipped behavior.

### What the extension adds

Remote dispatch is the same contract one hop further. A local gateway
resolves a coordinate to a target node, forwards a goal with the same
event stream semantics, the remote actus and telos pair executes, and
results stream back. The hard parts are resolution, transport, trust, and
observability. The agent contract itself already exists and is node
local; reusing it across nodes is a natural extension of the layer,
provided the remote side presents the same actus-facing surface.

### Points to resolve before this becomes design

1. Dispatcher identity. `tel` is an agent process, not a shell frontend.
   The `--eport` flag belongs on the actus CLI or on a thin client
   wrapper, or `tel` needs a client mode. The note should not imply the
   agent binary is a user shell today.
2. The O(1) claim. Coordinate-to-node resolution is O(1) only with a
   resolution structure that maps coordinates to live endpoints, such as
   a registry or a structured coordinate space with arithmetic neighbors.
   Without one, the first resolution is a lookup. Transport latency is
   never O(1); the claim should be scoped to logical routing cost.
3. The transport role of neXus. neXus is the data hub for logs,
   snapshots, and state. Using it as a command bus is a role expansion.
   The lower-risk shape keeps transport as direct WebSocket between
   gateways and makes neXus the audit and state record of each dispatch,
   which matches its hub role.
4. Trust. actus authenticates with a bearer token per node. Cross-node
   dispatch needs per-node credentials plus capability scoping so that a
   single coordinate does not authorize arbitrary goals on another node.
   Remote execution should run under the target node's own policy.
5. kineTics. The idea assigns coordinate interpretation and remote
   orchestration to kineTics. With no definition in the available
   documentation, this part stays an open question rather than a design
   input.

## Suggested minimal shape

A local actus instance, or a client, sends an addressed goal. Resolution
maps the coordinate to a gateway endpoint. The gateway forwards the goal
over the existing contract to the target actus, which launches its own
telos agent. Events stream back over the same contract. Every dispatch is
recorded in neXus as state and audit. This shape reuses the actus-telos
contract, keeps neXus in its hub role, and isolates the new surface to
resolution and gateway trust.

## Open questions

- Where does the coordinate-to-endpoint registry live and who maintains
  it?
- Does the goal travel as a prompt for the remote planner, or as an
  already-planned command sequence?
- What is the cancellation and timeout contract across nodes?
- What does kineTics define, and where does it sit relative to actus and
  telos?
- Is remote execution symmetric between nodes, or hub and spoke?

## Boundary

The stack layering, the coordinate thesis, and the actus-telos WebSocket
contract are established facts in the repositories. Every remote-dispatch
mechanism in this note is conjecture until resolution, transport, and
trust are specified. This note records the idea for evaluation.
