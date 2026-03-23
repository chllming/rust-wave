+++
id = 9
slug = "dogfood"
title = "Dogfood the Rust system on this repository"
mode = "dark-factory"
owners = ["operator", "integration"]
depends_on = [5, 7, 8]
validation = ["cargo test", "cargo run -p wave-cli -- control status --json", "cargo run -p wave-cli -- doctor --json"]
rollback = ["Pause self-hosting and continue implementation through the bootstrap CLI if live runtime gaps remain explicit."]
proof = ["README.md", "agents.md", "docs/implementation/rust-codex-refactor.md", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-trace/src/lib.rs", "docs/plans/wave-orchestrator.md", "docs/plans/context7-wave-orchestrator.md"]
+++
# Wave 9 - Dogfood the Rust system on this repository

**Commit message**: `Feat: land self-host runbook and dogfood evidence`

## Component promotions
- self-host-dogfood-runbook: repo-landed
- dark-factory-dogfood-evidence: repo-landed

## Deploy environments
- repo-local: custom default (repo-local self-host dogfood only; no live host mutation)

## Context7 defaults
- bundle: rust-async-runtime
- query: "Self-host orchestration runbooks, trace-aware execution, and dark-factory evidence collection for a Rust operator tool"

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
- .wave/reviews/wave-9-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether the Rust Wave implementation can now plan, execute, and explain its own remaining work on this repository.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/wave-orchestrator.md.

Specific expectations:
- do not PASS unless the repo has a real self-host runbook plus trace-backed evidence from using the system on itself
- treat hand-wavy dogfood claims or missing runtime evidence as blocking
- do not block on A0 appearing as `running` in the active run record while you are executing; judge closure from landed artifacts and prior-stage markers instead
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-9-cont-qa.md
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
- .wave/integration/wave-9.md
- .wave/integration/wave-9.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile self-host docs, runtime behavior, and trace evidence into one closure-ready dogfood verdict.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/context7-wave-orchestrator.md.

Specific expectations:
- treat dogfood claims without trace-backed evidence or operator-visible control-plane state as integration failures
- do not block on A8 appearing as `running` in the active run record while you are executing; judge closure from landed artifacts and prior-stage markers instead
- do not require the final trace bundle for the current run while you are executing; that bundle is emitted after launch closure
- decide ready-for-doc-closure only when the system can honestly operate on its own backlog
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-9.md
- .wave/integration/wave-9.json
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
- Keep shared plan docs aligned with whatever the self-host dogfood run proves or disproves about the Rust Wave system.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions when dogfood evidence changes what the project can now claim as repo-landed
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

## Agent A1: Self-Host Runbook And Operator Guidance

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-async-runtime
- query: "Self-host runbooks and operator guidance for using a Rust orchestration tool on its own repository"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- self-host-dogfood-runbook
- dark-factory-dogfood-evidence

### Capabilities
- self-host-runbook
- operator-guidance
- repo-dogfood-procedure

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- README.md
- agents.md
- docs/implementation/rust-codex-refactor.md

### File ownership
- README.md
- agents.md
- docs/implementation/rust-codex-refactor.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the self-host runbook and operator guidance for using the Rust Wave system on this repository itself.

Required context before coding:
- Read README.md.
- Read agents.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/wave-orchestrator.md.

Specific expectations:
- describe a real self-host flow grounded in the actual launcher, queue, TUI, and trace surfaces that exist
- keep the guidance explicit about remaining gaps instead of pretending the system is more complete than it is
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- README.md
- agents.md
- docs/implementation/rust-codex-refactor.md
```

## Agent A2: Dogfood Runtime And Trace Evidence

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=xhigh,model_verbosity=low

### Context7
- bundle: rust-async-runtime
- query: "Trace-backed self-host runtime execution and CLI support for dogfooding a Rust orchestration system"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- dark-factory-dogfood-evidence

### Capabilities
- self-host-execution
- trace-evidence
- operator-validation

### Exit contract
- completion: integrated
- durability: durable
- proof: live
- doc-impact: owned

### Deliverables
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-trace/src/lib.rs

### File ownership
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-trace/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the runtime, trace, and CLI support needed to generate real dogfood evidence from using the system on itself.

Required context before coding:
- Read README.md.
- Read docs/implementation/rust-codex-refactor.md.
- Read docs/plans/context7-wave-orchestrator.md.

Specific expectations:
- capture durable evidence from self-host execution rather than synthetic examples
- keep the runtime and trace surfaces honest about what still needs operator help
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-runtime/src/lib.rs
- crates/wave-cli/src/main.rs
- crates/wave-trace/src/lib.rs
```

## Agent A3: Dogfood Gap Ledger And Positioning Docs

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-async-runtime
- query: "Gap-ledger and positioning-document patterns for self-host adoption of orchestration systems"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-codex-orchestrator
- repo-wave-closure-markers

### Components
- self-host-dogfood-runbook

### Capabilities
- gap-ledger
- positioning-docs
- follow-up-planning

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- docs/plans/wave-orchestrator.md
- docs/plans/context7-wave-orchestrator.md
- docs/reference/repository-guidance.md

### File ownership
- docs/plans/wave-orchestrator.md
- docs/plans/context7-wave-orchestrator.md
- docs/reference/repository-guidance.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the dogfood gap ledger and positioning docs that make remaining work explicit after the self-host run.

Required context before coding:
- Read README.md.
- Read docs/plans/wave-orchestrator.md.
- Read docs/plans/context7-wave-orchestrator.md.
- Read docs/reference/repository-guidance.md.

Specific expectations:
- record the remaining dogfood gaps as concrete follow-up surfaces rather than diffuse caveats
- keep positioning docs aligned with the actual state proven by the self-host run
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- docs/plans/wave-orchestrator.md
- docs/plans/context7-wave-orchestrator.md
- docs/reference/repository-guidance.md
```
