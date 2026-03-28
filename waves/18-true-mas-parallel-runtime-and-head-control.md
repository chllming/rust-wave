+++
id = 18
slug = "true-mas-parallel-runtime-and-head-control"
title = "Land true MAS parallel runtime and integrated head control"
mode = "dark-factory"
owners = ["architecture", "runtime", "scheduler", "ux"]
depends_on = [17]
validation = ["cargo test -p wave-domain -p wave-events -p wave-coordination -p wave-reducer -p wave-projections --locked", "cargo test -p wave-runtime -p wave-cli -p wave-app-server -p wave-tui --locked", "cargo run -p wave-cli -- doctor --json", "cargo run -p wave-cli -- control show --wave 18 --json", "cargo run -p wave-cli -- control orchestrator show --wave 18 --json", "cargo run -p wave-cli -- trace latest --wave 18 --json", "cargo run -p wave-cli -- trace replay --wave 18 --json"]
rollback = ["Return MAS waves to serial intra-wave execution, disable per-agent sandbox admission, keep directive and orchestrator state as passive visibility only, and preserve accepted delivery or control-plane truth without claiming parallel agent execution until the runtime substrate regains parity."]
proof = ["Cargo.toml", "crates/wave-domain/src/lib.rs", "crates/wave-events/src/lib.rs", "crates/wave-coordination/src/lib.rs", "crates/wave-reducer/src/lib.rs", "crates/wave-projections/src/lib.rs", "crates/wave-runtime/src/lib.rs", "crates/wave-cli/src/main.rs", "crates/wave-app-server/src/lib.rs", "crates/wave-tui/src/lib.rs", "docs/implementation/design.md", "docs/implementation/parallel-wave-multi-runtime-architecture.md", "docs/implementation/true-multi-agent-wave-architecture.md", "docs/plans/master-plan.md", "docs/plans/current-state.md", "docs/plans/true-multi-agent-wave-rollout.md", "docs/plans/mas-end-state-operator-head.md"]
execution_model = "multi-agent"
concurrency_budget = { max_concurrent_implementation_agents = 2, max_concurrent_report_only_agents = 1, max_merge_operations = 1, max_conflict_resolution_agents = 1 }
+++
# Wave 18 - Land true MAS parallel runtime and integrated head control

**Commit message**: `Feat: land true MAS runtime and integrated head control`

## Component promotions
- mas-runtime-substrate: pilot-live
- agent-sandbox-manager: pilot-live
- merge-queue-authority: pilot-live
- invalidation-routing: baseline-proved
- orchestrator-head-control: pilot-live
- operator-autonomous-handoff: pilot-live

## Deploy environments
- repo-local: custom default (repo-local MAS pilot only; no live host mutation)

## Context7 defaults
- bundle: rust-control-plane
- query: "Intra-wave multi-agent scheduling, per-agent sandboxes, merge queue authority, invalidation routing, and integrated operator or autonomous head control"

## Wave contract
- This is the first wave that may claim true intra-wave MAS execution in repo-local use.
- A MAS wave must no longer treat one shared wave-local writable checkout as the execution unit; each running agent needs an isolated sandbox derived from the current accepted integration head.
- Accepted wave state must advance only through merge authority, not because a runtime process exited successfully.
- Clean, policy-valid implementation merges should auto-advance by default, while conflicts, invalidations, and reconciliation remain explicit operator or head-controlled states.
- The integrated TUI must expose one orchestrator workspace that can both view and signal per-agent MAS state.
- Control must switch cleanly between `operator` and `autonomous` modes without losing directive history, pending delivery state, or current runtime sessions.

## Current repo status
- The authored-wave MAS contract, per-agent sandbox path, MAS-ready-set launch path, orchestrator/head control path, recovery-required state, and operator shell product are all code-landed in the current worktree.
- The remaining closure gap is one real Wave 18 live proof run that demonstrates concurrent MAS execution, targeted recovery, and honest closure instead of fixture-only or partial proof.

## Live proof expectations
- Show one MAS-authored wave with at least two implementation agents running concurrently in separate sandboxes.
- Show distinct sandbox ids, lease state, and runtime session identity for those running agents.
- Show one clean merge auto-advancing the integration head.
- Show one operator or head-issued steering directive persisted and delivered to a specific agent.
- Show one switch from `operator` to `autonomous` and one switch back, with autonomous action issuance stopping immediately after operator reclaim.
- Show one conflict or invalidation path that preserves already accepted sibling work while routing only the affected agent back through rerun or rebase.

## Quality control expectations
- Add deterministic tests for ready-set selection, per-wave concurrency budgets, exclusive-resource locking, merge queue transitions, invalidation propagation, and lease or heartbeat recovery.
- Add runtime-backed tests that prove at least two safe implementation agents can run concurrently in separate sandboxes inside one wave.
- Add TUI and app-server tests that prove the orchestrator workspace renders MAS truth and that steering or mode-switch actions round-trip through durable directive state.
- Add replay tests that reconstruct the same MAS graph state, merge queue state, invalidation state, and control ownership from stored authority.
- Treat any design that requires hidden operator memory, shared writable sandboxes, or non-durable prompt steering as a failure of this wave.

## Documentation closure expectations
- Update architecture and current-state docs so they describe the live MAS boundary honestly: parallel intra-wave execution, per-agent sandboxes, merge queue authority, and integrated head control are live for MAS waves only.
- Record the operator/autonomous switching model clearly enough that later waves can build on it without inventing a second control path.
- Include one proof walkthrough showing the Wave 18 pilot from parallel launch through merge, steering, mode handoff, and selective recovery.

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

### Depends on agents
- A8
- A9

### Reads artifacts from
- mas-proof-bundle
- orchestrator-proof-bundle

### Barrier class
closure-barrier

### Parallel safety
serialized

### File ownership
- .wave/reviews/wave-18-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether Wave 18 turns the harness into a real intra-wave MAS with honest operator and autonomous control.

Required context before coding:
- Read docs/implementation/true-multi-agent-wave-architecture.md.
- Read docs/plans/true-multi-agent-wave-rollout.md.
- Read docs/plans/mas-end-state-operator-head.md.

Specific expectations:
- do not PASS unless at least two implementation agents can run concurrently inside one wave
- require per-agent sandboxes, merge authority, invalidation visibility, and durable control directives
- require the TUI to inspect and signal one selected agent directly
- require clean operator-to-autonomous handoff and immediate operator reclaim
- emit the final [wave-gate] marker as a plain last line before Verdict: ...

File ownership (only touch these paths):
- .wave/reviews/wave-18-cont-qa.md
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

### Depends on agents
- A1

### Reads artifacts from
- sandbox-state
- merge-queue-state
- directive-history

### Barrier class
report-only

### Parallel safety
parallel-safe

### File ownership
- .wave/design/wave-18.md

### Final markers
- [wave-design]

### Prompt
```text
Primary goal:
- Review the Wave 18 orchestrator UX and MAS proof against docs/implementation/design.md and the MAS end-state plan.

Required context before coding:
- Read docs/implementation/design.md.
- Read docs/plans/mas-end-state-operator-head.md.
- Read docs/implementation/true-multi-agent-wave-architecture.md.

Specific expectations:
- treat the TUI orchestrator workspace as the canonical operator and head surface
- require selected-agent visibility, merge or invalidation visibility, and direct per-agent signaling
- require seamless operator or autonomous switching without split-brain behavior
- keep the review report concise and end with the final [wave-design] state=<aligned|concerns|blocked> findings=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/design/wave-18.md
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

### Depends on agents
- A1
- A2

### Reads artifacts from
- sandbox-state
- merge-queue-state
- orchestrator-control-state

### Barrier class
integration-barrier

### Parallel safety
serialized

### File ownership
- .wave/integration/wave-18.md
- .wave/integration/wave-18.json

### Final markers
- [wave-integration]

### Prompt
```text
Primary goal:
- Reconcile the MAS runtime substrate, the merge or invalidation model, and the integrated head control path into one integration verdict.

Required context before coding:
- Read docs/implementation/true-multi-agent-wave-architecture.md.
- Read docs/plans/true-multi-agent-wave-rollout.md.
- Read docs/plans/master-plan.md.

Specific expectations:
- require reducer-backed ready-set truth rather than launcher-local heuristics
- require clean auto-merge for policy-valid cases and explicit handling for conflicts or invalidations
- require operator and autonomous control to travel through the same durable directive path
- emit the final [wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- .wave/integration/wave-18.md
- .wave/integration/wave-18.json
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

### Depends on agents
- A8
- A6

### Reads artifacts from
- mas-proof-bundle
- orchestrator-proof-bundle

### Barrier class
closure-barrier

### Parallel safety
serialized

### File ownership
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/true-multi-agent-wave-rollout.md
- docs/plans/mas-end-state-operator-head.md

### Final markers
- [wave-doc-closure]

### Prompt
```text
Primary goal:
- Keep the architecture and shared-plan docs aligned with the first live MAS pilot and integrated head control landing.

Required context before coding:
- Read docs/implementation/parallel-wave-multi-runtime-architecture.md.
- Read docs/plans/true-multi-agent-wave-rollout.md.
- Read docs/plans/mas-end-state-operator-head.md.

Specific expectations:
- document the live MAS boundary honestly and keep serial waves explicit as the default path
- record the integrated TUI head surface and operator or autonomous switching model clearly
- include a proof walkthrough for parallel launch, merge, steering, and reclaim
- use `state=closed` when shared-plan docs were updated successfully, `state=no-change` when no shared-plan delta was needed, and `state=delta` only if documentation closure is still incomplete
- emit the final [wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text> marker as a plain last line

File ownership (only touch these paths):
- docs/implementation/parallel-wave-multi-runtime-architecture.md
- docs/plans/master-plan.md
- docs/plans/current-state.md
- docs/plans/true-multi-agent-wave-rollout.md
- docs/plans/mas-end-state-operator-head.md
```

## Agent A1: Parallel Runtime Substrate

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Per-agent sandboxes, lease-backed MAS execution, merge queue authority, invalidation routing, and selective recovery"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-rust-workspace
- repo-wave-closure-markers

### Components
- mas-runtime-substrate
- agent-sandbox-manager
- merge-queue-authority
- invalidation-routing

### Capabilities
- agent-sandboxes
- lease-heartbeats
- clean-auto-merge
- conflict-preservation
- selective-recovery

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/src/lib.rs
- crates/wave-coordination/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs
- crates/wave-runtime/src/lib.rs

### Depends on agents
- none

### Reads artifacts from
- none

### Writes artifacts
- sandbox-state
- merge-queue-state
- invalidation-state
- ready-set-state

### Barrier class
independent

### Parallel safety
parallel-safe

### Exclusive resources
- runtime-core
- scheduler-authority

### Parallel with
- A2

### File ownership
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/src/lib.rs
- crates/wave-coordination/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs
- crates/wave-runtime/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the first real intra-wave MAS runtime substrate with per-agent sandboxes, merge queue authority, invalidation routing, and selective recovery.

Required context before coding:
- Read docs/implementation/true-multi-agent-wave-architecture.md.
- Read docs/plans/true-multi-agent-wave-rollout.md.
- Read crates/wave-runtime/src/lib.rs.

Specific expectations:
- launch at least two safe implementation agents concurrently inside one wave
- allocate one writable sandbox per running agent from the current accepted integration head
- make clean merges auto-advance and make conflicts or invalidations explicit reducer-backed state
- preserve already accepted work when a sibling agent conflicts, invalidates, or reruns
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- Cargo.toml
- crates/wave-domain/src/lib.rs
- crates/wave-events/src/lib.rs
- crates/wave-coordination/src/lib.rs
- crates/wave-reducer/src/lib.rs
- crates/wave-projections/src/lib.rs
- crates/wave-runtime/src/lib.rs
```

## Agent A2: Integrated Head Control And Operator Surface

### Executor
- profile: implement-codex
- model: gpt-5.4
- codex.config: model_reasoning_effort=high,model_verbosity=low

### Context7
- bundle: rust-control-plane
- query: "Integrated orchestrator TUI, per-agent signaling, autonomous head directives, and operator or autonomous mode switching"

### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- tui-design
- repo-wave-closure-markers

### Components
- orchestrator-head-control
- operator-autonomous-handoff

### Capabilities
- integrated-orchestrator-workspace
- per-agent-signal-composer
- autonomous-head-control
- seamless-mode-switching

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
- docs/implementation/design.md

### Depends on agents
- none

### Reads artifacts from
- sandbox-state
- merge-queue-state
- invalidation-state

### Writes artifacts
- orchestrator-session-state
- directive-history
- head-control-state

### Barrier class
independent

### Parallel safety
parallel-safe

### Exclusive resources
- operator-ux

### Parallel with
- A1

### File ownership
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
- docs/implementation/design.md

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the integrated TUI head surface with full per-agent view and signal control, plus seamless switching between operator and autonomous modes.

Required context before coding:
- Read docs/implementation/design.md.
- Read docs/plans/mas-end-state-operator-head.md.
- Read crates/wave-tui/src/lib.rs.

Specific expectations:
- make the orchestrator workspace the MAS-first operator surface
- allow the operator to inspect one selected agent and steer, pause, resume, rerun, or rebase it directly
- allow the autonomous head to issue the same class of directives through the same control path
- keep accepted sibling work when one MAS agent requires reconciliation, rerun, or rebase instead of collapsing the whole wave immediately
- make operator reclaim immediate and visible without restarting runtime sessions
- emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output

File ownership (only touch these paths):
- crates/wave-cli/src/main.rs
- crates/wave-app-server/src/lib.rs
- crates/wave-tui/src/lib.rs
- docs/implementation/design.md
```
