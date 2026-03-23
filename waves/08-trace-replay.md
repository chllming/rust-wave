+++
id = 8
slug = "trace-replay"
title = "Capture traces and validate replay semantics"
mode = "dark-factory"
owners = ["runtime", "audit"]
depends_on = [3, 4, 6]
validation = ["cargo test -p wave-trace"]
rollback = ["Disable replay validation and keep runtime execution only until trace semantics stabilize."]
proof = ["crates/wave-trace/src/lib.rs", "waves/08-trace-replay.md"]
+++
## Goal
Persist enough runtime state to replay scheduler decisions, proof actions, reruns, and closure results with confidence.

## Deliverables
- Versioned trace bundle format.
- Replay validator.
- Operator-visible audit history for actions that affect closure or reruns.

## Closure
- A successful run writes a trace bundle.
- Replay can validate the stored control outcomes.
- Operator actions are preserved in the trace.
