# Phase 3 Runtime Policy And Multi-Runtime Proof

This bundle closes the Wave 15 boundary in the checked-in repo.

## Classification

Proof classification for the artifacts in this bundle:

- Codex adapter proof: `fixture-backed`
- Claude adapter proof: `fixture-backed`
- Runtime boundary availability snapshot: `live`
- Operator runtime transport snapshot: `fixture-backed`
- Worktree-sensitive skill projection proof: `fixture-backed`

Why the bundle is classified this way on March 26, 2026:

- the live environment snapshots in [doctor.json](./doctor.json) and [project-show.json](./project-show.json) show `codex=ready` and `claude=ready`
- the checked-in parity proof for both adapters is still the deterministic fixture capture in [runtime-boundary-proof.json](./runtime-boundary-proof.json)
- [control-show-wave-15.json](./control-show-wave-15.json) is included as the required current-worktree validation capture; it has no latest run for Wave 15 in this workspace

This means both adapters are live in code, but the checked-in Wave 15 parity bundle remains fixture-backed.

## Files

- [runtime-boundary-proof.json](./runtime-boundary-proof.json)
  The main Wave 15 bundle. It shows the same authored wave contract resolving through Codex and Claude fixture paths, durable fallback metadata, and the execution-root skill projection proof.
- [operator-runtime-transport.json](./operator-runtime-transport.json)
  Operator-facing transport proof showing runtime summary, fallback count, per-agent runtime detail, and projected skills.
- [worktree-skill-projection.md](./worktree-skill-projection.md)
  Human-readable explanation of the repo-root vs execution-root divergence proof.
- [doctor.json](./doctor.json)
  Required validation capture for `cargo run -p wave-cli -- doctor --json`.
- [project-show.json](./project-show.json)
  Required validation capture for `cargo run -p wave-cli -- project show --json`.
- [control-show-wave-15.json](./control-show-wave-15.json)
  Required validation capture for `cargo run -p wave-cli -- control show --wave 15 --json`.

## What This Proves

- The runtime boundary is runtime-neutral and records explicit selection policy, fallback policy, runtime identity, and projected skills.
- The same authored wave contract resolves through Codex and Claude fixture adapters without changing reducer queue semantics.
- Runtime-aware skill projection is derived from the selected wave-local execution root, not the repo root.
- Operator transport carries the same selected runtime and fallback state that the runtime persisted.

## Remaining Wave 15 Limits

- The checked-in proof bundle does not include a live Codex execution even though the current environment snapshot reports Codex available.
- The checked-in proof bundle does not include a live Claude launch even though the current environment snapshot reports Claude available.
- `control show --wave 15 --json` in the current workspace still has no real latest Wave 15 run record; the operator runtime transport proof is therefore fixture-backed rather than taken from a live Wave 15 run in this worktree.
