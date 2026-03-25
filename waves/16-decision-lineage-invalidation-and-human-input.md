+++
id = 16
slug = "decision-lineage-invalidation-and-human-input"
title = "Land decision lineage, invalidation, and human input"
mode = "dark-factory"
owners = ["architecture", "control"]
depends_on = [15]
validation = ["cargo test -p wave-domain -p wave-events -p wave-coordination -p wave-reducer --locked", "cargo test -p wave-projections -p wave-runtime -p wave-cli -p wave-app-server -p wave-tui --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control status --json", "cargo run -p wave-cli -- control show --wave 16 --json"]
rollback = ["Treat questions, assumptions, decisions, invalidation, dependency handshakes, and human-input state as compatibility-only projections again, and route reopen or supersession behavior back through manual operator notes until durable workflow semantics regain parity."]
proof = ["Cargo.toml", "crates/wave-domain/src/lib.rs", "crates/wave-events/src/lib.rs", "crates/wave-coordination/src/lib.rs", "crates/wave-reducer/src/lib.rs", "crates/wave-projections/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-tui/src/lib.rs", "docs/implementation/parallel-wave-multi-runtime-architecture.md", "docs/plans/master-plan.md", "docs/plans/current-state.md"]
+++
# Wave 16 - Land decision lineage, invalidation, and human input

**Commit message**: `Feat: land decision lineage and invalidation workflow state`

## Component promotions
- decision-lineage-spine: baseline-proved
- contradiction-repair-loop: baseline-proved
- invalidation-supersession-spine: baseline-proved
- human-input-workflow: pilot-live
- dependency-handshake-spine: repo-landed

## Deploy environments
- repo-local: custom default (repo-local decision-lineage and invalidation landing only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Questions, assumptions, decisions, contradictions, human input, dependency handshakes, invalidation, and durable reopen semantics"

## Wave contract
- Questions, assumptions, decisions, and superseded decisions must become first-class durable state, not just review prose.
- Contradictions and human-input requests must be able to block downstream work through reducer-visible semantics.
- Implementation-discovered ambiguity must reopen or degrade the correct upstream design work and invalidate only the proofs or acceptance state that truly depend on the changed lineage.
- External dependency and handshake state must become durable workflow state rather than a note in wave markdown.

## Live proof expectations
- Show an unresolved question or required human-input request blocking downstream work.
- Show a superseded decision invalidating the right downstream wave or proof set.
- Show contradiction-aware reopen semantics against the right upstream design wave.
- Show dependency or handshake state as durable reducer or projection output, not as a free-form note.

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
- .wave/reviews/wave-15-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 16 makes decision lineage, invalidation, contradiction repair, dependency handshakes, and human-input state real workflow authority instead of prose.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/full-cycle-waves.md.
- Read docs/plans/current-state.md.

Specific expectations:
- do not PASS unless implementation-discovered ambiguity can block or reopen the correct upstream work through durable state
- require explicit proof that questions, assumptions, decisions, superseded decisions, contradictions, human-input requests, and dependency handshakes are not hidden in summaries alone
- require live proof showing selective invalidation rather than whole-system hand-waving
- require operator surfaces to explain why something is blocked, invalidated, or waiting on human or external input
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-15-cont-qa.md
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
- .wave/integration/wave-15.md
- .wave/integration/wave-15.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile question and decision lineage, contradiction repair, dependency handshakes, human-input flow, and invalidation semantics into one integration verdict.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/full-cycle-waves.md.
- Read docs/plans/master-plan.md.

Specific expectations:
- treat broad or manual invalidation semantics as an integration failure
- require dependency or handshake blocking to be durable and explainable
- require contradictions to cite the exact facts or decisions they conflict with
- require reopen routing to target the right upstream design or architecture wave
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-15.md
- .wave/integration/wave-15.json
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
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep the architecture and plan docs aligned with the decision-lineage, invalidation, contradiction, and human-input landing.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- document the new durable workflow semantics and the exact remaining gaps
- call out selective invalidation and reopen behavior explicitly
- explain the external dependency and handshake model introduced in this wave
- preserve the live-versus-target-state boundary honestly
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md
```

## Agent A1: Question, Assumption, And Decision Lineage

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Question records, assumption records, decision lineage, superseded decisions, and reducer-aware design completeness"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- decision-lineage-spine
- contradiction-repair-loop

### Capabilities
- question-lineage
- decision-supersession
- design-reopen-routing

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
- Introduce first-class question, assumption, decision, and superseded-decision lineage so design-first loops stop leaking back into prose.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/full-cycle-waves.md.
- Read crates/wave-domain/src/lib.rs.

Specific expectations:
- add durable domain and authority types for questions, assumptions, decisions, and superseded decisions or an equivalent explicit lineage model
- make the reducer aware of unresolved upstream ambiguity and design completeness implications
- tie contradictions to the exact facts or decisions they invalidate
- prove that a superseded decision can invalidate the right downstream work selectively
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/src/lib.rs
- crates/wave-coordination/src/lib.rs
- crates/wave-reducer/src/lib.rs
```

## Agent A2: Human Input, Dependency Handshakes, And Invalidation

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Human input workflows, dependency tickets, handshake requests, invalidation scope, and selective proof reuse"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- human-input-workflow
- dependency-handshake-spine
- invalidation-supersession-spine

### Capabilities
- human-input-routing
- dependency-handshake
- selective-invalidation

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs

### File ownership
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Make human-input, dependency-handshake, and invalidation semantics durable and operator-visible instead of informal side channels.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/current-state.md.
- Read crates/wave-runtime/src/lib.rs.

Specific expectations:
- make required human input and dependency handshakes block the correct downstream work
- support selective invalidation and proof reuse instead of broad rerun folklore
- surface these states through operator-facing or proof-facing state
- prove with fixtures or live proof artifacts that one invalidated design lineage does not erase unrelated wave truth
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
```
