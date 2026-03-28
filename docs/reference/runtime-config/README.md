# Runtime Configuration Reference

This directory documents the live Wave 15 runtime boundary in the Rust workspace.

Important scope rule:

- [README.md](./README.md) is the canonical statement of live Rust runtime behavior
- [codex.md](./codex.md) describes the live Codex adapter
- [claude.md](./claude.md) describes the live Claude adapter

## Current Scope

The live runtime is now a runtime-neutral adapter boundary in `wave-runtime`:

- project config comes from `wave.toml`
- per-agent runtime policy comes from the authored wave `### Executor` block
- Codex and Claude are sibling adapters behind one runtime-neutral execution plan
- runtime selection, fallback, execution identity, and projected skills are persisted as runtime-neutral records
- scheduler and reducer remain the owners of queue semantics above that boundary

Wave 14 and Wave 15 together mean the live repo-local runtime now has:

- parallel admission for non-conflicting waves
- one wave-local execution worktree per active wave under `.wave/state/worktrees/`
- one shared wave-local filesystem view for every agent inside the same wave
- late-bound runtime-aware skill projection derived from that wave-local execution root after final runtime selection and fallback
- operator-facing runtime identity and fallback visibility in run detail transport

That shared per-wave filesystem view is the live runtime boundary, not the later intra-wave MAS target. The follow-on per-agent sandbox model is documented in [../../implementation/true-multi-agent-wave-architecture.md](../../implementation/true-multi-agent-wave-architecture.md) and [../../plans/true-multi-agent-wave-rollout.md](../../plans/true-multi-agent-wave-rollout.md).

## Live Versus Proof Classification

Read the runtime docs in this directory using these truth levels:

- `live`
  The Rust workspace implements the boundary and will execute it when the local runtime binary and auth are ready.
- `dry-run-backed`
  The repo can produce the boundary artifacts without a live external execution.
- `fixture-backed`
  The repo proves the boundary with deterministic fake binaries or synthetic transport fixtures.
- `reference-upstream`
  Broader Wave or upstream package material that this repo uses only as design input.

Today:

- the Codex adapter is `live`
- the Claude adapter is `live`
- the captured Wave 15 proof bundle may still classify individual artifacts as `live`, `dry-run-backed`, or `fixture-backed`

The current proof classification for the checked-in Wave 15 bundle is recorded in:

- [docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/README.md](../../implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/README.md)

## Resolution Rules

- `wave.toml` chooses project defaults such as canonical authority roots and shared catalog locations.
- Each agent's `### Executor` block is authoritative for runtime choice and runtime-specific fields.
- `id` and `fallbacks` define explicit runtime policy for the agent.
- If no explicit runtime is authored, the runtime policy defaults to `codex`.
- The selected runtime is the first available runtime in the authored order; fallback is recorded before work starts.
- Runtime-aware skill projection happens after final runtime selection and uses the wave-local execution root or worktree, not the repo root.
- A declared skill that is absent from the execution root is dropped from the projected runtime skill set and recorded as dropped.
- If `skills/runtime-<selected-runtime>/` exists in the execution root, Wave auto-attaches it and records that auto-attachment.

This keeps the runtime overlay, projected skill directories, and actual execution filesystem in one coherent view.

## Active Executor Fields

The live Rust runtime currently honors these cross-runtime `### Executor` keys:

| Wave `### Executor` key | Launch effect |
| --- | --- |
| `id` | Selects the requested runtime explicitly |
| `fallbacks` | Declares ordered fallback runtimes |
| `model` | Passes the model flag for the selected runtime when supported |
| `profile` | Still participates in runtime inference and authored metadata |

Runtime-specific keys are documented in [codex.md](./codex.md) and [claude.md](./claude.md).

## Generated Runtime Artifacts

For each launched agent, the runtime now writes runtime-neutral artifacts in the agent bundle directory under `.wave/state/build/specs/<run-id>/agents/<agent-id>/`:

- `prompt.md`
- `runtime-prompt.md`
- `runtime-skill-overlay.md`
- `runtime-detail.json`
- `last-message.txt`
- `events.jsonl`
- `stderr.txt`

The Claude adapter also writes:

- `claude-system-prompt.txt`
- `claude-settings.json` when an inline settings overlay is generated

The runtime detail snapshot records:

- selected runtime
- runtime selection reason
- fallback metadata, when fallback happened
- execution identity
- runtime artifact paths
- declared, projected, dropped, and auto-attached skills

The structured result envelope under `.wave/state/results/` carries the same runtime record so transport and proof surfaces can read durable runtime identity without depending only on compatibility run state.

## Operator Surfaces

The live operator/runtime surface exposes runtime state through:

- `wave doctor --json`
- `wave project show --json`
- `wave control show --wave <id> --json`
- app-server `latest_run_details` and `active_run_details`
- the TUI `Overview`, `Agents`, `Proof`, and `Control` tabs

Those surfaces are runtime-neutral. They show selected runtime, fallback count, and runtime detail without leaking runtime-specific fields into reducer queue truth.

## Validation Path

Use:

```bash
cargo run -p wave-cli -- project show --json
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- control show --wave 15 --json
```

Then inspect the Wave 15 proof bundle under:

```text
docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/
```
