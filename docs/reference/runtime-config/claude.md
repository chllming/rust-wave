# Claude Runtime Configuration

This page documents the live Claude adapter behind the Wave 15 runtime-neutral boundary.

Claude is no longer target-state in the Rust runtime. The adapter is implemented in `wave-runtime`, selected through the same runtime policy record as Codex, and persists the same runtime-neutral identity and fallback metadata.

The proof classification for a specific checked-in Wave 15 bundle may still be `live`, `dry-run-backed`, or `fixture-backed`. See:

- [docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/README.md](../../implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/README.md)

## Live Invocation Shape

Today the launcher invokes Claude roughly like this:

```bash
claude -p \
  --no-session-persistence \
  --append-system-prompt-file <bundle-dir>/agents/<agent-id>/claude-system-prompt.txt \
  --settings <resolved-settings-path> \
  --model <resolved-model> \
  --agent <resolved-agent> \
  --permission-mode <mode> \
  --permission-prompt-tool <tool> \
  --effort <level> \
  --max-turns <n> \
  --mcp-config <resolved-path> ... \
  --strict-mcp-config \
  --output-format <format> \
  --allowedTools <tool> ... \
  --disallowedTools <tool> ... \
  --add-dir <wave-execution-root>/skills/<projected-skill> ... \
  "<runtime prompt text>"
```

The command runs with `current_dir=<wave-execution-root>`.

## Supported `### Executor` Keys

The live Claude adapter currently honors:

| Wave `### Executor` key | Launch effect |
| --- | --- |
| `id: claude` | Explicitly requests the Claude runtime |
| `fallbacks` | Records ordered fallback runtimes if Claude is unavailable |
| `model` | Adds `--model <name>` |
| `claude.agent` | Adds `--agent <name>` |
| `claude.permission_mode` | Adds `--permission-mode <mode>` |
| `claude.permission_prompt_tool` | Adds `--permission-prompt-tool <tool>` |
| `claude.effort` | Adds `--effort <level>` |
| `claude.max_turns` | Adds `--max-turns <n>` |
| `claude.mcp_config` | Adds repeated `--mcp-config <path>` after execution-root resolution |
| `claude.strict_mcp_config` | Adds `--strict-mcp-config` when truthy |
| `claude.settings` | Resolves a base settings file against the execution root |
| `claude.settings_json` | Merges inline JSON into the generated settings overlay |
| `claude.hooks_json` | Writes top-level `hooks` into the generated settings overlay |
| `claude.allowed_http_hook_urls` | Writes top-level `allowedHttpHookUrls` into the generated settings overlay |
| `claude.output_format` | Adds `--output-format <format>` |
| `claude.allowed_tools` | Adds repeated `--allowedTools <tool>` |
| `claude.disallowed_tools` | Adds repeated `--disallowedTools <tool>` |

## Settings Overlay Behavior

Wave always writes `claude-system-prompt.txt` for the Claude harness instructions.

Wave resolves `claude.settings` relative to the selected wave-local execution root.

Wave writes `claude-settings.json` only when at least one inline overlay input is present:

- `claude.settings_json`
- `claude.hooks_json`
- `claude.allowed_http_hook_urls`

Merge order:

1. base `claude.settings` JSON file, if provided
2. inline `claude.settings_json`
3. inline `claude.hooks_json` under top-level `hooks`
4. inline `claude.allowed_http_hook_urls` under top-level `allowedHttpHookUrls`

If no inline overlay data is present, Wave passes the resolved base settings file directly through `--settings` and records that path in runtime detail.

## Runtime Skill Projection

Claude uses the same runtime skill projection rules as Codex:

1. resolve the selected runtime and fallback first
2. read manifests from the wave-local execution root or worktree
3. filter by `activation.runtimes`
4. drop declared skills absent from the execution root
5. auto-attach `runtime-claude` when that bundle exists in the execution root
6. pass the resulting directories through repeated `--add-dir`

This keeps the runtime overlay, the Claude system prompt, the settings overlay, and the actual Claude working directory rooted in one filesystem view.

## Recorded Artifacts

For Claude executions, `runtime-detail.json` records:

- `prompt`
- `skill_overlay`
- `runtime_detail`
- `system_prompt`
- `settings` when a base settings file or generated overlay is used

The runtime detail snapshot also records selected runtime, selection reason, fallback metadata, and projected skills in the same runtime-neutral schema used by Codex.

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
