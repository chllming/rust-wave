# Runtime Configuration Reference

This directory now documents the active Rust/Codex runtime surface in this repo, not the older package-era `wave.config.json` layering model.

## Current Scope

The live runtime is intentionally narrow:

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

## Resolution Rules

- `wave.toml` chooses the project defaults such as `default_mode`, canonical authority roots, role-prompt paths, and shared catalog locations.
- Each agent's `### Executor` block is authoritative for that agent.
- The current launcher only consumes the fields it actually implements. Unsupported keys are inert documentation until the runtime grows to honor them.
- Skills are not auto-attached from config yet. The live contract is explicit per-agent `### Skills` in `waves/*.md`.
- `wave doctor` now verifies the configured role-prompt files and that the canonical authority roots stay under `.wave/state/`.
- `wave project show --json` and `wave doctor --json` expose and validate the typed authority-root contract; planning, queue, and operator projections are reducer-backed over compatibility run inputs, while replay and proof lifecycle still depend on compatibility run and trace artifacts.

## Active Codex Fields

The current launcher uses these fields from `### Executor`:

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
- a compatibility run-state record in `.wave/state/runs/<run-id>.json`
- a compatibility trace bundle in `.wave/traces/runs/<run-id>.json`

Wave 0.2 also reserves canonical authority roots for control events, coordination records, results, derived state, projections, and traces under `.wave/state/`. Those roots are now typed, resolved, and doctor-checked. Planning, queue, and control surfaces are already reducer-backed over compatibility run inputs in this stage, but replay and proof lifecycle have not yet moved fully onto event and envelope authority.

The TUI and `wave control ...` now consume reducer-backed projections over the compatibility artifacts above. `wave trace ...` and proof lifecycle remain compatibility-backed until later cutover waves replace `.wave/state/runs/` and `.wave/traces/runs/` with event, envelope, and replay-v2 authority.

## Recommended Validation Path

Use the Rust CLI directly:

```bash
cargo run -p wave-cli -- project show --json
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- lint --json
cargo run -p wave-cli -- launch --wave 0 --dry-run --json
```

Then inspect the compiled bundle and preflight report under `.wave/state/build/specs/`.
