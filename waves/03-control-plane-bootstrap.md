+++
id = 3
slug = "control-plane-bootstrap"
title = "Build the initial planning status and control-plane bootstrap"
mode = "dark-factory"
owners = ["implementation", "operator"]
depends_on = [2]
validation = ["cargo test -p wave-control-plane"]
rollback = ["Revert the planning status reducer and keep only lintable wave inputs."]
proof = ["crates/wave-control-plane/src/lib.rs", "waves/03-control-plane-bootstrap.md"]
+++
## Goal
Introduce the first Rust control-plane model so the repo can compute wave readiness and blocked-by state before live runtime orchestration exists.

## Deliverables
- Planning status reducer that turns waves and lint findings into queue state.
- `wave control status` command surface.
- JSON output suitable for later TUI panel consumption.

## Closure
- `wave control status --json` renders the committed waves.
- The ready set only includes dependency-free and lint-clean waves.
- The reducer is covered by unit tests.
