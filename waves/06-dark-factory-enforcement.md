+++
id = 6
slug = "dark-factory-enforcement"
title = "Make dark-factory an enforced execution profile"
mode = "dark-factory"
owners = ["runtime", "safety"]
depends_on = [2, 3, 4]
validation = ["cargo test -p wave-dark-factory -p wave-runtime -p wave-cli"]
rollback = ["Fall back to planning-only dark-factory semantics if hard preflight gates block valid local execution paths."]
proof = ["crates/wave-dark-factory/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "docs/concepts/operating-modes.md", "docs/reference/repository-guidance.md"]
+++
# Wave 6 - Make dark-factory an enforced execution profile

**Commit message**: `Feat: land dark-factory preflight and fail-closed policy`

## Component promotions
- dark-factory-preflight: repo-landed
- fail-closed-launch-policy: repo-landed

## Deploy environments
- repo-local: custom default (repo-local policy and preflight work only; no live host mutation)

## Context7 defaults
- bundle: rust-policy-gates
- query: "Fail-closed preflight checks, diagnostics, and launch policy modeling for a Rust dark-factory profile"

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
- .wave/reviews/wave-6-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether dark-factory is now enforced as a real fail-closed execution profile rather than a descriptive label.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/concepts/operating-modes.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- do not PASS unless launch preflight rejects under-specified waves before runtime mutation
- treat vague diagnostics or silent policy downgrades as blocking
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-6-cont-qa.md
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
- .wave/integration/wave-6.md
- .wave/integration/wave-6.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile lint policy, runtime launch behavior, and operator diagnostics into one closure-ready dark-factory verdict.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/concepts/operating-modes.md.
- Read docs/reference/repository-guidance.md.

Specific expectations:
- treat any mismatch between dark-factory lint requirements and runtime launch checks as an integration failure
- decide ready-for-doc-closure only when failure modes are explicit and machine-actionable
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-6.md
- .wave/integration/wave-6.json
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
- Keep shared plan docs aligned with dark-factory becoming an enforced execution profile.

Required context before coding:
- Read README.md.
- Read docs/concepts/operating-modes.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions if dark-factory enforcement changes how later queue or dogfood waves must be authored
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

## Agent A1: Dark-Factory Policy Gates

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-policy-gates
- query: "Rust fail-closed policy checks and launch requirement validation for dark-factory waves"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- dark-factory-preflight
- fail-closed-launch-policy

### Capabilities
- policy-gates
- contract-validation
- fail-closed-diagnostics

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-dark-factory/src/lib.rs

### File ownership
- crates/wave-dark-factory/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the dark-factory policy gates that reject incomplete launch contracts before execution begins.

Required context before coding:
- Read README.md.
- Read wave.toml.
- Read docs/concepts/operating-modes.md.
- Read docs/reference/repository-guidance.md.

Specific expectations:
- validate environment, rollback, proof, and closure requirements as explicit machine-readable contracts
- keep policy errors precise enough for operators to fix the authored wave without guesswork
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-dark-factory/src/lib.rs
```

## Agent A2: Runtime Preflight And Launch Refusal

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-policy-gates
- query: "Runtime preflight execution and operator diagnostics for fail-closed launch behavior"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- dark-factory-preflight
- fail-closed-launch-policy

### Capabilities
- runtime-preflight
- launch-refusal
- operator-diagnostics

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs

### File ownership
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land runtime preflight and launch-refusal behavior that makes dark-factory failures visible before any mutation begins.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/concepts/operating-modes.md.

Specific expectations:
- refuse launch when required contracts are missing instead of downgrading silently
- surface diagnostics in a form the CLI and future TUI can both present directly
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
```

## Agent A3: Dark-Factory Guidance And Repository Rules

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-policy-gates
- query: "Execution-profile guidance and repository-level operator rules for fail-closed runtime work"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- fail-closed-launch-policy

### Capabilities
- operating-mode-docs
- repository-guidance
- dark-factory-runbooks

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- docs/concepts/operating-modes.md
- docs/reference/repository-guidance.md
- docs/implementation/rust-codex-refactor.md

### File ownership
- docs/concepts/operating-modes.md
- docs/reference/repository-guidance.md
- docs/implementation/rust-codex-refactor.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Document the enforced dark-factory posture and the repository rules operators must follow when using it.

Required context before coding:
- Read README.md.
- Read docs/concepts/operating-modes.md.
- Read docs/reference/repository-guidance.md.

Specific expectations:
- explain what dark-factory now requires at authoring time and at launch time
- keep the docs aligned with real CLI/runtime behavior rather than future plans
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- docs/concepts/operating-modes.md
- docs/reference/repository-guidance.md
- docs/implementation/rust-codex-refactor.md
```
