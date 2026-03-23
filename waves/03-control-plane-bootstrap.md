+++
id = 3
slug = "control-plane-bootstrap"
title = "Build planning status and control-plane bootstrap"
mode = "dark-factory"
owners = ["implementation", "operator"]
depends_on = [2]
validation = ["cargo test -p wave-control-plane -p wave-cli"]
rollback = ["Revert the planning-status reducer and CLI status projections if they stop matching the authored-wave contract."]
proof = ["crates/wave-control-plane/src/lib.rs", "crates/wave-cli/src/main.rs", "docs/guides/terminal-surfaces.md", "docs/implementation/rust-codex-refactor.md"]
+++
# Wave 3 - Build planning status and control-plane bootstrap

**Commit message**: `Feat: land control-plane bootstrap and planning status`

## Component promotions
- planning-status: repo-landed
- queue-json-surface: repo-landed

## Deploy environments
- repo-local: custom default (repo-local control-plane work only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Serde-backed queue projections, blocker modeling, and status surfaces for a Rust control-plane bootstrap"

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
- .wave/reviews/wave-3-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether planning status, blocker visibility, and operator-facing status output land together as one truthful control-plane bootstrap.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- do not PASS unless control status reflects the authored-wave model rather than ad hoc text summaries
- treat hidden blocker state or missing closure visibility as blocking gaps
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-3-cont-qa.md
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
- .wave/integration/wave-3.md
- .wave/integration/wave-3.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile control-plane types, CLI status output, and operator-facing status guidance into one closure-ready bootstrap verdict.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- treat any mismatch between JSON status, text status, and documented operator expectations as an integration failure
- decide ready-for-doc-closure only when planning state is typed and consistently projected
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-3.md
- .wave/integration/wave-3.json
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
- Keep shared plan docs aligned with the new planning-status and queue-visibility baseline.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions if planning status or queue visibility changes what later waves may rely on
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

## Agent A1: Control-Plane Queue And Reducer Model

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Typed reducer design, blocker fields, and queue-read-model construction for Rust control-plane state"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- planning-status
- queue-json-surface

### Capabilities
- queue-reducer
- blocker-modeling
- closure-coverage

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
- Land the typed control-plane reducer and queue/read-model fields that later launcher and TUI waves will consume.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- model blocker state, agent counts, and closure coverage explicitly rather than hiding them in strings
- keep the control-plane surface serializable and suitable for both CLI and TUI consumption
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-control-plane/src/lib.rs
```

## Agent A2: CLI Status And JSON Reporting

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-cli-core
- query: "Rust CLI status rendering and JSON output patterns for a control-plane bootstrap command surface"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- planning-status
- queue-json-surface

### Capabilities
- control-status-command
- json-reporting
- operator-summary

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-cli/src/main.rs

### File ownership
- crates/wave-cli/src/main.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the CLI status surface that exposes the new control-plane fields honestly in both human and JSON views.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/guides/terminal-surfaces.md.

Specific expectations:
- keep the text shell concise while preserving the same truth as the JSON output
- do not add future runtime claims to the CLI before the owning runtime waves land
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-cli/src/main.rs
```

## Agent A3: Control-Plane Operator Guidance

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Operator-facing status guidance and terminal-surface expectations for a Rust planning bootstrap"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- queue-json-surface

### Capabilities
- operator-guidance
- status-contract
- bootstrap-docs

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
- Align operator-facing docs with the real control-plane bootstrap so status semantics are documented before live runtime work starts.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- document the current status fields, queue semantics, and future TUI dependency on the same control-plane truth
- keep the docs explicit about what is planning-only versus not yet implemented
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- docs/guides/terminal-surfaces.md
- docs/implementation/rust-codex-refactor.md
```
