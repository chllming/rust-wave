+++
id = 15
slug = "runtime-policy-and-multi-runtime-adapters"
title = "Land runtime policy and multi-runtime adapters"
mode = "dark-factory"
owners = ["architecture", "runtime"]
depends_on = [14]
validation = ["cargo test -p wave-domain -p wave-results --locked", "cargo test -p wave-runtime -p wave-cli -p wave-app-server --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control show --wave 15 --json", "cargo run -p wave-cli -- project show --json"]
rollback = ["Route execution back through the current Codex-only runtime path, keep any executor API or runtime-policy artifacts as non-authoritative scaffolding, and leave skill projection explicit per-agent until runtime plurality reaches parity."]
proof = ["Cargo.toml", "crates/wave-domain/src/lib.rs", "crates/wave-results/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-app-server/src/lib.rs", "docs/reference/runtime-config/README.md", "docs/reference/runtime-config/codex.md", "docs/reference/runtime-config/claude.md", "docs/reference/skills.md", "docs/implementation/parallel-wave-multi-runtime-architecture.md", "docs/plans/master-plan.md", "docs/plans/current-state.md"]
+++
# Wave 15 - Land runtime policy and multi-runtime adapters

**Commit message**: `Feat: land runtime policy and multi-runtime adapter boundary`

## Component promotions
- executor-api-spine: baseline-proved
- runtime-policy-engine: baseline-proved
- runtime-aware-skill-projection: repo-landed
- runtime-plurality: pilot-live

## Deploy environments
- repo-local: custom default (repo-local runtime-policy and multi-runtime cutover only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Executor API boundaries, Codex and Claude adapter seams, runtime policy, and late-bound skill projection"

## Wave contract
- The same authored wave contract must be explainable and executable through Codex and Claude without changing reducer semantics.
- Runtime selection must move into explicit policy, not launcher-local folklore.
- Skill projection must become late-bound after final runtime selection and fallback.
- This wave may use a fixture-backed or dry-run-backed Claude proof path if a live Claude binary is unavailable, but the adapter contract and operator-visible runtime identity must still be real.
- The scheduler remains the owner of queue semantics; runtime policy only decides how an assigned task should run.

## Live proof expectations
- Show the same wave contract resolved against Codex and Claude paths through one runtime-neutral interface.
- Show runtime identity, selected adapter, and any fallback metadata visible in stored state or projections.
- Show runtime-aware skill overlays recomputed after runtime selection.
- Keep docs honest about whether the Claude proof is live or fixture-backed in this wave.

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
- .wave/reviews/wave-14-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 15 introduces a real runtime-policy and multi-runtime adapter boundary without leaking runtime-specific semantics back into the core wave model.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/reference/runtime-config/README.md.
- Read docs/reference/runtime-config/claude.md.
- Read docs/reference/skills.md.

Specific expectations:
- do not PASS unless the same authored wave contract is demonstrably routed through Codex and a Claude path or fixture-backed equivalent through one shared execution model
- require runtime policy to be explicit: runtime selection, fallback, and allowed runtime mix must not remain hidden launch-time judgment
- treat runtime-specific skill overlays resolved before runtime choice as a blocking defect
- require live proof or fixture-backed proof that operators can inspect, not just adapter trait definitions
- keep docs honest about what is truly live versus target-state in this wave
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-14-cont-qa.md
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
- .wave/integration/wave-14.md
- .wave/integration/wave-14.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile runtime policy, executor boundary, Codex and Claude adapters, late-bound skill projection, and operator/runtime docs into one multi-runtime verdict.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/reference/runtime-config/README.md.
- Read docs/reference/skills.md.
- Read docs/plans/current-state.md.

Specific expectations:
- require adapter parity at the semantic layer even if runtime-specific artifacts differ
- require runtime identity and fallback metadata to be visible in authoritative or projected state
- treat a multi-runtime design that leaves policy implicit as an integration failure
- require late-bound runtime skill projection, not static pre-runtime skill attachment masquerading as policy
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-14.md
- .wave/integration/wave-14.json
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
- docs/reference/runtime-config/README.md
- docs/reference/runtime-config/codex.md
- docs/reference/runtime-config/claude.md
- docs/reference/skills.md
- docs/plans/master-plan.md
- docs/plans/current-state.md

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep the runtime and architecture docs aligned with the new runtime-policy layer and sibling Codex and Claude adapter model.

Required context before coding:
- Read docs/reference/runtime-config/README.md.
- Read docs/reference/runtime-config/codex.md.
- Read docs/reference/runtime-config/claude.md.
- Read docs/reference/skills.md.

Specific expectations:
- record Wave 15 as the runtime-policy and runtime-plurality cutover wave
- keep docs explicit about whether Claude proof in this wave is live or fixture-backed
- explain late-bound runtime skill projection clearly
- state that queue semantics still belong to scheduler and reducer layers, not runtime policy
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/reference/runtime-config/README.md
- docs/reference/runtime-config/codex.md
- docs/reference/runtime-config/claude.md
- docs/reference/skills.md
- docs/plans/master-plan.md
- docs/plans/current-state.md
```

## Agent A1: Executor API And Codex Adapter

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Executor APIs, launch specs, Codex adapter extraction, runtime identity, and adapter parity"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- executor-api-spine
- runtime-plurality

### Capabilities
- executor-api
- codex-adapter-split
- runtime-identity-tracking

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/reference/runtime-config/codex.md

### File ownership
- Cargo.toml
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/reference/runtime-config/codex.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Extract a runtime-neutral execution boundary and make Codex a first-class adapter behind it.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/reference/runtime-config/codex.md.
- Read crates/wave-runtime/src/lib.rs.

Specific expectations:
- separate runtime-neutral launch specification and execution identity from Codex-specific invocation details
- preserve current Codex behavior through the new boundary
- expose runtime identity so later projections and traces can explain which adapter ran the work
- cite exact tests or proof commands in the final proof
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/reference/runtime-config/codex.md
```

## Agent A2: Claude Adapter And Skill Projection

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Claude adapter seams, runtime-aware skill overlays, and late-bound runtime projection"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- runtime-plurality
- runtime-aware-skill-projection

### Capabilities
- claude-adapter
- late-bound-skill-projection
- runtime-fallback-metadata

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-domain/src/lib.rs
- crates/wave-results/src/lib.rs
- docs/reference/runtime-config/claude.md
- docs/reference/skills.md

### File ownership
- crates/wave-domain/src/lib.rs
- crates/wave-results/src/lib.rs
- docs/reference/runtime-config/claude.md
- docs/reference/skills.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the Claude adapter path and late-bound runtime-aware skill projection so runtime plurality becomes a real architectural seam.

Required context before coding:
- Read docs/reference/runtime-config/claude.md.
- Read docs/reference/skills.md.
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.

Specific expectations:
- add a Claude adapter path or fixture-backed equivalent through the same execution boundary as Codex
- resolve runtime-specific skill overlays after final runtime selection
- keep runtime-specific fields and overlays out of reducer semantics
- expose fallback or runtime-choice metadata as durable execution evidence
- in the final proof, state clearly whether the Claude path was exercised live or through a fixture-backed proof surface
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-domain/src/lib.rs
- crates/wave-results/src/lib.rs
- docs/reference/runtime-config/claude.md
- docs/reference/skills.md
```

## Agent A3: Runtime Policy And Live Proof

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Runtime selection policy, fallback order, capability floors, sandbox rules, and operator-facing runtime proof"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- runtime-policy-engine
- runtime-plurality

### Capabilities
- runtime-selection-policy
- fallback-policy
- live-runtime-proof

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-app-server/src/lib.rs
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/current-state.md

### File ownership
- crates/wave-app-server/src/lib.rs
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/current-state.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Make runtime selection and fallback a real policy layer and produce live proof that the multi-runtime boundary is inspectable by operators.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/current-state.md.
- Read crates/wave-app-server/src/lib.rs.

Specific expectations:
- introduce explicit runtime policy concepts rather than leaving selection to launcher folklore
- surface runtime choice, policy, and fallback details in operator-facing state
- add proof artifacts or command surfaces that show the same contract resolved against Codex and Claude paths
- keep current-state docs honest about what is fully live and what is fixture-backed in this wave
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-app-server/src/lib.rs
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/current-state.md
```
