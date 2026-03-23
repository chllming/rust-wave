+++
id = 1
slug = "workspace-bootstrap"
title = "Bootstrap the Rust workspace and operator command surface"
mode = "dark-factory"
owners = ["implementation", "operator"]
depends_on = [0]
validation = ["cargo test -p wave-cli -p wave-config -p wave-runtime -p wave-tui -p wave-app-server -p wave-trace"]
rollback = ["Remove the Rust workspace bootstrap changes and keep the repo on the authored-wave planning baseline until the command surface is restabilized."]
proof = ["Cargo.toml", "crates/wave-cli/src/main.rs", "wave.toml", "README.md", "agents.md"]
+++
# Wave 1 - Bootstrap the Rust workspace and operator command surface

**Commit message**: `Feat: land Rust workspace bootstrap and command surface`

## Component promotions
- rust-workspace: repo-landed
- wave-cli-bootstrap: repo-landed

## Deploy environments
- repo-local: custom default (repo-local workspace bootstrap only; no live host mutation)

## Context7 defaults
- bundle: rust-cli-core
- query: "Cargo workspace bootstrap, clap command trees, and Rust operator CLI surfaces for the Wave binary"

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
- .wave/reviews/wave-1-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether the Rust workspace, bootstrap crates, and operator command surface land together without leaving the repo in a half-scaffolded state.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- do not PASS unless cargo metadata, command entrypoints, and bootstrap docs all agree on the same crate layout
- treat missing stub crates or misleading CLI help as blocking gaps
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-1-cont-qa.md
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
- .wave/integration/wave-1.md
- .wave/integration/wave-1.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile workspace manifest, crate stubs, and CLI surfaces into one repo-landed bootstrap verdict.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- treat crate-layout mismatches, missing command stubs, or stale bootstrap docs as integration failures
- decide ready-for-doc-closure only when the workspace shape is consistent across manifests, code, and docs
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-1.md
- .wave/integration/wave-1.json
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
- Keep shared plan docs aligned with the landed Rust workspace and command-surface bootstrap.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan sequencing if the workspace bootstrap changes what later waves can safely assume
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

## Agent A1: Workspace Manifest And Crate Layout

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-cli-core
- query: "Cargo workspace manifests, crate naming, and bootstrap ownership for a multi-crate Rust CLI repo"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- rust-workspace

### Capabilities
- cargo-workspace
- crate-topology
- manifest-bootstrap

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-cli/Cargo.toml
- crates/wave-config/Cargo.toml
- crates/wave-control-plane/Cargo.toml
- crates/wave-dark-factory/Cargo.toml

### File ownership
- Cargo.toml
- crates/wave-cli/Cargo.toml
- crates/wave-config/Cargo.toml
- crates/wave-control-plane/Cargo.toml
- crates/wave-dark-factory/Cargo.toml

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the root workspace manifest and crate-level ownership so the Rust rewrite has a stable compileable skeleton.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- keep the workspace aligned with the accepted crate map and avoid speculative extra crates
- make crate membership and shared dependencies explicit enough for later runtime waves
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-cli/Cargo.toml
- crates/wave-config/Cargo.toml
- crates/wave-control-plane/Cargo.toml
- crates/wave-dark-factory/Cargo.toml
```

## Agent A2: Command Surface And Stub Runtime Crates

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-cli-core
- query: "Clap command trees and compileable Rust crate stubs for an orchestration CLI bootstrap"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- rust-workspace
- wave-cli-bootstrap

### Capabilities
- command-surface
- stub-runtime-crates
- compileable-entrypoints

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-app-server/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-tui/src/lib.rs
- crates/wave-trace/src/lib.rs

### File ownership
- crates/wave-app-server/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-tui/src/lib.rs
- crates/wave-trace/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land compileable stub runtime crates and the bootstrap command-surface expectations those crates support.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- keep stub crates honest about their current scope while preserving stable landing zones for later waves
- do not imply launch, autonomous, trace, or TUI behavior exists before the owning waves land
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-app-server/src/lib.rs
- crates/wave-runtime/src/lib.rs
- crates/wave-tui/src/lib.rs
- crates/wave-trace/src/lib.rs
```

## Agent A3: Project Config And Operator Onboarding

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-cli-core
- query: "Operator onboarding and project-config bootstrap guidance for a Rust terminal tool"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- wave-cli-bootstrap

### Capabilities
- project-config
- operator-onboarding
- bootstrap-docs

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- wave.toml
- README.md
- agents.md

### File ownership
- wave.toml
- README.md
- agents.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the project config and operator-facing bootstrap guidance that make the Rust workspace usable before live runtime waves exist.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- document only the commands and behaviors that actually exist in the bootstrap slice
- keep config paths consistent with the accepted repo layout and project-scoped state roots
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- wave.toml
- README.md
- agents.md
```
