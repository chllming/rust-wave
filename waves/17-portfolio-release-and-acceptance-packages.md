+++
id = 17
slug = "portfolio-release-and-acceptance-packages"
title = "Land portfolio, release, and acceptance packages"
mode = "dark-factory"
owners = ["architecture", "delivery"]
depends_on = [16]
validation = ["cargo test -p wave-domain -p wave-reducer -p wave-projections --locked", "cargo test -p wave-cli -p wave-app-server -p wave-tui --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control show --wave 17 --json", "cargo run -p wave-cli -- project show --json"]
rollback = ["Collapse portfolio, release, and acceptance-package state back into informational projections or docs-only summaries, and treat ship or no-ship reasoning as operator procedure until the delivery layer regains parity."]
proof = ["Cargo.toml", "crates/wave-domain/src/lib.rs", "crates/wave-reducer/src/lib.rs", "crates/wave-projections/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-tui/src/lib.rs", "docs/implementation/parallel-wave-multi-runtime-architecture.md", "docs/implementation/design.md", "docs/plans/master-plan.md", "docs/plans/current-state.md", "docs/plans/full-cycle-waves.md"]
+++
# Wave 17 - Land portfolio, release, and acceptance packages

**Commit message**: `Feat: land portfolio and release delivery model`

## Component promotions
- portfolio-delivery-model: baseline-proved
- release-promotion-model: baseline-proved
- acceptance-package-spine: pilot-live

## Deploy environments
- repo-local: custom default (repo-local portfolio and release-model landing only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Portfolio state above waves, release trains, outcome contracts, rollout readiness, and acceptance packages"

## Wave contract
- The harness must grow a delivery layer above waves so it can reason about initiatives, milestones, release trains, outcomes, and ship/no-ship status.
- Release and promotion semantics must become first-class state rather than implicit conclusions from wave completion.
- Acceptance packages must connect design intent, implementation proof, release state, known risk, and unresolved debt into one durable delivery artifact.

## Live proof expectations
- Show one initiative or outcome contract aggregating multiple waves.
- Show one release or promotion object moving through explicit readiness states.
- Show one acceptance package explaining why something is or is not ready to ship.
- Show known risk and outstanding debt as durable delivery state, not only prose.
- Show proof, acceptance, risk, and debt visibility that fits the `Proof` and delivery-facing `Overview` UX in `docs/implementation/design.md`.

## Quality control expectations
- Add deterministic tests for initiative aggregation, release-state transitions, acceptance-package assembly, and known-risk or debt propagation.
- Prove that ship or no-ship state is derived from durable delivery objects rather than inferred from a single wave completion bit.
- Require at least one operator-facing proof surface that can explain why a release is blocked, ready, or rejected.
- Treat any delivery-layer design that leaves ship reasoning as undocumented operator procedure as a failure of this wave.

## Documentation closure expectations
- Update architecture and full-cycle docs to explain why waves are necessary but not sufficient for delivery truth.
- Record the initiative, release, and acceptance-package model clearly enough that later waves can build on it without reinterpreting the delivery layer.
- Include one live proof walkthrough showing multiple waves rolled up into one delivery or release decision.

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
- .wave/reviews/wave-17-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 17 turns the harness into a delivery-aware system with portfolio, release, and acceptance-package state above waves.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/full-cycle-waves.md.
- Read docs/plans/current-state.md.

Specific expectations:
- do not PASS unless the delivery layer can explain more than local wave readiness
- require portfolio or release truth to be durable and reducer-visible
- require acceptance packages to connect design intent, implementation proof, release readiness, risk, and debt
- require live proof that a ship or no-ship state can be inspected directly
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-17-cont-qa.md
```

## Agent A6: Design Review Steward

### Role prompts
- docs/agents/wave-design-role.md

### Executor
- profile: review-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: none
- query: "Repository docs remain canonical for design review"

### Skills
- wave-core
- role-design
- tui-design
- repo-wave-closure-markers

### File ownership
- .wave/design/wave-17.md

### Final markers
- [wave-design]

### Prompt
```text
Primary goal:
- Review Wave 17 against docs/implementation/design.md and judge whether proof, acceptance, release readiness, risk, and debt are surfaced clearly enough for delivery-facing operator UX.

Required context before coding:
- Read docs/implementation/design.md.
- Read docs/plans/full-cycle-waves.md.
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.

Specific expectations:
- treat docs/implementation/design.md as the canonical review source
- require the operator surface to distinguish wave success from release readiness, and to show proof, acceptance, risk, and debt in a way that matches the Proof and delivery-facing Overview UX
- treat missing or misleading delivery-facing acceptance visibility as a blocking design defect
- keep the review report concise and end with the final [wave-design] state=<aligned|concerns|blocked> findings=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/design/wave-17.md
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
- .wave/integration/wave-17.md
- .wave/integration/wave-17.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile portfolio state, release promotion state, acceptance packages, and operator-facing delivery views into one integration verdict.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/full-cycle-waves.md.

Specific expectations:
- require the delivery layer to sit above waves rather than pretending wave completion is enough for ship readiness
- require release state, known risk, and outstanding debt to be explicit and inspectable
- require live proof that multiple waves can aggregate into one coherent initiative or release view
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
- repo-rust-control-plane
- repo-wave-closure-markers

### File ownership
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/full-cycle-waves.md

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep the architecture and plan docs aligned with the portfolio, release, and acceptance-package delivery layer.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/full-cycle-waves.md.

Specific expectations:
- explain why waves are necessary but not sufficient for delivery truth
- document the initiative, milestone, release-train, and acceptance-package layer clearly
- include live proof expectations for release and ship-state inspection
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/full-cycle-waves.md
```

## Agent A1: Portfolio Layer

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Portfolio state above waves, initiatives, milestones, release trains, outcome contracts, and delivery packets"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- portfolio-delivery-model

### Capabilities
- initiative-state
- milestone-state
- release-train-state

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs

### File ownership
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land a portfolio layer above waves so initiatives, milestones, release trains, and outcome contracts become first-class reducer state.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/full-cycle-waves.md.
- Read crates/wave-domain/src/lib.rs.

Specific expectations:
- add portfolio-level delivery concepts above waves
- allow reducer or projections to aggregate multiple waves into one delivery view
- preserve wave-level truth while adding the higher-level model
- prove with fixtures or live proof artifacts that one initiative can aggregate multiple waves coherently
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs
```

## Agent A2: Release And Acceptance Packages

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Release objects, promotion state, rollout readiness, ship decisions, known risks, and outstanding debt sets"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- release-promotion-model
- acceptance-package-spine

### Capabilities
- release-object
- promotion-state
- acceptance-package

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs

### File ownership
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
- Land release and acceptance-package semantics so the harness can explain why something is ready or not ready to ship.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/current-state.md.
- Read crates/wave-cli/src/main.rs.

Specific expectations:
- introduce release or promotion state beyond simple wave completion
- make known risks and outstanding debt first-class delivery state
- produce a durable acceptance-package concept that ties design intent, implementation proof, release state, and signoff together
- surface ship or no-ship state in operator-facing views or proof artifacts
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
```
