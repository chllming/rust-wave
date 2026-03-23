+++
id = 4
slug = "codex-launcher"
title = "Implement the Codex-backed launcher and agent lifecycle manager"
mode = "dark-factory"
owners = ["runtime", "implementation"]
depends_on = [3]
validation = ["cargo test -p wave-runtime -p wave-app-server -p wave-cli"]
rollback = ["Disable the launcher path and return to planning-only commands until Codex-backed execution and state roots stabilize."]
proof = ["crates/wave-runtime/src/lib.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-cli/src/main.rs", "third_party/codex-rs/UPSTREAM.toml", "docs/reference/runtime-config/codex.md", "wave.toml"]
+++
# Wave 4 - Implement the Codex-backed launcher and agent lifecycle manager

**Commit message**: `Feat: land Codex-backed launcher substrate`

## Component promotions
- codex-launcher-substrate: repo-landed
- project-scoped-codex-home: repo-landed

## Deploy environments
- repo-local: custom default (repo-local Codex launcher work only; no live host mutation)

## Context7 defaults
- bundle: rust-async-runtime
- query: "Tokio process orchestration, launcher loops, and project-scoped runtime state for Codex-backed execution"

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
- .wave/reviews/wave-4-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether the Codex-backed launcher, app-server glue, and project-scoped Codex state roots land together as one fail-closed runtime slice.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/reference/runtime-config/codex.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- do not PASS unless Codex is treated as the real operator runtime rather than a placeholder adapter
- treat global-state leakage or missing project-scoped state roots as blocking
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-4-cont-qa.md
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
- .wave/integration/wave-4.md
- .wave/integration/wave-4.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile Codex vendor pinning, runtime launcher behavior, and operator command entrypoints into one closure-ready runtime substrate verdict.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read third_party/codex-rs/UPSTREAM.toml.
- Read docs/reference/runtime-config/codex.md.

Specific expectations:
- treat mismatches between runtime state roots, launcher behavior, and CLI entrypoints as integration failures
- decide ready-for-doc-closure only when Codex is clearly the primary runtime and the repo-local state contract is explicit
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-4.md
- .wave/integration/wave-4.json
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
- Keep shared plan docs aligned with the Codex-first launcher substrate and project-scoped runtime-state contract.

Required context before coding:
- Read README.md.
- Read docs/reference/runtime-config/codex.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions if the launcher changes what later TUI, queue, or dogfood waves may safely rely on
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

## Agent A1: Codex Runtime Substrate

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-async-runtime
- query: "Tokio launcher loops, process supervision, and Codex-specific runtime-state handling for a project-scoped orchestrator"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- codex-launcher-substrate
- project-scoped-codex-home

### Capabilities
- launcher-runtime
- codex-home-isolation
- agent-lifecycle

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- third_party/codex-rs/UPSTREAM.toml
- crates/wave-runtime/src/lib.rs

### File ownership
- third_party/codex-rs/UPSTREAM.toml
- crates/wave-runtime/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the Codex-backed runtime substrate and project-scoped Codex home behavior that make Wave execution real in this repo.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read third_party/codex-rs/UPSTREAM.toml.
- Read docs/reference/runtime-config/codex.md.

Specific expectations:
- keep Codex as the first-class runtime rather than preserving generic executor abstractions for v1
- isolate runtime state under the project roots declared in wave.toml
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- third_party/codex-rs/UPSTREAM.toml
- crates/wave-runtime/src/lib.rs
```

## Agent A2: Launcher Command Path And App-Server Actions

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-async-runtime
- query: "Control-plane command entrypoints and app-server style control actions for a Codex-backed launcher"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- codex-launcher-substrate

### Capabilities
- launch-command
- app-server-glue
- control-actions

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-app-server/src/lib.rs
- crates/wave-cli/src/main.rs

### File ownership
- crates/wave-app-server/src/lib.rs
- crates/wave-cli/src/main.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the operator command path and app-server integration points that expose the launcher as a real control-plane action surface.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/reference/runtime-config/codex.md.

Specific expectations:
- keep non-interactive CLI launch behavior aligned with future TUI and app-server control actions
- fail closed when launcher prerequisites are missing instead of silently downgrading behavior
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-app-server/src/lib.rs
- crates/wave-cli/src/main.rs
```

## Agent A3: Runtime Config And Operator Documentation

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-cli-core
- query: "Project-scoped runtime configuration and operator documentation for a Codex-first Rust launcher"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- project-scoped-codex-home

### Capabilities
- runtime-config
- operator-docs
- project-state-roots

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- wave.toml
- docs/reference/runtime-config/codex.md
- docs/implementation/rust-codex-refactor.md

### File ownership
- wave.toml
- docs/reference/runtime-config/codex.md
- docs/implementation/rust-codex-refactor.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Document and land the project-scoped Codex runtime configuration the launcher depends on.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/reference/runtime-config/codex.md.

Specific expectations:
- keep runtime paths, launcher assumptions, and project-scoped Codex home behavior explicit in both config and docs
- do not imply TUI or autonomous queue behavior that belongs to later waves
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- wave.toml
- docs/reference/runtime-config/codex.md
- docs/implementation/rust-codex-refactor.md
```
