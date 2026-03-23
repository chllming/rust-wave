# Repo Rust Control Plane

Use this skill when a wave changes Wave's typed state, queue projections, readiness logic, or operator-facing summaries.

## Working Rules

- Prefer an authoritative typed model first, then derive CLI/TUI views from it.
- Keep status surfaces deterministic and serializable. If a field matters to operators, make it a typed field rather than burying it in prose.
- Separate source-of-truth state from derived read models. Reducers should not depend on rendering concerns.
- Closure coverage, dependency blockers, proof state, and next-wave readiness are first-class fields, not comments.
- When a control-plane assumption changes, update both the tests and the authored waves that depend on that assumption.

## State Modeling

- Favor explicit structs and enums over `HashMap<String, Value>` unless the surface is intentionally extensible.
- Model missing readiness as structured blockers, not inferred text.
- Keep counts and booleans side by side when the UI needs both summary and detail.
- Preserve compatibility between `wave control status --json` and future TUI consumers.

## Queue And Closure Discipline

1. Read the wave dependency graph before touching queue logic.
2. Treat closure-agent coverage as explicit state.
3. Never mark a wave ready when lint or required blockers still fail.
4. Name blockers in a way the operator can act on directly.
5. Add unit tests for positive and negative readiness cases.

## Projection Rules

- CLI output should be derived from the same structs serialized in JSON.
- TUI-facing fields should be added to the control-plane model before adding UI-only behavior.
- If a planned field is not yet authoritative, keep it out of the public status until it becomes real.
