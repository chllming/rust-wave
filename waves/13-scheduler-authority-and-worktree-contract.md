+++
id = 13
slug = "scheduler-authority-and-worktree-contract"
title = "Land scheduler authority and the wave worktree contract"
mode = "dark-factory"
owners = ["architecture", "control"]
depends_on = [12]
validation = ["cargo test -p wave-domain -p wave-events -p wave-coordination -p wave-reducer --locked", "cargo test -p wave-projections -p wave-runtime -p wave-cli -p wave-app-server --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control status --json", "cargo run -p wave-cli -- control show --wave 13 --json"]
rollback = ["Route queue ownership back to reducer-derived readiness over compatibility-backed run state, remove any half-landed claim or lease semantics from the canonical authority model, and treat worktree identity as documentation-only until scheduler authority reaches parity."]
proof = ["Cargo.toml", "crates/wave-domain/src/lib.rs", "crates/wave-events/src/lib.rs", "crates/wave-coordination/src/lib.rs", "crates/wave-reducer/src/lib.rs", "crates/wave-projections/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-app-server/src/lib.rs", "docs/implementation/parallel-wave-multi-runtime-architecture.md", "docs/implementation/rust-wave-0.2-architecture.md", "docs/implementation/rust-wave-0.3-notes.md", "docs/plans/master-plan.md", "docs/plans/current-state.md", "docs/plans/migration.md"]
+++
# Wave 13 - Land scheduler authority and the wave worktree contract

**Commit message**: `Feat: land scheduler authority and wave worktree contract`

## Status
- This wave is now landed in the Rust repo.
- Scheduler claims are canonical and locally exclusive under concurrent launcher paths.
- Live runtime leases now renew through heartbeat and can expire or release in canonical scheduler authority without claiming true parallel execution.
- True parallel-wave execution, per-wave worktree mutation, and runtime plurality remain later work beginning with Wave 14.

## Component promotions
- scheduler-authority-spine: baseline-proved
- wave-worktree-contract: contract-frozen
- launcher-supervisor-split: repo-landed

## Deploy environments
- repo-local: custom default (repo-local scheduler authority and worktree-contract landing only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Scheduler claims, leases, reducer-backed ownership state, launcher-supervisor boundaries, and wave-scoped worktree contracts"

## Wave contract
- Readiness is not ownership. This wave must introduce first-class claim and lease authority instead of inferring ownership from queue readiness alone.
- The isolation unit for future parallel execution is one worktree per active parallel wave. It is not one worktree per agent.
- Agents participating in the same wave must be modeled as sharing one wave-local worktree and one wave-local execution context.
- Runtime behavior may remain effectively serial in this wave, but scheduler authority, ownership semantics, and worktree identity must become canonical control-plane concepts.
- Concurrent local launchers must not be able to admit the same wave twice. Claim acquisition has to be exclusive before execution starts.
- Queue, app-server, CLI, and future TUI surfaces must be able to distinguish `ready`, `claimed`, `active`, `lease-expired`, and ownership-conflicted states from the same reducer-backed source.

## Live proof expectations
- Show one wave as ready but unclaimed in reducer/projection output.
- Show the same wave transitioning to claimed or leased state through canonical scheduler authority.
- Show that a second claimant path is refused before live execution begins.
- Show a live renewed lease plus release or expiry semantics in canonical scheduler state.
- Show wave-local worktree identity or reservation in operator-facing state, even if the worktree manager is still contract-level in this wave.
- Keep docs honest that full parallel execution is still future work after this wave.

## Agent A0: Running cont-QA

### Role prompts
- docs/agents/wave-cont-qa-role.md

### Executor
- profile: review-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: none
- query: "Repository docs remain canonical for cont-QA"

### Skills
- wave-core
- role-cont-qa
- repo-rust-control-plane
- repo-wave-closure-markers

### File ownership
- .wave/reviews/wave-13-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 13 really introduces scheduler authority and wave-level worktree isolation as canonical concepts without overstating full parallel execution.

Required context before coding:
- Read README.md.
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/plans/current-state.md.

Specific expectations:
- do not PASS unless scheduler claim and lease state is canonical and reducer-visible rather than reconstructed from readiness alone
- treat any design that implies one worktree per agent as a blocking architectural defect; the isolation unit must be one worktree per active parallel wave
- require live proof in addition to tests: queue or control surfaces must show ready versus claimed versus released semantics and must surface wave-local worktree identity or reservation
- do not PASS if runtime still hides queue policy or ownership semantics inside launcher-local logic
- map every PASS or BLOCKED claim to exact reducer fixtures, projection snapshots, commands, or proof artifacts
- keep the remaining boundary honest: this wave lands scheduler authority and worktree contract, not true concurrent multi-wave execution yet
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-13-cont-qa.md
```

## Agent A8: Integration Steward

### Role prompts
- docs/agents/wave-integration-role.md

### Executor
- profile: review-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: none
- query: "Repository docs remain canonical for integration"

### Skills
- wave-core
- role-integration
- repo-rust-control-plane
- repo-wave-closure-markers

### File ownership
- .wave/integration/wave-13.md
- .wave/integration/wave-13.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile scheduler authority, reducer state, worktree contract, launcher boundaries, and operator-facing ownership surfaces into one integration verdict.

Required context before coding:
- Read README.md.
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/implementation/rust-wave-0.3-notes.md.
- Read docs/plans/master-plan.md.

Specific expectations:
- treat any mismatch between canonical claim or lease state and operator-facing queue or control surfaces as an integration failure
- require explicit proof that worktree identity is wave-scoped, not agent-scoped
- require queue readiness and ownership state to remain distinct in reducer and projection outputs
- keep the next work explicit: runtime policy and multi-runtime adapters remain later waves even after scheduler authority lands
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-13.md
- .wave/integration/wave-13.json
```

## Agent A9: Wave Documentation Steward

### Role prompts
- docs/agents/wave-documentation-role.md

### Executor
- profile: docs-codex
- model: gpt-5.4

### Context7
- bundle: none
- query: "Shared-plan documentation only"

### Skills
- wave-core
- role-documentation
- repo-rust-control-plane
- repo-wave-closure-markers

### File ownership
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/migration.md
- docs/implementation/parallel-wave-multi-runtime-architecture.md

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep the architecture and shared-plan docs aligned with the scheduler-authority landing and the one-worktree-per-wave contract.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/current-state.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/migration.md.

Specific expectations:
- record Wave 13 as the scheduler-authority and wave-worktree-contract landing
- state clearly that this wave freezes one worktree per active parallel wave as the intended isolation unit
- do not claim full parallel execution or runtime plurality as live in this wave
- describe live proof expectations alongside tests so later reviewers have concrete evidence to inspect
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/migration.md
- docs/implementation/parallel-wave-multi-runtime-architecture.md
```

## Agent A1: Scheduler Authority Core

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Scheduler claims, task leases, lease expiry, causation metadata, and reducer-backed ownership state"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- scheduler-authority-spine
- wave-worktree-contract

### Capabilities
- claim-and-lease-events
- reducer-owned-ownership-state
- lease-expiry-and-conflict-model

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/src/lib.rs
- crates/wave-coordination/src/lib.rs
- crates/wave-reducer/src/lib.rs

### File ownership
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/src/lib.rs
- crates/wave-coordination/src/lib.rs
- crates/wave-reducer/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land first-class scheduler authority so claims, leases, lease expiry, and worktree reservation become canonical state and reducer-owned semantics.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read crates/wave-domain/src/lib.rs.
- Read crates/wave-reducer/src/lib.rs.

Specific expectations:
- add canonical state for wave claims, task leases, lease owners, expiry or heartbeat semantics, release or revocation, and wave-local worktree identity or reservation
- make reducer output distinguish readiness from ownership
- keep the worktree contract wave-scoped: one active worktree per active parallel wave, not per agent
- land deterministic tests that prove ready, claimed, conflicted, expired, and released states
- cite exact fixtures and reducer outputs in the final proof
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/src/lib.rs
- crates/wave-coordination/src/lib.rs
- crates/wave-reducer/src/lib.rs
```

## Agent A2: Launcher And Worktree Contract

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Launcher-supervisor boundaries, scheduler admission, wave-local worktree reservation, and repo-local execution contracts"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- launcher-supervisor-split
- wave-worktree-contract

### Capabilities
- launcher-boundary-cutover
- supervisor-observation
- wave-worktree-reservation

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/implementation/parallel-wave-multi-runtime-architecture.md

### File ownership
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/implementation/parallel-wave-multi-runtime-architecture.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Separate runtime observation from queue semantics enough that scheduler authority and the wave-local worktree contract can sit above the runtime path.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/implementation/rust-wave-0.3-notes.md.
- Read crates/wave-runtime/src/lib.rs.

Specific expectations:
- move the code toward launcher and supervisor roles without pretending full parallel execution already exists
- ensure any worktree reservation or identity is wave-scoped and visible as execution metadata
- do not introduce per-agent worktrees
- keep current execution safe and repo-local while preparing the next runtime-policy wave
- prove the boundary with commands or snapshots that a reviewer can run or inspect locally
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/implementation/parallel-wave-multi-runtime-architecture.md
```

## Agent A3: Projection And Live Proof Surfaces

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Projection visibility for claimed and leased states, queue ownership narratives, and operator-facing live proof artifacts"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-codex-orchestrator
- repo-ratatui-operator
- repo-wave-closure-markers

### Components
- scheduler-authority-spine
- queue-json-surface

### Capabilities
- ownership-projection
- live-proof-artifacts
- operator-queue-visibility

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-projections/src/lib.rs
- crates/wave-app-server/src/lib.rs
- docs/plans/current-state.md

### File ownership
- crates/wave-projections/src/lib.rs
- crates/wave-app-server/src/lib.rs
- docs/plans/current-state.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Surface scheduler authority and wave-local worktree identity in projections and produce live proof that operators can inspect, not just unit tests.

Required context before coding:
- Read docs/plans/current-state.md.
- Read crates/wave-projections/src/lib.rs.
- Read crates/wave-app-server/src/lib.rs.

Specific expectations:
- expose ready versus claimed versus active versus expired ownership states through projections
- surface wave-local worktree identity or reservation in operator-facing state
- produce repo-local proof artifacts or documented commands that demonstrate the new ownership semantics
- keep current-state docs honest: scheduler authority may land here even if full parallel execution still does not
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-projections/src/lib.rs
- crates/wave-app-server/src/lib.rs
- docs/plans/current-state.md
```
