# Repo Ratatui Operator

Use this skill when work changes the interactive operator shell or the right-side TUI panel.

## Working Rules

- The right-side panel is the dashboard surface. Treat it as a first-class product surface, not debug chrome.
- Bind widgets to authoritative Wave state. Avoid UI state that disagrees with `wave control status --json`.
- Layout should stay readable in both wide and narrow terminals. Define the fallback explicitly.
- Keep the main pane focused on logs/history/conversation and the right pane focused on run, agents, queue, and control state.
- Keyboard actions should map to real control-plane actions, not placeholder shortcuts.

## Panel Expectations

- `Run` tab: lane, active wave, closure stage, timers, and overall health.
- `Agents` tab: agent id, role/title, state, current task, and proof status.
- `Queue` tab: next waves, blockers, readiness, and dependencies.
- `Control` tab: open tasks, rerun intents, proofs, and operator actions.

## Implementation Discipline

1. Add typed fields to the control plane before building a widget that needs them.
2. Keep layout code isolated from business logic.
3. Cover narrow-terminal fallback with deterministic tests or snapshots.
4. Prefer a small number of durable widgets over speculative dashboard sprawl.
5. When a control surface is not implemented, render that state honestly rather than faking readiness.
