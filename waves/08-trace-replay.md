+++
id = 8
slug = "trace-replay"
title = "Capture traces and validate replay semantics"
mode = "dark-factory"
owners = ["runtime", "audit"]
depends_on = [3, 4, 6]
validation = ["cargo test -p wave-trace -p wave-runtime -p wave-cli"]
rollback = ["Disable replay validation and keep runtime execution only until trace semantics stabilize."]
proof = ["crates/wave-trace/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "docs/guides/terminal-surfaces.md", "docs/implementation/rust-codex-refactor.md"]
+++
# Wave 8 - Capture traces and validate replay semantics

**Commit message**: `Feat: land trace bundle and replay validation`

## Component promotions
- trace-bundle-v1: repo-landed
- replay-validation: repo-landed

## Deploy environments
- repo-local: custom default (repo-local trace and replay work only; no live host mutation)

## Context7 defaults
- bundle: rust-trace-replay
- query: "Trace bundles, audit artifacts, and deterministic replay validation for a Rust orchestration runtime"

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
- .wave/reviews/wave-8-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether traces, replay validation, and operator audit visibility land together as durable runtime evidence rather than ad hoc logging.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- do not PASS unless trace artifacts can explain runtime decisions and replay can validate stored outcomes
- treat ephemeral or non-replayable evidence as blocking
- do not block on A0 appearing as `running` in the active run record while you are executing; judge closure from landed artifacts and prior-stage markers instead
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-8-cont-qa.md
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
- .wave/integration/wave-8.md
- .wave/integration/wave-8.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile trace schema, replay behavior, and operator audit surfaces into one closure-ready runtime evidence verdict.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- treat any mismatch between stored traces and replay validation logic as an integration failure
- decide ready-for-doc-closure only when the same trace data can drive runtime audit and replay checks
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-8.md
- .wave/integration/wave-8.json
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
- Keep shared plan docs aligned with trace capture, replay validation, and audit-history expectations.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions if traces change what later dogfood or closure waves can prove automatically
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

## Agent A1: Trace Bundle Schema And Persistence

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-trace-replay
- query: "Versioned trace schema and durable artifact persistence for runtime and operator actions"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- trace-bundle-v1
- replay-validation

### Capabilities
- trace-schema
- artifact-persistence
- audit-records

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-trace/src/lib.rs

### File ownership
- crates/wave-trace/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the versioned trace bundle and durable artifact model for runtime decisions, proofs, and operator actions.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- keep traces semantic enough to validate scheduler decisions, proofs, reruns, and closure outcomes
- store durable artifacts rather than terminal-only summaries
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-trace/src/lib.rs
```

## Agent A2: Replay Validator And Trace Commands

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-trace-replay
- query: "Deterministic replay validation and trace-facing operator commands for a Rust orchestration runtime"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- replay-validation

### Capabilities
- replay-validator
- trace-commands
- runtime-audit

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs

### File ownership
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land replay validation and the command surfaces that let operators inspect or verify trace outcomes.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- keep replay checks deterministic and grounded in stored trace semantics rather than best-effort heuristics
- expose trace actions through operator surfaces that later dogfood waves can rely on
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
```

## Agent A3: Audit Guidance And Replay Documentation

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-trace-replay
- query: "Operator audit guidance and replay-documentation patterns for trace-aware orchestration systems"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- trace-bundle-v1

### Capabilities
- audit-guidance
- replay-docs
- trace-operator-surface

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- docs/guides/terminal-surfaces.md
- docs/implementation/rust-codex-refactor.md

### File ownership
- docs/guides/terminal-surfaces.md
- docs/implementation/rust-codex-refactor.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Document the trace and replay surfaces operators can rely on once runtime audit becomes first-class.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- explain the trace bundle and replay story in terms of durable operator evidence, not debug logging
- keep the docs aligned with the real commands and artifacts the code produces
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- docs/guides/terminal-surfaces.md
- docs/implementation/rust-codex-refactor.md
```
