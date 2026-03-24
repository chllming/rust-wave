+++
id = 12
slug = "result-envelope-proof-lifecycle"
title = "Land result envelopes and proof lifecycle"
mode = "dark-factory"
owners = ["architecture", "control"]
depends_on = [11]
validation = ["cargo test -p wave-results -p wave-gates -p wave-runtime --locked", "cargo test -p wave-app-server -p wave-cli -p wave-trace --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control proof show --wave 12 --json", "cargo run -p wave-cli -- trace latest --wave 12 --json"]
rollback = ["Route proof lifecycle and closure input back through the current marker-first compatibility path until structured envelopes reach parity, while keeping any newly written envelope artifacts as derived compatibility data only."]
proof = ["Cargo.toml", "crates/wave-domain/src/lib.rs", "crates/wave-results/Cargo.toml", "crates/wave-results/src/lib.rs", "crates/wave-gates/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-trace/src/lib.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-cli/src/main.rs", "docs/implementation/rust-codex-refactor.md", "docs/reference/runtime-config/README.md", "docs/plans/master-plan.md", "docs/plans/current-state.md", "docs/plans/migration.md", "docs/plans/component-cutover-matrix.md", "docs/plans/component-cutover-matrix.json"]
+++
# Wave 12 - Land result envelopes and proof lifecycle

**Commit message**: `Feat: cut proof lifecycle over to structured result envelopes`

## Component promotions
- result-envelope-lifecycle: repo-landed
- proof-lifecycle-spine: repo-landed
- envelope-first-closure: repo-landed

## Deploy environments
- repo-local: custom default (repo-local proof lifecycle cutover only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Structured result envelopes, proof lifecycle cutover, legacy marker adapters, and closure gates over typed results"

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
- .wave/reviews/wave-12-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 12 really moves proof lifecycle and closure input onto structured result envelopes without overstating the remaining compatibility boundary.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-wave-0.3-notes.md.
- Read docs/plans/current-state.md.

Specific expectations:
- do not PASS unless `wave control proof show`, the app-server proof snapshot, and closure decisions all read structured result envelopes or an explicit legacy adapter path instead of raw free-form text scans
- treat direct marker scanning outside the `wave-results` compatibility adapter as a blocking failure
- require tests for envelope serialization, legacy compatibility adaptation, and closure parity over the same proof semantics
- do not PASS unless operator-facing proof surfaces work for the latest completed or failed wave as well as active runs; active-run-only proof is a blocking regression
- require persisted envelope, run, and trace artifact paths to reload correctly from repo-local state; relative-path readback failures are blocking defects, not follow-up cleanup
- map every PASS or BLOCKED claim to exact envelope fixtures, adapter artifacts, or validation commands; stale earlier readiness claims do not survive contrary later evidence
- keep the compatibility boundary honest: replay can remain compatibility-backed in this wave, but proof lifecycle and closure input must no longer depend directly on ad hoc text parsing
- require live proof and evidence surfaces to recompute from current stored envelope or explicit compatibility-adapter state rather than trusting stale embedded snapshots alone
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-12-cont-qa.md
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
- .wave/integration/wave-12.md
- .wave/integration/wave-12.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile the result-envelope crate, gates, runtime, proof surfaces, compatibility adapters, and docs into one envelope-first proof-lifecycle verdict.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-wave-0.3-notes.md.
- Read docs/plans/component-cutover-matrix.md.

Specific expectations:
- treat any mismatch between stored result envelopes, closure-gate input, and operator-facing proof surfaces as an integration failure
- decide ready-for-doc-closure only when structured envelopes are the primary machine contract and the legacy marker path is visibly demoted to compatibility adapter behavior
- require proof consumers to resolve the latest relevant attempt for completed and failed waves, not only active-run details
- require repo-root-stable artifact readback across envelopes, compatibility runs, and traces; path normalization bugs that break proof or replay visibility still block this wave
- require the docs to keep the next control-discipline work explicit: Wave 13 owns post-agent gates, Wave 14 owns targeted mid-wave checkpoints, and Wave 19 owns planner-emitted invariants and staged gate plans
- name the exact proof surface, owner, and resolution condition for every blocker; summary-level parity is not enough if a single proof consumer still relies on raw marker scans
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-12.md
- .wave/integration/wave-12.json
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
- docs/plans/component-cutover-matrix.md
- docs/plans/component-cutover-matrix.json

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep the shared-plan docs and component matrix aligned with the result-envelope and proof-lifecycle landing and its exact compatibility boundary.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-wave-0.3-notes.md.
- Read docs/plans/current-state.md.

Specific expectations:
- record Wave 12 as the result-envelope and proof-lifecycle landing and move the next executable work to Wave 13 runtime breakup plus post-agent gate foundations
- keep the component matrix honest about what is envelope-first now versus what still depends on compatibility trace or replay artifacts
- describe proof surfaces as envelope-first for active and latest completed or failed runs, not as active-run-only views
- explicitly carry the hardening mapping forward in the shared-plan docs: Wave 13 for post-agent gates, Wave 14 for targeted mid-wave checkpoints and retry, Wave 19 for planner-emitted invariants and staged gate plans
- do not mark cont-QA closed before A0 runs; shared-plan docs may describe the landing and next wave, but final QA closure belongs to the A0 gate
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/migration.md
- docs/plans/component-cutover-matrix.md
- docs/plans/component-cutover-matrix.json
```

## Agent A1: Result Envelope Core

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Structured result envelopes, typed proof artifacts, compatibility adapters, and immutable attempt-scoped result storage"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- result-envelope-lifecycle
- envelope-first-closure

### Capabilities
- typed-result-envelope
- legacy-marker-normalization
- proof-artifact-normalization

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-results/Cargo.toml
- crates/wave-results/src/lib.rs

### File ownership
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-results/Cargo.toml
- crates/wave-results/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land `wave-results` as the structured result-envelope layer and align the typed domain surface so proof, doc-delta, and closure input become machine-readable artifacts.

Required context before coding:
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-wave-0.3-notes.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read crates/wave-domain/src/lib.rs.

Specific expectations:
- introduce `wave-results` as the immutable attempt-scoped result-envelope layer with typed storage, validation, and legacy marker adaptation
- keep result-envelope schema aligned with the existing `wave-domain` authority types instead of inventing a second result contract
- normalize proof artifacts, final markers, doc-delta state, and closure input into typed envelope payloads while keeping human-readable markers as evidence only
- model partial and failed attempts explicitly enough that later proof surfaces can render the latest completed or failed run without falling back to marker heuristics
- keep the legacy marker path visibly isolated as compatibility adapter logic inside `wave-results`
- land deterministic tests that cover fresh envelope writes and adaptation of legacy marker-first run artifacts
- in the final proof summary, map each deliverable to the exact test, fixture, or artifact that proves it and name which paths remain compatibility-only
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-results/Cargo.toml
- crates/wave-results/src/lib.rs
```

## Agent A2: Gate, Runtime, And Proof Surface Cutover

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Envelope-first closure gates, proof snapshots, runtime result persistence, and compatibility-backed replay boundaries"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-codex-orchestrator
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- proof-lifecycle-spine
- envelope-first-closure
- control-proof-surface

### Capabilities
- closure-gate-cutover
- proof-snapshot-cutover
- compatibility-proof-adapter

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-gates/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-trace/src/lib.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/implementation/rust-codex-refactor.md
- docs/reference/runtime-config/README.md

### File ownership
- crates/wave-gates/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-trace/src/lib.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/implementation/rust-codex-refactor.md
- docs/reference/runtime-config/README.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Move runtime closure evaluation, proof snapshots, and operator-facing proof surfaces onto structured result envelopes while keeping replay compatibility explicit.

Required context before coding:
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-wave-0.3-notes.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read crates/wave-runtime/src/lib.rs.
- Read crates/wave-app-server/src/lib.rs.

Specific expectations:
- route runtime result persistence through `wave-results` so launched agents write structured envelopes alongside the existing compatibility artifacts
- cut gate and closure evaluation over to envelope-backed input and isolate any remaining text parsing behind the explicit compatibility adapter
- update CLI proof surfaces and app-server snapshots so operator-facing proof state reflects typed envelopes rather than inferred marker completeness alone
- make proof surfaces resolve the latest relevant run for completed and failed waves as well as active ones; do not leave `wave control proof show` or app-server proof views active-run-only
- normalize persisted envelope, run, and trace artifact paths on write or load so repo-local proof and replay consumers remain stable when re-reading stored state
- keep replay and trace compatibility boundaries honest in this wave: replay may still depend on compatibility run and trace artifacts even after proof lifecycle moves to envelopes
- recompute operator-facing proof and evidence from current stored envelope or compatibility-adapter state rather than trusting stale embedded trace snapshots alone
- prove parity across `wave control proof show`, app-server proof snapshots, and closure-gate input from the same envelope truth, then cite the exact commands or fixtures in the final proof
- update the live Rust implementation and runtime-reference docs to explain the new envelope-first proof boundary and the upcoming Wave 13 post-agent gate work
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-gates/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-trace/src/lib.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/implementation/rust-codex-refactor.md
- docs/reference/runtime-config/README.md
```
