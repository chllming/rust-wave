+++
id = 11
slug = "reducer-projection-spine"
title = "Land the reducer-backed planning and projection spine"
mode = "dark-factory"
owners = ["architecture", "control"]
depends_on = [10]
validation = ["cargo test -p wave-reducer -p wave-gates -p wave-projections --locked", "cargo test -p wave-runtime -p wave-trace -p wave-control-plane -p wave-app-server -p wave-tui -p wave-cli --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control status --json", "cargo run -p wave-cli -- project show --json"]
rollback = ["Route planning, status, app-server, and TUI queue/control surfaces back through the compatibility-backed `wave-control-plane` implementation until reducer-backed projections reach parity again."]
proof = ["Cargo.toml", "crates/wave-reducer/src/lib.rs", "crates/wave-gates/src/lib.rs", "crates/wave-projections/src/lib.rs", "crates/wave-control-plane/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-trace/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-tui/src/lib.rs", "docs/implementation/rust-codex-refactor.md", "docs/guides/terminal-surfaces.md", "docs/plans/master-plan.md", "docs/plans/current-state.md", "docs/plans/migration.md", "docs/plans/component-cutover-matrix.md", "docs/plans/component-cutover-matrix.json"]
+++
# Wave 11 - Land the reducer-backed planning and projection spine

**Commit message**: `Feat: cut planning and status over to reducer-backed projections`

## Component promotions
- reducer-state-spine: baseline-proved
- gate-verdict-spine: baseline-proved
- planning-status: baseline-proved
- queue-json-surface: baseline-proved

## Deploy environments
- repo-local: custom default (repo-local reducer and projection cutover only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Pure Rust reducers, typed gate verdicts, queue/read-model projections, and operator-facing status surfaces over compatibility-backed inputs"

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
- .wave/reviews/wave-11-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 11 really moves planning and operator projections onto a reducer-backed spine without overstating the remaining compatibility boundary.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/plans/current-state.md.

Specific expectations:
- do not PASS unless `wave control status`, the app-server snapshot, and the TUI queue/control surfaces all derive from the same reducer-backed projection contract
- treat a `wave-control-plane` crate that still contains the authoritative planning logic instead of a shim or forwarding layer as a blocking failure
- require deterministic reducer and blocker fixtures that prove parity with the current queue and closure semantics
- require row-level queue state to preserve explicit readiness truth, including `active`; blocker strings alone are not enough to prove parity
- require compatibility adapter inputs to remain scheduler-safe in this wave: dry-run or preflight paths must not clear rerun intent or create synthetic latest-run truth that pollutes the reducer-backed queue
- map every PASS or BLOCKED claim to exact fixture paths, commands, or surfaced snapshots; stale earlier readiness claims do not survive contrary later evidence
- keep the compatibility boundary honest: compatibility run records may remain reducer inputs in this wave, but they must no longer be the direct planning implementation surface
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-11-cont-qa.md
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
- .wave/integration/wave-11.md
- .wave/integration/wave-11.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile the reducer, gates, projections, shim, CLI, app-server, TUI, and docs slices into one reducer/projection cutover verdict.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/plans/component-cutover-matrix.md.

Specific expectations:
- treat any mismatch between reducer output, projection output, and operator-facing status surfaces as an integration failure
- decide ready-for-doc-closure only when queue readiness, blocker truth, closure coverage, and operator snapshots are all reducer-backed while the compatibility-input boundary remains explicit
- require the `wave-control-plane` crate to be visibly demoted to shim or forwarding behavior instead of keeping the old implementation as hidden authority
- require compatibility-backed runtime inputs to preserve scheduler truth: dry-run or preflight cannot clear reruns, create faux completed runs, or otherwise perturb projection readiness
- name the exact reducer/projection consumer surface, owner, and resolution condition for every blocker; summary-level parity is not enough if row-level queue state still disagrees
- require explicit readiness parity at row level: `active` must survive through projection, app-server, and TUI queue rows instead of being reconstructed from blocker strings
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-11.md
- .wave/integration/wave-11.json
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
- Keep the shared-plan docs and component matrix aligned with the reducer/projection spine landing and its exact compatibility boundary.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/plans/current-state.md.

Specific expectations:
- record Wave 11 as the reducer/projection spine landing and move the next executable work to the result-envelope and proof-lifecycle migration
- keep the component matrix honest about what is baseline-proved now versus what still depends on compatibility run records or trace bundles
- do not mark cont-QA closed before A0 runs; shared-plan docs may describe the landing and next wave, but final QA closure belongs to the A0 gate
- make the remaining compatibility boundary explicit: reducer-backed planning and operator projections are live, but result envelopes, proof lifecycle, and replay ratification still remain later work
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/migration.md
- docs/plans/component-cutover-matrix.md
- docs/plans/component-cutover-matrix.json
```

## Agent A1: Reducer And Gate Core

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Pure reducers, typed gate verdicts, compatibility-backed inputs, and deterministic queue and blocker semantics"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- reducer-state-spine
- gate-verdict-spine

### Capabilities
- pure-planning-reducer
- typed-gate-verdicts
- compatibility-input-adapter

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-reducer/Cargo.toml
- crates/wave-reducer/src/lib.rs
- crates/wave-gates/Cargo.toml
- crates/wave-gates/src/lib.rs

### File ownership
- Cargo.toml
- crates/wave-reducer/Cargo.toml
- crates/wave-reducer/src/lib.rs
- crates/wave-gates/Cargo.toml
- crates/wave-gates/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the pure reducer and typed gate core that turns authored waves plus compatibility-backed run inputs into deterministic queue, blocker, closure, and gate state.

Required context before coding:
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read crates/wave-control-plane/src/lib.rs.

Specific expectations:
- introduce `wave-reducer` as a pure state reducer over authored waves, lint findings, rerun intents, and compatibility run inputs
- move blocker classification, readiness state, per-wave lifecycle summaries, and closure contract facts behind reducer output instead of leaving them as ad hoc control-plane logic
- introduce `wave-gates` with typed gate and closure verdict helpers that projections can consume without re-deriving semantics from run records
- keep compatibility run records as explicit adapter inputs in this wave rather than pretending canonical events or result envelopes already exist
- land deterministic tests in the new crates that prove parity with the current queue, blocker, and closure semantics before the consumer cutover happens
- in the final proof summary, map each deliverable to the exact test, fixture, or artifact that proves it and name which inputs remain compatibility adapters rather than authority
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-reducer/Cargo.toml
- crates/wave-reducer/src/lib.rs
- crates/wave-gates/Cargo.toml
- crates/wave-gates/src/lib.rs
```

## Agent A2: Projection And Consumer Cutover

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Reducer-backed read models, queue and status projections, app-server snapshots, and terminal operator consumers"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-codex-orchestrator
- repo-ratatui-operator
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- planning-status
- queue-json-surface
- reducer-backed-projections

### Capabilities
- projection-cutover
- operator-snapshot-unification
- control-plane-shim

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-projections/Cargo.toml
- crates/wave-projections/src/lib.rs
- crates/wave-control-plane/Cargo.toml
- crates/wave-control-plane/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-trace/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
- docs/implementation/rust-codex-refactor.md
- docs/guides/terminal-surfaces.md

### File ownership
- crates/wave-projections/Cargo.toml
- crates/wave-projections/src/lib.rs
- crates/wave-control-plane/Cargo.toml
- crates/wave-control-plane/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-trace/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
- docs/implementation/rust-codex-refactor.md
- docs/guides/terminal-surfaces.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Move planning and operator-facing status surfaces onto reducer-backed projections and demote `wave-control-plane` to a shim or forwarding layer.

Required context before coding:
- Read docs/implementation/rust-wave-0.2-architecture.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read crates/wave-app-server/src/lib.rs.
- Read crates/wave-tui/src/lib.rs.

Specific expectations:
- introduce `wave-projections` as the human-facing read-model layer for planning status, queue projections, control/status projections, and operator snapshot inputs
- cut `wave-cli` doctor and control-status rendering, `wave-app-server` snapshot assembly, and the TUI queue/control surfaces over to the new projection contract
- reduce `wave-control-plane` to shim or forwarding behavior only; it must not keep the authoritative reducer or projection implementation hidden behind old names
- keep compatibility adapter inputs truthful enough for the reducer spine: dry-run or preflight must not write durable run state, clear reruns, or otherwise perturb queue truth
- preserve the existing CLI and operator semantics closely enough that deterministic reducer and projection fixtures still match the current surfaces
- preserve explicit readiness state through queue rows; do not re-derive `active` or `blocked` labels from blocker strings alone
- prove row-level and summary-level parity across `wave control status`, app-server snapshots, and TUI queue/control views from the same projection truth, then cite the exact commands or fixtures in the final proof
- if compatibility run or trace path handling prevents repo-local proof or trace readback from the same stored state, treat fixing those adapter semantics in runtime/trace as in scope for this wave
- update the live Rust implementation docs to explain that planning and operator projections are now reducer-backed even though compatibility run records remain adapter inputs for this stage
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-projections/Cargo.toml
- crates/wave-projections/src/lib.rs
- crates/wave-control-plane/Cargo.toml
- crates/wave-control-plane/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-trace/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
- docs/implementation/rust-codex-refactor.md
- docs/guides/terminal-surfaces.md
```
