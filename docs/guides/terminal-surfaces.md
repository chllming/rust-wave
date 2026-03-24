# Terminal Surfaces And Dashboards

The active Rust rewrite uses the built-in Ratatui shell as its operator surface.

## Current Operator Surface

- `wave` on an interactive terminal opens the built-in Ratatui shell.
- The shell's right-side operator panel is the built-in dashboard surface for the repo.
- `wave` on a non-interactive terminal falls back to a text summary that carries the same control-plane truth in a narrower form.

This is the live operator surface in this repo. Older terminal-surface patterns are not part of the active contract unless this doc says otherwise.

## Right-Side Panel

The right-side panel is the built-in operator dashboard. It is shipped inside the TUI, not as a separate app or a placeholder surface. The current TUI exposes four tabs:

- `Run`
  Active wave, run id, elapsed time, proof counts, and declared proof artifacts.
- `Agents`
  Per-agent state, marker completeness, and deliverables.
- `Queue`
  Ready waves, blockers, dependency-driven queue truth, and wave readiness.
- `Control`
  Rerun intents, replay/proof status, and the available keybindings.

The `Queue` view is the operator planning surface. It reflects the same control-plane truth used by `wave control status --json`, including:

- wave readiness
- blocker state
- dependency-driven ordering
- whether a wave is waiting on upstream work or is ready to claim

The panel should keep consuming that same queue truth. The UI may change, but it should not invent a second source of status state.
In the current Rust implementation, that queue/control truth is reducer-backed: `wave-reducer` computes the planning state and `wave-projections` turns it into the `ProjectionSpine`, operator snapshot input read models, and queue/control status helper read models that `wave control status --json`, `wave-app-server`, and the TUI consume. `wave-app-server` now carries that control-status read model through the operator snapshot so the TUI can render the queue decision story and control attention lines without rebuilding them locally. `wave-control-plane` is now only a forwarding shim over that contract. Compatibility run records still enter as adapter inputs for active-run and replay facts in this stage.
Parity is covered by repo-local fixtures: `cargo test -p wave-cli`, `cargo test -p wave-app-server`, and `cargo test -p wave-tui` all exercise the same reducer-backed queue/control payload from different consumer edges.
When multiple waves are active, the `Run`, `Agents`, and `Control` tabs follow the currently selected wave instead of whichever run happens to appear first in the snapshot.

## Keybindings

These are the live actions currently shipped in the TUI:

- `Tab` / `Shift+Tab`
  Cycle the right-side tabs.
- `j` / `k`
  Move the selected wave.
- `r`
  Request a rerun for the selected wave.
- `c`
  Clear the selected wave's rerun intent.
- `q`
  Quit the shell.

In narrow terminals, the shell keeps the same data model but collapses the surface into the fallback text summary rather than trying to render a broken split layout. That fallback is live behavior, not a planned one.
The fallback keeps the repo's operator truth visible by rendering condensed `Run`, `Agents`, `Queue`, and `Control` sections from the same snapshot used by the wide panel, but it does not pretend the right-side dashboard fits when there is no space for it.

## Live Actions

The current TUI actions that actually ship are:

- tab switching with `Tab` and `Shift+Tab`
- wave selection movement with `j` and `k`
- rerun-intent creation with `r`
- rerun-intent clearing with `c`
- quitting with `q`

Any other dashboard interactions should be treated as planned follow-on work until the implementation lands. In particular, anything beyond tab switching, wave movement, rerun-intent creation, rerun-intent clearing, and quit is not yet part of the shipped TUI contract.

The right-side panel is therefore a shipped dashboard, but only these actions are live today:

- tab cycling with `Tab` and `Shift+Tab`
- wave movement with `j` and `k`
- rerun-intent creation with `r`
- rerun-intent clearing with `c`
- quitting with `q`

## State Sources

The shell is backed by the same repo-local inputs and projection contract as the CLI:

- `waves/`
- `.wave/state/build/specs/`
- `.wave/state/runs/`
- `.wave/state/control/reruns/`
- `.wave/traces/runs/`

Planning, queue, and control tabs are reducer-backed projections assembled from those inputs, rather than UI-local readiness logic. `wave-app-server` now maps reducer-backed operator snapshot inputs plus the projection-owned control-status read model into the transport snapshot the TUI reads, and the TUI queue/control tabs render that snapshot payload so the closure-blocked story, closure-attention lines, and skill-issue lines stay aligned with `wave control status`. `.wave/state/projections/` remains the canonical root for persisted projection material once later waves start writing those read models out durably.

The trace surface is evidence, not debug logging. `wave trace latest` reports the recorded run, replay result, and trace path for each wave, while `wave trace replay` rechecks the stored record or v1 trace bundle against the current run state and emits replay issues when something diverges.

The stored trace bundle records:

- the run record for the completed wave
- per-agent artifact presence for `prompt.md`, `last-message.txt`, `events.jsonl`, and `stderr.txt`
- run-level artifact presence for the bundle directory and the project-scoped Codex home

Replay validation is read-only. It does not rebuild the run; it verifies that the durable artifacts still match the recorded outcome.
The planning/control projections are reducer-backed in memory today, but replay is still compatibility-backed until the canonical trace/result layers replace the current adapters.

The planning-status surface is therefore control-plane first. Any future TUI dependency should read from the same status model rather than recomputing readiness, blockers, or queue order locally in the UI layer, and any future dashboard transport should start from the same reducer-backed operator snapshot inputs that `wave-app-server` uses today.

## Current Non-Goals

These older surfaces are not the live operator contract in the Rust rewrite:

- tmux-managed per-wave dashboards
- `.vscode/terminals.json` integration
- lane-scoped terminal-surface flags

Planned additions may extend the right-side panel, but they should stay documented as planned until the runtime supports them.

If those come back later, they should be treated as new runtime work rather than assumed from the package-era docs.

## Suggested Validation Path

```bash
cargo run -p wave-cli --
cargo run -p wave-cli -- control status --json
cargo run -p wave-cli -- control show --wave 0 --json
cargo run -p wave-cli -- trace latest
cargo run -p wave-cli -- trace replay --wave 0
cargo test -p wave-projections -p wave-cli -p wave-app-server -p wave-tui
```

For planning-only bootstrap work, validate the queue/status path first. If those commands disagree, the UI docs should be treated as stale until the control-plane model is fixed.
