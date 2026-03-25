# Phase 1 Live Proof: Scheduler Authority

This folder is the human-inspectable proof bundle for Phase 1 of the parallel-wave multi-runtime plan.

What landed in this phase:

- scheduler truth is now canonical under `.wave/state/events/scheduler/`
- reducer and projections distinguish `ready`, `claimed`, `active`, and stale-lease states
- runtime emits honest serial scheduler authority: budget, wave claims, and task leases

What is still not live:

- true parallel wave execution
- per-wave worktree isolation
- Claude runtime support

## Proof Files

- `scheduler-events.jsonl`
  canonical scheduler-event fixture showing budget, claim acquisition, lease grant, lease expiry, lease release, and claim release
- `projection-snapshot.json`
  projection-style JSON excerpt showing how ready, claimed, active, and stale-lease waves are now surfaced

## What To Inspect

1. Open `projection-snapshot.json`.
   Wave `20` is `ready` with no claim.
2. In the same file, wave `21` is `claimed`.
   It is present in `claimed_wave_ids` and absent from `claimable_wave_ids`.
3. Wave `22` is `active`.
   Its ownership state includes an active granted lease.
4. Wave `23` is `claimed` with a stale lease.
   Its ownership state includes an expired lease and the blocker summary shows the lease-expired condition explicitly.
5. Open `scheduler-events.jsonl`.
   The `wave-30` events show released lease and released claim records, proving that release semantics are represented in canonical scheduler state even though they do not remain in current-truth projections after cleanup.

This proof bundle is intentionally Phase-1 scoped: it proves scheduler authority landed without claiming that the runtime already executes multiple waves in parallel.
