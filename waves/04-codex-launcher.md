+++
id = 4
slug = "codex-launcher"
title = "Implement the Codex-backed launcher and agent lifecycle manager"
mode = "dark-factory"
owners = ["runtime", "implementation"]
depends_on = [3]
validation = ["cargo test -p wave-runtime"]
rollback = ["Disable the new launcher path and keep using planning-only commands until runtime parity is ready."]
proof = ["crates/wave-runtime/src/lib.rs", "third_party/codex-rs/UPSTREAM.toml", "waves/04-codex-launcher.md"]
+++
## Goal
Connect Wave scheduling to Codex OSS so a single wave can launch, monitor, and close through a project-scoped Codex runtime.

## Deliverables
- Codex runtime integration crate.
- Launcher command path.
- Agent lifecycle state model.

## Closure
- `wave launch` can execute a single wave against the Codex runtime.
- Project-scoped Codex state lives under `.wave/codex/`.
- Runtime errors fail closed instead of skipping closure.
