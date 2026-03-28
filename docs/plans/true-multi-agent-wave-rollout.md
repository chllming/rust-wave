# True Multi-Agent Wave Rollout

Status: in-progress target-state.

This document translates the true intra-wave multi-agent architecture into an implementation plan that fits the Rust-first product-factory branch.

The repo now has a partial Wave 18 MAS landing. This document therefore tracks both:

- what is already live in the current worktree
- what still remains before Wave 18 can close and `factory-mas-v2` can be called landed

For live behavior today, read:

- `docs/plans/current-state.md`
- `docs/implementation/parallel-wave-multi-runtime-architecture.md`
- `docs/implementation/rust-codex-refactor.md`

For the higher-level product-factory framing, read:

- `docs/plans/product-factory-cutover.md`
- `docs/plans/delivery-catalog.json`

## Summary

The next major release after `factory-core-v1` is `factory-mas-v2`.

That release changes the execution unit from:

- one active wave
- one writable wave worktree
- one running agent at a time

to:

- one active wave claim
- one wave-scoped integration lineage
- zero or more concurrent agent sandboxes inside that wave
- one merge queue
- one reducer-backed ready set

This is the cut that turns Wave from parallel-wave orchestration into true intra-wave MAS execution.

## Current Implementation Status

Wave 18 is no longer just authored intent. The current worktree already includes:

- opt-in MAS authored-wave metadata through `execution_model = multi-agent`
- per-wave MAS budget metadata and per-agent dependency, barrier, artifact, and resource fields
- durable orchestrator-session records and durable control directives for agent-level and wave-level steering
- CLI, app-server, and TUI orchestrator surfaces that expose MAS agent detail, directive history, and operator or autonomous mode
- a separate MAS runtime launch path with per-agent sandboxes, ready-set computation, parallel-safe concurrent launch, merge sidecars, and invalidation sidecars
- an active autonomous-head steering loop for MAS runs plus durable steering-delivery state that now progresses beyond static placeholder records
- durable MAS control actions for pause, resume, rerun, rebase, reconciliation, and merge approval or rejection through the same control-plane record family
- runtime handling that preserves accepted sibling work when one MAS agent hits merge conflict or rejection and moves the wave into recovery-required state instead of discarding accepted work immediately
- reducer/projection/app-server/TUI recovery visibility for unresolved recovery plans, preserved sibling work, and required recovery actions
- directive-delivery transport semantics that now distinguish live injection, checkpoint overlays, and deferred delivery
- a finished operator shell product pass with transcript search, compare mode, explicit alt-screen policy, and fresh-session controls

Wave 18 is still not closable. The remaining gaps are:

- one live Wave 18 proof run that demonstrates concurrent launch, steering, recovery-required state, targeted repair, and honest closure

Later expansion after that proof boundary may still deepen autonomous/operator-head behavior beyond the current safe action family and proposal synthesis, but it is no longer the blocker for calling the current MAS cut code-landed.

## First Authored Milestone

The first concrete authored milestone for this release should be `Wave 18`.

`Wave 18` is the first repo-local MAS pilot. Its closure bar remains:

- at least two implementation agents running concurrently inside one wave
- per-agent sandboxes and lease-backed runtime identity
- clean auto-merge for policy-valid merges
- conflict and invalidation routing that preserves already accepted sibling work
- an integrated TUI orchestrator workspace with direct per-agent signaling
- full `operator` and `autonomous` mode switching through one durable control path

That pilot wave should remain narrow enough for repo-local proof, but broad enough that the MAS runtime, merge authority, and head-control surfaces are all exercised by one real authored wave rather than only by fixtures.

## Release Boundary

`factory-mas-v2` should branch from the merged post-Wave-17 `factory-core-v1` baseline.

It should not begin from:

- the dirty pre-merge main checkout
- the existing Wave 17 worktree
- package-era launcher state under `.tmp/<lane>-wave-launcher/`

The repo has now moved past documentation-only seeding. The current worktree already contains compatibility-safe MAS contract work plus a partial runtime and operator-surface landing. The remaining implementation should build on that baseline rather than re-design it.

## Carry-Forward From The Sibling Repo

The sibling `agent-wave-orchestrator` repo still contributes useful operational ideas:

- versioned signal wrappers and wake loops built on control-status projections
- report-only design and documentation work that can begin from partial merged evidence
- blocker severities that distinguish `hard`, `soft`, `stale`, `advisory`, `proof-critical`, and `closure-critical`
- targeted recovery that restarts the smallest valid scope first

This repo should deliberately diverge in four ways:

- keep `.wave/state/` authoritative and replayable
- keep Rust reducers and projections as truth instead of launcher-local state
- keep repo-local TUI, CLI, and app-server as primary operator surfaces
- keep the system aimed at product-factory execution end to end, not only executor parity

## Design Goals

The release should satisfy all of these at once:

- true concurrent execution for non-conflicting agents inside one wave
- one canonical control plane above any runtime choice
- replayable truth from events, envelopes, merge records, and projections
- runtime-agnostic execution through adapters
- safe concurrent writable sandboxes without shared agent mutation
- selective recovery from stalls, crashes, lease expiry, merge conflict, and invalidation
- honest operator visibility into ready, running, merged, conflicted, invalidated, and blocked agent state

## Non-Goals

The release should not try to:

- run every agent in parallel regardless of dependencies
- let multiple agents mutate one writable checkout
- move planning truth into runtime prompts or session transcripts
- let the TUI or app-server infer scheduler truth locally
- remove wave-level isolation between concurrently active waves

## Public Contract Changes

The authored-wave contract should stay backward compatible and add explicit opt-in for MAS execution.

Wave-level additions:

- `execution_model = serial | multi-agent`
- `max_concurrent_implementation_agents`
- `max_concurrent_report_only_agents`
- `max_merge_operations`
- `max_conflict_resolution_agents`

Per-agent additions for `execution_model = multi-agent` waves:

- `depends_on_agents`
- `reads_artifacts_from`
- `writes_artifacts`
- `barrier_class`
- `exclusive_resources`
- optional `parallel_with`

Defaults:

- waves that omit `execution_model` stay `serial`
- existing closure ordering still compiles into the default graph
- current waves continue to parse and lint unchanged

## Soft State And Severity Model

Delivery and operator soft state stays exactly as already landed:

- `clear`
- `advisory`
- `degraded`
- `stale`

Those values annotate delivery objects and machine-facing control signals. They do not decide claimability or closure by themselves.

MAS execution adds a separate coordination severity model for blockers, invalidations, and helper work:

- `hard`
- `soft`
- `stale`
- `advisory`
- `proof-critical`
- `closure-critical`

That separation is deliberate:

- delivery soft state answers "how much should an operator trust this view?"
- coordination severity answers "how strongly should this record constrain execution or closure?"

## Domain And Event Additions

Most of this record set is now seeded and used in the current worktree. The remaining gap is proving the end-to-end recovery path against a real Wave 18 run rather than only through fixtures.

The Rust domain and event model should grow these core records:

- `BarrierClass`
- `ParallelSafetyClass`
- `ExclusiveResource`
- `ArtifactDependency`
- `AgentSandboxRecord`
- `MergeIntentRecord`
- `MergeResultRecord`
- `InvalidationRecord`
- `AgentLeaseRecord`
- wave-scoped concurrency budget records

The scheduler and control streams should then record:

- sandbox created or released
- agent lease issued
- agent heartbeat renewed
- agent lease expired
- merge queued
- merge accepted
- merge rejected
- invalidation raised
- invalidation cleared
- reconciliation requested
- per-wave budget changed

## Planner And Reducer Work

The planner must emit a real agent graph rather than a serial prompt list.

For each MAS agent, planner output should include:

- role
- owned paths
- deliverables
- required inputs
- produced artifacts
- dependency edges
- barrier class
- parallel-safety class
- exclusive resources
- closure semantics

The reducer must compute from authoritative state:

- ready agents
- blocked agents
- running agents
- merge-pending agents
- merged agents
- conflicted agents
- invalidated agents
- closure eligibility
- rerun eligibility

No projection should infer those states from launcher temp files.

Current status:

- MAS-authored waves now parse and lint through the extended contract
- projections and app snapshots now expose MAS agent detail such as sandbox ids, merge state, barrier reasons, directive delivery, and recovery-required state
- reducer and replay authority now compute the selective-recovery and MAS-control story from persisted records; the remaining work is a real proof run that ratifies those paths end to end

## Runtime And Workspace Model

The runtime should supervise execution, not own policy.

Per running agent it must:

- materialize the sandbox
- project runtime-specific skills
- launch the adapter
- stream activity
- renew the lease
- collect artifacts
- classify exit
- persist an envelope-ready result packet

The first sandbox backend should be Git-based and rooted under the wave worktree:

- one wave-scoped base snapshot
- one integration branch or integration workspace
- one writable sandbox per running agent
- one merge area for queued merge intents and reports

Each running agent sandbox must be derived from the current accepted integration head. Shared writable agent mutation is not allowed.

Current status:

- the runtime now allocates per-agent MAS sandboxes under the wave worktree
- it computes a launch-ready set for parallel-safe MAS agents and can launch more than one agent concurrently
- it records merge intents, merge results, and invalidations as runtime-side authoritative sidecars
- it can merge accepted work back into the integration head for clean policy-valid cases
- it can now issue autonomous-head steering directives during MAS execution and persist steering overlays plus delivery acknowledgements or deferrals for later checkpoints

What still remains:

- one real repo-local proof run that exercises the already-landed lease, merge, reconciliation, rerun, and recovery paths under live Wave 18 conditions
- later expansion for broader head behaviors beyond the current safe action family after the pilot boundary is proven

## Operator Surfaces

`wave control status`, `wave control show --wave <id>`, the TUI, and the app-server should surface:

- the per-wave DAG
- ready-set counts
- running, merge-pending, merged, conflicted, and invalidated agents
- per-agent sandbox id
- per-agent lease heartbeat age
- per-agent runtime and session identity
- merge queue summaries
- per-wave concurrency budgets
- barrier explanations

Operator actions should remain thin requests into the control plane:

- pause or resume one agent
- rerun one agent
- rebase one invalidated sandbox
- approve or reject a merge
- steer one agent or one wave
- switch the wave between `operator` and `autonomous`

Current status:

- `wave control orchestrator mode|show|steer` is live
- app-server and TUI orchestrator surfaces now show MAS agent sandbox ids, merge state, barrier reasons, and directive history
- the selected-agent TUI view is MAS-aware rather than single-agent-run oriented
- steering directives now progress through durable delivery state instead of remaining static placeholders only

What still remains:

- live Wave 18 proof that the already-landed operator and autonomous controls behave honestly under real concurrent-agent conditions
- later ergonomics and policy expansion after the pilot boundary closes

## Rollout Phases

### Phase 1: Honest Graph And Visibility

- add `execution_model`
- add agent graph and barrier metadata
- expose ready-set and barrier explanations in projections
- keep runtime serial inside each wave

Acceptance:

- a MAS-authored wave renders a deterministic DAG
- the control plane can explain why any non-running agent is blocked

### Phase 2: Sandbox And Merge Authority

- add sandbox, merge-intent, merge-result, and invalidation records
- add reducer-backed visibility for those records
- keep the launch count capped at one agent per wave while merge semantics stabilize

Acceptance:

- every launched MAS agent has a durable sandbox record and merge intent
- replay reconstructs the same merge queue and invalidation state

### Phase 3: Parallel Implementation Agents

- allow more than one implementation agent inside the same wave
- enforce exclusive resources and ownership safety
- enforce per-wave concurrency budgets
- make lease heartbeat and expiry authoritative

Acceptance:

- one wave can show multiple running implementation agents at once
- safe agents launch in parallel by default when the budget permits

### Phase 4: Overlapping Report-Only Closure

- allow design and documentation reviewers to start from partial merged evidence when declared safe
- keep integration and cont-QA behind stricter merged-state barriers

Acceptance:

- report-only closure can overlap with late implementation work without reading private sandbox state

### Phase 5: Fine-Grained Recovery

- add `agent-only` and `conflict-resolution-only` rerun scopes
- add sandbox rebase after invalidation
- preserve already-merged successful work during recovery

Acceptance:

- one failed sandbox does not force successful merged agents to rerun

### Phase 6: Policy Breadth

- add runtime-specific budgets
- add richer fallback policy
- add portfolio-aware critical-path scheduling across concurrent MAS waves

Acceptance:

- the scheduler can explain runtime capacity, starvation prevention, and priority choices across active waves

## Verification Plan

- parser and lint tests that preserve current serial waves unchanged
- planner tests for graph compilation, barrier classes, and budget fields
- reducer replay tests for ready-set, merge-pending, conflict, and invalidation reconstruction
- scheduler tests for parallel launch, exclusive-resource blocking, lease expiry, and starvation prevention
- runtime tests for sandbox isolation, merge fast path, merge conflict, semantic invalidation, and selective rerun
- operator snapshot tests for DAG visibility, barrier explanations, merge queue summaries, and lease health
- end-to-end proof of one wave with at least two concurrent implementation agents, one report-only reviewer, one merge acceptance, one invalidation or retry case, and replay rebuilding the same graph from stored authority

## Acceptance Criteria

This release is only landed when all of these are true:

- a wave can show multiple running agents at once
- each running agent has its own sandbox and lease
- the control plane can explain any blocked agent
- successful merged agents do not rerun just because another sandbox failed
- closure reads accepted merged state rather than private sandbox state
- operators can see merge, conflict, and invalidation directly
- replay reconstructs the same intra-wave execution graph from stored authority

## Bottom Line

`factory-mas-v2` should not be framed as "make the current launcher faster."

It is a real control-plane expansion:

- parallel waves across the repository
- concurrent agents inside a wave when dependencies allow
- isolated writable sandboxes per agent
- explicit merge and invalidation control
- reducer-backed operator truth
- runtime-agnostic execution at the edge
