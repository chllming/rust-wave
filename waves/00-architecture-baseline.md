+++
id = 0
slug = "architecture-baseline"
title = "Freeze the authored-wave architecture baseline"
mode = "dark-factory"
owners = ["planner", "operator"]
depends_on = []
validation = ["cargo test -p wave-spec -p wave-dark-factory -p wave-control-plane -p wave-cli"]
rollback = ["Revert the authored-wave schema, lint enforcement, and planning-status changes if the richer contract blocks repo bootstrap work."]
proof = ["crates/wave-spec/src/lib.rs", "crates/wave-dark-factory/src/lib.rs", "crates/wave-control-plane/src/lib.rs", "crates/wave-cli/src/main.rs", "README.md", "agents.md", "docs/implementation/rust-codex-refactor.md", "docs/reference/skills.md"]
+++
# Wave 0 - Freeze the authored-wave architecture baseline

**Commit message**: `Feat: land authored-wave schema and closure contracts`

## Component promotions
- authored-wave-schema: repo-landed
- closure-contracts-and-skill-catalog: repo-landed

## Deploy environments
- repo-local: custom default (repo-local authored-wave bootstrap only; no live host mutation)

## Context7 defaults
- bundle: rust-config-spec
- query: "Rich markdown wave specs, Rust parser modeling, and lint contracts for authored-wave bootstrap work"

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
- .wave/reviews/wave-0-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether the authored-wave schema, closure-agent contract, and richer lint/control surfaces honestly land together at repo-landed.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/reference/skills.md.

Specific expectations:
- do not PASS unless the wave parser, linter, control status, and operator guidance all agree on the same authored-wave model
- treat missing closure-agent coverage or weak skill resolution as blocking gaps
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-0-cont-qa.md
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
- .wave/integration/wave-0.md
- .wave/integration/wave-0.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile the parser, linter, planning-status, and guidance-doc slices into one closure-ready authored-wave baseline.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/reference/skills.md.

Specific expectations:
- treat mismatches between wave authoring rules and CLI behavior as integration failures
- decide ready-for-doc-closure only when authored-wave structure, skill resolution, and queue visibility all land together
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-0.md
- .wave/integration/wave-0.json
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
- Keep shared plan docs aligned with the new authored-wave baseline and its implications for future Rust/Codex waves.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared plan docs when Wave 0 changes what later waves may assume about parser, skill, or closure behavior
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

## Agent A1: Authored-Wave Schema And Lint Contracts

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-config-spec
- query: "Rust markdown section parsing, serde-backed wave modeling, and lint rule design for authored-wave specs"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- authored-wave-schema
- closure-contracts-and-skill-catalog

### Capabilities
- wave-parser
- rich-agent-contracts
- lint-enforcement

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-spec/src/lib.rs
- crates/wave-dark-factory/src/lib.rs

### File ownership
- crates/wave-spec/src/lib.rs
- crates/wave-dark-factory/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the rich authored-wave schema plus fail-closed lint checks for closure agents, skills, file ownership, and final markers.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/reference/skills.md.
- Read docs/context7/bundles.json.

Specific expectations:
- make waves/*.md the canonical rich authored-wave surface rather than a minimal checklist format
- reject missing commit messages, missing closure agents, missing owned paths, weak prompts, and unknown skills
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-spec/src/lib.rs
- crates/wave-dark-factory/src/lib.rs
```

## Agent A2: Planning Status And Doctor Surface

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Typed Rust planning-status projections, queue summaries, and doctor-style validation surfaces for authored waves"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- authored-wave-schema
- closure-contracts-and-skill-catalog

### Capabilities
- planning-status
- doctor-checks
- queue-visibility

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-control-plane/src/lib.rs
- crates/wave-cli/src/main.rs

### File ownership
- crates/wave-control-plane/src/lib.rs
- crates/wave-cli/src/main.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land control-status and doctor projections that surface authored-wave agent counts, closure coverage, and skill-catalog health.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- keep JSON output authoritative for future TUI consumption
- expose enough queue detail that missing closure coverage is visible before runtime work begins
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-control-plane/src/lib.rs
- crates/wave-cli/src/main.rs
```

## Agent A3: Repo Guidance And Skills Baseline

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-cli-core
- query: "Bootstrap repo guidance and operator-facing documentation patterns for a Rust CLI migration"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- closure-contracts-and-skill-catalog

### Capabilities
- repo-guidance
- skill-guidance
- operator-onboarding

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- README.md
- agents.md
- docs/implementation/rust-codex-refactor.md
- docs/reference/skills.md
- skills/README.md

### File ownership
- README.md
- agents.md
- docs/implementation/rust-codex-refactor.md
- docs/reference/skills.md
- skills/README.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Align repo guidance and skill documentation with the richer authored-wave contract so future waves use the same operating model the tool now enforces.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/reference/skills.md.
- Read skills/README.md.

Specific expectations:
- document the authored-wave structure, closure roles, repo-specific skills, and fail-closed lint expectations
- keep the guidance practical for future implementation agents, not aspirational
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- README.md
- agents.md
- docs/implementation/rust-codex-refactor.md
- docs/reference/skills.md
- skills/README.md
```
