# MAS End-State Operator Head Plan

Status: target-state with a partial live Wave 18 landing.

This document defines the end-state plan for Wave as a true multi-agent system with:

- real concurrent execution inside a single wave
- an integrated orchestrator or head surface inside the TUI
- seamless switching between operator-led and autonomous control
- durable prompt steering and per-agent signaling through the control plane

It is narrower than `docs/plans/true-multi-agent-wave-rollout.md` and more product-facing than `docs/implementation/true-multi-agent-wave-architecture.md`.

Use this document as the target contract for the final MAS operating model.

Current repo status relative to this end state:

- the MAS authored-wave contract is partially live
- durable orchestrator-session records and MAS control directives are live
- CLI, app-server, and TUI orchestrator surfaces are live
- the runtime has a per-agent-sandbox MAS path with concurrent launch of parallel-safe agents
- MAS waves now have runtime-backed operator and autonomous head control, broader durable MAS control actions, steering-delivery transport semantics, and reducer-backed recovery visibility

The remaining gap is not contract definition. It is closure of the live proof boundary: one real Wave 18 proof run that exercises concurrent MAS execution, targeted recovery, and honest closure.

## Scope

The end state combines four capabilities that must land together:

1. true intra-wave MAS execution
2. durable coordination and merge authority
3. integrated TUI head or orchestrator control
4. seamless operator and autonomous mode switching

This is not "a faster serial launcher."

This is a control-plane-first MAS operating system for one repository.

## End-State Summary

The final execution model is:

- portfolio scheduler admits waves
- each active wave gets one wave-scoped integration lineage
- multiple safe agents run concurrently inside that wave
- each running agent has its own sandbox, lease, heartbeat, and runtime session
- accepted wave state advances only through merge authority
- all coordination, steering, invalidation, and recovery is durable and replayable
- the TUI exposes one integrated head surface that can both observe and signal
- control can move seamlessly between a human operator and an autonomous head agent

The system remains Rust-first and runtime-agnostic:

- authored waves remain the contract
- reducers remain the source of truth
- projections remain rebuildable
- runtime adapters remain edge executors

## Principles

The final architecture must satisfy all of these:

- operators can explain why any agent is ready, blocked, running, conflicted, invalidated, or merge-pending
- no two agents share one writable filesystem view
- successful work is preserved when a sibling agent fails
- prompt steering is durable and replayable, never ephemeral-only
- autonomous control uses the same control plane and permissions model as human control
- a human can take control back immediately without leaving the wave in ambiguous state
- the TUI is an integrated operator surface, not a second source of truth

## End-State Architecture

### 1. Portfolio layer

The portfolio scheduler decides:

- which waves may run concurrently
- which waves are claimable
- how global capacity is allocated
- whether closure capacity must be reserved

### 2. Wave layer

Each admitted wave receives:

- a wave claim
- a run id
- an integration branch or integration workspace
- a per-wave concurrency budget
- a merge queue
- a barrier state summary
- an orchestrator mode

### 3. Agent layer

Each runnable agent receives:

- an agent sandbox
- a lease
- a heartbeat
- a runtime assignment
- a compiled inbox or coordination view
- a prompt packet derived from current accepted wave state

### 4. Merge layer

An agent is not complete because the executor exited.

An agent becomes accepted wave state only after:

- envelope validation
- ownership validation
- merge validation
- invalidation analysis
- reducer recomputation

### 5. Head layer

Each active MAS wave also has a head or orchestrator session.

That head may be:

- `operator`
- `autonomous`

The head consumes the same projections the human sees and emits the same control directives a human may emit.

## End-State Authored Contract

MAS waves remain explicit opt-in.

Wave-level requirements:

- `execution_model = multi-agent`
- per-wave concurrency budget
- explicit barrier and closure semantics
- explicit ownership and shared-resource rules

Per-agent requirements:

- `depends_on_agents`
- `reads_artifacts_from`
- `writes_artifacts`
- `barrier_class`
- `parallel_safety`
- `exclusive_resources`
- optional `parallel_with`

The system must continue to support `serial` waves unchanged.

## Coordination Model

The control plane acts as a typed blackboard.

Canonical coordination records include:

- blocker
- handoff
- evidence
- clarification
- decision
- contradiction
- artifact published
- merge conflict
- invalidation notice
- steering directive
- directive acknowledgement

Agents may depend on:

- accepted wave state
- published artifacts
- durable coordination records
- reducer-computed readiness and barrier explanations

Agents must not depend on:

- another agent's private terminal transcript
- another sandbox's unmerged filesystem state
- operator memory

## Prompt Steering Model

Prompt steering is a first-class control-plane operation.

The end state supports:

- steering one running agent
- steering a blocked agent before resume
- steering the autonomous head itself
- attaching clarifications or corrections to one agent inbox
- issuing wave-scoped guidance when multiple agents are affected

Every steering action writes:

- directive id
- origin
- target wave or agent
- message
- requested by
- requested at
- delivery state
- acknowledgement or failure detail

Delivery rules:

- if the runtime adapter supports in-session injection, deliver immediately
- otherwise pause at the next safe checkpoint, append the steering overlay, and resume
- if delivery is impossible, preserve the directive as pending or deferred rather than dropping it

No steering action may exist only in UI memory.

## Operator And Autonomous Modes

The end state has two top-level control modes.

### Operator mode

The human operator is primary.

The head session may still exist, but it only:

- summarizes state
- proposes actions
- drafts steering
- recommends merge or rerun decisions

Nothing executes without explicit human action.

### Autonomous mode

The autonomous head becomes an active control-plane client.

It may issue:

- pause or resume
- rerun
- rebase invalidated sandbox
- steering directives
- merge approve or reject
- reconciliation requests
- budget adjustments within configured limits

It does not bypass the reducer, scheduler, or merge authority.

### Seamless switching

The switch between `operator` and `autonomous` must be seamless.

That means:

- switching mode writes one durable orchestrator session update
- in-flight directives remain durable and visible
- the autonomous head stops issuing new directives immediately when the human switches back to operator mode
- the human can inspect autonomous proposals and recent actions without losing continuity
- runtime sessions do not need to restart just because the control mode changed

Human override is always highest priority.

## TUI End State

The TUI becomes the integrated operator and head console.

It must expose one MAS-first workspace that combines:

- wave graph
- ready set
- running agents
- merge queue
- invalidation graph
- coordination feed
- steering history
- head session state
- control-mode state

### Required panes

The orchestrator workspace should include:

- wave overview pane
- selected-agent pane
- sandbox and lease pane
- merge and invalidation pane
- coordination or inbox pane
- head session pane
- signal composer pane

### Required capabilities

The TUI must let the operator:

- select an active MAS wave
- select an individual agent
- inspect that agent's dependencies, barrier reasons, runtime state, and sandbox identity
- steer that agent with prompt text
- pause or resume that agent
- request rerun or rebase
- approve or reject merge
- inspect invalidations and reconciliation needs
- switch the wave between operator and autonomous
- inspect the autonomous head's recent directives and proposals

### Head pane behavior

The head pane should show:

- current mode
- active head session id
- runtime if any
- recent decisions
- queued proposals
- recent directives
- pending acknowledgements
- ownership of control

### Signal composer behavior

The signal composer is the core interactive surface.

It must support:

- steer selected agent
- steer selected wave
- steer head
- attach clarification
- attach contradiction
- attach decision
- send pause or resume
- confirm merge disposition
- hand control to autonomous
- reclaim control for operator

## Control API End State

The CLI and TUI should converge on one control-plane API.

Canonical control operations:

- `wave control agent list`
- `wave control agent show`
- `wave control agent pause`
- `wave control agent resume`
- `wave control agent rerun`
- `wave control agent rebase`
- `wave control agent steer`
- `wave control merge list`
- `wave control merge approve`
- `wave control merge reject`
- `wave control merge reconcile`
- `wave control sandbox list`
- `wave control sandbox show`
- `wave control orchestrator show`
- `wave control orchestrator mode`
- `wave control orchestrator steer`
- `wave control budget get`
- `wave control budget set`

The TUI should call the same control-plane logic the CLI uses.

## Runtime End State

The runtime must become a true MAS supervisor.

Per wave it owns:

- sandbox creation
- lease renewal
- runtime launch and recovery
- artifact collection
- merge intake
- merge execution
- conflict preservation
- invalidation signaling

Per agent it owns:

- one sandbox
- one runtime session
- one lease
- one current delivery status

The runtime does not own policy.

Policy still lives in:

- planner output
- reducers
- scheduler
- merge authority

## Merge And Invalidation End State

The end state requires three distinct paths.

### Fast path

Auto-accept when:

- ownership is valid
- resources are valid
- merge is clean
- validations pass
- no semantic invalidation blocks the merge

### Conflict path

When merge fails:

- preserve the sandbox
- write conflict records
- block downstream work as needed
- enqueue reconciliation

### Invalidation path

When merge is textually clean but semantically supersedes dependent work:

- write invalidation records
- mark downstream agents invalidated
- preserve accepted merged work
- route rerun or rebase only to affected agents

## Recovery End State

Recovery must be selective and durable.

Required cases:

- runtime crash
- launcher crash
- orphaned session
- expired lease
- missing heartbeat
- conflict-resolution rerun
- invalidated sandbox rebase
- operator reclaim from autonomous mode

The system must recover from stored authority rather than synthetic reset.

## Observability End State

Machine-facing status should expose:

- active MAS waves
- ready agent counts
- running agent counts
- merge-pending counts
- invalidated counts
- blocked reasons
- current orchestrator mode
- control ownership
- pending directive deliveries

The app-server snapshot should expose full MAS detail without inventing local truth.

## Acceptance Criteria

The architecture is only complete when all of these are true:

- one wave can show multiple running agents at once
- each running agent has its own sandbox and lease
- the reducer can explain why any non-running agent is blocked
- the TUI can inspect and steer an individual running agent
- the TUI can switch seamlessly between operator and autonomous mode
- the autonomous head can issue the same class of control directives as the human operator
- the human can reclaim control immediately
- successful agents do not rerun because an unrelated sibling failed
- merge, conflict, and invalidation state are visible directly in the TUI
- every steering action and head action is durable and replayable
- replay reconstructs the same MAS state from stored authority

## Implementation Sequence

1. Finish the MAS contract and reducer truth.
   Add all graph, barrier, resource, lease, merge, invalidation, and directive records.

2. Land per-agent sandbox and merge authority.
   Replace the shared writable wave worktree model with per-agent writable sandboxes.

3. Turn on parallel intra-wave scheduling.
   Launch all safe ready agents by default within explicit budgets.

4. Ship the integrated TUI head workspace.
   Add full view and signal capability with agent selection, merge control, and mode switching.

5. Ship seamless operator and autonomous switching.
   Make the head a durable client of the same control plane.

6. Finish live steering and recovery.
   Deliver in-session inject where supported, checkpoint delivery where not, and full selective recovery.

## Bottom Line

The correct end state is:

- parallel waves across the repository
- concurrent agents inside a wave when dependencies permit
- one control plane and one reducer truth
- one integrated TUI for both view and signal
- one head model that supports operator and autonomous control without split-brain behavior
- durable steering, merge, invalidation, and recovery records

That is the architecture that makes Wave a real MAS product rather than a serial orchestrator with nicer dashboards.
