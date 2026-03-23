# Runtime Configuration Reference

This directory now documents the active Rust/Codex runtime surface in this repo, not the older package-era `wave.config.json` layering model.

## Current Scope

The live runtime is intentionally narrow:

- project config comes from `wave.toml`
- per-agent runtime settings come from the authored wave `### Executor` block
- the only live runtime today is Codex
- `wave adhoc` and `wave dep` are still pending

The active runtime paths are repo-local:

- compiled bundles: `.wave/state/build/specs/`
- run state: `.wave/state/runs/`
- rerun intents: `.wave/state/control/reruns/`
- traces: `.wave/traces/runs/`
- project-scoped Codex home: `.wave/codex/`

## Resolution Rules

- `wave.toml` chooses the project defaults such as `default_mode`.
- Each agent's `### Executor` block is authoritative for that agent.
- The current launcher only consumes the fields it actually implements. Unsupported keys are inert documentation until the runtime grows to honor them.
- Skills are not auto-attached from config yet. The live contract is explicit per-agent `### Skills` in `waves/*.md`.

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
- a run-state record in `.wave/state/runs/<run-id>.json`
- a trace bundle in `.wave/traces/runs/<run-id>.json`

The TUI, `wave control ...`, and `wave trace ...` all project from those same repo-local artifacts.

## Recommended Validation Path

Use the Rust CLI directly:

```bash
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- lint --json
cargo run -p wave-cli -- launch --wave 0 --dry-run --json
```

Then inspect the compiled bundle and preflight report under `.wave/state/build/specs/`.
