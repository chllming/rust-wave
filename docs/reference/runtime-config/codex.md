# Codex Runtime Configuration

This wave documents the launcher substrate only: project-scoped Codex state, runtime paths, and the invocation shape used by `wave launch` and `wave autonomous`.

The current Rust launcher runs Codex with `codex exec`, pipes the compiled agent prompt through stdin, and writes the final assistant message to the per-agent `last-message.txt`.

## Live Invocation Shape

Today the launcher always invokes Codex roughly like this:

```bash
codex exec \
  --json \
  --skip-git-repo-check \
  --dangerously-bypass-approvals-and-sandbox \
  --color never \
  -C <repo-root> \
  -o <bundle-dir>/agents/<agent-id>/last-message.txt \
  --model <resolved-model> \
  -c <resolved-config-entry> ...
```

It also sets:

- `CODEX_HOME=.wave/codex`
- `CODEX_SQLITE_HOME=.wave/codex`

This keeps auth, sqlite state, and session logs project-scoped instead of mutating the operator's global Codex home.

The launcher also assumes the compiled wave bundle already exists under `.wave/state/build/specs/<run-id>/`, so the runtime reads prompts from the build artifact and does not invent alternate paths at launch time.

## Supported `### Executor` Keys

The live launcher currently honors:

| Wave `### Executor` key | Launch effect |
| --- | --- |
| `model` | Adds `--model <name>` |
| `codex.config` | Adds repeated `-c key=value` overrides |

The `profile` field is still valuable as authored metadata, but the current Rust launcher does not yet synthesize a separate runtime profile layer from it.

Launcher behavior is intentionally narrow in this wave:

- it consumes compiled prompts from the build bundle
- it respects the repo-local Codex home from `wave.toml`
- it records the final assistant message to `last-message.txt`
- it does not imply a TUI, queue manager, or autonomous scheduler beyond the launch entrypoint itself

## Operator Overrides

Two env vars override authored Codex settings for a launch:

| Env var | Launch effect |
| --- | --- |
| `WAVE_CODEX_MODEL_OVERRIDE` | Replaces the resolved `model` for every agent |
| `WAVE_CODEX_CONFIG_OVERRIDE` | Replaces the resolved `codex.config` entries for every agent |

Example:

```bash
WAVE_CODEX_MODEL_OVERRIDE=gpt-5.4-mini \
WAVE_CODEX_CONFIG_OVERRIDE=model_reasoning_effort=low,model_verbosity=low \
cargo run -p wave-cli -- launch --wave 1 --json
```

Use that path when you need to speed up long queue execution without rewriting every wave file.

## Current Limits

These Codex-era knobs from the older package launcher are not live in the Rust rewrite yet:

- `codex.command`
- `codex.sandbox`
- `codex.profile_name`
- `codex.search`
- `codex.images`
- `codex.add_dirs`
- `codex.json`
- `codex.ephemeral`

Do not treat those as implemented until the runtime explicitly starts consuming them.

## Runtime Paths

The launcher substrate expects these repo-local paths:

- `.wave/codex/`
  project-scoped Codex auth and sqlite state
- `.wave/state/build/specs/<run-id>/`
  compiled wave bundle and per-agent prompts
- `.wave/state/build/specs/<run-id>/agents/<agent-id>/last-message.txt`
  final assistant message written by the launcher
- `.wave/state/runs/`
  recorded run state for launch and dry-run execution
- `.wave/traces/runs/`
  replay bundles and trace artifacts

If any of those paths move, update `wave.toml`, the launcher, and the docs together.

## Validation Path

Use:

```bash
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- launch --wave 0 --dry-run --json
```

Then inspect:

- `.wave/state/build/specs/<run-id>/preflight.json`
- `.wave/state/build/specs/<run-id>/agents/<agent-id>/prompt.md`
