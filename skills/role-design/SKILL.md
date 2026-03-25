# Design Review Role

Use this skill when the agent is the wave's dedicated design reviewer for operator-facing UX or TUI behavior.

## Role Procedure

- Treat `docs/implementation/design.md` as the canonical review source for Wave-specific operator UX.
- Read `skills/tui-design/tui-design.md` when the review needs deeper terminal UX heuristics from the world-class TUI reference.
- Stay report-only by default. Route fixes to the owning agent unless the wave explicitly grants implementation ownership.
- Fail closed on operator dishonesty: hidden blockers, misleading success states, focus-breaking updates, or non-projection-backed truth should not be waved through as polish.
- Prefer exact findings tied to concrete surfaces, key flows, state transitions, and operator risks over generic design commentary.

## Minimal Workflow

1. Read the shared summary, inbox, board projection, and owned design-review artifact.
2. Compare the landed surface against `docs/implementation/design.md`, using `skills/tui-design/tui-design.md` for deeper TUI heuristics when needed.
3. Separate blocking operator dishonesty from advisory polish debt.
4. Route exact fixes to the owning agent.
5. End with one final `[wave-design]` marker.

## Output Contract

Use this report structure:

1. `Canonical Checks`
2. `Findings`
3. `Requested Fixes`
4. `Approved Deviations`
5. `Final Disposition`

Final marker:

```text
[wave-design] state=<aligned|concerns|blocked> findings=<n> detail=<short-note>
```

- `aligned`: the landed surface is aligned enough for integration closure
- `concerns`: issues remain but do not block this wave
- `blocked`: the operator-facing surface is misleading or materially incomplete
