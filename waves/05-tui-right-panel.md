+++
id = 5
slug = "tui-right-panel"
title = "Build the right-side operator panel in the TUI"
mode = "dark-factory"
owners = ["tui", "operator"]
depends_on = [3, 4]
validation = ["cargo test -p wave-tui -p wave-app-server -p wave-control-plane"]
rollback = ["Hide the right-side panel behind a bootstrap shell fallback until layout and subscriptions stabilize."]
proof = ["crates/wave-tui/src/lib.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-control-plane/src/lib.rs", "docs/guides/terminal-surfaces.md", "docs/implementation/rust-codex-refactor.md"]
+++
# Wave 5 - Build the right-side operator panel in the TUI

**Commit message**: `Feat: land right-side operator panel`

## Component promotions
- tui-right-side-panel: repo-landed
- operator-status-tabs: repo-landed

## Deploy environments
- repo-local: custom default (repo-local TUI work only; no live host mutation)

## Context7 defaults
- bundle: rust-tui
- query: "Ratatui side panels, terminal layout, and right-tab operator dashboard patterns for a Codex-first shell"

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
- .wave/reviews/wave-5-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether the right-side TUI panel lands as a truthful operator surface backed by authoritative Wave state.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- do not PASS unless the right-side panel shows real run, agent, queue, and control state rather than placeholder UI
- treat narrow-terminal regressions or UI-only state drift as blocking
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-5-cont-qa.md
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
- .wave/integration/wave-5.md
- .wave/integration/wave-5.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile TUI layout, control-plane subscriptions, and operator guidance into one closure-ready dashboard verdict.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- treat mismatches between panel widgets and authoritative status data as integration failures
- decide ready-for-doc-closure only when the right-side panel behaves like the operator surface described in the plan
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-5.md
- .wave/integration/wave-5.json
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
- Keep shared plan docs aligned with the new TUI operator surface and its implications for later queue and dogfood waves.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/plans/master-plan.md.
- Read docs/plans/current-state.md.

Specific expectations:
- update shared-plan assumptions if the TUI changes what operators can see or control directly
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

## Agent A1: TUI Shell And Layout Scaffold

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-tui
- query: "Ratatui layout scaffolds and persistent right-side panels for a terminal operator shell"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-ratatui-operator
- repo-wave-closure-markers

### Components
- tui-right-side-panel
- operator-status-tabs

### Capabilities
- tui-layout
- right-panel-shell
- operator-navigation

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-tui/src/lib.rs

### File ownership
- crates/wave-tui/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the TUI shell layout that reserves the right-side panel as a first-class operator surface.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- keep the main pane focused on conversation or logs and the right pane focused on orchestration state
- define the wide and narrow layout behavior explicitly instead of leaving it emergent
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-tui/src/lib.rs
```

## Agent A2: Status Bindings And Control Subscriptions

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-tui
- query: "Terminal dashboard data binding and app-server subscription patterns for queue, run, and agent status widgets"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-ratatui-operator
- repo-rust-control-plane
- repo-wave-closure-markers

### Components
- operator-status-tabs

### Capabilities
- control-subscriptions
- status-bindings
- queue-and-agent-widgets

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-app-server/src/lib.rs
- crates/wave-control-plane/src/lib.rs

### File ownership
- crates/wave-app-server/src/lib.rs
- crates/wave-control-plane/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the authoritative status bindings and app-server subscriptions the right-side tabs depend on.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- bind Run, Agents, Queue, and Control tabs to real control-plane fields instead of widget-local guesses
- surface incomplete or unavailable actions honestly if the control plane does not support them yet
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-app-server/src/lib.rs
- crates/wave-control-plane/src/lib.rs
```

## Agent A3: Operator Panel Guidance And Fallback Behavior

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-tui
- query: "Operator dashboard guidance and narrow-terminal fallback behavior for a Rust TUI shell"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-ratatui-operator
- repo-wave-closure-markers

### Components
- tui-right-side-panel

### Capabilities
- operator-guidance
- fallback-behavior
- panel-documentation

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
- Document the right-side operator panel, its tabs, and the narrow-terminal fallback behavior the implementation actually supports.

Required context before coding:
- Read README.md.
- Read docs/guides/terminal-surfaces.md.
- Read docs/implementation/rust-codex-refactor.md.

Specific expectations:
- document the right-side panel as the built-in dashboard surface
- keep the docs honest about what actions are live versus still planned
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- docs/guides/terminal-surfaces.md
- docs/implementation/rust-codex-refactor.md
```
