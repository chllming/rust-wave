+++
id = 9
slug = "dogfood"
title = "Dogfood the Rust system on this repository"
mode = "dark-factory"
owners = ["operator", "integration"]
depends_on = [5, 7, 8]
validation = ["cargo test", "cargo run -p wave-cli -- control status --json"]
rollback = ["Pause dogfooding and continue implementation through the bootstrap CLI if runtime gaps remain."]
proof = ["waves/09-dogfood.md", ".wave/traces/", "docs/implementation/rust-codex-refactor.md"]
+++
## Goal
Run the remaining refactor work through the Rust Wave implementation itself so the tool proves its own planning, control, and queue model.

## Deliverables
- Dogfood runbook for this repo.
- Recorded traces from self-hosted waves.
- Gap list for whatever still blocks a full dark-factory run.

## Closure
- The Rust Wave implementation can plan and track its own remaining work.
- At least one self-hosted run produces a valid trace bundle.
- Remaining gaps are explicit and prioritized instead of implicit.
