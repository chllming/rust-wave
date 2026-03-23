# Rust/Codex Refactor Baseline

This repository now has the first executable slice of the refactor:

- a Rust workspace with the target crate layout
- `wave.toml` as the new project config
- `waves/` as the new human-authored wave source directory
- a compileable `wave` CLI that can show project state, lint waves, and render planning status
- pinned upstream review metadata for Codex OSS and the Wave control-plane docs branch

## Reviewed Upstreams

- Codex OSS: `https://github.com/openai/codex` at commit `5e3793def286099deaf5a6ae625e1f31ad584790`
- Wave control-plane docs branch: `https://github.com/chllming/agent-wave-orchestrator/tree/docs/wave-positioning-refresh` at commit `8b421c79d58713b8be3f137e16d8777ebd445851`

## Current Scope

This slice implements Waves 0 through 3 at a bootstrap level:

1. freeze the repo layout and command map
2. make the new config and wave formats concrete
3. add lint and planning-status primitives
4. keep the runtime-heavy crates present as placeholders so later waves land into stable paths

## Command Map In This Slice

- `wave`
  Renders a textual operator summary. This is the pre-TUI bootstrap surface.
- `wave project show [--json]`
  Prints the parsed `wave.toml`.
- `wave doctor [--json]`
  Verifies config loading, wave parsing, and upstream metadata presence.
- `wave lint [--json]`
  Validates wave files and dark-factory requirements.
- `wave control status [--json]`
  Shows dependency-driven wave readiness and lint state.

The launch, autonomous, TUI, app-server, runtime, and trace paths are intentionally stubbed until the Codex-backed runtime waves land.
