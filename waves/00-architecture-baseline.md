+++
id = 0
slug = "architecture-baseline"
title = "Freeze the Rust and Codex architecture baseline"
mode = "dark-factory"
owners = ["planner", "operator"]
depends_on = []
validation = ["cargo test -p wave-config -p wave-spec -p wave-dark-factory -p wave-control-plane"]
rollback = ["Remove the Rust workspace changes and keep the JS starter scaffold untouched."]
proof = ["docs/implementation/rust-codex-refactor.md", "wave.toml", "waves/00-architecture-baseline.md"]
+++
## Goal
Lock the command map, state layout, upstream review pins, and crate layout before deeper runtime work begins.

## Deliverables
- Architecture baseline document for the Rust rewrite.
- Pinned upstream metadata for Codex OSS and the Wave control-plane docs branch.
- New `wave.toml` and `waves/` input model.

## Closure
- The baseline document matches the accepted implementation plan.
- `wave lint` reports no errors.
- `wave control status --json` shows only wave 0 as ready.
