+++
id = 1
slug = "workspace-bootstrap"
title = "Bootstrap the Rust workspace and operator command surface"
mode = "dark-factory"
owners = ["implementation", "operator"]
depends_on = [0]
validation = ["cargo test -p wave-cli"]
rollback = ["Remove Cargo workspace files and keep the repo on the JS-only scaffold."]
proof = ["Cargo.toml", "crates/wave-cli/src/main.rs", "waves/01-workspace-bootstrap.md"]
+++
## Goal
Create the Rust workspace, the `wave` binary, and the top-level command surface that later runtime waves will extend.

## Deliverables
- Root Cargo workspace with the target crate names.
- Compileable `wave` binary with `project`, `doctor`, `lint`, and `control status`.
- Stub crates for the runtime-heavy subsystems so later waves land into stable paths.

## Closure
- `cargo test -p wave-cli` passes.
- `wave` runs without arguments and prints a project summary.
- The CLI help reflects the planned top-level command surface.
