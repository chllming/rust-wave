# Phase 1 Live Proof: Scheduler Authority

This folder is the human-inspectable proof bundle for the landed Phase 1 scheduler-authority work.

What is live now:

- scheduler truth is canonical under `.wave/state/events/scheduler/`
- local claim admission is exclusive under concurrent launcher paths
- reducer and projections distinguish `ready`, `claimed`, `active`, `blocked`, and expired-lease ownership states
- live runtime leases now grant, renew, release, revoke, and expire through canonical scheduler events

What is still not live:

- true parallel-wave execution
- per-wave worktree mutation
- Claude runtime support

## Proof Files

- `scheduler-events.jsonl`
  runtime-style scheduler authority sequence showing budget bootstrap, claim acquisition, lease grant, lease renewal, lease release, lease expiry, and claim release
- `projection-snapshot.json`
  scenario-style projection excerpts showing ready-unclaimed, claimed-and-non-claimable, active-with-live-lease, expired-lease, and budget-blocked ownership states
- `concurrent-claim-refusal.json`
  human-readable proof that one launcher path acquires the claim and a second launcher path is refused before run-state mutation

## What To Inspect

1. Open `projection-snapshot.json`.
   The `ready_unclaimed` scenario shows a wave that is planning-ready and claimable without any held claim.
2. In the same file, `claimed_and_nonclaimable` shows a wave that is still planning-ready but is no longer claimable because another scheduler owner holds the claim.
3. `active_with_live_lease` shows a granted lease with a live heartbeat timestamp and a future expiry timestamp.
4. `expired_lease` shows the reducer or projection shape after a live lease expiry path.
5. `budget_blocked` shows a wave that is planning-ready but still cannot start because scheduler capacity is exhausted.
6. Open `concurrent-claim-refusal.json`.
   It records the admitted claim plus the refused concurrent launcher path and shows that no run-state file was written for the loser.
7. Open `scheduler-events.jsonl`.
   Wave `30` shows the live serial path through grant, renewal, release, and claim release.
   Wave `31` shows the live expiry path through grant, expiry, and claim release.

This proof bundle is intentionally Phase-1 scoped: scheduler authority is now canonical and operationally enforced for serial execution, but true parallel-wave execution still belongs to Wave 14 and later.
