# Phase 2 Live Proof: Parallel-Wave Execution

This folder is the human-inspectable proof bundle for the Wave 14 repo-local parallel-wave cutover.

What is live now:

- two non-conflicting waves can execute concurrently in repo-local use
- each active wave gets one isolated worktree under `.wave/state/worktrees/`
- agents inside the same wave share that wave-local worktree
- app-server `latest_run_details` and `active_run_details` now carry the same worktree, promotion, and scheduling execution state that the reducer exposes for the wave
- promotion state is explicit before closure, and `ready` now means the candidate passed a scratch merge validation against the current target snapshot
- released worktrees are actually removed from Git and the filesystem before the runtime records `state=released`
- FIFO fairness within the claimable implementation lane now affects admission order, while reserved closure capacity and lease-level preemption still outrank that lane
- reducer-backed projections expose worktree identity, promotion state, scheduler phase, fairness rank, protected closure state, and preemption evidence

What is still not live:

- Claude runtime adapters
- a runtime policy engine beyond the current scheduler-budget and scheduling-state visibility
- portfolio or release-layer delivery state
- question, contradiction, and decision-lineage invalidation
- per-agent worktrees

## Proof Files

- `active-run-detail-transport.json`
  app-server transport snapshot showing that `latest_run_details` and `active_run_details` now include the nested `execution` object with worktree, promotion, and scheduling state
- `fairness-admission-order.json`
  proof artifact showing raw claimable order, the computed FIFO fairness order, the selected wave, and the waiting scheduling record that preserves the deferred wave's fairness rank
- `parallel-runtime-proof.json`
  runtime-backed proof report showing two non-conflicting waves launched concurrently, distinct worktrees per active wave, shared intra-wave worktree reuse, overlapping execution windows, and explicit promotion state
- `projection-snapshot.json`
  reducer-backed planning and operator snapshot captured from the proof fixture, showing worktree identity, promotion state, scheduling state, fairness rank, and closure-protection fields in projection truth
- `promotion-conflict.json`
  runtime-backed proof that promotion conflict is explicit and blocks closure before dishonest closure completion, using the same scratch merge validation path that marks `ready`
- `reserved-closure-capacity.json`
  runtime-backed proof that reserved closure capacity changes parallel admission and publishes waiting/fairness state for deferred implementation waves
- `preemption-proof.json`
  runtime-backed proof that a closure lease can revoke a saturated implementation lease and that the preempted wave is surfaced through scheduling records rather than hidden launcher behavior
- `scheduler-events.jsonl`
  canonical scheduler-event stream from the parallel proof fixture, including worktree, promotion, and scheduling updates
- `trace-latest-wave-14.json`
  stored `wave trace latest --wave 14 --json` output from the generated local proof trace
- `trace-replay-wave-14.json`
  stored `wave trace replay --wave 14 --json` output from the generated local proof trace

## What To Inspect

1. Which worktree does a wave own?
   Open `parallel-runtime-proof.json`.
   Confirm `distinct_worktrees=true`, then inspect each wave entry's `worktree.worktree_id` and `worktree.path`.
   The two waves should have different paths, while each wave's `agent_worktree_markers` all point at that one shared path.
2. Does the active-run transport carry the same execution state?
   Open `active-run-detail-transport.json`.
   Confirm `latest_run_details[*].execution` and `active_run_details[*].execution` both include `worktree`, `promotion`, `scheduling`, `merge_blocked`, and `closure_blocked_by_promotion`.
3. What promotion state is the wave in, and is merge blocked?
   Open `active-run-detail-transport.json` for the transport path and `projection-snapshot.json` for the reducer-backed path.
   Inspect `execution.promotion.state`, `execution.promotion.detail`, `execution.merge_blocked`, and `execution.closure_blocked_by_promotion`.
4. How does a real promotion conflict look?
   Open `promotion-conflict.json`.
   Confirm `evaluated_promotion.state=conflicted`, `conflict_paths` is populated, and the detail names the merge-blocked file.
5. Why is a wave waiting?
   Open `fairness-admission-order.json`.
   Confirm `claimable_wave_ids=[5,6]`, `fairness_ordered_wave_ids=[6,5]`, `selected_wave_ids=[6]`, and that the deferred wave keeps `state=waiting`, `fairness_rank=2`, and `last_decision="waiting for fairness turn behind older claimable waves"`.
6. When is closure capacity reserved?
   Open `reserved-closure-capacity.json`.
   Confirm `closure_capacity_reserved=true`, `selected_wave_ids=[]`, and that the deferred implementation waves stay in `state=waiting` with `last_decision="waiting because closure capacity is reserved ahead of new implementation work"`.
7. When did preemption happen?
   Open `preemption-proof.json`.
   Confirm `wait_outcome.kind=lease_revoked`, `wait_outcome.detail` names the closure-capacity preemption, and the captured scheduling event shows `state=preempted` with the same detail in `last_decision`.
8. Is the canonical scheduler stream explicit?
   Open `scheduler-events.jsonl`.
   Confirm there are explicit `WaveWorktreeUpdated`, `WavePromotionUpdated`, and `WaveSchedulingUpdated` events.
9. Do replay surfaces still line up with the seeded proof run?
   Compare `trace-latest-wave-14.json` and `trace-replay-wave-14.json`.
   These are the stored outputs from the repo-local proof trace that `wave trace latest --wave 14 --json` and `wave trace replay --wave 14 --json` now return in this workspace.

## Reproduce

1. Regenerate the proof bundle and local trace seed:
   `cargo test -p wave-runtime --lib tests::generate_phase_2_parallel_wave_execution_live_proof_bundle -- --ignored --exact --nocapture`
2. Regenerate the transport snapshot artifact:
   `cargo test -p wave-app-server --lib tests::export_phase_2_parallel_wave_active_run_transport_snapshot -- --ignored --exact --nocapture`
3. Re-run the validation surfaces:
   `cargo run -p wave-cli -- doctor --json`
   `cargo run -p wave-cli -- control status --json`
   `cargo run -p wave-cli -- trace latest --wave 14 --json`
   `cargo run -p wave-cli -- trace replay --wave 14 --json`

This proof bundle is intentionally Wave-14 scoped: repo-local parallel-wave execution is now real, but runtime plurality, richer runtime policy, and delivery-layer aggregation remain Wave 15 and later work.
