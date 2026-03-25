Here’s the architecture I’d propose, using the Rust system as the base and aiming squarely at what you want:

**an operator-driven, reducer-backed multi-agent harness that can run true parallel waves, with design/spec/product loops first, implementation second, and hardening after.**

The key shift is this:

**Codex should be the task executor, not the orchestrator.**
The Rust system should own orchestration, state, readiness, contradiction management, leasing, and wave closure. That fits the current repo direction much better than the current serial runtime does. The repo already has the right raw pieces for this: a typed domain model, canonical control-event logs, canonical coordination logs, a reducer, projections/read models, an app-server snapshot layer, and a deliberately thin TUI.

Right now the live runtime is still basically serial: select a wave, compile prompts, then execute ordered agents one by one through `codex exec`. 
So the proposal below is a **real step beyond the current runtime**, not just a refactor of what exists.

## 1. Core design goal

The system should support three different kinds of work, all on the same substrate:

1. **design loops**
   Spec, architecture, product, UX, API, rollout, and ops design are iterated until they are complete enough.

2. **implementation waves**
   Code is generated only when the relevant design slices are closure-ready.

3. **post-implementation loops**
   Integration, verification, rollout hardening, documentation, QA, and live-readiness close the loop.

That means waves should not all be treated as “implementation with a different prompt.” The current authored-wave model is already rich enough that you can evolve it into **wave classes with different contracts**, not just parameterized prompts. The domain and authored-wave mapping are already explicit about tasks, closure roles, dependencies, owned paths, deliverables, and result envelopes.

## 2. Architectural principle

The system should be built around five truths:

### A. Canonical state is evented

Keep canonical roots under `.wave/state/`, as the repo already intends:

* control events
* coordination records
* structured results
* derived state
* projections
* traces

### B. The reducer is the decision engine

Queue readiness, blocker state, contradiction pressure, design completeness, implementation readiness, and closure readiness should all come from reducer output, not from launcher-local logic. The repo already has a good planning reducer and projection spine; that should become broader and more authoritative.

### C. The scheduler owns parallelism

A new scheduler/orchestrator layer should decide:

* which waves are claimable
* how many can run in parallel
* which tasks inside a wave can run in parallel
* when closure tasks can start
* when a wave must loop back into design
* when a wave can unlock downstream implementation

### D. Codex is an executor substrate

`wave-runtime` should become a task supervisor/executor adapter, not the place where global orchestration policy lives. Right now it still serializes wave execution and makes launch decisions in the runtime layer. 

### E. The TUI is a control surface, not a planner

The current TUI is correctly thin. Keep it that way. It should consume reducer/app-server state and send operator actions, not derive queue truth itself.

## 3. Proposed wave model

Add a first-class wave classification to the domain and authored-wave schema.

### New `WaveClass`

I would add:

* `spec`
* `architecture`
* `product_design`
* `implementation`
* `integration`
* `verification`
* `hardening`
* `rollout`

You already have enough structure in `TaskRole`, `ClosureRole`, `TaskSeed`, `GateVerdict`, and `ResultEnvelope` to support this cleanly. 

### New `WaveIntent`

Add something like:

* `explore`
* `converge`
* `implement`
* `verify`
* `close`

This lets the scheduler treat design loops differently from build waves.

### New `WaveLoopPolicy`

For pre-implementation waves, add:

* maximum loop count
* contradiction threshold
* open question threshold
* completeness rubric threshold
* human-review requirement

This is how you make spec/design iteration an explicit system behavior rather than informal replanning.

## 4. Proposed domain extensions

Build on `wave-domain`, not around it.

### Extend `TaskRole`

Add roles like:

* `SpecAuthor`
* `Architect`
* `ProductDesigner`
* `ApiDesigner`
* `OpsDesigner`
* `Verifier`
* `Synthesizer`
* `Critic`

The current `TaskRole` already goes beyond implementation and includes integration, documentation, cont-QA, cont-EVAL, security, infra, deploy, and research. Extending that is natural. 

### Add `DesignCompletenessState`

Something like:

* `Underspecified`
* `Fragmented`
* `StructurallyComplete`
* `ImplementationReady`
* `Verified`

### Add `QuestionRecord`

You already have facts, contradictions, human input, and coordination records. Add a first-class unresolved-question entity or model it as a typed fact/coordination subtype. Right now coordination records support claim, evidence, blocker, clarification, handoff, contradiction, escalation, and decision. 
That is close, but for design loops you want a stronger model of:

* open question
* assumption
* decision
* superseded decision

### Add `LeaseRecord`

You already have `TaskState::Leased` in the domain, which is a strong hint. 
Make that real with:

* `wave_lease_id`
* `task_lease_id`
* owner executor/session
* expiry/heartbeat
* status
* preemption rules

This is required for safe parallel waves.

### Add `WavePhase`

Not just `WaveState`.
Something like:

* `DesignLoop`
* `DesignClosure`
* `Implementation`
* `Integration`
* `Verification`
* `Hardening`
* `Complete`

That gives the reducer and scheduler more useful control than a flat state enum.

## 5. Proposed canonical stores

Keep the current event-oriented root layout, but add one more authoritative stream:

### Existing roots to keep

* `.wave/state/events/control/`
* `.wave/state/events/coordination/`
* `.wave/state/results/`
* `.wave/state/derived/`
* `.wave/state/projections/`
* `.wave/state/traces/`

### Add

* `.wave/state/events/scheduler/`

Why add this?

Because once you want true parallel waves, you need authoritative records for:

* wave claimed
* wave released
* task leased
* task heartbeat
* task preempted
* concurrency budget assigned
* closure slot opened
* design loop reopened
* downstream waves unlocked

The current control-event log is already typed and per-wave. It is a good foundation, but you need a scheduler-level stream or scheduler-level event kinds that make parallelism visible and replayable. 

## 6. Proposed reducers

You should split reduction into two layers.

### A. Execution reducer

Consumes:

* control events
* coordination records
* result envelopes
* human input
* proof bundles
* contradictions
* scheduler lease events

Produces:

* per-wave execution state
* per-task execution state
* contradiction pressure
* open question count
* closure eligibility
* loop-back recommendation

This is the missing generalization of the current planning reducer. The existing reducer is good, but it is still mostly planning/queue oriented and fed partly by compatibility-backed run facts. 

### B. Portfolio reducer

Consumes execution-reducer outputs across all waves and produces:

* claimable waves
* blocked waves
* parallel-ready waves
* dependency unlocks
* active capacity usage
* priority ordering
* starvation / contention signals

The current projection spine is already close to this at the planning layer. It should evolve into a more complete portfolio model.

## 7. Proposed scheduler/orchestrator crate

This is the missing major crate.

### Add `wave-scheduler`

Responsibilities:

* read portfolio reducer output
* claim waves
* assign concurrency slots
* assign task leases
* start or stop execution adapters
* honor loop policy
* unlock downstream waves
* serialize closure-phase entry

### Scheduler rules for parallel waves

#### Claiming

A wave is claimable only if:

* upstream dependencies are closure-ready or complete
* no exclusive ownership overlap exists with already-active waves
* no higher-priority contradiction lock exists
* required human input is resolved
* design completeness threshold is met for implementation waves

#### Parallelism

Allow multiple active waves concurrently when:

* their owned paths or owned components do not overlap
* they do not contend for exclusive shared design artifacts
* global executor budget allows it

#### Intra-wave parallelism

Within a wave:

* design/spec/product tasks may run in parallel
* synthesis/integration/doc/QA tasks are phase-gated
* closure roles never start until the reducer says their prerequisites are satisfied

#### Loop-back

A design or architecture wave can reopen if:

* contradiction count exceeds threshold
* required decisions are missing
* implementation uncovers upstream ambiguity
* operator injects a reroute or reopen request

### Why this matters

Right now `autonomous_launch` is a serial queue drainer. That is not enough for your stated goal. 
A real scheduler is the core missing piece.

## 8. Proposed wave classes in practice

### Spec wave

Goal:
define scope, requirements, constraints, success criteria

Output:

* PRD section
* user stories / acceptance criteria
* open questions list
* assumptions list
* success metrics

Closure:

* required sections complete
* open questions below threshold
* contradictions resolved or explicitly accepted

### Architecture wave

Goal:
define system shape and seams

Output:

* architecture doc
* component boundaries
* API contracts
* sequence/state diagrams
* migration plan
* rollout + rollback

Closure:

* owned components explicit
* downstream implementation slices derivable
* contradictions resolved
* implementation readiness = yes

### Product/design wave

Goal:
define UX and operational behavior

Output:

* flows
* edge cases
* error states
* operator workflows
* design acceptance criteria

Closure:

* critical journeys covered
* error handling specified
* unresolved product ambiguity below threshold

### Implementation wave

Goal:
land one implementation seam

Output:

* code
* tests
* docs delta
* typed result envelope
* implementation markers

Closure:

* owned slice proven
* integration not blocked
* required design artifacts referenced

### Verification / hardening wave

Goal:
prove the result is closure-ready in practice

Output:

* integration verdict
* QA verdict
* rollout readiness
* hardening fixes

Closure:

* all gates pass
* contradictions resolved
* no critical open blockers

## 9. Proposed result envelopes for non-code waves

This is important.

Your current result-envelope direction is right, but it needs to grow beyond code-closure semantics. The current domain already has `ResultEnvelope`, `ProofEnvelope`, `DocDeltaEnvelope`, `ClosureInputEnvelope`, and structured closure verdict payloads. 

For design-first waves, add structured result payloads like:

### `SpecResult`

* requirements added
* assumptions added
* decisions made
* open questions remaining
* referenced artifacts

### `ArchitectureResult`

* components defined
* interfaces defined
* migrations defined
* risks identified
* implementation slices unlocked

### `ProductDesignResult`

* user journeys covered
* operator journeys covered
* failure modes covered
* unresolved questions count

That way, the reducer is not trying to infer design completeness from markdown and ad hoc markers.

## 10. Proposed gate engine

The current gates are too implementation/closure flavored for your future state. Expand them.

### Design gates

* `completeness_gate`
* `contradiction_gate`
* `traceability_gate`
* `implementation_readiness_gate`

### Implementation gates

* `owned_slice_gate`
* `artifact_gate`
* `integration_dependency_gate`

### Closure gates

* `integration_gate`
* `documentation_gate`
* `qa_gate`
* `rollout_gate`

### Parallel-safety gates

* `ownership_conflict_gate`
* `shared_artifact_contention_gate`
* `lease_validity_gate`

These should produce typed `GateVerdict`s and be reduced into wave readiness and phase transitions. The current domain already supports that pattern. 

## 11. Proposed TUI architecture

Keep `wave-tui` thin, but make it a better operator cockpit.

### Tabs

I would evolve the right-side panel into:

* `Portfolio`
* `Wave`
* `Tasks`
* `Questions`
* `Contradictions`
* `Leases`
* `Proof`
* `Control`

### Operator actions

Add:

* claim/release wave
* pause/resume wave
* reopen design loop
* escalate question
* approve decision
* force reroute
* raise/lower priority
* inspect overlap/conflict
* admit downstream implementation waves

The current TUI already has the correct philosophical posture—thin consumer, no local planning logic. Keep that, but deepen the control plane it consumes.

## 12. Proposed app-server role

Keep `wave-app-server` thin as a snapshot assembler, but expand its snapshot model.

It should provide one canonical transport snapshot with:

* portfolio queue state
* active waves
* active tasks
* leases
* contradictions
* open questions
* human input requests
* proof state
* rerun/reopen requests
* operator actions

That preserves the good current pattern: snapshot assembly is not a second planner. 

## 13. Proposed runtime role

`wave-runtime` should shrink in authority and grow in substrate quality.

### It should own

* launching Codex executor sessions
* managing repo-scoped Codex homes
* per-task execution
* process supervision
* orphan recovery
* result envelope persistence
* low-level artifacts

### It should stop owning

* queue policy
* next-wave selection
* parallelism policy
* loop policy
* closure sequencing policy

Today it still owns too much by virtue of being the actual launcher. 
In the target model, it should be closer to an executor adapter.

## 14. Proposed end-to-end flow

### Stage 1: initiative declared

Operator defines an initiative with design-first mode.

Scheduler seeds parallel waves:

* spec wave
* architecture wave
* product-design wave
* ops wave

### Stage 2: design loop

These waves run in parallel where ownership does not overlap.

Agents produce:

* facts
* decisions
* contradictions
* open questions
* structured design result envelopes

Reducer computes:

* design completeness
* contradiction pressure
* implementation readiness

If not ready:

* wave loops
* unresolved questions escalate
* new subwaves can be spawned

### Stage 3: synthesis gate

A synthesizer or integration-design wave consolidates design truth into implementation packets.

Output:

* implementation-ready slices
* file/component ownership
* acceptance criteria
* rollout constraints

### Stage 4: implementation waves

Scheduler now admits multiple implementation waves in parallel, subject to:

* ownership overlap
* dependency graph
* concurrency budget

### Stage 5: post-loop closure

Integration, verification, QA, rollout-hardening, docs, and final closure run.

If implementation uncovers ambiguity:

* scheduler reopens the relevant upstream design wave
* downstream waves depending on that seam get blocked or degraded accordingly

That is the loop you described, expressed as a real wave architecture.

## 15. Proposed crate changes

### Keep

* `wave-domain`
* `wave-events`
* `wave-coordination`
* `wave-reducer`
* `wave-projections`
* `wave-runtime`
* `wave-app-server`
* `wave-tui`

### Add

* `wave-scheduler`
* `wave-policy`
* maybe `wave-questions` if you want unresolved questions as a first-class entity
* maybe `wave-portfolio` if you want a clean separation between per-wave reduction and global scheduling

### Reduce

* `wave-control-plane` should remain a shim only temporarily; `wave-projections` should be the real source of read-model truth, which is already the direction.

## 16. Biggest architectural risks

### Risk 1: leaving orchestration in `wave-runtime`

That would recreate the old launcher-centric trap in Rust clothing.

### Risk 2: treating design waves like code waves

Then you will end up with lousy completeness signals and fake closure.

### Risk 3: parallelizing without leases

That will give you racey, non-replayable chaos.

### Risk 4: making the TUI smart

Keep it thin. It should be a window into the reducer/scheduler, not another source of truth.

## 17. Final recommendation

If I were steering this repo, I would do the next three waves like this:

### Wave A: domain + reducer expansion

* add wave classes, intents, phases, lease records, question records
* extend reducer from planning-only to execution + loop-state reduction

### Wave B: scheduler crate

* add wave claiming
* add per-task leases
* add parallel wave selection
* add loop-back/reopen behavior

### Wave C: runtime demotion

* move queue/orchestration policy out of `wave-runtime`
* keep runtime as Codex execution substrate only
* wire app-server and TUI to scheduler/reducer truth

That gets you from:
**serial Codex wave runner with good architecture bones**

to:
**real parallel-wave multi-agent harness with design-first iteration and implementation later**

And that is exactly the system your Rust repo is already pointing toward, even if it has not landed there yet.
