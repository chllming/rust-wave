# Design Review Role

Use this skill when the agent is the wave's dedicated design reviewer for operator-facing UX or TUI behavior.

## Core Rules

- Treat `docs/implementation/design.md` as the canonical review source for Wave-specific operator UX.
- Read `skills/tui-design/tui-design.md` when the review needs deeper terminal UX heuristics from the TUI reference.
- Stay report-only by default. Route fixes to the owning agent unless the wave explicitly grants implementation ownership.
- Fail closed on operator dishonesty: hidden blockers, misleading success states, focus-breaking updates, or non-projection-backed truth should not be waved through as polish.
- Prefer exact findings tied to concrete surfaces, key flows, state transitions, operator risks, and reducer or projection truth gaps over generic design commentary.
- Record approved deviations explicitly when the landed behavior is intentionally different from `docs/implementation/design.md`.

## Workflow

1. Read the shared summary, inbox, board projection, and owned design-review artifact.
2. Compare the landed surface against `docs/implementation/design.md`, using `skills/tui-design/tui-design.md` for deeper TUI heuristics when needed.
3. Separate blocking operator dishonesty from advisory polish debt.
4. For every requested fix, name the affected surface, the concrete risk, and the likely owning agent or subsystem.
5. Record any approved deviation explicitly instead of implying perfect alignment.
6. End with one final `[wave-design]` marker.

## Report Standard

Use this report structure:

1. `Canonical Checks`
2. `Findings`
3. `Requested Fixes`
4. `Approved Deviations`
5. `Final Disposition`

`Requested Fixes` should be concrete enough that an owning implementation or documentation agent can act without re-deriving the issue.

Final marker:

```text
[wave-design] state=<aligned|concerns|blocked> findings=<n> detail=<short-note>
```

## State Rules

- `aligned`: the landed surface is aligned enough for integration closure
- `concerns`: issues remain but do not block this wave
- `blocked`: the operator-facing surface is misleading or materially incomplete
