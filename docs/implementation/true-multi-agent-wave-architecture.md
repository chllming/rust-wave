# True Multi-Agent Wave Architecture

Status: target-state.

This document defines how Wave should support real multi-agent execution inside a single wave.

Live behavior today is narrower:

- up to two non-conflicting waves may run in parallel
- each active wave gets one wave-scoped worktree
- agents inside a wave still execute one at a time

That live boundary is documented in:

- [parallel-wave-multi-runtime-architecture.md](./parallel-wave-multi-runtime-architecture.md)
- [../plans/current-state.md](../plans/current-state.md)
- [../concepts/runtime-agnostic-orchestration.md](../concepts/runtime-agnostic-orchestration.md)

This document answers the next architectural question:

how should the Rust control plane, scheduler, runtime, and operator surfaces work when a wave is a real MAS execution graph instead of a serial agent list?

## Problem

The current runtime keeps orchestration authority in the Rust control plane rather than in the executor.

Inside a wave, however, it still behaves like a serial launcher:

- pick one agent
- give it the wave worktree
- wait for it to finish
- move to the next agent

That is safe, but it leaves too much value on the table:

- specialists cannot overlap even when ownership does not conflict
- wave latency becomes the sum of all agent latency
- report-only closure starts later than it needs to
- the operator sees an agent list while the runtime is still effectively single-threaded inside the wave

The fix is not "fan the current launcher out faster."

The fix is a real intra-wave architecture with:

- explicit task dependencies
- explicit ownership and resource slices
- explicit leases and heartbeats
- safe concurrent writable sandboxes
- deterministic merge and invalidation rules
- reducer-backed ready-set and barrier transitions

## Architectural Stance

The control-plane stance does not change:

- `waves/*.md` and typed planner output define the contract
- reducers compute truth
- projections render operator-facing views
- `wave-runtime` supervises execution but does not own global planning policy
- runtime adapters translate one task packet into one executor session

What changes is the execution unit.

Today:

- one active wave
- one writable worktree
- one running agent

Target state:

- one active wave claim
- one wave-scoped integration lineage
- zero or more concurrent agent sandboxes
- one merge queue
- one reducer-backed ready set

## Core Model

The architecture is hierarchical.

### Portfolio level

The scheduler decides which waves may run concurrently at all.

That includes:

- dependency readiness
- claimability
- leases
- fairness
- closure protection

### Wave level

An admitted wave gets a durable execution record:

- wave claim
- wave run id
- workspace root
- integration head
- concurrency budget
- barrier state
- merge queue state

### Agent level

Each runnable agent gets its own lease-backed sandbox:

- sandbox id
- agent id
- base integration head
- ownership slice
- dependency set
- runtime assignment
- heartbeat
- current status

### Merge level

Agent output becomes wave truth only after:

- result-envelope validation
- ownership validation
- merge validation
- invalidation checks
- barrier updates

## Planner Responsibilities

The planner must compile authored waves into a machine-usable execution graph rather than a prompt list.

Per MAS agent it should produce:

- role
- owned paths
- deliverables
- required inputs
- produced artifacts
- explicit dependencies
- barrier class
- parallel-safety class
- closure semantics

## Reducer Responsibilities

The reducer must compute:

- ready agents
- blocked agents
- running agents
- merge-pending agents
- conflicted agents
- invalidated agents
- closure eligibility
- rerun eligibility

It should not infer those from launcher temp files.

## Scheduler Responsibilities

The scheduler must own:

- wave admission
- intra-wave concurrency budget
- agent lease issuance
- lease expiry
- preemption
- ready-set selection
- merge-slot selection
- rerun routing
- starvation prevention

## Runtime Responsibilities

`wave-runtime` should supervise execution, not decide policy.

Per launched agent it should:

- materialize the sandbox
- project runtime-specific skills
- start the adapter
- stream activity
- renew the lease
- collect artifacts
- classify exit
- persist envelope-ready outputs

## Projection Responsibilities

The projection spine must render:

- the wave DAG
- the ready set
- active leases
- per-agent sandbox state
- merge queue
- conflict and invalidation state
- barrier state
- closure readiness

The TUI, CLI, and app-server remain thin consumers.

## Workspace Model

The current live model uses one writable worktree per active wave shared by all agents in that wave.

That is not sufficient for true MAS execution.

The target workspace model is:

- one wave-scoped base worktree
- one integration branch or integration workspace for accepted wave state
- one writable agent sandbox per running agent

An example layout:

```text
.wave/state/worktrees/
  wave-17-<run-id>/
    base/
    integration/
    agents/
      A1/
      A2/
      A6/
    artifacts/
    merge/
```

The mechanism can be lightweight worktrees, stacked branches, or equivalent isolated sandboxes. The architectural requirement is stricter than the mechanism:

each running agent must have an isolated writable view derived from the current accepted wave head.

## Barrier And Dependency Model

Every MAS agent should declare:

- `depends_on_agents`
- `reads_artifacts_from`
- `writes_artifacts`
- `barrier_class`
- `parallel_with`
- `exclusive_resources`

At minimum, the scheduler should understand these barrier classes:

- `independent`
- `merge_after`
- `integration_barrier`
- `closure_barrier`
- `report_only`

Owned paths remain necessary, but they are not sufficient. Parallel safety must also account for shared resources such as:

- manifests
- schemas
- API contracts
- generated artifacts
- deploy configuration

## Ready-Set Computation

An agent is ready only if:

- its wave is active
- required upstream agents are satisfied
- its ownership and resource slice does not conflict with running work
- its barrier class is satisfied
- its wave budget allows another launch
- it is not invalidated

If the ready set contains more than one safe agent and budget permits, more than one agent should run. That should be the default behavior for MAS waves, not an optional optimization.

## Merge Model

Accepted wave state advances only through explicit merge events.

Fast path:

- ownership is valid
- declared resources are valid
- merge to the current integration head is clean
- required validations pass
- no downstream invalidation is required

Conflict path:

- record a conflict event
- preserve the sandbox
- block dependents
- enqueue a reconciliation task

Invalidation path:

- record semantic invalidation when a clean textual merge still supersedes downstream assumptions
- block or reroute dependent work
- recompute the ready set from reducer state

## Closure Model

Mandatory closure roles remain real, but they should become dependency-driven rather than "everyone waits for all implementation agents."

Examples:

- documentation can start once the first merged implementation envelope exposes doc deltas
- design review can start once relevant architectural evidence is merged
- integration starts once all integration-relevant implementation work is merged or handed off
- cont-QA starts only after integration, documentation, and required gates are green

Closure must read accepted merged state, not private sandbox state.

## Result Envelope Model

For MAS waves, result envelopes must also carry:

- sandbox id
- base integration head
- produced artifacts
- consumed artifacts
- ownership claim
- merge intent
- invalidation hints
- runtime session identity

Success is not "the agent said it finished."

Success is:

- the envelope validated
- the merge succeeded or was explicitly deferred
- downstream state was recomputed from authoritative records

## Failure And Recovery Model

Every running agent lease needs:

- issued time
- last heartbeat
- expiry time
- runtime session id
- sandbox id

If heartbeat expires:

- mark the lease suspect
- freeze merge admission for that sandbox
- attempt recovery when supported
- otherwise terminate the lease and preserve the sandbox

Crash recovery should reconcile leases, sandboxes, and worktrees from authoritative partial progress rather than synthesizing a clean restart.

Rerun scopes should eventually include:

- `full`
- `from-first-incomplete`
- `closure-only`
- `promotion-only`
- `agent-only`
- `conflict-resolution-only`

## Operator Surface

`wave control show --wave <id>` and the TUI should surface:

- the agent DAG
- ready agents
- running agents
- merged agents
- conflicted agents
- invalidated agents
- per-agent sandbox id
- per-agent runtime and session state
- merge queue state
- per-wave concurrency budget
- barrier reasons
- last heartbeat age

Those actions remain thin control-plane requests rather than local UI state:

- pause or resume one agent
- rerun one agent
- rebase one sandbox
- approve or reject a merge
- request reconciliation
- widen or narrow a concurrency budget

## Runtime-Agnostic Boundary

Nothing about this architecture is Codex-specific.

The stable contract remains:

- the planner emits the graph
- the scheduler issues leases
- the runtime adapter executes one task packet in one sandbox
- the result envelope returns structured outputs

Codex, Claude, and later runtimes differ only at the adapter edge:

- launch command
- runtime flags
- skill projection format
- artifact collection details
- session recovery capabilities

## Rollout Direction

This architecture should land incrementally:

1. honest graph and visibility
2. sandbox and merge authority
3. parallel implementation agents
4. overlapping report-only closure
5. fine-grained rerun and invalidation
6. richer runtime and portfolio policy

The detailed execution plan lives in [../plans/true-multi-agent-wave-rollout.md](../plans/true-multi-agent-wave-rollout.md).

## Bottom Line

The right target is not a faster serial launcher.

The right target is:

- parallel waves across the repository
- concurrent agents inside a wave when dependencies allow
- isolated writable sandboxes per agent
- explicit merge and invalidation control
- reducer-backed operator truth
- runtime-agnostic execution at the edge
