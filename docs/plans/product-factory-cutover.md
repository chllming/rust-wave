# Product-Factory Cutover

This document is the implementation contract for the product-factory branch in the dedicated worktree at `/home/coder/codex-wave-product-factory`.

Wave 17 is starting soon. The branch is intentionally isolated so Wave 17 can complete on the main line first, then absorb this work with a controlled merge once the delivery-layer direction is stable.

## Goals

- Land a delivery layer above waves with typed initiative, release, acceptance-package, risk, and debt objects.
- Keep Rust-first authority, reducer-backed queue truth, and repo-local state roots as the product core.
- Add a real repo-local `adhoc` lane with isolated state and a promotion path back into `waves/`.
- Add pre-implementation design gates so design workers can block code execution without collapsing back into closure-only review.
- Expose machine-facing signal output and stable shell wrappers for resident agents, CI, and automation.
- Stage the next release cut for true intra-wave MAS execution without pretending the live runtime already supports it.

## Release Boundary

Two release cuts now matter:

- `factory-core-v1`
  The merge target for this worktree once Wave 17 completes on the main line. This is the branch that lands delivery truth above waves, repo-local `adhoc`, design gates, and machine-facing signal surfaces.
- `factory-mas-v2`
  The next release after `factory-core-v1` merges. This is where true intra-wave multi-agent execution lands: graph-aware agents, per-agent sandboxes, merge and invalidation records, and reducer-backed ready-set selection inside one active wave.

The current worktree should seed the plan, catalog, and target-state documentation for `factory-mas-v2`, but it should not claim that the live runtime already moved past serial intra-wave execution.

## Deliberate Divergence

- Keep `.wave/state/` as the canonical local authority. Do not reintroduce package-era temp roots as product truth.
- Keep hard scheduler and closure state separate from soft advisory state.
- Keep soft states because they are operationally useful, but scope them to delivery objects and operator advisories.
- Keep Wave itself as the execution unit, then let initiative, release, and acceptance-package objects explain ship and no-ship decisions above it.

## Soft State Rules

Soft states are retained because they have been useful in practice.

- `clear`: no advisory signal is active.
- `advisory`: something deserves operator attention but does not change queue readiness or proof gating by itself.
- `degraded`: confidence is reduced and operator attention is required, but hard release or closure state still decides blocking.
- `stale`: the delivery view is out of date enough that operators should distrust it until refreshed or reconciled.

Soft states must never, by themselves:

- claim or release scheduler authority
- make a blocked wave ready
- satisfy proof or closure
- override a hard release or acceptance decision

The delivery and operator soft-state axis remains separate from coordination severity. The follow-on MAS release keeps the useful sibling Wave severities:

- `hard`
- `soft`
- `stale`
- `advisory`
- `proof-critical`
- `closure-critical`

Those coordination severities belong to blocker and invalidation records. They are not a replacement for the delivery-level `clear | advisory | degraded | stale` axis, and they should not be collapsed into one enum.

## Delivery Layer

The first implementation branch introduces:

- `Initiative`
- `Release`
- `AcceptancePackage`
- `DeliveryRisk`
- `DeliveryDebt`

The first hard-state slice is intentionally small:

- `ReleaseState = planned | assembling | candidate | ready | shipped | rejected`
- `AcceptancePackageState = draft | collecting_evidence | review_ready | accepted | rejected`

The first branch also keeps `milestone_id` and `release_train_id` as scalar fields on `Release`. Dedicated milestone and train reducers can come later if the product factory needs them.

## Wave Metadata

Waves stay backward compatible, but the authored schema grows these optional frontmatter fields:

- `wave_class`
- `intent`
- `delivery`
- `design_gate`

Rules:

- Existing waves continue to parse unchanged.
- Delivery-aware waves must declare a delivery link.
- Non-implementation waves must declare an intent.
- Design gates are only valid on implementation waves.
- Design-gated implementation waves must list non-closure design workers and use the `ready-for-implementation` gate marker.

The follow-on `factory-mas-v2` release extends this contract again with opt-in MAS metadata:

- `execution_model = serial | multi-agent`
- per-agent graph and resource declarations such as `depends_on_agents`, `reads_artifacts_from`, `writes_artifacts`, `barrier_class`, and `exclusive_resources`
- explicit per-wave concurrency budgets

The default remains `serial` until the MAS scheduler and runtime phases land.

## Adhoc Lane

The adhoc lane is repo-local and isolated.

- Planned runs live under `.wave/state/adhoc/runs/<run-id>/`.
- Runtime authority for adhoc runs lives under `.wave/state/adhoc/runtime/<run-id>/`.
- The stored run record includes `request.json`, `spec.json`, `wave-0.md`, and `result.json`.
- Promotion writes a numbered wave into `waves/` and records the promoted wave id back into the adhoc result.

## Operator Surfaces

The branch extends:

- `wave control status --json` with a compact `signal` payload
- per-wave `soft_state` in planning status
- `wave delivery status`
- `wave delivery initiative show --id <id>`
- `wave delivery release show --id <id>`
- `wave delivery acceptance show --id <id>`
- the TUI with a delivery tab
- the app-server snapshot with delivery read models
- shell wrappers in `scripts/wave-status.sh` and `scripts/wave-watch.sh`

The `factory-mas-v2` follow-on extends those same surfaces rather than inventing a second operator stack. The TUI, CLI, and app-server should later show:

- the per-wave agent DAG
- the ready set
- running, merge-pending, conflicted, and invalidated agents
- per-agent sandbox and lease health
- per-wave concurrency budgets
- barrier explanations and merge-queue state

## Sibling Carry-Forward

The sibling `agent-wave-orchestrator` repo remains useful input, but only in bounded ways.

Useful carry-forward:

- versioned signal wrappers and shell-friendly wake loops
- design-steward overlap and report-only closure that can begin from partial evidence
- coordination severities that distinguish `soft`, `stale`, `advisory`, and proof-critical blocking work
- targeted rerun and recovery instead of broad restart-by-default behavior

Deliberate divergence:

- keep `.wave/state/` as the canonical authority root rather than package-era `.tmp/<lane>-wave-launcher/`
- keep reducers and projections as the truth source rather than launcher-local temp state
- keep the built-in TUI, app-server, and Rust CLI as the operator surface rather than tmux or editor attachment becoming the primary product
- keep Wave positioned as a product factory and delivery control plane, not only a coding harness

## True MAS Follow-On

The dedicated follow-on architecture and rollout plan live in:

- `docs/implementation/true-multi-agent-wave-architecture.md`
- `docs/plans/true-multi-agent-wave-rollout.md`

That follow-on is intentionally staged after `factory-core-v1` merges. It should start from the merged post-Wave-17 product-factory baseline, not from the dirty pre-merge main checkout and not from package-era launcher state.

## Merge Intent

This branch is meant to merge after Wave 17 completes, not before. Until then it is the place to integrate delivery truth, soft-state handling, adhoc flows, and product-factory surfaces without destabilizing the Wave 17 landing sequence.
