+++
id = 10
slug = "authority-core"
title = "Land the Wave 0.2 authority core"
mode = "dark-factory"
owners = ["architecture", "control"]
depends_on = [9]
validation = ["cargo test", "cargo run -p wave-cli -- project show --json", "cargo run -p wave-cli -- doctor --json"]
rollback = ["Disable the Wave 0.2 authority roots and continue projecting queue and replay truth from compatibility run records until the reducer cutover is ready."]
proof = ["Cargo.toml", "wave.toml", "README.md", "docs/README.md", "crates/wave-domain/src/lib.rs", "crates/wave-events/src/lib.rs", "crates/wave-coordination/src/lib.rs", "crates/wave-config/src/lib.rs", "crates/wave-cli/src/main.rs", "docs/reference/runtime-config/README.md", "docs/implementation/rust-codex-refactor.md", "docs/plans/master-plan.md", "docs/plans/current-state.md", "docs/plans/component-cutover-matrix.md", "docs/plans/component-cutover-matrix.json"]
+++
# Wave 10 - Land the Wave 0.2 authority core

**Commit message**: `Feat: land authority core and typed authority roots`

## Component promotions
- authority-core-domain: repo-landed
- authority-event-log: repo-landed
- authority-coordination-log: repo-landed
- typed-authority-roots: repo-landed

## Deploy environments
- repo-local: custom default (repo-local authority-core work only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Event-sourced control planes, durable coordination records, and typed authority models for a Rust orchestration runtime"

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
- repo-wave-closure-markers

### File ownership
- .wave/reviews/wave-10-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether the Wave 0.2 authority-core landing is real, typed, and honest about what still remains on compatibility run records.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/current-state.md.

Specific expectations:
- do not PASS unless the repo now has typed authority crates plus explicit canonical authority roots under `.wave/state/`
- treat docs that imply reducer cutover or authoritative event use before the code actually does it as blocking
- do not block on A0 appearing as `running` in the active run record while you are executing; judge closure from landed artifacts and prior-stage markers instead
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-10-cont-qa.md
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
- repo-wave-closure-markers

### File ownership
- .wave/integration/wave-10.md
- .wave/integration/wave-10.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile the new domain, event, coordination, config, CLI, and documentation slices into one authority-core closure verdict.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/component-cutover-matrix.md.

Specific expectations:
- treat any mismatch between typed authority roots, the CLI doctor surface, and shared-plan claims as an integration failure
- decide ready-for-doc-closure only when the authority-core crates, config contract, and docs all describe the same compatibility boundary
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-10.md
- .wave/integration/wave-10.json
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
- Keep the shared-plan docs and component matrix aligned with the authority-core landing and its exact compatibility boundary.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- record Wave 10 as the authority-core landing and move the next executable work to the reducer/projection cutover
- keep the component matrix honest about what is repo-landed now versus what still depends on compatibility run records
- do not mark cont-QA closed before A0 runs; shared-plan docs may describe the authority-core landing and next wave, but final QA closure belongs to the A0 gate
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/migration.md
- docs/plans/component-cutover-matrix.md
- docs/plans/component-cutover-matrix.json
```

## Agent A1: Authority Domain And Durable Logs

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Typed task seeds, durable control events, and coordination records for an event-oriented Rust control plane"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- authority-core-domain
- authority-event-log
- authority-coordination-log

### Capabilities
- typed-task-seeds
- append-only-control-events
- durable-coordination-records

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-domain/Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/Cargo.toml
- crates/wave-events/src/lib.rs
- crates/wave-coordination/Cargo.toml
- crates/wave-coordination/src/lib.rs

### File ownership
- Cargo.toml
- crates/wave-domain/Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/Cargo.toml
- crates/wave-events/src/lib.rs
- crates/wave-coordination/Cargo.toml
- crates/wave-coordination/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the authority-core domain model plus durable append/query primitives for control and coordination state.

Required context before coding:
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read Cargo.toml.

Specific expectations:
- add typed task, attempt, gate, contradiction, fact, proof, rerun, and human-input structures that later reducer work can reuse
- map authored waves into declared task seeds with explicit closure dependencies
- add append/query helpers for canonical control and coordination logs under `.wave/state/events/`
- keep tests close to the new crates so later cutover waves can extend them instead of rewriting them
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-domain/Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/Cargo.toml
- crates/wave-events/src/lib.rs
- crates/wave-coordination/Cargo.toml
- crates/wave-coordination/src/lib.rs
```

## Agent A2: Typed Authority Roots And Doctor Surface

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Typed config roots, doctor surfaces, and project-state path resolution for a Rust orchestration runtime"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- typed-authority-roots

### Capabilities
- typed-config-roots
- project-show-surface
- authority-health-checks

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- wave.toml
- crates/wave-config/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-control-plane/src/lib.rs
- crates/wave-runtime/src/lib.rs

### File ownership
- wave.toml
- crates/wave-config/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-control-plane/src/lib.rs
- crates/wave-runtime/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Make the canonical authority roots a typed project-config contract and surface them through `project show` and `doctor`.

Required context before coding:
- Read wave.toml.
- Read crates/wave-config/src/lib.rs.
- Read crates/wave-cli/src/main.rs.
- Read docs/reference/runtime-config/README.md.

Specific expectations:
- port the authority roots and role-prompt paths into typed config defaults and resolved paths
- make `wave project show` and `wave doctor` expose the new roots clearly without pretending the reducer cutover already happened
- materialize the canonical `.wave/state/` authority roots on disk during runtime bootstrap so the landed roots are real, not only declared
- keep `wave doctor` honest about the authority roots it reports by distinguishing between typed configured paths and materialized canonical roots
- keep compatibility run-state tests working while the authority roots are still additive
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- wave.toml
- crates/wave-config/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-control-plane/src/lib.rs
- crates/wave-runtime/src/lib.rs
```

## Agent A3: Authority-Core Repo Guidance

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Architecture-baseline docs and runtime reference guidance for an authority-core cutover in a Rust operator repo"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- typed-authority-roots

### Capabilities
- runtime-reference-docs
- operator-guidance
- compatibility-boundary-docs

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- README.md
- docs/README.md
- docs/reference/runtime-config/README.md
- docs/implementation/rust-codex-refactor.md

### File ownership
- README.md
- docs/README.md
- docs/reference/runtime-config/README.md
- docs/implementation/rust-codex-refactor.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Update the repo guidance and architecture baseline so they describe the new authority roots and the remaining compatibility boundary precisely.

Required context before coding:
- Read README.md.
- Read docs/reference/runtime-config/README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.

Specific expectations:
- document the canonical authority roots under `.wave/state/`
- label `.wave/state/runs/` and `.wave/traces/runs/` as compatibility outputs until later cutover waves replace them
- keep the docs explicit that this wave lands authority-core scaffolding, not reducer-backed queue truth
- stop routing readers from `docs/README.md` into coordination-as-authority docs or package-era operator runbooks that the current Rust CLI does not ship yet
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- README.md
- docs/README.md
- docs/reference/runtime-config/README.md
- docs/implementation/rust-codex-refactor.md
```
