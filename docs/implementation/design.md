# TUI UX And Operator Ergonomics Design

## Status

Partially live.

The current shell already lands the thin right-side operator panel, queue selection, rerun controls, live runtime and stall visibility, and confirm-first manual-close actions.

What is still missing is the full detailed TUI UX and ergonomics spec at the level this harness now needs.

## What exists today

At the architecture level, the current docs already cover the intended operator surfaces in broad terms:

- portfolio, wave, tasks, questions, contradictions, leases, proof, and control views
- operator actions such as claim, release, pause, resume, reopen, escalate, reroute, and priority changes
- scheduler-visible state such as claims, leases, worktrees, fairness, and merge status
- the rule that the TUI stays a thin consumer of reducer and projection truth

That is the right information architecture and the right control-plane posture.

The live repo-local shell currently proves a narrower slice of that design:

- a stable right-side `Run`, `Agents`, `Queue`, and `Control` panel
- queue selection plus direct rerun control from the shell
- explicit runtime identity, fallback, last-activity, and stalled-run visibility
- confirm-first manual-close apply and clear actions for the selected wave via `m` and `M`

## What is still missing

We do not yet have a detailed TUI UX plan that specifies all of this concretely:

- exact screen layout and panel hierarchy
- keyboard model and navigation flows
- action-state UX for pending, running, succeeded, failed, needs-approval, and blocked states
- per-agent activity UX for what the agent is doing now, what it last did, what it is blocked on, and what proof it produced
- blocker triage UX across dependency, contradiction, lease, merge, and human-input blockers
- orchestrator interaction UX for recommendations, approvals, overrides, and escalations
- action feedback ergonomics for optimistic versus confirmed state, command history, failure explanation, retry, and rollback
- multi-wave concurrency UX for worktrees, contention, fairness, budgets, and reserved closure capacity
- live proof and acceptance UX for what is shown inline, what is summarized, and what requires drill-down

This document fills that gap.

## Canonical Review Source

This document is also the canonical review source for the optional wave design reviewer role.

When a wave includes a design reviewer, that role should judge operator-facing layout, navigation, action-state behavior, blocker triage, orchestrator interaction, concurrency visibility, and proof ergonomics against this document rather than against ad hoc taste or generic UI advice.

## Design Goal

Build the best possible terminal UX for Wave by starting with the strengths of known coding TUIs and keeping one hard constraint:

the TUI must remain a thin layer over the real control plane.

That means:

- the reducer, scheduler, results, and projections own truth
- the TUI renders that truth and sends operator intents
- the TUI never becomes a hidden planner, scheduler, or source of semantic state

## Baseline From Known TUIs

Known tools such as Codex-style coding shells, Claude Code, Lazygit, K9s, and Htop point to a few strong interaction patterns.

### What to keep

- transcript-first interaction that keeps the main task in view
- keyboard-first navigation with very low friction
- dense but legible status rows
- direct drill-down from summary rows into detailed state
- explicit pending and running feedback
- command-oriented interaction for advanced operators
- no dependence on a mouse or browser shell

### What to avoid

- hidden modal state that makes the interface feel brittle
- local UI state that can drift from scheduler truth
- tabs that hide urgent blockers or approval requests
- overloaded dashboards that force operators to decode too much at once
- large full-screen mode switches for tasks that should be quick side actions

## Product Stance

The Wave TUI should feel like:

- a Codex-like operator shell in the main pane
- a production control cockpit in the side pane
- a terminal-native incident board when the system is degraded

It should not feel like:

- a browser dashboard crammed into a terminal
- a planner disguised as a status view
- a second implementation surface parallel to the CLI and app-server

## Primary UX Principles

### 1. Transcript first

The main pane should keep the active transcript, activity log, or focused detail view visible at all times. The operator should never lose the narrative of what the system is doing.

### 2. One selection model

There should be one clearly focused object at a time:

- selected wave
- selected agent
- selected blocker
- selected proof item
- selected action request

Every side panel, footer hint, and drill-down should reflect that single focus.

### 3. Summary first, proof one step away

Rows should answer:

- what is happening
- why it is happening
- what is blocking it
- what the operator can do next

Detailed evidence should always be one interaction away, never hidden behind multiple screens.

### 4. No dishonest optimism

The UI may show local pending request state, but it must distinguish:

- requested
- accepted by authority
- applied
- failed
- expired or superseded

The operator must never mistake a keypress for an authoritative state transition.

### 5. Failure must become easier to understand, not harder

The more the harness does, the more important it is that failures collapse into clear explanations:

- what failed
- where it failed
- who owns the next move
- whether retry is safe
- whether rollback or human approval is required

### 6. Thin by design

If a UI behavior requires local inference of scheduler or reducer truth, it should not ship until the projection model provides that truth directly.

## Interaction Model

The TUI should have three layers that are always visible in some form:

1. main narrative layer
2. right-side operator layer
3. footer command and status layer

### Main narrative layer

This is the left or center pane. It is transcript-first and focus-sensitive.

Depending on the current focus, it shows:

- the active orchestrator transcript
- the selected wave timeline
- the selected agent activity log
- blocker details
- proof details
- action history

### Right-side operator layer

This remains the primary structured control surface.

It should expose the system through stable tabs:

- `Overview`
- `Wave`
- `Agents`
- `Blockers`
- `Proof`
- `Control`

`Portfolio` may appear as a top-level alias or replace `Overview` once Wave 17-level delivery state is live.

### Footer layer

The footer should always show:

- current focus path
- hotkeys relevant to the current focus
- connection and refresh health
- active request count
- approval count
- blocker severity count

## Default Layout

### Wide layout

Use a stable three-band composition:

- top header: one or two rows
- body: main pane plus right-side panel
- bottom footer: one or two rows

Recommended proportions:

- main pane: 62 to 70 percent width
- right panel: 30 to 38 percent width

The current repo already treats the right-side panel as the operator surface. Keep that. Do not replace it with a grid of equal cards.

### Top header

The header should show:

- project or repo name
- selected initiative or wave
- global runtime state
- active waves count
- blocked waves count
- approval queue count
- operator mode

### Bottom footer

The footer should show:

- focus-sensitive hotkeys
- request status
- transient notifications
- whether displayed state is live, stale, or degraded

## Narrow Layout

When the terminal is too narrow for the split layout, do not try to preserve the full side panel.

Use a stacked mode:

- narrative pane on top
- compact status strip below
- tab content replaces the lower region when opened

In narrow mode:

- show fewer columns
- keep one focused object at a time
- preserve the same command set
- never invent different semantics than wide mode

If space is extremely constrained, fall back to the existing truthful text-summary behavior rather than a broken dashboard.

## Primary Tabs

## `Overview`

Purpose:

- answer what the system is doing right now
- summarize active capacity, queue state, blockers, and approvals
- surface recommended next actions

Contents:

- active waves table
- blocked waves summary
- fairness and budget strip
- closure-capacity strip
- urgent approvals
- urgent escalations
- recent failures

Row fields for each wave:

- wave id and title
- class and intent
- owner or runtime
- state chip
- top blocker
- worktree state
- current phase
- proof health
- next recommended action

## `Wave`

Purpose:

- show one selected wave deeply

Contents:

- wave header with id, title, class, intent, priority
- dependency state
- worktree and branch state
- task list
- current blockers
- required approvals
- recent results and proof summaries
- downstream unlock implications

## `Agents`

Purpose:

- show live per-agent execution and recent history

Each agent row should answer:

- what it is doing now
- what it last completed
- what it is waiting on
- what runtime it is using
- what artifact or proof it most recently produced

Required fields:

- agent id and role
- runtime and session id
- current state chip
- now line
- last action line
- waiting-on line
- lease health
- output summary

## `Blockers`

Purpose:

- turn system blockage into a triage workflow instead of scattered red text

Blockers must be grouped by kind:

- dependency
- contradiction
- lease or contention
- merge
- human input
- runtime or policy
- proof or acceptance

Each blocker row should show:

- severity
- affected wave or waves
- root cause
- owning actor
- unblock options
- whether operator approval is needed

## `Proof`

Purpose:

- make evidence visible without drowning the operator in files

Show:

- proof summary by wave
- latest result envelopes
- acceptance state
- known risks
- outstanding debt
- replay or trace pointers where relevant

Inline summaries should stay short. Drill-down should reveal:

- artifact references
- result envelope summaries
- gate verdicts
- diff or doc delta references
- acceptance reasoning

## `Control`

Purpose:

- central place for commands, approvals, overrides, and history

Show:

- queued operator requests
- pending approvals
- orchestrator recommendations
- recent command history
- failed requests
- rollback or retry affordances

## State Chips And Action States

Use a small, stable vocabulary across all views.

### Wave and agent state chips

- `ready`
- `claimed`
- `active`
- `blocked`
- `needs-approval`
- `merge-blocked`
- `waiting-input`
- `complete`
- `failed`
- `superseded`

These chips should be consistent between CLI, app-server, and TUI wording.

### Operator request states

- `drafted`
- `requested`
- `accepted`
- `applied`
- `rejected`
- `failed`
- `expired`

### Visual rule

Use color as a reinforcement, not the only signal. Every state must remain legible in monochrome terminals.

## Keyboard Model

The keyboard model should be fast, memorable, and shallow.

### Global keys

- `1` through `6`: jump to top-level tabs
- `Tab` and `Shift+Tab`: cycle tabs
- `j` and `k`: move selection
- `h` and `l`: move focus between panes or collapse detail
- `Enter`: open detail or confirm safe action
- `Esc`: back out of detail or clear transient UI state
- `/`: filter current list
- `:`: open command bar
- `?`: open keyboard help

### Domain keys

- `o`: focus orchestrator transcript or recommendation stream
- `w`: focus worktree details for the selected wave
- `b`: focus blockers for the selected wave
- `p`: focus proof for the selected wave
- `a`: open action menu for the selected object
- `r`: request rerun or retry where valid
- `m`: open manual-close confirmation for the selected wave
- `M`: clear the selected wave's manual-close override after confirmation
- `u`: approve selected action when approval is allowed
- `x`: reject or cancel selected request

### Safety keys

Dangerous actions should never be single-key destructive actions without confirmation.

For any action that can materially change execution routing, merge state, or release state:

- first key opens the action sheet
- confirmation step shows exact target and consequence
- final confirmation records an operator intent, not a local mutation

## Orchestrator Interaction UX

The operator should be able to interact with the orchestrator in two ways:

1. structured actions
2. constrained free-form commands

### Structured actions

Use these for common actions:

- claim or release wave
- pause or resume wave
- reopen design loop
- escalate question
- approve decision
- reroute work
- adjust priority
- admit downstream implementation

Structured actions are preferred because they:

- map cleanly to control-plane intent
- are easier to audit
- have clearer validation

### Constrained free-form commands

The command bar should exist for expert operators, but it should compile to explicit operator intents rather than becoming a hidden shell inside the TUI.

Examples:

- `:pause wave 14`
- `:reroute wave 16 to review`
- `:approve decision D-42`

The UI should render the parsed action before submission.

### Recommendation model

The orchestrator may recommend actions, but recommendations must be rendered as first-class objects with:

- proposed action
- rationale
- affected waves
- risk level
- whether approval is required

The operator can then:

- accept
- reject
- inspect rationale
- defer

### Escalation model

Agent-to-orchestrator and orchestrator-to-operator escalations should not disappear into logs.

Each escalation should have:

- source
- target
- reason
- blocking impact
- suggested next action
- timestamp and correlation id

## Per-Agent Activity Model

Every active agent needs a compact but truthful activity card.

Required fields:

- `Now`: current operation in plain language
- `Last`: most recent completed meaningful step
- `Waiting on`: dependency, approval, lease, merge, or input blocker
- `Artifact`: most recent proof, doc delta, result envelope, or output reference
- `Runtime`: selected runtime and session identity
- `Lease`: lease health and expiry or heartbeat status
- `Worktree`: path or id of the wave-local worktree

The operator should not have to infer what an agent is doing from a raw transcript alone.

## Blocker Triage UX

The blocker view should behave like an incident queue.

### Sort order

Default sort order:

1. blockers that halt active waves
2. blockers that consume reserved closure capacity
3. blockers waiting on operator approval
4. blockers waiting on human input
5. blockers that only affect future admission

### Required blocker details

Each blocker detail view should show:

- human-readable summary
- canonical blocker type
- affected objects
- reducer or scheduler evidence reference
- recommended next step
- whether the operator may act directly

### Blocker-specific UX

Dependency blocker:

- show upstream wave or external dependency
- show exact condition that is unmet

Contradiction blocker:

- show conflicting facts or decisions
- show invalidated proof or downstream work

Lease or contention blocker:

- show current owner
- show expiry, fairness impact, and contention scope

Merge blocker:

- show worktree and branch state
- show conflict summary and merge readiness

Human-input blocker:

- show question, owner, SLA, and impact radius

## Multi-Wave Concurrency UX

True parallel execution is where the TUI must become much better than a simple status page.

### Active wave strip

The `Overview` tab should show all active waves together with:

- worktree id
- branch state
- runtime mix
- lease health
- top blocker
- fairness position

### Contention visibility

When two waves are in tension, the operator should see:

- what resource or ownership scope is contested
- whether the contention is hard or soft
- which wave currently holds the claim
- whether preemption is allowed

### Fairness and budget visibility

Operators should be able to answer:

- why was this wave admitted
- why was that wave delayed
- what capacity is reserved for closure
- whether any wave is starving

This should appear as explicit projection-owned state, not inferred UI decoration.

## Live Proof And Acceptance UX

Inline proof should be concise.

The operator should see, at a glance:

- proof present or missing
- latest gate status
- whether evidence is fresh or superseded
- whether ship or acceptance state is blocked

Drill-down should reveal:

- result envelope summary
- proof artifact references
- gate verdict details
- known risks
- outstanding debt
- acceptance-package state once that layer lands

## Notification And Feedback Ergonomics

Use transient notifications sparingly.

Good notification cases:

- request accepted
- request failed
- new approval required
- new contradiction blocks an active wave
- merge state changed

Do not use notifications as the only source of truth. Every notification must link back to durable state in the relevant tab.

## Command History

The `Control` tab should preserve a durable operator action history with:

- command text or structured action
- target object
- request id
- result
- failure reason if any
- timestamp

This history is critical for trust.

## Dangerous Actions

Dangerous actions should be visually and interaction-wise distinct from safe actions.

Dangerous actions include:

- force reroute
- force release
- override fairness
- bypass blocker
- approve ship or release state
- accept degraded proof

Rules:

- put them behind the action sheet
- require confirmation
- show exact impact scope
- record them durably

## Projection Contract Needed By The TUI

To stay thin, the TUI needs richer projections rather than more local logic.

The projection layer should eventually provide:

- stable top-level dashboard snapshot
- per-wave detail snapshot
- per-agent activity summary
- blocker queue snapshot
- approval queue snapshot
- action-history snapshot
- worktree and merge-state snapshot
- fairness and budget snapshot
- proof and acceptance summary snapshot
- orchestrator recommendation snapshot

If a view cannot be rendered truthfully from these projection-owned objects, it should not be approximated locally.

## Relationship To Existing Docs

This document is the detailed UX specification.

Other docs still own different layers:

- `docs/implementation/parallel-wave-multi-runtime-architecture.md`
  owns architecture and authority boundaries
- `docs/plans/master-plan.md`
  owns phase ordering and next-wave framing
- `docs/plans/full-cycle-waves.md`
  owns the full-cycle control-plane model
- `waves/*.md`
  own implementation contracts for specific slices

## Wave Mapping

This UX design should shape the coming waves as follows.

### Wave 14

Must make parallel-wave, worktree, fairness, and merge state visible enough that the `Overview`, `Wave`, and `Blockers` views can explain concurrency honestly.

The current Wave 14 landing now surfaces the first repo-local slice of that truth in the live shell: selected-wave and run views show worktree identity, promotion state, scheduler phase, fairness, protected closure state, and merge blocking from reducer-backed projections. The fuller `Overview` and `Blockers` ergonomics in this document still belong to a dedicated later UX wave.

### Wave 15

Must make runtime identity, runtime selection, fallback reason, and runtime-specific operator visibility fit the `Agents` and `Control` views without leaking runtime semantics into the TUI.

The current repo-local shell now also carries the recovery slice that landed immediately after Wave 15 closeout: manual-close override visibility plus confirm-first apply and clear actions in the `Control` view.
That live path is now transactional with rerun preservation rather than best-effort ordering: a failed override write or event append restores the prior rerun and override file state instead of silently discarding rerun intent.

### Wave 16

Must make contradictions, human-input requests, dependency handshakes, and invalidation visible enough that blocker triage and approval flows are first-class.

The current repo-local shell now carries the first live Wave 16-grade operator selection slice as well: dependency-handshake classification is typed workflow state rather than route-name folklore, and the `Control` and `Blockers` views can step through multiple actionable approvals or escalations on the selected wave via `[` and `]` before `u` or `x` confirms the chosen action.

### Wave 17

Must make proof, acceptance, release readiness, risk, and debt visible enough that the `Proof` and delivery-facing `Overview` views can distinguish local wave success from ship readiness.

### Dedicated future operator UX wave

The repo should add a dedicated future wave for TUI and control-plane ergonomics that lands:

- the keyboard and navigation model
- operator command and approval flows
- per-agent live activity cards
- blocker triage views
- proof drill-down flows
- multi-wave concurrency ergonomics

That wave should implement this document directly rather than treating operator UX as incidental follow-through on backend work.

### Wave 19 and later planner work

Planner-emitted invariants and staged gate plans should integrate with this UX through structured views and approval flows, not by turning the TUI into a second planner.

## Non-Goals

This design does not require:

- replacing the built-in TUI with a browser UI
- making the TUI authoritative over scheduler truth
- exposing every internal data structure directly
- reintroducing mouse-first interaction
- matching package-era attach surfaces exactly

## Bottom Line

The architecture already has the right direction.

What it still needs is a detailed terminal UX that feels as fast and legible as the best coding TUIs while surfacing the much richer operator and orchestrator model Wave is trying to build.

The right answer is not a thicker TUI.

It is a sharper TUI:

- transcript-first
- keyboard-first
- projection-first
- concurrency-literate
- blocker-literate
- proof-literate
- still thin
