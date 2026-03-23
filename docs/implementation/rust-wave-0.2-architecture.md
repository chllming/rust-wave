# Rust Wave Architecture 0.2

This document is the aggressive next-step architecture for the Rust rewrite.

The current repository already proves the bootstrap is viable:

- typed project config in `wave.toml`
- typed authored-wave parsing from `waves/*.md`
- fail-closed lint and launch preflight
- a shipped `wave` CLI plus a built-in Ratatui operator shell
- repo-scoped Codex execution under `.wave/codex/`
- file-backed run records, rerun intents, trace bundles, and replay checks

That means 0.2 should not polish the bootstrap as if it were the final system. It should turn the bootstrap into the front edge of the final authority model now, while the repo is still early enough to refactor hard.

This doc extends the shipped baseline in [rust-codex-refactor.md](./rust-codex-refactor.md). That baseline explains what is already live. This document explains what the next architecture should become.

## 0.1 Readout

The code in this repository already establishes a clear 0.1 bootstrap:

- `wave-config`
  Owns typed project config, execution mode, repo-local roots, and resolved paths.
- `wave-spec`
  Owns authored-wave parsing, agent contracts, exit-contract parsing, prompt-section parsing, component promotions, and deploy-environment parsing.
- `wave-dark-factory`
  Owns fail-closed authored-wave lint, skill-catalog validation, Context7 bundle validation, role-boundary checks, marker-contract checks, and ownership enforcement.
- `wave-control-plane`
  Owns queue readiness, blocker classification, closure coverage, and planning-status projection from authored waves, lint findings, latest run records, and rerun intents.
- `wave-runtime`
  Owns drafting, launch preflight, Codex invocation, project-scoped Codex home bootstrap, run-record persistence, rerun intents, agent ordering, marker collection, closure-marker checks, queue refresh, orphan reconciliation, and autonomous launch.
- `wave-trace`
  Owns the run record schema, trace-bundle schema, replay checks, artifact snapshotting, and self-host evidence synthesis.
- `wave-app-server`
  Owns operator snapshot assembly from authored waves, planning projection, latest runs, rerun intents, and replay state.
- `wave-tui`
  Owns the interactive shell as a thin renderer over operator snapshots plus rerun actions.
- `wave-cli`
  Owns the command tree and directly wires config loading, spec loading, lint, planning status, launch, autonomous runs, proof surfaces, and traces together.

The bootstrap waves already landed the following repo-local capabilities:

| Wave | What it established | Current status |
| --- | --- | --- |
| `0` | Authored-wave schema, closure-agent contract, skill catalog discipline | `repo-landed` |
| `1` | Rust workspace and bootstrap command surface | `repo-landed` |
| `2` | Typed config, parser, and dark-factory lint | `repo-landed` |
| `3` | Planning status, queue visibility, and blocker projections | `repo-landed` |
| `4` | Codex-backed launcher plus project-scoped Codex home | `repo-landed` |
| `5` | Built-in TUI operator shell and right-side panel | `repo-landed` |
| `6` | Fail-closed launch preflight and dark-factory runtime policy | `repo-landed` |
| `7` | Autonomous queue selection and dependency-aware scheduling | `repo-landed` |
| `8` | Trace bundles and replay validation | `repo-landed` |
| `9` | Self-host proof wave | still the current dogfood edge, not the final architecture |

That is enough proof to stop treating the current crate seams as sacred.

## Latest Upstream Reference

The newest upstream package repo is already ahead of the vendored docs snapshot in this worktree.

- vendored reference in this repo:
  `third_party/agent-wave-orchestrator/UPSTREAM.toml` still points at `docs/wave-positioning-refresh` commit `8b421c79d58713b8be3f137e16d8777ebd445851`
- latest upstream inspected for this doc:
  `https://github.com/chllming/agent-wave-orchestrator` `main` at commit `8a7a0ee4b4fb4c3ef8ea60c350b4c736ddde797c`
- latest upstream release surface:
  `@chllming/wave-orchestration@0.7.1`

That upstream now includes features the Rust rewrite does not yet model:

- a canonical control-plane event log
- a unified `wave control` mutation surface for tasks, proof bundles, reruns, and telemetry
- planner project profiles, agentic draft packets, and ad-hoc flows
- multi-runtime executor profiles and runtime-specific skill projection
- local-first telemetry delivery to the hosted `services/wave-control` analysis plane
- benchmark and verification command surfaces

The 0.2 plan should therefore target the current upstream architecture, not just the older docs branch pinned in `third_party/`.

## What 0.2 Must Fix

The current rewrite is strong at the edges and still too coupled in the middle.

The strongest long-term pieces already exist:

- `wave-config` is already the right repo-scoped config foundation.
- `wave-spec` is already close to the right long-term declaration parser.
- `wave-dark-factory` already proves the repo can enforce a strong authored contract.
- `wave-app-server` and `wave-tui` already act like projection consumers rather than alternate planners.

The main architecture problems are concentrated elsewhere:

- `wave-runtime` is still too file-backed and too launcher-centric.
  It owns launch, rerun, drafting, Codex execution, marker gathering, closure checks, queue refresh, orphan reconciliation, and autonomous scheduling in one crate.
- `wave-control-plane` is not yet a true control plane.
  Today it is mostly a planning/status projection layer over authored waves plus run records.
- `wave-trace` is still centered on `WaveRunRecord` plus artifact presence.
  Replay is useful, but it is still below the end-state reducer model.
- `wave-cli` still directly wires bootstrap crates together instead of calling into a thinner orchestration layer.
- closure is still marker-centric.
  The runtime still looks for plain-text final markers and closure marker payloads in `last-message.txt` and fallback `.wave` files.

0.2 should correct those seams directly instead of extending them.

## Authority Model

The right 0.2 authority model is not "one event log and everything else disappears." The current repo already has two different kinds of durable truth: authored declarations and runtime state. The target model should make that explicit.

### Canonical Authority Set

- `waves/*.md`
  Immutable wave declarations for a given run. These define goals, ownership, closure expectations, proof intent, deploy targets, and agent structure.
- control-plane event log
  Append-only lifecycle and workflow facts such as wave-selected, attempt-started, attempt-finished, rerun-requested, gate-evaluated, closure-blocked, or wave-completed.
- coordination event log
  Append-only blackboard state for blockers, claims, evidence, clarifications, handoffs, contradictions, and human-escalation records.
- structured result envelopes
  Immutable attempt-scoped agent results with role-aware payloads, proof references, and machine-readable closure input.

Everything else is a projection or compatibility cache.

### Projection Set

These should be explicitly non-canonical in 0.2:

- `WaveRunRecord`
- queue snapshots
- dashboard/operator snapshots
- TUI view state
- compiled prompt bundles
- trace summaries
- proof summaries
- retry plans
- dependency snapshots
- task ledgers

Those files remain useful, but they should be derivable from the canonical authority set plus reducer logic.

## Architectural Principles

- Keep authored waves as the declaration contract.
- Keep config and declaration parsing separate from runtime execution.
- Move all semantic workflow truth into events, envelopes, and reducer state.
- Keep executor-specific logic at the edges.
- Keep UI, dashboard, CLI summaries, and trace summaries as projections.
- Make artifacts immutable and attempt-scoped.
- Replace marker-first closure with envelope-first closure.
- Preserve compatibility adapters only as migration shims, not as the end-state contract.

## Target Crate Graph

### Foundation Crates

#### `wave-config`

Keep it. Its long-term role is still:

- parse `wave.toml`
- resolve canonical repo-local roots
- expose backend, projection, and executor configuration
- own execution posture defaults

#### `wave-spec`

Keep it. Its long-term role is:

- parse authored waves
- validate declaration-only syntax and shared sections
- expose immutable `WaveDocument` and `WaveAgent` models
- synthesize declaration-level task seeds
- stay free of runtime decisions

#### `wave-planner` (new)

The latest upstream planner surface is now large enough that 0.2 should give it an explicit home instead of smearing authoring logic across CLI and docs helpers.

It should own:

- project profile memory
- interactive draft templates
- agentic draft packets
- draft review and apply flows
- ad-hoc plan and promote flows

#### `wave-domain` (new)

Add a dedicated domain crate so types stop leaking across runtime, projection, and reducer layers.

It should own:

- task
- attempt
- fact
- contradiction
- proof bundle
- gate verdict
- rerun request
- human input workflow state
- result envelope
- closure state
- event payload types
- artifact classification metadata

### Canonical State Crates

#### `wave-events` (new)

This becomes the real append-only control-plane store.

It should own:

- canonical event schema
- event ids and versioning
- append and query APIs
- causation and correlation metadata
- stream path conventions

#### `wave-coordination` (new)

Split blackboard coordination out from planning projection.

It should own:

- coordination log schema
- blockers, claims, evidence, clarifications, and handoffs
- contradiction and escalation linkages
- coordination-specific append and query APIs

#### `wave-results` (new)

This is the structured result layer.

It should own:

- role-aware result envelopes
- validation
- immutable attempt-scoped storage
- legacy marker adapters
- proof-artifact normalization and hashing

### State Computation Crates

#### `wave-reducer` (new)

This is the 0.2 heart of the system.

Inputs:

- wave declarations
- control-plane events
- coordination records
- result envelopes

Outputs:

- current wave state
- task graph state
- blockers
- proof availability
- contradiction state
- retry eligibility
- closure readiness
- human-input workflow state

This crate must stay pure. It does not launch anything and does not write canonical state.

#### `wave-gates` (new)

Move closure logic here.

It should evaluate:

- owned-slice proof
- optional eval proof
- integration proof
- documentation closure
- cont-QA closure
- deploy proof
- security proof
- unresolved blockers or contradictions

It emits typed gate verdicts. It does not own process lifecycle.

#### `wave-retry` (new)

Make retry policy deterministic and testable.

Inputs:

- reducer state
- rerun requests
- result envelopes
- contradiction state
- gate verdicts
- executor history

Outputs:

- retry plan
- invalidation scope
- reusable proof scope
- resume point
- fallback eligibility
- blocking reasons

#### `wave-derived` (new)

This owns rebuildable caches only.

Examples:

- shared summaries
- per-agent inboxes
- docs queue
- ledger
- assignment snapshot
- dependency snapshot
- integration summary
- proof summary
- retry summary

### Telemetry And Benchmark Crates

#### `wave-benchmarks` (new)

Latest upstream treats benchmarks and eval verification as first-class runtime evidence, not just extra docs.

It should own:

- benchmark catalog loading
- local benchmark case execution
- external benchmark adapter manifests
- benchmark result normalization
- validity classification for benchmark results

#### `wave-telemetry` (new)

Remote analysis should stay explicitly non-authoritative, but the latest upstream proves the local-first telemetry layer is still worth preserving.

It should own:

- local telemetry spool
- artifact descriptor selection
- best-effort remote delivery
- query and flush helpers for the local queue

The remote service must stay an analysis plane, never the scheduler of record.

### Runtime And Execution Crates

#### `wave-executor-api` (new)

Define the executor boundary as a trait layer.

It should describe:

- launch spec
- executor capabilities
- artifact expectations
- fallback hooks
- sandbox and profile support

#### `wave-executor-codex` (new)

Move Codex-specific logic here.

It should own:

- `codex exec` launch spec assembly
- project-scoped `CODEX_HOME` handling
- Codex-specific artifacts and overlays
- result-envelope collection or synthesis

#### `wave-supervisor` (new)

Move process lifecycle here.

It should own:

- process spawn and wait
- lock management
- PID tracking
- orphan detection
- timeouts
- runtime-failure retries
- observed lifecycle event emission

Rule:

- supervisor executes and observes
- supervisor does not decide queue semantics, closure, or retry policy

#### `wave-launcher` (new)

Introduce a thin orchestration crate that coordinates reducer, retry, executor selection, and supervisor work.

It should:

1. load declarations and canonical state
2. reduce current state
3. compute retry or resume plan
4. choose runnable tasks
5. build executor launch specs
6. hand launch specs to the supervisor
7. persist result envelopes
8. append canonical events
9. re-run reducer and gates
10. trigger projection refresh

#### `wave-runtime`

Shrink it heavily or retire it into a compatibility crate.

Most current responsibilities should move into:

- `wave-launcher`
- `wave-supervisor`
- `wave-executor-codex`
- `wave-results`
- `wave-retry`

### Projection And UI Crates

#### `wave-projections` (new)

This should own all human-facing state materialization:

- queue summaries
- control status
- dashboard models
- proof snapshots
- TUI-ready view models
- trace summaries

This is where most of today's `wave-control-plane` projection behavior should land.

#### `wave-trace`

Keep it, but narrow it.

Its 0.2 role is:

- immutable attempt-scoped trace bundles
- replay fixtures
- replay report serialization
- regression archives

Semantic replay should depend on `wave-reducer` and `wave-gates`, not live entirely inside `wave-trace`.

#### `wave-app-server`

Keep it as a projection assembler and API surface.

It should consume reducer state and projection models. It should never become an alternate authority source.

#### `wave-tui`

Keep it thin.

It should:

- read projection state
- render it
- invoke explicit operator actions

It should not re-derive planning state locally.

#### `wave-cli`

Keep it, but make it thinner.

It should:

- parse commands
- load config
- call reducer-backed orchestration or projection APIs
- render output

It should stop directly encoding orchestration policy.

## Canonical Persisted State

0.2 should move from wave-level mutable files toward attempt-scoped immutable state.

### Canonical Event Logs

- `.wave/state/events/control/wave-<N>.jsonl`
- `.wave/state/events/coordination/wave-<N>.jsonl`

### Canonical Result And Trace State

- `.wave/state/results/wave-<N>/attempt-<A>/<agent-id>.json`
- `.wave/state/traces/wave-<N>/attempt-<A>/outcome.json`
- `.wave/state/traces/wave-<N>/attempt-<A>/gate-snapshot.json`
- `.wave/state/traces/wave-<N>/attempt-<A>/run-metadata.json`

### Derived Caches

- `.wave/state/derived/tasks/wave-<N>.json`
- `.wave/state/derived/assignments/wave-<N>.json`
- `.wave/state/derived/dependencies/wave-<N>.json`
- `.wave/state/derived/ledger/wave-<N>.json`
- `.wave/state/derived/docs-queue/wave-<N>.json`
- `.wave/state/derived/proof/wave-<N>.json`
- `.wave/state/derived/retry/wave-<N>.json`
- `.wave/state/derived/integration/wave-<N>.json`
- `.wave/state/derived/security/wave-<N>.json`

### Human Projections

- `.wave/state/projections/dashboard/global.json`
- `.wave/state/projections/dashboard/wave-<N>.json`
- `.wave/state/projections/boards/wave-<N>.md`
- `.wave/state/projections/inboxes/wave-<N>/<agent-id>.md`
- `.wave/state/projections/summaries/wave-<N>-shared.md`

### Compatibility Roots During Migration

These stay temporarily, but they stop being near-canonical:

- `.wave/state/runs/`
  compatibility snapshots for the current CLI and trace surfaces
- `.wave/traces/runs/`
  compatibility trace location for the current v1 trace surface
- `.wave/codex/`
  still the project-scoped executor home for Codex-backed launches

## Core Domain Model

### Task

A task is a stable unit of work, not just a line in a markdown prompt.

It should carry:

- semantic id
- declaration source
- ownership
- proof families
- dependency edges
- lease or assignee state
- closure state

### Attempt

An attempt is immutable execution history for a concrete task or wave slice.

It should carry:

- attempt id
- target task or wave id
- executor selection
- timestamps
- artifact references
- result-envelope references
- lifecycle events

### Fact

Facts should be first-class typed records, not buried in free-form summaries.

Each fact should track:

- semantic id
- source artifact
- introduced-by event or result
- citations
- contradiction links
- supersession links

### Contradiction

Contradictions should be explicit state, not implied by human review prose.

They should represent:

- conflicting claims
- conflicting proof
- cross-component incompatibility
- maturity disagreements

### Result Envelope

Result envelopes should be role-aware, not one giant generic blob.

Common header:

- schema version
- wave id
- task id or agent id
- attempt id
- executor
- timestamps
- exit status

Role payloads:

- implementation payload
- integration payload
- documentation payload
- cont-QA payload
- eval payload
- security payload
- deploy payload

## State Machines

### Task State

- `declared`
- `leased`
- `in_progress`
- `owned_slice_proven`
- `blocked`
- `closed`

### Wave State

- `planned`
- `running`
- `closure_pending`
- `wave_closure_ready`
- `completed`
- `failed`
- `blocked`

### Human Input State

- `pending`
- `assigned`
- `answered`
- `rerouted`
- `escalated`
- `resolved`
- `timed_out`

### Contradiction State

- `detected`
- `acknowledged`
- `repair_in_progress`
- `resolved`
- `waived`

## Runtime Loop

The live 0.2 flow should be:

1. CLI loads config and wave declarations.
2. Reducer rebuilds state from declarations, events, coordination records, and result envelopes.
3. Retry planner computes invalidation, reuse, and resume scope.
4. Launcher selects runnable tasks.
5. Executor adapter builds launch specs.
6. Supervisor starts or resumes sessions and emits observed lifecycle events.
7. Executor adapter collects immutable result envelopes and runtime artifacts.
8. Control-plane events and coordination records are appended.
9. Reducer recomputes current truth.
10. Gates evaluate proof, contradictions, closure, and blocking conditions.
11. Projection writers refresh dashboard, queue, summaries, and trace outputs.

Important rule:

- supervisor writes observations
- engines write decisions
- reducer computes state
- projections render state

None of those roles should collapse back into the current `wave-runtime` monolith.

## Immediate 0.2 Refactor Rules

### 1. Stop Treating `WaveRunRecord` As Near-Canonical

`WaveRunRecord` should become a derived cache or compatibility snapshot.

The authoritative state should instead be:

- append-only lifecycle events
- immutable result envelopes
- reducer-derived current state

### 2. Split `wave-control-plane`

Today it mostly computes planning truth from waves, lint, reruns, and latest runs.

That should split into:

- `wave-events`
- `wave-reducer`
- `wave-projections`

The current crate name overstates its authority.

### 3. Replace Marker-Centric Closure With Envelope-Centric Closure

Current runtime behavior still:

- reads `last-message.txt`
- scans fallback `.wave` files
- parses plain-text markers
- enforces closure state from marker lines

Migration path:

1. add `agent_result_envelope.json`
2. teach prompts and adapters to emit both envelope and legacy markers
3. normalize old marker-based results through `wave-results`
4. make gates consume normalized envelopes only

Markers can remain as human-readable closure evidence. They should stop being the primary machine contract.

### 4. Make Artifacts Attempt-Scoped

Do this early.

The current repo already has good root directories, but 0.2 should make every durable runtime artifact clearly scoped by:

- wave
- attempt
- agent

That avoids path ambiguity later when retries, replay, or partial reruns become real.

## Current Crates To Target Moves

| Current crate | Current role | 0.2 move |
| --- | --- | --- |
| `wave-config` | typed project config and repo roots | keep, expand for backend and projection settings |
| `wave-spec` | declaration parsing and agent contract helpers | keep, add declaration-level task synthesis only |
| `wave-dark-factory` | fail-closed authored-wave lint | keep, later consume richer domain types where useful |
| `wave-control-plane` | queue and planning projection | split across `wave-events`, `wave-reducer`, and `wave-projections` |
| `wave-runtime` | launcher, reruns, markers, preflight, queue refresh, autonomy | split across `wave-launcher`, `wave-supervisor`, `wave-executor-codex`, `wave-results`, `wave-retry` |
| `wave-trace` | trace bundle plus replay | narrow to trace persistence, replay fixtures, and report serialization |
| `wave-app-server` | snapshot assembler | keep as projection consumer |
| `wave-tui` | operator shell | keep thin and projection-only |
| `wave-cli` | command tree plus orchestration policy wiring | keep thinner and push policy down |

## Upstream Feature Gap Map

Use this section as the bridge between the latest upstream package surface and the Rust 0.2 architecture.

`Map level` means:

- `core`
  required to close the architectural gap before 0.2 cutover
- `post-cutover`
  should map after the new authority model is stable
- `intentional divergence`
  preserve the concept, not necessarily the exact upstream UX

| Upstream feature family | Latest upstream surface | Rust rewrite today | Map level | 0.2 mapping | Target wave |
| --- | --- | --- | --- | --- | --- |
| Canonical coordination log | `.tmp/<lane>-wave-launcher/coordination/*.jsonl`, shared summary, inboxes, ledger, integration summary | missing as scheduler truth | `core` | `wave-coordination`, `wave-derived`, `wave-projections` | `10-11` |
| Canonical control-plane event log | append-only control-plane events backing task, rerun, proof, telemetry projections | missing | `core` | `wave-events`, reducer-backed projections | `10-11` |
| Unified `wave control` surface | `status`, `task`, `rerun`, `proof`, `telemetry` | only a much smaller read-mostly control surface exists | `core` | `wave-events`, `wave-coordination`, `wave-results`, `wave-retry`, thinner CLI | `11-14` |
| Task lifecycle and blocking edges | targeted tasks with blocking and informational kinds | missing | `core` | task entities in `wave-domain`, task reducers in `wave-reducer` | `14` |
| Proof bundle lifecycle | active, superseded, revoked proof bundles | missing | `core` | `wave-results` plus `wave-gates` and proof projections | `12` |
| Targeted rerun and reuse control | selected agents, reuse selectors, invalidation, resume cursor | only wave-level rerun intent exists | `core` | `wave-retry` plus task graph and attempt lineage | `14` |
| Clarification and human-input chains | clarification routing, escalation, answer flows | missing | `core` | human-input workflow state in `wave-domain` and `wave-coordination` | `15` |
| Cross-lane dependency tickets | `wave dep` and dependency snapshots | missing | `post-cutover` | dependency-ticket model on top of `wave-domain` plus backend support | `20-21` |
| Multi-runtime executor boundary | Codex, Claude, OpenCode, local with runtime-specific overlays and fallbacks | Codex-only | `core` | `wave-executor-api`, `wave-executor-codex`, later Claude/OpenCode adapters | `13,17` |
| Skill activation and runtime projection | role/runtime/deploy-kind activation, metadata-first skill projection | skills exist but launcher-time resolution/projection is minimal | `core` | planner plus launcher-side skill resolution and runtime projection | `17,19` |
| Planner project profile and draft packets | `wave project setup|show`, interactive and agentic draft, apply-run | missing beyond prompt compilation | `post-cutover` | `wave-planner` | `19` |
| Ad-hoc plan, run, and promote | `wave adhoc plan|run|show|promote` | stubbed | `post-cutover` | `wave-planner` + `wave-launcher` over the same domain model | `19` |
| Trace v2 and launch previews | attempt-scoped traces, launch previews, quality snapshots, richer replay fixtures | v1 run-record-oriented traces only | `core` | narrower `wave-trace` plus reducer-backed replay and launch-preview projections | `16` |
| Local-first telemetry and remote Wave Control | local event spool, artifact descriptors, best-effort delivery, hosted analysis service | missing | `post-cutover` | `wave-telemetry`; optional service parity later | `20` |
| Benchmark and eval telemetry | benchmark CLI, benchmark validity classes, verification and review events | optional `E0` role exists, benchmark runtime does not | `post-cutover` | `wave-benchmarks`, `wave-gates`, `wave-telemetry` | `20` |
| Dashboard attach and terminal surfaces | tmux and VS Code attach surfaces, resident orchestrator, dashboard attach | intentionally replaced by built-in TUI today | `intentional divergence` | keep the built-in TUI primary; only preserve attachable supervisor semantics if needed | `21` |
| Workspace/package lifecycle | `wave init`, `wave upgrade`, `wave self-update`, `wave changelog` | missing | `post-cutover` | package-layer tooling after architecture cutover | `21` |

## Closure-Goal Vocabulary

0.2 waves should use one explicit closure-goal vocabulary. For this repo, those goals should align with the existing component maturity model rather than inventing a second ladder.

- `repo-landed`
  Code, docs, tests, and migration shims are in tree. The new architecture slice exists, but live self-host proof is not yet the bar.
- `baseline-proved`
  Reducer fixtures, replay fixtures, and projection parity tests prove deterministic behavior on local state.
- `pilot-live`
  The repo-local self-host loop uses the new slice for at least one real wave or attempt path.
- `qa-proved`
  Closure gates, replay, reruns, and failure-path evidence show the slice behaves correctly under real self-host use.
- `cutover-ready`
  The legacy path can be removed or fully demoted to compatibility mode without losing operator capability.

This repo is still local-first. When someone says "deployed" in the 0.2 plan, the closest honest term is usually `pilot-live`, not remote fleet deployment.

## 0.2 Implementation Waves

The next architectural phases should be authored as waves, not as vague background refactors.

| Wave | Theme | Primary components | Closure goal | Required exit evidence |
| --- | --- | --- | --- | --- |
| `10` | Authority reset | `wave-domain`, `wave-events`, `wave-coordination` | `repo-landed` | canonical event types, append/query APIs, declaration-to-domain mapping, docs updated |
| `11` | Reducer and control projection spine | `wave-reducer`, `wave-projections`, unified status model | `baseline-proved` | deterministic queue, closure, task, and blocker fixtures match current surfaces |
| `12` | Result envelope and proof lifecycle | `wave-results`, legacy marker adapter, proof lifecycle | `repo-landed` | envelope schema, proof lifecycle, adapter coverage, envelope-first gate inputs |
| `13` | Runtime breakup and executor boundary | `wave-launcher`, `wave-supervisor`, `wave-executor-api`, `wave-executor-codex` | `repo-landed` | process lifecycle split from policy, launch path still functional, Codex edge isolated |
| `14` | Task graph and targeted rerun | task model, retry planner, attempt lineage | `baseline-proved` | retry fixtures, invalidation scope tests, task-targeted reruns, explicit reuse controls |
| `15` | Contradictions, clarifications, human input | contradiction/fact lineage, clarification chains, human-input workflow | `pilot-live` | self-host run produces contradiction-aware integration and human-input state |
| `16` | Trace v2 and replay ratification | reducer-backed replay, richer trace fixtures, launch previews | `qa-proved` | replay compares computed state to stored outcomes, not just artifact presence |
| `17` | Runtime plurality and skill projection | Claude/OpenCode adapters, runtime-aware skill projection | `pilot-live` | non-Codex runtime path works without changing wave semantics |
| `18` | Backend boundary and 0.2 cutover | backend trait plus local-file backend, compatibility reduction | `cutover-ready` | legacy `WaveRunRecord` and marker-only paths are demoted or removable |
| `19` | Planner and ad-hoc parity | `wave-planner`, project profile, draft packets, ad-hoc specs | `repo-landed` | planner and ad-hoc flows target the same declaration and reducer model |
| `20` | Telemetry, benchmarks, and dependencies | `wave-benchmarks`, `wave-telemetry`, dependency ticket model | `pilot-live` | local-first telemetry and benchmark/eval evidence flow from the 0.2 state model |
| `21` | Package parity and optional attach surfaces | workspace lifecycle tooling, optional dashboard attach semantics | `qa-proved` | package and operator parity close without reintroducing a second authority model |

Waves `10` through `18` are the core 0.2 cutover. Waves `19` through `21` close the remaining upstream package-surface gap after the new authority model is stable.

### Control-Discipline Overlays

The architectural waves should also absorb the control-discipline hardening that execution exposed:

- Wave `12`
  typed result envelopes should carry machine-readable proof, doc-delta, and closure input so gate evaluation stops depending directly on free-form marker scans
- Wave `13`
  the launcher and supervisor split should add mandatory post-agent gates so a broken source workspace cannot advance just because one agent emitted markers
- Wave `14`
  task graph and retry planning should support mid-wave checkpoints and targeted retry instead of whole-wave restarts by default
- Wave `15`
  code, docs, and component-state mismatches should become durable contradiction and clarification state instead of review-only observations
- Wave `16`
  replay should ratify reducer state, projection state, and gate outcomes rather than artifact presence alone
- Wave `18`
  hard cutover should make it impossible for compatibility run artifacts or marker-first closure to slip back in as hidden authority paths
- Wave `19`
  the planner should emit architecture sections in scope, invariants to preserve, staged gate plans, and doc-parity expectations as part of the authored wave contract
- Wave `20`
  telemetry should record gate failures, doc-parity drift, and contradiction-driven repair loops without making any remote service authoritative

### Wave 10: Authority Reset

Primary goal:

- introduce `wave-domain`
- introduce `wave-events`
- introduce `wave-coordination`
- define canonical authority boundaries in code

Must be true at closure:

- every new workflow fact is expressible as a typed event
- current queue and closure inputs can be represented without `WaveRunRecord` being the semantic source of truth
- projection crates can read from the new authority set without inventing new semantics

### Wave 11: Reducer Spine

Primary goal:

- implement `wave-reducer`
- move queue readiness, closure coverage, and blocker truth behind reducer output
- move planning/status rendering into `wave-projections`

Must be true at closure:

- `wave control status`
- TUI queue state
- app-server queue state

all derive from the same reducer-backed projection contract.

### Wave 12: Result Envelope Migration

Primary goal:

- add structured `agent_result_envelope.json`
- normalize legacy marker output through `wave-results`
- move gate input from text markers to envelopes

Must be true at closure:

- new runs produce structured envelopes
- old runs still replay through the adapter
- closure logic no longer depends directly on scanning free-form text files
- machine-readable proof, doc-delta, and closure inputs exist at the result-envelope layer even if human-readable markers remain as evidence

### Wave 13: Runtime Breakup And Executor Boundary

Primary goal:

- separate orchestration, supervision, executor wiring, and result collection

Must be true at closure:

- queue and retry policy no longer live inside the process supervisor
- Codex-specific behavior is isolated to the Codex adapter
- launcher code coordinates engines instead of owning all semantics itself
- launcher flow can stop after implementation slices, run post-agent verification gates, and refuse to advance on a broken source workspace

### Wave 14: Task Graph And Targeted Retry

Primary goal:

- introduce stable task identities
- make reruns task-targeted
- make retry reuse and invalidation deterministic

Must be true at closure:

- retry plans are explainable from reducer state
- reruns do not require whole-wave restarts by default
- `owned_slice_proven` is distinct from final wave closure
- mid-wave checkpoints can fail only the implicated owners and feed targeted retry scope

### Wave 15: Contradictions, Clarifications, And Human Input

Primary goal:

- add first-class contradictions and fact lineage
- add clarification and human-input chains as durable workflow state
- make integration closure block on unresolved material conflicts

Must be true at closure:

- integration summaries can cite specific contradictory facts
- closure state can explain why reconciliation is blocked
- self-host runs can record contradiction-aware repair loops
- clarification routing and escalation stay inside durable workflow state
- doc, component, and implementation mismatches can be persisted as contradictions instead of remaining review-only notes

### Wave 16: Trace V2 And Replay Ratification

Primary goal:

- make replay reducer-driven
- replace the current v1 run-record-centered trace shape with attempt-scoped trace bundles and launch previews
- treat historical traces as regression fixtures

Must be true at closure:

- replay compares stored outcomes against recomputed reducer and gate state
- artifact presence alone is not considered sufficient replay proof
- regression fixtures cover success, failure, rerun, and contradiction paths
- replay evidence can explain mismatches in projection state and gate outcomes, not just missing artifacts

### Wave 17: Runtime Plurality And Skill Projection

Primary goal:

- add at least one non-Codex executor path on the new architecture
- make skill resolution and projection runtime-aware without changing the higher-level wave contract
- preserve executor-native settings behind the adapter seam

Must be true at closure:

- the same wave declaration and reducer state can drive Codex and one additional runtime
- runtime-specific overlays do not leak into the reducer or declaration model
- skill projection is recomputed after executor resolution and fallback

### Wave 18: Backend Boundary And 0.2 Cutover

Primary goal:

- introduce a backend trait
- keep the local-file backend first
- stop hard-coding file layout into orchestration decisions
- demote or remove the legacy run-record and marker-first assumptions

Must be true at closure:

- launcher and reducer depend on backend interfaces instead of direct file-walking everywhere
- timers, human input, and workflow bookkeeping can move behind the backend seam
- local-file backend remains the default and still powers self-host runs
- the repo can dogfood the 0.2 path on itself with compatibility files clearly marked derived
- compatibility run artifacts and marker-first closure can no longer regain hidden semantic authority after the cutover

### Wave 19: Planner And Ad-Hoc Parity

Primary goal:

- add project profile memory
- add interactive and agentic draft packets
- add ad-hoc plan, run, show, and promote flows on the same domain model

Must be true at closure:

- planner output targets the same declaration and reducer contract used by live execution
- ad-hoc work is stored as first-class packets instead of bypassing the authority model
- draft review and apply flows do not invent a second spec format
- authored waves can declare architecture sections in scope, invariants to preserve, staged gate expectations, and doc-parity requirements without inventing a second control plane

### Wave 20: Telemetry, Benchmarks, And Dependencies

Primary goal:

- add local-first telemetry egress
- add benchmark and verification result modeling
- add dependency-ticket state beyond a single repo-local lane

Must be true at closure:

- remote analysis remains best-effort and non-authoritative
- benchmark results are typed and validity-scored
- dependency chains can be represented without collapsing them into generic blockers
- telemetry can describe gate failures, doc-parity drift, and contradiction-driven repair loops without becoming scheduler truth

### Wave 21: Package Parity And Optional Attach Surfaces

Primary goal:

- close remaining package-surface gaps only after the 0.2 authority model is stable
- add workspace lifecycle tooling and optional attach semantics where they still make sense

Must be true at closure:

- package lifecycle commands do not bypass the reducer-backed state model
- any attachable dashboard or resident-orchestrator surface remains a projection or supervisor concern, not a second planner
- the built-in Rust TUI can remain the primary operator surface without blocking package parity

## Non-Goals For 0.2

0.2 should not try to solve everything.

It should not:

- reintroduce the old Node package architecture as the planning center
- turn the TUI into a second planner
- turn `wave-app-server` into a hidden authority source
- make live-host deployment proof the requirement for every architectural wave
- block the crate split on `wave adhoc` or `wave dep`

Those surfaces still matter. They are not the dependency that should hold up the reducer and authority reset.

## Direct Recommendation

The current Rust rewrite has already proved enough to justify a harder refactor.

Do not keep polishing the current `wave-runtime` and `wave-control-plane` split as if it were already the right architecture. It is a good bootstrap, but it is still a bootstrap:

- file-backed
- run-record centric
- marker-centric
- launcher-centric

0.2 should turn it into:

- declaration-backed
- event-backed
- envelope-backed
- reducer-driven
- projection-only at the UI layer

That is the shortest path from the current repo to the actual long-term Rust architecture.
