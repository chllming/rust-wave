# Phase 2 Live Proof: Parallel-Wave Execution

This folder is the human-inspectable proof bundle for the Wave 14 repo-local parallel-wave cutover.

What is live now:

- two non-conflicting waves can execute concurrently in repo-local use
- each active wave gets one isolated worktree under `.wave/state/worktrees/`
- agents inside the same wave share that wave-local worktree
- promotion state is explicit before closure and conflicts block closure
- reducer-backed projections expose worktree identity, promotion state, scheduler phase, fairness rank, and protected closure state

What is still not live:

- Claude runtime adapters
- a runtime policy engine beyond the current scheduler-budget and scheduling-state visibility
- portfolio or release-layer delivery state
- question, contradiction, and decision-lineage invalidation
- per-agent worktrees

## Proof Files

- `parallel-runtime-proof.json`
  runtime-backed proof report showing two non-conflicting waves launched concurrently, distinct worktrees per active wave, shared intra-wave worktree reuse, overlapping execution windows, and explicit promotion state
- `projection-snapshot.json`
  reducer-backed planning and operator snapshot captured from the proof fixture, showing worktree identity, promotion state, scheduling state, fairness rank, and closure-protection fields in projection truth
- `promotion-conflict.json`
  runtime-backed proof that promotion conflict is explicit and blocks closure before dishonest closure completion
- `scheduler-events.jsonl`
  canonical scheduler-event stream from the parallel proof fixture, including worktree, promotion, and scheduling updates
- `trace-latest-wave-14.json`
  stored `wave trace latest --wave 14 --json` output from the generated local proof trace
- `trace-replay-wave-14.json`
  stored `wave trace replay --wave 14 --json` output from the generated local proof trace

## What To Inspect

1. Open `parallel-runtime-proof.json`.
   Confirm `overlap_observed=true`, `distinct_worktrees=true`, and `per_agent_worktrees_used=false`.
2. In the same file, inspect each wave entry.
   The `worktree.path` values differ across waves, while each wave's `agent_worktree_markers` all point at that one shared path.
3. Open `promotion-conflict.json`.
   Confirm `promotion.state=conflicted` and that the conflicting paths are recorded before closure.
4. Open `projection-snapshot.json`.
   Inspect the per-wave `execution` and `ownership.budget` fields for worktree, promotion, scheduler phase, fairness, and closure-protection visibility.
5. Open `scheduler-events.jsonl`.
   Confirm there are explicit `WaveWorktreeUpdated`, `WavePromotionUpdated`, and `WaveSchedulingUpdated` events.
6. Compare `trace-latest-wave-14.json` and `trace-replay-wave-14.json`.
   These are the stored outputs from the repo-local proof trace that `wave trace latest --wave 14 --json` and `wave trace replay --wave 14 --json` now return in this workspace.

## Reproduce

1. Regenerate the proof bundle and local trace seed:
   `cargo test -p wave-runtime generate_phase_2_parallel_wave_execution_live_proof_bundle -- --ignored --exact --nocapture`
2. Re-run the validation surfaces:
   `cargo run -p wave-cli -- doctor --json`
   `cargo run -p wave-cli -- control status --json`
   `cargo run -p wave-cli -- trace latest --wave 14 --json`
   `cargo run -p wave-cli -- trace replay --wave 14 --json`

This proof bundle is intentionally Wave-14 scoped: repo-local parallel-wave execution is now real, but runtime plurality, richer runtime policy, and delivery-layer aggregation remain Wave 15 and later work.
