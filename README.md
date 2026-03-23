# Codex Wave Mode

This repository is the in-progress Rust rewrite of Wave, rebuilt around Codex OSS as the operator and orchestration runtime.

The current state is a bootstrap implementation:

- a Rust workspace with the target crate layout
- a new repo config in `wave.toml`
- human-authored implementation waves under `waves/`
- a compileable `wave` CLI for config inspection, linting, and planning status
- pinned upstream review metadata for Codex OSS and the Wave control-plane reference branch

## Status

The runtime-heavy pieces are not implemented yet.

Working now:

- `wave`
- `wave project show [--json]`
- `wave doctor [--json]`
- `wave lint [--json]`
- `wave control status [--json]`

Planned but still stubbed:

- `wave draft`
- `wave adhoc`
- `wave launch`
- `wave autonomous`
- `wave dep`
- `wave trace`

## Repo Layout

- `crates/`
  Rust workspace crates for CLI, config, spec parsing, control-plane bootstrapping, runtime, TUI, app-server, dark-factory policy, and traces.
- `waves/`
  The implementation backlog for this refactor, expressed in the new wave format.
- `wave.toml`
  The new project-scoped config for the Rust implementation.
- `docs/implementation/rust-codex-refactor.md`
  The current architecture baseline for this repo.
- `third_party/codex-rs/UPSTREAM.toml`
  Reviewed Codex OSS pin.
- `third_party/agent-wave-orchestrator/UPSTREAM.toml`
  Reviewed Wave control-plane reference pin.
- `docs/`
  Seeded upstream Wave docs that remain useful as reference input during the rewrite.

## Getting Started

1. Install Rust stable.
2. Run `cargo test`.
3. Run `cargo run -p wave-cli -- doctor --json`.
4. Run `cargo run -p wave-cli -- control status --json`.

If you want the short textual shell instead of JSON output:

```bash
cargo run -p wave-cli --
```

## Implementation Model

The refactor is being driven by the tool itself. The committed waves in `waves/` are the execution sequence:

1. freeze the architecture and repo shape
2. bootstrap the Rust workspace and command surface
3. implement config, spec parsing, and dark-factory lint
4. bootstrap control-plane status
5. add the Codex-backed launcher
6. add the right-side TUI panel
7. enforce dark-factory as a runtime policy
8. add autonomous queueing
9. add trace and replay
10. dogfood the Rust system on this repo

## Upstreams Reviewed

- Codex OSS: `https://github.com/openai/codex`
- Wave control-plane reference: `https://github.com/chllming/agent-wave-orchestrator/tree/docs/wave-positioning-refresh`

The exact reviewed commits are recorded in the `third_party/*/UPSTREAM.toml` files.
