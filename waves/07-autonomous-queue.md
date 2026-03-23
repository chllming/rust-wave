+++
id = 7
slug = "autonomous-queue"
title = "Implement autonomous next-wave scheduling and dependency gating"
mode = "dark-factory"
owners = ["runtime", "operator"]
depends_on = [3, 4, 6]
validation = ["cargo test -p wave-runtime -p wave-control-plane"]
rollback = ["Keep launch manual and disable automatic next-wave promotion if queue stability is not ready."]
proof = ["crates/wave-runtime/src/lib.rs", "crates/wave-control-plane/src/lib.rs", "waves/07-autonomous-queue.md"]
+++
## Goal
Teach the runtime to select the next ready wave, honor dependencies, and expose that queue state to the operator surface.

## Deliverables
- Autonomous scheduler path.
- Queue reducer that knows ready, blocked, and next waves.
- Dependency-aware operator status.

## Closure
- The scheduler never starts a blocked wave.
- The queue state matches dependency and lint reality.
- The TUI panel shows the same next-wave decisions as the CLI.
