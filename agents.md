# Agent Guidance

This repository is the in-progress Rust rewrite of Wave, rebuilt around Codex OSS.

## Current State

- The Rust workspace is the active implementation surface.
- `wave.toml` is the current project config.
- `waves/` is the current implementation backlog and sequencing source.
- The Codex-backed launcher, TUI, app-server integration, and trace runtime are not implemented yet.
- The seeded `docs/` tree is reference material from upstream Wave, not the canonical runtime implementation for this repo.

## Source Of Truth

- Read `README.md` first for repo purpose and current status.
- Read `docs/implementation/rust-codex-refactor.md` for the accepted architecture baseline.
- Read `wave.toml` for project-scoped defaults and paths.
- Read `waves/*.md` for the implementation order and exit criteria.
- Treat `third_party/codex-rs/UPSTREAM.toml` and `third_party/agent-wave-orchestrator/UPSTREAM.toml` as the reviewed upstream pins.

## Working Commands

- `cargo test`
- `cargo run -p wave-cli --`
- `cargo run -p wave-cli -- project show --json`
- `cargo run -p wave-cli -- doctor --json`
- `cargo run -p wave-cli -- lint --json`
- `cargo run -p wave-cli -- control status --json`

Do not assume `wave launch`, `wave autonomous`, `wave dep`, `wave trace`, or the TUI paths are implemented. They are still planned surfaces.

## Editing Rules

- Prefer touching the Rust crates under `crates/` over editing seeded Node-era config and docs.
- Keep the command surface aligned with the accepted plan: `wave` remains the primary binary.
- Preserve the clean-break direction. Do not reintroduce compatibility work for the old JS runtime layout unless explicitly requested.
- Keep dark-factory assumptions explicit. Validation, rollback, proof, and closure expectations should stay machine-readable where possible.
- When adding a new subsystem, land it in the existing target crate path instead of inventing a new top-level structure.
- Update `README.md`, `docs/implementation/rust-codex-refactor.md`, or the relevant `waves/*.md` file when behavior or sequencing changes.

## Validation Expectations

- Run `cargo fmt` and the relevant `cargo test` targets for the crates you touch.
- If you change parsing, linting, or control-plane logic, update or add unit tests in the same crate.
- If you add a new command surface, verify it through `cargo run -p wave-cli -- ...`.

## Implementation Priorities

- Keep moving through the committed wave order unless a blocking design issue forces a resequence.
- Prefer finishing bootstrap-quality end-to-end slices over scattering partial framework code across all crates.
- The next major milestones are the Codex-backed launcher, the right-side TUI panel, and enforced dark-factory runtime checks.
