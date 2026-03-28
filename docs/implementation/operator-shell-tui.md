# Operator Shell TUI

## Status

The Wave-native operator shell is the live terminal product.

Slice 1, Slice 2, and Slice 3 are now implemented:

- durable shell sessions
- durable shell turns
- durable head proposals
- proposal-backed operator mode
- runtime-backed autonomous mode
- cross-wave head workspace for active waves
- richer `Overview`, `Agents`, and `Control` surfaces for MAS and autonomy state
- shell-local transcript search and compare views
- explicit `wave tui --alt-screen auto|always|never`
- explicit `wave tui --fresh-session`

The shell-product pass is closed in code. The remaining gap is not the shell contract. It is the repo-local proof boundary for the broader MAS architecture cut:

- one real Wave 18 proof run showing concurrent agents, recovery-required state, targeted repair, and honest closure

This document is the canonical contract for the shell, the head lane behind it, and the remaining work. It replaces older “dashboard-first” descriptions.

## Live Today

### Entry points

- `wave` on an interactive terminal opens the shell
- `wave tui` opens the shell explicitly
- `wave tui --alt-screen auto|always|never` selects terminal-screen policy explicitly
- `wave tui --fresh-session` starts a new repo-local shell session instead of resuming the latest active one
- non-interactive `wave` falls back to the text path instead of painting a dead TUI
- startup renders immediately with a loading state while the first snapshot refreshes in the background

### Layout

The shell is split into two intentional surfaces:

- left side: `header + transcript + composer`
- right side: stable dashboard with `Overview`, `Agents`, `Queue`, `Proof`, and `Control`

In narrow terminals, the shell collapses into the single-pane summary fallback instead of drawing a broken split layout.

### Shell scopes

The shell has three explicit scopes:

- `head`
- `wave`
- `agent`

`head` with no explicit wave target is now a cross-wave workspace for the repo’s active waves. It is no longer just shorthand for “head on the currently selected wave.”

### Live command set

- `/wave <id>`
- `/agent <id>`
- `/scope head|wave|agent`
- `/mode operator|autonomous`
- `/launch [wave-id]`
- `/rerun [full|closure-only|promotion-only]`
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

### Operator mode

In `operator` mode:

- plain text in `head` scope creates durable head turns and head proposals
- proposals become first-class operator queue items
- proposals can be applied or dismissed explicitly
- wave and agent scope plain text still dispatches direct steer guidance through runtime helpers

### Autonomous mode

In `autonomous` mode:

- the head backend runs through the runtime, not through TUI-local heuristics
- autonomous head cycles act across all active waves when the shell is in repo-level `head` scope
- the same proposal model is still used, but allowed actions are auto-applied through existing runtime helpers
- autonomous actions remain evidenced through durable turns and proposal-resolution records
- autonomous mode does not auto-launch new ready waves in this slice; it manages active waves only

## Product Goal

The operator shell is meant to support one continuous loop:

1. observe queue, delivery, proof, and MAS state
2. focus `head`, `wave`, or `agent`
3. ask for guidance or issue control
4. review proposals, approvals, escalations, or autonomous actions
5. verify resulting blockers, proof, and delivery state without leaving the shell

This is not a passive dashboard and not a generic chat surface. It is an operator control product.

## Authority Model

The shell must not become a second planner or scheduler.

Reducer and projection authority still own:

- readiness
- blockers
- queue state
- delivery state
- proof state
- acceptance state

Runtime helpers still own:

- launch
- rerun
- orchestrator mode changes
- steer directives
- MAS agent controls
- merge approvals or rejections
- reconciliation
- manual close
- human approvals and escalations

The shell owns only:

- focus
- session preferences
- transcript presentation
- composer state
- target selection
- proposal review flow
- local view modes

The shell may ask, summarize, propose, and dispatch through runtime helpers. It may not invent queue, proof, or delivery truth locally.

## Data Model

### Operator shell session

The session record persists:

- session id
- current scope
- current wave id
- current agent id
- selected tab
- follow mode
- operator or autonomous posture
- active marker
- started and updated timestamps

### Operator shell turn

The turn record persists:

- turn id
- session id
- origin: `operator`, `head`, or `system`
- scope
- cycle id
- target wave id
- target agent id
- input text
- output text
- status
- created timestamp
- failed reason when present

`cycle_id` groups one head pass, especially for autonomous cycles that touch more than one wave.

### Head proposal

The proposal record persists:

- proposal id
- session id
- turn id
- cycle id
- target wave id
- target agent id when present
- action kind
- structured action payload
- state
- summary
- detail
- resolution metadata
- created and updated timestamps

Resolution metadata distinguishes:

- pending
- applied by operator
- applied autonomously
- dismissed
- rejected

## Transport Contract

`wave-app-server` now exposes shell and autonomy state as first-class snapshot transport, including:

- active shell session
- transcript items
- head proposals
- command metadata
- cross-wave autonomous summary
- per-wave orchestrator summary
- per-agent MAS control context

The right-side dashboard consumes that transport. It does not reconstruct autonomy, queue, or proof truth on its own.

## Dashboard Contract

### Overview

`Overview` now acts as the head workspace:

- shell target
- queue summary
- cross-wave autonomous summary
- recent autonomous actions
- active-wave rows with mode, current agent, pending proposals, and recent head outcome
- selected-wave delivery and blocker detail

### Agents

`Agents` now shows MAS control context for the selected wave:

- per-agent status
- merge state
- sandbox state
- pending directive count
- last head action affecting that agent

### Queue

`Queue` remains projection-backed queue truth. It does not change scheduler semantics.

### Proof

`Proof` remains the acceptance, replay, risk, and debt drill-down.

### Control

`Control` now separates:

- reruns and manual-close state
- review queue items
- recovery-required state and recent recovery actions
- cross-wave autonomous summary
- recent autonomous actions and failures
- shell keybinding actions

## Slice Status

### Slice 1

Done.

Delivered:

- persisted shell sessions
- persisted shell turns
- persisted head proposals
- proposal-backed head interaction in `operator` mode
- proposal apply and dismiss flow through existing runtime helpers

### Slice 2

Done.

Delivered:

- runtime-backed autonomous head cycles
- proposal resolution metadata for operator-applied vs autonomous-applied actions
- cross-wave head workspace for active waves
- per-wave and per-agent autonomous/MAS context in the snapshot
- `Overview`, `Agents`, and `Control` upgrades for autonomy and MAS visibility

### Slice 3

Done.

Delivered:

- transcript search and filtering
- wave and MAS agent compare mode in the main pane
- explicit `wave tui --alt-screen auto|always|never`
- explicit `wave tui --fresh-session`
- recovery visibility in `Overview`, `Agents`, `Proof`, and `Control`

## Remaining Product Work

The remaining work now sits above the shell:

- richer cross-wave overview polish when the repo is operating many active MAS waves at once
- a true operator-agent backend that can move beyond proposal/action synthesis into deeper planning help
- the real Wave 18 proof run that ratifies the broader MAS runtime and recovery boundary in live repo execution

## Validation

```bash
cargo test -p wave-runtime -p wave-app-server -p wave-cli -p wave-tui
target/debug/wave --help
target/debug/wave tui --help
```

If the shell and `wave control ...` disagree, treat the reducer/projection/runtime authority path as correct and the shell as stale until fixed.
