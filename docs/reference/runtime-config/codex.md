# Codex Runtime Configuration

This page documents the live Codex adapter behind the Wave 15 runtime-neutral boundary.

Codex is no longer a special-case launcher substrate. It is one sibling adapter behind the same runtime plan that also feeds Claude.

## Live Invocation Shape

Today the launcher invokes Codex roughly like this:

```bash
codex exec \
  --json \
  --skip-git-repo-check \
  --dangerously-bypass-approvals-and-sandbox \
  --color never \
  -C <wave-execution-root> \
  -o <bundle-dir>/agents/<agent-id>/last-message.txt \
  --model <resolved-model> \
  -c <resolved-config-entry> ... \
  --add-dir <wave-execution-root>/skills/<projected-skill> ...
```

It also sets:

- `CODEX_HOME=.wave/codex`
- `CODEX_SQLITE_HOME=.wave/codex`

The important Wave 15 change is that Codex now executes from the selected wave-local execution root, and runtime skill projection is derived from that same execution root.

## Supported `### Executor` Keys

The live Codex adapter currently honors:

| Wave `### Executor` key | Launch effect |
| --- | --- |
| `id: codex` | Explicitly requests the Codex runtime |
| `fallbacks` | Records ordered fallback runtimes if Codex is unavailable |
| `model` | Adds `--model <name>` |
| `codex.config` | Adds repeated `-c key=value` overrides |

`profile` still matters for runtime inference and authored metadata, but the runtime boundary is now driven by explicit runtime policy records rather than by profile folklore.

## Runtime Skill Projection

For Codex, the runtime:

1. starts from the agent's declared `### Skills`
2. resolves the selected runtime and any fallback first
3. reads `skills/*` from the wave-local execution root or worktree
4. drops declared skills that are absent from that execution root
5. filters by `activation.runtimes`
6. auto-attaches `runtime-codex` when that bundle exists in the execution root
7. passes the resulting directories through repeated `--add-dir`

The runtime detail snapshot records:

- declared skills
- projected skills
- dropped skills
- auto-attached runtime skills

## Operator Overrides

Two env vars still override authored Codex settings for a launch:

| Env var | Launch effect |
| --- | --- |
| `WAVE_CODEX_MODEL_OVERRIDE` | Replaces the resolved `model` for every agent |
| `WAVE_CODEX_CONFIG_OVERRIDE` | Replaces the resolved `codex.config` entries for every agent |

Example:

```bash
WAVE_CODEX_MODEL_OVERRIDE=gpt-5.4-mini \
WAVE_CODEX_CONFIG_OVERRIDE=model_reasoning_effort=low,model_verbosity=low \
cargo run -p wave-cli -- launch --wave 15 --json
```

## Recorded Artifacts

For Codex executions, the runtime records these Codex-relevant artifacts in `runtime-detail.json`:

- `prompt`
- `skill_overlay`
- `runtime_detail`

The overlay and projected skill paths are rooted in the wave execution root, not the repo root.

## Validation Path

Use:

```bash
cargo run -p wave-cli -- project show --json
cargo run -p wave-cli -- doctor --json
```

Then inspect the Wave 15 proof bundle:

```text
docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/
```
