# Wave 5 Integration Summary

## Scope

- Wave: `5` (`tui-right-panel`)
- Agent: `A8` (`Integration Steward`)
- Generated at: `2026-03-23T00:00:00Z`

## Evidence

- `README.md` states the interactive `wave` shell has a right-side panel with `Run`, `Agents`, `Queue`, and `Control`, and that `Queue` is the live planning/status surface projected from `wave control status --json`.
- `docs/guides/terminal-surfaces.md` says the built-in Ratatui shell is the active operator surface, the right-side panel is the dashboard surface, and the panel must consume the same queue truth as `wave control status --json`.
- `docs/implementation/rust-codex-refactor.md` reinforces that `Queue` and `control status` are not separate truths and that the TUI is a consumer of control-plane truth, not an independent planner.
- `docs/plans/component-cutover-matrix.md` marks `tui-right-side-panel` and `operator-status-tabs` as `repo-landed`, which matches the documented operator-surface contract.
- The same docs align on the narrow-terminal fallback: preserve the same data model and collapse to text summary rather than inventing a second dashboard mode.
- `crates/wave-tui/src/lib.rs` now resolves `Run`, `Agents`, and `Control` through `selected_active_run(snapshot, selected_wave_id)`, which prefers the selected wave's active run and only falls back to the first active run when no selected match exists.
- The queue tab continues to render from the authoritative planning snapshot, so the right-side surface now projects one shared control-plane model across all tabs.

## Open Claims

- None.

## Conflicts

- None.

## Blockers

- None.

## Deploy Risks

- `repo-local` only; no live host mutation.

## Doc Drift

- None.

## Decision

The right-side operator panel, control-plane subscriptions, and operator guidance now agree on one authoritative queue and selection model, so this slice is ready for doc closure.

[wave-integration] state=ready-for-doc-closure claims=0 conflicts=0 blockers=0 detail=right-side panel now binds to selected-wave run state and shared queue truth
