# Terminal Surfaces And Operator Shell

The active Rust rewrite uses the built-in Ratatui operator shell as its terminal surface.

## Current Operator Surface

- `wave` on an interactive terminal opens the built-in operator shell.
- `wave tui` opens that same shell explicitly.
- `wave tui --alt-screen auto|always|never` controls terminal-screen policy explicitly.
- `wave tui --fresh-session` starts a new shell session instead of resuming the latest active one.
- `wave` on a non-interactive terminal falls back to the text summary path instead of trying to paint the TUI.
- The shell now renders an immediate loading state while the first operator snapshot refreshes in the background. Startup should not look like a dead black screen.

This is the live contract in this repo. Older package-era terminal-surface patterns are not active unless a Rust-specific doc says otherwise.

## Layout Contract

The TUI is now split into two intentional surfaces:

- Left side: the operator shell
  - transcript
  - composer
  - shell header with current target, follow mode, snapshot freshness, and selected wave state
- Right side: the stable dashboard
  - `Overview`
  - `Agents`
  - `Queue`
  - `Proof`
  - `Control`

In narrow terminals, the shell collapses into a one-column shell that still shows the transcript, composer, and dashboard stack rather than drawing a broken split layout.

## Operator Shell

The left side is no longer just a passive log window. It is the primary interaction lane.

Plain text in the composer becomes operator guidance for the current target:

- head target in `operator` mode: persisted head turn plus proposal set
- head target in `autonomous` mode: runtime-backed autonomous head cycle and durable autonomous evidence
- wave target: persisted wave-level steer directive
- agent target: persisted agent-level steer directive

The shell target is explicit and lives in one of three scopes:

- `head`
- `wave`
- `agent`

`head` with no explicit wave target is a cross-wave active-run workspace. It is not just a label for the currently selected wave anymore.

Wave targeting is intentionally split:

- plain-text guidance follows the current shell target
- wave hotkeys and implicit wave commands act on the visibly selected wave in the dashboard
- `/wave` and `/agent` retarget the shell and align the visible selection to the same context

The shell transcript is projection-backed transport assembled by `wave-app-server`. It is not a local UI log. Today it carries:

- durable operator, head, and system turns
- recent run state updates
- directives and directive delivery
- head proposals and their outcomes
- approvals and escalations
- rerun intent changes
- manual-close override records

This keeps the transcript on the same authority path as the rest of the operator surfaces.

## Dashboard Tabs

The right-side dashboard is the stable context lane. It should stay scannable while the operator works in the shell.

- `Overview`
  Head-workspace summary: queue state, cross-wave autonomous summary, active-wave rows, selected-wave delivery summary, and top blockers.
- `Agents`
  MAS agent control context for the selected wave: state, merge, sandbox, pending directives, and recent head action.
- `Queue`
  Reducer-backed queue story, queue-ready state, and wave queue labels.
- `Proof`
  Acceptance, signoff, proof artifacts, replay state, risks, and debt.
- `Control`
  Shell target state, rerun/manual-close status, review queue, autonomous action/failure summary, launcher availability, and live operator actions.

The dashboard stays a consumer of operator snapshot truth. It must not recompute queue readiness, signoff, or proof state locally.

## Commands And Keys

The live keyboard model is now focus-driven rather than tab-only.

### Focus lanes

- `Tab` / `Shift+Tab`
  Cycle `Transcript`, `Composer`, and `Dashboard`.
- `Esc`
  Leave transient state, help, or the composer focus.
- `Ctrl+C`
  Cancel the pending local action, or quit when nothing is pending.

### Navigation

- `j` / `k` or arrows
  Scroll transcript when `Transcript` is focused.
  Move dashboard selection when `Dashboard` is focused.
- `[` / `]`
  Move between right-side dashboard tabs.
- `?`
  Open contextual help with command and keybinding reference.
- `q`
  Quit the shell.

### Direct hotkeys

- `r`
  Request a full rerun for the selected wave.
- `c`
  Clear rerun intent for the selected wave.
- `m`
  Prepare manual close for the selected wave.
- `M`
  Prepare clearing the active manual close override.
- `u`
  Prepare approval or acknowledgment for the selected operator action.
- `x`
  Prepare rejection or dismissal for the selected operator action.

### Follow behavior

- `/follow run`
  Follow the active run wave and current agent on snapshot refresh.
- `/follow agent`
  Pin the selected MAS agent and keep wave/agent selection aligned to it.
- `/follow off`
  Preserve manual selection and transcript position.

### Slash commands

These are the live shell commands:

- `/wave <id>`
- `/agent <id>`
- `/scope head|wave|agent`
- `/mode operator|autonomous`
- `/launch [wave-id]`
- `/rerun [full|from-first-incomplete|closure-only|promotion-only]`
- `/clear-rerun`
- `/pause`
- `/resume`
- `/rerun-agent`
- `/rebase`
- `/reconcile`
- `/approve-merge`
- `/reject-merge`
- `/approve`
- `/reject`
- `/close`
- `/open overview|agents|queue|proof|control`
- `/follow run|agent|off`
- `/search <text>`
- `/clear-search`
- `/compare wave <id> | /compare agent <id>`
- `/clear-compare`
- `/help`

The command surface is intentionally small. It maps to existing runtime and control-plane actions rather than inventing a second backend.

`/mode operator|autonomous` now behaves differently by scope:

- in wave or agent scope it remains wave-scoped
- in repo-level `head` scope it applies across all active waves

## State Sources

The shell is backed by the same repo-local authority roots and projection contract as the CLI:

- `waves/`
- `.wave/state/build/specs/`
- `.wave/state/runs/`
- `.wave/state/control/`
- `.wave/traces/runs/`

Planning, queue, proof, and control truth come from the reducer-backed operator snapshot assembled by `wave-app-server`. The TUI consumes that snapshot. It does not own scheduling or delivery semantics.

Operator actions flow through `wave-runtime` helpers such as:

- launch
- rerun request and clear
- manual close apply and clear
- mode switch
- steer wave
- steer agent
- approval and escalation handling

Autonomous head behavior also flows through `wave-runtime`. The TUI does not run its own autonomy loop or invent a second scheduler.

That keeps the shell on the same authority path as `wave control ...`, `wave launch`, and the other CLI entry points.

## Current Non-Goals

These are still out of scope for the current shell:

- embedding the Codex backend as the primary TUI authority
- a second planner or scheduler model inside the UI
- mouse-driven workflows
- a full free-form assistant backend inside the shell

Plain text guidance is persisted as Wave directives. The shell is not yet a general-purpose conversational agent runtime.

What is still missing now is above the shell surface:

- the real Wave 18 proof run for concurrent MAS execution plus targeted recovery
- deeper operator-agent assistance beyond proposal and action synthesis
- broader overview polish for very busy multi-wave sessions

## Validation Path

```bash
cargo build -p wave-cli
target/debug/wave --help
target/debug/wave tui --help
cargo test -p wave-app-server -p wave-cli -p wave-tui
```

If the TUI and `wave control ...` disagree about queue, proof, or control state, treat the snapshot or reducer path as authoritative and the UI as stale until it is fixed.
