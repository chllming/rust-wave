+++
id = 7
slug = "autonomous-queue"
title = "Implement autonomous next-wave scheduling and dependency gating"
mode = "dark-factory"
owners = ["runtime", "operator"]
depends_on = [3, 4, 6]
validation = ["cargo test -p wave-runtime -p wave-control-plane -p wave-tui -p wave-cli"]
rollback = ["Keep launch manual and disable automatic next-wave promotion if scheduler or queue state proves unstable."]
proof = ["crates/wave-runtime/src/lib.rs", "crates/wave-control-plane/src/lib.rs", "crates/wave-tui/src/lib.rs", "crates/wave-cli/src/main.rs"]
+++
# Wave 7 - Implement autonomous next-wave scheduling and dependency gating

**Commit message**: `Feat: land autonomous queue and dependency-aware scheduling`

## Component promotions
- autonomous-wave-queue: repo-landed
- dependency-aware-scheduler: repo-landed

## Deploy environments
- repo-local: custom default (repo-local queue and scheduler work only; no live host mutation)

## Context7 defaults
- bundle: rust-async-runtime
- query: "Tokio scheduling loops, queue promotion logic, and dependency gating for autonomous wave selection"

## Agent A0: Running cont-QA

### Role prompts
- docs/agents/wave-cont-qa-role.md

### Executor
- profile: review-codex
- model: gpt-5.4

### Context7
- bundle: none
- query: "Repository docs remain canonical for cont-QA"

### Skills
- wave-core
- role-cont-qa
- repo-wave-closure-markers

### File ownership
- .wave/reviews/wave-7-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether autonomous next-wave selection, dependency gating, and operator status all land together without starting blocked work.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- do not PASS unless blocked waves stay blocked and next-wave visibility matches the same queue truth everywhere
- treat scheduler decisions derived from stale or partial state as blocking
- do not block on A0 appearing as `running` in the active run record while you are executing; judge closure from landed artifacts and prior-stage markers instead
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-7-cont-qa.md
```

## Agent A8: Integration Steward

### Role prompts
- docs/agents/wave-integration-role.md

### Executor
- profile: review-codex
- model: gpt-5.4

### Context7
- bundle: none
- query: "Repository docs remain canonical for integration"

### Skills
- wave-core
- role-integration
- repo-wave-closure-markers

### File ownership
- .wave/integration/wave-7.md
- .wave/integration/wave-7.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile scheduler logic, queue state, and operator surfaces into one closure-ready autonomous-queue verdict.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- treat mismatches between runtime queue selection and control-plane projections as integration failures
- decide ready-for-doc-closure only when next-wave decisions are explainable from authoritative state
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-7.md
- .wave/integration/wave-7.json
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
- repo-wave-closure-markers

### File ownership
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/migration.md
- docs/plans/component-cutover-matrix.md
- docs/plans/component-cutover-matrix.json

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep shared plan docs aligned with autonomous queueing and dependency-aware execution behavior.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions if queue autonomy changes what the operator or later dogfood waves can rely on
- leave an exact closed or no-change note for cont-QA
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/migration.md
- docs/plans/component-cutover-matrix.md
- docs/plans/component-cutover-matrix.json
```

## Agent A1: Queue Reducer And Readiness Logic

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Queue reducers, readiness fields, and dependency blocker modeling for autonomous wave promotion"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- autonomous-wave-queue
- dependency-aware-scheduler

### Capabilities
- readiness-logic
- queue-projections
- dependency-gates

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-control-plane/src/lib.rs

### File ownership
- crates/wave-control-plane/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the queue and readiness logic that tells the runtime which wave can run next and why others remain blocked.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- model next-wave readiness and blockers explicitly so the scheduler never has to infer them from prose
- keep queue semantics compatible with both CLI and TUI operator surfaces
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-control-plane/src/lib.rs
```

## Agent A2: Autonomous Scheduler Runtime

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-async-runtime
- query: "Tokio scheduling loops and safe autonomous promotion of next-ready work in a Rust orchestrator"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- autonomous-wave-queue
- dependency-aware-scheduler

### Capabilities
- scheduler-loop
- next-wave-promotion
- dependency-honoring-execution

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-runtime/src/lib.rs

### File ownership
- crates/wave-runtime/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the autonomous scheduler loop that promotes only ready waves and never starts blocked work.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read wave.toml.

Specific expectations:
- keep scheduling decisions grounded in authoritative queue state instead of launcher-local heuristics
- preserve manual operator understanding of why a wave was or was not promoted
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-runtime/src/lib.rs
```

## Agent A3: Queue Visibility Across CLI And TUI

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-tui
- query: "Operator queue visibility and next-wave surfaces across terminal dashboards and CLI summaries"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-ratatui-operator
- repo-wave-closure-markers

### Components
- autonomous-wave-queue

### Capabilities
- queue-visibility
- next-wave-surface
- operator-consistency

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-cli/src/main.rs
- crates/wave-tui/src/lib.rs

### File ownership
- crates/wave-cli/src/main.rs
- crates/wave-tui/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Expose autonomous queue decisions consistently in the CLI and the right-side operator panel.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- show the same next-wave decision and blocker story in both CLI and TUI surfaces
- avoid UI-specific queue semantics that diverge from control-plane truth
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-cli/src/main.rs
- crates/wave-tui/src/lib.rs
```
