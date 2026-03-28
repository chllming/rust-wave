# Phase 3 Runtime Policy And Multi-Runtime Proof

This bundle closes the Wave 15 boundary in the checked-in repo and records the manual-close override that unblocked Wave 16 in the current workspace.

## Classification

Proof classification for the artifacts in this bundle:

- Codex adapter parity proof: `fixture-backed`
- Claude adapter parity proof: `fixture-backed`
- Runtime boundary availability snapshot: `live`
- Operator runtime transport snapshot: `fixture-backed`
- Worktree-sensitive skill projection proof: `fixture-backed`
- Manual-close and dependency-unblock proof: `live`

Why the bundle is classified this way on March 26, 2026:

- the live environment snapshots in [doctor.json](./doctor.json) and [project-show.json](./project-show.json) show `codex=ready` and `claude=ready`
- the checked-in parity proof for both adapters is still the deterministic fixture capture in [runtime-boundary-proof.json](./runtime-boundary-proof.json)
- [operator-runtime-transport.json](./operator-runtime-transport.json) remains the deterministic fixture transport capture generated from the shared runtime-neutral boundary
- [control-show-wave-15.json](./control-show-wave-15.json) is now a live current-worktree validation capture with a failed latest Wave 15 run plus an applied closure override
- [control-show-wave-16.json](./control-show-wave-16.json) is the live dependent-wave capture showing that Wave 16 became `ready` and `claimable` only after the explicit Wave 15 override

This means both adapters are live in code, the parity bundle remains fixture-backed, and the closeout/unblock evidence is live.

The repo-local operator shell now also has live manual-close parity with the CLI for this control path: the `Control` tab exposes confirm-first `m` and `M` actions, the runtime helper behind those actions only accepts repo-relative existing-file evidence paths or the derived default evidence bundle from the selected terminal source run, and override application is now transactional with rerun preservation instead of best-effort clearing before the override write.

## Files

- [runtime-boundary-proof.json](./runtime-boundary-proof.json)
  The main Wave 15 bundle. It shows the same authored wave contract resolving through Codex and Claude fixture paths, durable fallback metadata, and the execution-root skill projection proof.
- [operator-runtime-transport.json](./operator-runtime-transport.json)
  Operator-facing transport proof showing runtime summary, fallback count, per-agent runtime detail, and projected skills.
- [worktree-skill-projection.md](./worktree-skill-projection.md)
  Human-readable explanation of the repo-root vs execution-root divergence proof.
- [doctor.json](./doctor.json)
  Live validation capture for `cargo run -p wave-cli -- doctor --json`, including the active Wave 15 closure override and Wave 16 claimability.
- [project-show.json](./project-show.json)
  Live validation capture for `cargo run -p wave-cli -- project show --json`, including runtime availability and closure overrides.
- [control-show-wave-15.json](./control-show-wave-15.json)
  Live validation capture for `cargo run -p wave-cli -- control show --wave 15 --json`. It shows `latest_run.status=failed`, `closure_override_applied=true`, and the durable override record side by side.
- [control-show-wave-16.json](./control-show-wave-16.json)
  Live validation capture for `cargo run -p wave-cli -- control show --wave 16 --json`. It shows that Wave 16 is `ready` and `claimable` after the Wave 15 override.
- [closure-override-wave-15.json](./closure-override-wave-15.json)
  The durable manual-close record copied from `.wave/state/control/closure-overrides/wave-15.json`.

## What This Proves

- The runtime boundary is runtime-neutral and records explicit selection policy, fallback policy, runtime identity, and projected skills.
- The same authored wave contract resolves through Codex and Claude fixture adapters without changing reducer queue semantics.
- Runtime-aware skill projection is derived from the selected wave-local execution root, not the repo root.
- Operator transport carries the same selected runtime and fallback state that the runtime persisted.
- A failed Wave 15 latest run can be waived explicitly through durable closure-override metadata without being rewritten as success.
- Downstream dependency gating accepts that explicit override and makes Wave 16 claimable while Wave 15 still shows `last_run_status=failed`.
- The same manual-close path is now operable from the TUI/operator shell, not just the CLI, and its evidence policy is fail-closed rather than free-form strings.
- The same operator shell now also uses typed human-input workflow semantics for dependency handshakes and lets operators move between multiple actionable approvals or escalations with `[` and `]` before `u` or `x` acts on the selected item.

## Remaining Wave 15 Limits

- The checked-in parity bundle still does not include a live Claude Wave 15 launch; Claude remains fixture-backed in the parity proof even though the local environment snapshot reports it available.
- The checked-in `operator-runtime-transport.json` remains fixture-backed rather than copied from the live manually closed Wave 15 workspace state.
- Wave 15 is closed in the current workspace through an explicit operator override because the live rerun failed at promotion after the relevant files had already been reconciled in the root workspace; this bundle does not claim a normal successful Wave 15 promotion.
