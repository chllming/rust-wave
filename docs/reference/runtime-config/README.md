# Runtime Configuration Reference

This directory documents the runtime surface for the Rust rewrite in this repo.

Important scope rule:

- [README.md](./README.md)
  is the canonical statement of live Rust runtime behavior
- `codex.md`
  describes the live Codex launcher substrate
- `claude.md`
  is target-state/reference material for the Rust rewrite until a Rust executor adapter actually ships Claude support

## Current Scope

The live Rust runtime is intentionally narrow:

- project config comes from `wave.toml`
- per-agent runtime settings come from the authored wave `### Executor` block
- the only live runtime today is Codex
- Wave 0.2 authority roots are typed in `wave.toml`, and planning, queue, and control projections now read through reducer-backed models over compatibility run inputs
- `wave adhoc` and `wave dep` are still pending

The active runtime paths are repo-local. The canonical Wave 0.2 authority-root set under `.wave/state/` is:

- control events: `.wave/state/events/control/`
- coordination records: `.wave/state/events/coordination/`
- structured results: `.wave/state/results/`
- derived state roots: `.wave/state/derived/`
- projection roots: `.wave/state/projections/`
- canonical trace roots: `.wave/state/traces/`

Supporting repo-local roots in the same tree are:

- compiled bundles: `.wave/state/build/specs/`
- rerun intents: `.wave/state/control/reruns/`

The remaining repo-local compatibility outputs are:

- compatibility run state: `.wave/state/runs/`
- compatibility trace bundles: `.wave/traces/runs/`
- project-scoped Codex home: `.wave/codex/`

## Live Versus Target-State

Read the runtime docs in this directory using these truth levels:

- `live`
  The current Rust workspace actually implements it.
- `target-state`
  The Rust architecture intends to implement it, but the code in this repo does not yet do so.
- `reference-upstream`
  The broader package or upstream Wave surface supports it, but this repo may only use it as design input.

Today:

- Codex runtime behavior is `live`
- Claude runtime behavior is `target-state` for the Rust rewrite
- broader multi-runtime package surfaces are `reference-upstream` unless a Rust-specific doc says otherwise

## Resolution Rules

- `wave.toml` chooses the project defaults such as `default_mode`, canonical authority roots, role-prompt paths, and shared catalog locations.
- Each agent's `### Executor` block is authoritative for that agent within the limits of the live Rust runtime.
- The current launcher only consumes the fields it actually implements. Unsupported keys are inert documentation until the runtime grows to honor them.
- Skills are not auto-attached from config yet. The live contract is explicit per-agent `### Skills` in `waves/*.md`.
- `wave doctor` now verifies the configured role-prompt files and that the canonical authority roots stay under `.wave/state/`.
- `wave project show --json` and `wave doctor --json` expose and validate the typed authority-root contract; planning, queue, and operator projections are reducer-backed over compatibility run inputs, while proof and closure surfaces are now envelope-first and replay still depends on compatibility run and trace artifacts.

## Active Codex Fields

The current Rust launcher uses these fields from `### Executor`:

| Wave `### Executor` key | Launch effect |
| --- | --- |
| `profile` | Stored as authored metadata today; not used to synthesize a separate runtime layer |
| `model` | Adds `--model <name>` to `codex exec` |
| `codex.config` | Adds repeated `-c key=value` overrides |

Two operator env vars can override authored settings at launch time without rewriting the wave files:

| Env var | Effect |
| --- | --- |
| `WAVE_CODEX_MODEL_OVERRIDE` | Overrides every agent's resolved `model` |
| `WAVE_CODEX_CONFIG_OVERRIDE` | Overrides every agent's resolved `codex.config` entries |

This is primarily for operator control of latency and reasoning effort during long queue runs.

## Generated Artifacts

For each launched wave, the runtime writes:

- `preflight.json` in the compiled bundle directory
- one `prompt.md` per agent under `.wave/state/build/specs/<run-id>/agents/<agent-id>/`
- one `last-message.txt`, `events.jsonl`, and `stderr.txt` per completed agent in the same directory
- one structured result envelope per completed agent attempt under `.wave/state/results/wave-<id>/<attempt-id>/agent_result_envelope.json`, written through `wave-results`
- a compatibility run-state record in `.wave/state/runs/<run-id>.json`
- a compatibility trace bundle in `.wave/traces/runs/<run-id>.json`

Wave 0.2 also reserves canonical authority roots for control events, coordination records, results, derived state, projections, and traces under `.wave/state/`. Those roots are now typed, resolved, and doctor-checked. Planning, queue, and control surfaces are already reducer-backed over compatibility run inputs in this stage. Proof snapshots, `wave control proof show`, and closure-gate reads now recompute from the current stored result envelopes first, with the `wave-results` legacy adapter as the only remaining marker-scan path. For closure agents, the result layer also re-reads the owned integration and cont-QA artifacts so the machine-readable closure verdict survives an incomplete terminal summary. Replay still has an explicit compatibility dependency on `.wave/state/runs/` and `.wave/traces/runs/`, although the replay checks now compare normalized run, trace, and result-envelope references instead of raw path formatting.

The TUI and `wave control ...` now consume reducer-backed projections plus envelope-first proof state over the artifacts above. Proof views and app-server snapshots resolve the latest relevant run for a wave, not only an active run. `wave trace ...` remains compatibility-backed in this wave, and Wave 13 is still the planned home for the launcher-side post-agent gate work that will stop after each implementation slice and run mandatory validation before advancing.

This file does not claim that the Rust runtime already supports:

- live Claude execution
- runtime fallback across providers
- runtime-aware skill projection
- true parallel-wave scheduling

Those remain architecture targets described elsewhere in the docs.

The repo-local parity checks for this boundary are:

- `cargo test -p wave-runtime persist_agent_result_envelope_writes_canonical_result_path`
- `cargo test -p wave-gates compatibility_run_input_prefers_structured_result_envelope_markers`
- `cargo test -p wave-app-server build_run_detail_prefers_structured_result_envelope_for_proof`
- `cargo test -p wave-cli proof_report_falls_back_to_latest_completed_run`
- `cargo test -p wave-results`

## Recommended Validation Path

Use the Rust CLI directly:

```bash
cargo run -p wave-cli -- project show --json
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- lint --json
cargo run -p wave-cli -- launch --wave 0 --dry-run --json
```

Then inspect the compiled bundle and preflight report under `.wave/state/build/specs/`.
