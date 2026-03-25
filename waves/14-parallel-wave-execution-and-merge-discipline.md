+++
id = 14
slug = "parallel-wave-execution-and-merge-discipline"
title = "Land true parallel-wave execution and merge discipline"
mode = "dark-factory"
owners = ["architecture", "runtime", "delivery"]
depends_on = [13]
validation = ["cargo test -p wave-domain -p wave-reducer -p wave-projections --locked", "cargo test -p wave-runtime -p wave-cli -p wave-app-server -p wave-tui --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control status --json", "cargo run -p wave-cli -- trace latest --wave 14 --json", "cargo run -p wave-cli -- trace replay --wave 14 --json"]
rollback = ["Return to the scheduler-authority and worktree-contract path without allowing more than one active wave to mutate repo state at a time, and treat merge or fairness state as derived evidence only until true parallel execution regains parity."]
proof = ["Cargo.toml", "crates/wave-domain/src/lib.rs", "crates/wave-reducer/src/lib.rs", "crates/wave-projections/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-tui/src/lib.rs", "docs/implementation/parallel-wave-multi-runtime-architecture.md", "docs/plans/master-plan.md", "docs/plans/current-state.md"]
+++
# Wave 14 - Land true parallel-wave execution and merge discipline

**Commit message**: `Feat: land true parallel-wave execution and merge discipline`

## Component promotions
- parallel-wave-execution: pilot-live
- wave-worktree-manager: pilot-live
- merge-discipline: baseline-proved
- fairness-and-preemption: baseline-proved
- reserved-closure-capacity: repo-landed

## Deploy environments
- repo-local: custom default (repo-local parallel-wave execution landing only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Parallel-wave scheduling, wave-scoped worktree management, merge discipline, fairness policy, preemption, and protected closure capacity"

## Wave contract
- This is the first wave that may claim true parallel-wave execution in repo-local use.
- Each active parallel wave must own one isolated worktree, and agents inside that wave share the same wave-local filesystem view.
- Merge, integration, and promotion back to the shared line must be explicit and durable.
- Fairness, preemption, and protected closure capacity must be visible scheduler policy, not invisible operator lore.
- Live proof for this wave must show at least two non-conflicting waves progressing concurrently in separate worktrees.

## Live proof expectations
- Show two non-conflicting waves active concurrently in separate worktrees.
- Show merge or promotion state for each wave captured explicitly.
- Show scheduler fairness, priority, or reserved closure capacity visible in operator-facing state.
- Show that conflicts are detected before dishonest closure.

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
- repo-codex-orchestrator
- repo-wave-closure-markers

### File ownership
- .wave/reviews/wave-17-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 14 is the first honest landing of true parallel-wave execution rather than a serial launcher with better terminology.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/current-state.md.
- Read docs/plans/master-plan.md.

Specific expectations:
- do not PASS unless at least two non-conflicting waves can be shown active concurrently in distinct wave-local worktrees
- require merge or promotion state to be explicit and durable
- require fairness or protected closure capacity to be visible enough that operators can understand why one wave was or was not admitted
- treat hidden shared-root mutation across supposedly parallel waves as a blocking defect
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-17-cont-qa.md
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
- repo-codex-orchestrator
- repo-wave-closure-markers

### File ownership
- .wave/integration/wave-17.md
- .wave/integration/wave-17.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile worktree management, merge discipline, fairness, protected closure capacity, and operator-facing proof of parallel execution into one integration verdict.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- require concurrency proof to be repo-local and inspectable, not theoretical
- require merge or conflict state to be explicit before closure
- require fairness or reserved closure capacity to be visible enough that starvation or emergency-lane behavior is explainable
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-17.md
- .wave/integration/wave-17.json
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
- repo-codex-orchestrator
- repo-wave-closure-markers

### File ownership
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep the architecture and current-state docs aligned with the first honest parallel-wave landing.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- do not let docs claim more than repo-local proof actually demonstrates
- document one worktree per active wave, explicit merge discipline, and visible fairness policy
- record the exact live proof standard for parallel-wave landing
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md
```

## Agent A1: Worktree Manager And Merge Discipline

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Wave-scoped worktree management, merge discipline, integration checks, and explicit promotion back to shared state"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- wave-worktree-manager
- merge-discipline

### Capabilities
- wave-local-worktrees
- merge-state-tracking
- conflict-detection

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs

### File ownership
- Cargo.toml
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Implement the wave-scoped worktree manager and explicit merge discipline required for safe parallel-wave execution.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read crates/wave-runtime/src/lib.rs.
- Read docs/plans/master-plan.md.

Specific expectations:
- allocate one isolated worktree per active wave, not per agent
- keep agents in the same wave sharing that wave-local filesystem view
- make merge or promotion back to the shared line explicit and durable
- detect conflicts before dishonest closure
- cite exact live proof artifacts or commands in the final proof
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
```

## Agent A2: Parallel Scheduler And Fairness Policy

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Parallel-wave admission, fairness, priority classes, preemption, and reserved closure capacity"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- parallel-wave-execution
- fairness-and-preemption
- reserved-closure-capacity

### Capabilities
- concurrent-wave-admission
- fairness-policy
- closure-capacity-protection

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-domain/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs

### File ownership
- crates/wave-domain/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Make parallel-wave admission, fairness, preemption, and protected closure capacity real scheduler semantics instead of informal operator behavior.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read crates/wave-reducer/src/lib.rs.
- Read crates/wave-projections/src/lib.rs.

Specific expectations:
- admit multiple non-conflicting waves concurrently
- model and expose fairness, starvation handling, and protected closure capacity
- keep these semantics visible in reducer and projection output
- prove that operators can inspect why a wave was admitted, delayed, preempted, or protected
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-domain/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
```
