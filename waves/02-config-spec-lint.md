+++
id = 2
slug = "config-spec-lint"
title = "Implement typed config, authored-wave parsing, and dark-factory lint"
mode = "dark-factory"
owners = ["implementation", "planner"]
depends_on = [1]
validation = ["cargo test -p wave-config -p wave-spec -p wave-dark-factory -p wave-cli"]
rollback = ["Revert the config/spec/lint crates and return to the workspace bootstrap if the typed authored-wave contract proves unstable."]
proof = ["wave.toml", "crates/wave-config/src/lib.rs", "crates/wave-spec/src/lib.rs", "crates/wave-dark-factory/src/lib.rs", "docs/context7/bundles.json"]
+++
# Wave 2 - Implement typed config, authored-wave parsing, and dark-factory lint

**Commit message**: `Feat: land typed config, authored-wave parser, and lint`

## Component promotions
- wave-config-and-spec: repo-landed
- dark-factory-lint: repo-landed

## Deploy environments
- repo-local: custom default (repo-local config and parser work only; no live host mutation)

## Context7 defaults
- bundle: rust-config-spec
- query: "Serde derive, TOML parsing, and rich markdown section parsing for typed config and authored-wave enforcement"

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
- .wave/reviews/wave-2-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether typed config loading, authored-wave parsing, and lint enforcement land together without leaving hidden format gaps.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/reference/skills.md.
- Read docs/context7/bundles.json.

Specific expectations:
- do not PASS unless both config parsing and rich wave parsing are typed, testable, and reflected in lint behavior
- treat weak Context7 enforcement or parser/lint mismatches as blocking
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-2-cont-qa.md
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
- .wave/integration/wave-2.md
- .wave/integration/wave-2.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile config loading, wave parsing, Context7 defaults, and lint behavior into one closure-ready parser stack verdict.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/context7/bundles.json.

Specific expectations:
- treat any disagreement between parser output and lint enforcement as an integration failure
- decide ready-for-doc-closure only when authored-wave structure and config fields are both executable and validated
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-2.md
- .wave/integration/wave-2.json
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
- Keep shared plan docs aligned with the typed config and authored-wave parsing contract that future waves now depend on.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions if parser or lint rules change how waves must be authored
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

## Agent A1: Typed Project Config Loader

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-config-spec
- query: "Serde TOML config loading, typed defaults, and path modeling for Rust project config"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- wave-config-and-spec

### Capabilities
- config-loader
- typed-paths
- project-defaults

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-config/src/lib.rs
- wave.toml

### File ownership
- crates/wave-config/src/lib.rs
- wave.toml

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the typed Rust config loader and project-default contract that later launcher and TUI waves can rely on.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- keep path roots and dark-factory defaults explicit rather than inferred inside the CLI
- preserve a stable typed config surface for project-scoped Codex, state, and trace roots
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-config/src/lib.rs
- wave.toml
```

## Agent A2: Authored-Wave Parser

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-config-spec
- query: "Markdown section parsing and typed rich-wave models for multi-agent authored-wave files"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- wave-config-and-spec

### Capabilities
- authored-wave-parser
- agent-contract-modeling
- section-validation

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-spec/src/lib.rs

### File ownership
- crates/wave-spec/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the parser for rich multi-agent authored-wave files, including commit messages, deploy environments, Context7 defaults, and agent sections.

Required context before coding:
- Read README.md.
- Read docs/reference/skills.md.
- Read docs/context7/bundles.json.

Specific expectations:
- parse the authored-wave markdown structure directly instead of hiding meaning in freeform prose
- keep the model explicit enough for lint, doctor, queue status, and later launcher compilation
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-spec/src/lib.rs
```

## Agent A3: Dark-Factory Lint And Context7 Catalog

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-policy-gates
- query: "Fail-closed lint rules, skill resolution checks, and narrow Context7 bundle enforcement for authored-wave inputs"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- dark-factory-lint

### Capabilities
- lint-enforcement
- context7-catalog
- doctor-integration

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-dark-factory/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/context7/bundles.json

### File ownership
- crates/wave-dark-factory/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/context7/bundles.json

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land fail-closed lint rules and Context7 catalog expectations so authored waves cannot omit validation-critical structure.

Required context before coding:
- Read README.md.
- Read docs/reference/skills.md.
- Read docs/context7/bundles.json.

Specific expectations:
- reject weak Context7 defaults, unknown skills, missing closure agents, and malformed authored-wave prompts
- keep the Context7 bundle catalog narrow and task-shaped rather than broad or aspirational
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-dark-factory/src/lib.rs
- crates/wave-cli/src/main.rs
- docs/context7/bundles.json
```
