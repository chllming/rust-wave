# Repo Rust Workspace

Use this skill when work changes crate layout, workspace wiring, config/spec crates, or the bootstrap CLI surface in this repository.

## Working Rules

- Keep the workspace shallow and explicit. Prefer adding to the existing target crates over inventing new top-level packages.
- `wave` is the primary binary. New command surfaces should land in `crates/wave-cli` unless they are clearly reusable library code.
- Put typed config and authored-wave parsing in dedicated crates (`wave-config`, `wave-spec`) rather than leaking parsing into the CLI.
- Keep crate APIs narrow. Export the smallest surface the next wave actually needs.
- When a change affects repo guidance or command expectations, update `README.md`, `agents.md`, or `docs/implementation/rust-codex-refactor.md` in the same slice.

## Repo Structure Expectations

- `Cargo.toml` at the repo root remains the workspace manifest.
- `wave.toml` stays the human-edited project config.
- `waves/` stays the human-authored backlog and execution contract surface.
- `crates/wave-spec` owns authored-wave parsing and typed wave document models.
- `crates/wave-dark-factory` owns lint and fail-closed policy checks.
- `crates/wave-control-plane` owns queue/read-model projections.
- `crates/wave-runtime`, `crates/wave-tui`, and `crates/wave-app-server` are the landing zones for runtime-heavy work.

## Change Discipline

1. Read the current wave and `docs/implementation/rust-codex-refactor.md` before changing crate boundaries.
2. Keep ownership tight: if a change only needs one crate, do not spill into unrelated crates.
3. Prefer typed Rust data models over ad hoc string parsing in command handlers.
4. Keep JSON-facing output serializable with `serde`.
5. Add or update unit tests in the crate that owns the changed behavior.
6. Run `cargo fmt` and the narrowest useful `cargo test` target before closing.

## Authored-Wave Awareness

- Treat `waves/*.md` as production inputs, not placeholder docs.
- When a Rust surface changes what future waves can safely assume, route that fact to the documentation steward.
- When you change parser/lint/control-plane behavior, make the wave files and guidance docs reflect the new rules in the same effort.
