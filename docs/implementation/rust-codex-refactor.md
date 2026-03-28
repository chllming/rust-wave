# Rust/Codex Refactor Baseline

This repository now has a working local operator/runtime slice of the refactor:

- a Rust workspace with the target crate layout
- `wave.toml` as the new project config
- `waves/` as the new rich authored-wave source directory
- a compileable `wave` CLI and interactive Ratatui shell
- repo-specific skills for Rust workspace, control-plane, Codex runtime, TUI, and closure-marker work
- pinned and vendored upstream baselines for Codex OSS and the Wave control-plane docs branch
- live Codex-backed launcher state under the repo-local `.wave/codex/`
- canonical authority roots under `.wave/state/events/control/`, `.wave/state/events/coordination/`, `.wave/state/events/scheduler/`, `.wave/state/results/`, `.wave/state/derived/`, `.wave/state/projections/`, and `.wave/state/traces/`
- compatibility run outputs under `.wave/state/runs/` and replay-aware compatibility trace bundles under `.wave/traces/runs/` until later cutover waves replace them

This is still a bootstrap slice. The docs should describe what the control-plane can already prove, not imply that every live operator feature has shipped.

For `dark-factory`, the shipped behavior is now fail-closed at both authoring and launch time:

- authored waves must satisfy the structured contract that lint and draft consume
- launch writes a preflight report before mutation
- launch refuses when the report indicates missing required contracts
- the diagnostic path is local and repo-scoped; it is not a live-host mutation workflow

## Reviewed Upstreams

- Codex OSS: `https://github.com/openai/codex` at commit `5e3793def286099deaf5a6ae625e1f31ad584790`
- Wave control-plane docs branch: `https://github.com/chllming/agent-wave-orchestrator/tree/docs/wave-positioning-refresh` at commit `8b421c79d58713b8be3f137e16d8777ebd445851`

## Current Scope

This slice now spans the earlier bootstrap waves and parts of the runtime/operator waves:

1. freeze the repo layout and command map
2. make the new config and wave formats concrete
3. add lint and planning-status primitives
4. add the Codex-backed launcher, operator snapshot, and right-side TUI shell
5. add rerun intents, trace bundles, replay validation, and control-plane actions over recorded run state

## Authority Core Baseline

Wave 0.2 authority work is now present in-tree as typed Rust crates and config, and the planning/operator read models now cut over to reducer-backed projections even though compatibility run adapters still feed those reducers in this stage:

- `wave-domain`
  typed task ids, attempt ids, closure roles, gate dispositions, fact and contradiction records, rerun requests, human-input requests, and declaration-to-task-seed mapping from authored waves
- `wave-events`
  append/query primitives for canonical control-event logs under `.wave/state/events/control/` plus scheduler-authority logs under `.wave/state/events/scheduler/`
- `wave-coordination`
  append/query primitives for durable coordination records under `.wave/state/events/coordination/`
- `wave-projections`
  reducer-backed human-facing read models for planning status, queue projections, control/status views, and operator snapshot inputs, including the `ProjectionSpine` contract plus queue/control status helper read models that feed CLI status surfaces and the TUI queue narrative
- `wave-control-plane`
  forwarding shim that re-exports the `wave-projections` contract for existing callers while the workspace finishes the dependency cutover; it no longer owns authoritative planner or operator projection logic

`wave.toml` now carries the authority-root contract directly. `wave project show --json` exposes those paths, and `wave doctor --json` verifies both role-prompt availability and that the authority roots stay under `.wave/state/`.

The canonical Wave 0.2 authority roots under `.wave/state/` are:

- control events: `.wave/state/events/control/`
- coordination records: `.wave/state/events/coordination/`
- scheduler authority events: `.wave/state/events/scheduler/`
- structured results: `.wave/state/results/`
- derived state: `.wave/state/derived/`
- projections: `.wave/state/projections/`
- canonical traces: `.wave/state/traces/`

This is now an authority-core plus projection-spine landing. Scheduler ownership is canonical in the reducer through explicit claim, lease, and budget events, so readiness is no longer treated as ownership. Wave 0.2 still keeps compatibility run and trace artifacts under `.wave/state/runs/` and `.wave/traces/runs/` as adapter inputs, and the live runtime still executes serially. True parallel waves, per-wave worktrees, and multi-runtime execution remain later work.

## Authored-Wave Contract

The active wave format is now production-style and multi-agent:

- frontmatter with repo-planning metadata (`id`, `slug`, `title`, `mode`, `owners`, `depends_on`, `validation`, `rollback`, `proof`)
- top-level commit message, component promotions, deploy environments, and Context7 defaults
- mandatory closure agents `A0`, `A8`, and `A9`
- implementation agents with exact deliverables, file ownership, exit contracts, and final markers
- fail-closed lint for weak prompts, missing closure coverage, and unknown skill ids
- compiled per-agent prompts under `.wave/state/build/specs/` that preserve the same authored-wave contract at runtime

For dark-factory waves, the authored contract is the thing the launcher enforces. The authoring surface must already include the environment, validation, rollback, proof, and ownership data that runtime preflight will check.

This repo is no longer using the earlier minimal wave shape as the active authoring model.

More specifically, the bootstrap contract already assumes:

- `waves/*.md` is the canonical authored surface that Rust code parses directly
- implementation prompts must contain the required structured sections instead of informal instructions
- the owned-path list inside `### Prompt` must restate the `### File ownership` list
- implementation ownership must include any manifest or dependency-edge files required to complete the architectural seam the agent owns
- non-closure agents must declare skills, components, capabilities, deliverables, exit contracts, and the implementation marker set
- closure agents must include the correct role-prompt files and only the markers they own
- compatibility boundaries must be named explicitly: each wave should say what is already authoritative, what remains adapter-backed, and what cannot regress into hidden truth
- authored waves are expected to be strong enough that `wave draft`, `wave lint`, `wave doctor`, and queue status all describe the same task model

The practical authoring shape for future waves is:

1. frontmatter for repo-level sequencing and validation
2. shared markdown sections for commit, components, deploy target, and Context7 defaults
3. closure-agent sections that declare role prompts, owned closure artifacts, and exactly one closure marker family
4. implementation-agent sections that declare executor, skills, subsystem identity, exit contract, owned files, and the implementation marker set
5. a prompt block that restates owned paths and gives coding instructions in the required headings the linter knows how to read
6. ownership aligned to architectural seams so an agent can land the dependency edges, manifests, and runtime code its seam requires without late handoff

Treat that shape as an executable schema, not a docs convention. Future waves should be authored as if the parser, linter, and operator shell are all first-order consumers of the same markdown.

An authored-wave success marker is only meaningful when the owned architectural seam is actually closed. “The code landed but the dependency edge or manifest change belongs to another owner” is a wave-contract defect, not acceptable proof.

## Wave Lifecycle Through The Toolchain

The current bootstrap already uses one authored-wave contract across authoring, validation, and runtime surfaces:

1. `waves/*.md` provides the canonical human-authored spec
2. `wave-spec` parses the rich markdown into typed wave and agent models
3. `wave-dark-factory` rejects underspecified waves, ownership drift, weak prompts, marker drift, and bad skill references
4. `wave-projections` computes reducer-backed planning, queue, and control read models from the typed wave model plus compatibility-backed run inputs, and `build_projection_spine_with_state(...)` packages those with operator snapshot inputs for CLI and app-server consumers while `wave-control-plane` forwards the contract
5. `wave draft` writes compiled per-agent prompts under `.wave/state/build/specs/`
6. `wave launch` and `wave autonomous` write `preflight.json`, refuse closed on missing requirements, and only then use the compiled prompts with the project-scoped Codex runtime under `.wave/codex/`
7. `wave-app-server` and `wave-tui` render the current operator views from authored waves, lint findings, rerun intents, compatibility run outputs, and replay state

That end-to-end flow is why repo guidance must stay practical and synchronized with enforcement. If the docs describe a looser model than the toolchain accepts, operators will author broken waves.

That same model feeds the operator surfaces today:

- `wave-spec`
  parses the rich markdown structure into typed wave and agent models
- `wave-dark-factory`
  rejects underspecified or inconsistent authored waves
- `wave-projections`
  computes reducer-backed planning status, queue readiness, scheduler-owned claim and lease visibility, closure coverage, control/status views, and operator snapshot inputs from the typed wave model plus compatibility-backed run adapters; this is the authoritative projection/read-model crate
- `wave-control-plane`
  forwards the projection contract so existing runtime and CLI callers compile while the manifests still point at the older crate name
- `wave-cli`
  exposes the model through `wave`, `wave doctor`, `wave lint`, `wave control ...`, `wave launch`, `wave autonomous`, and `wave trace ...`, with `doctor` and `control status` now reading the same projection spine contract that feeds the operator shell and emitting the projection-owned control-status read model in their JSON reports
- `wave-app-server`
  assembles transport snapshots from reducer-backed operator snapshot inputs plus compatibility-backed active-run details, rerun intents, and replay state; it now carries the projection-owned control-status payload alongside the operator panels instead of re-deriving queue/control truth locally
- `wave-tui`
  renders the right-side operator panel for `Run`, `Agents`, `Queue`, and `Control`, with the queue story and control attention lines coming from the app-server snapshot's reducer-backed control-status payload rather than terminal-local recomputation

The current repo-local cutover point is therefore explicit:

- `wave-projections`
  is the authoritative reducer-backed planning and operator read-model layer
- `wave-control-plane`
  is compatibility naming only; it forwards the `wave-projections` surface and does not hide separate reducer or planner logic
- `wave-cli`, `wave-app-server`, and `wave-tui`
  prove summary-level and row-level parity against the same projection truth in unit fixtures rather than rebuilding queue/control truth per consumer

Trace data is part of the same operator evidence boundary. `wave trace latest` surfaces the durable record for each completed wave, including the trace path and replay verdict, and `wave trace replay` validates that the stored compatibility trace bundle or compatibility run record still matches the live run state and artifact inventory.

The repo guidance docs should therefore describe the same concrete contract that these crates enforce, not a looser future-state summary.

## Planning Status Model

Planning status is now a first-class control-plane concern in this refactor. The documented status surface should stay aligned with the queue and operator snapshot rather than with any UI-specific interpretation.
The live Rust implementation now routes this through `wave-reducer` into `wave-projections`, with `wave-control-plane` left as a forwarding layer for compatibility. Compatibility run records still act as adapter inputs until canonical attempt/result envelopes replace them.
`wave-projections` now also owns the operator-facing queue/control panel inputs and the queue/control status helper read models, so CLI status, app-server snapshot assembly, and the TUI all consume the same reducer-backed read-model spine instead of rebuilding those surfaces independently. The queue decision story, closure-gap attention lines, and skill-issue lines now come from projection helpers rather than from per-surface formatting code.

The current status model should be read as:

- `Run`
  the active wave, run id, elapsed time, proof counts, and declared proof artifacts
- `Agents`
  per-agent state, proof or marker completeness, and deliverables
- `Queue`
  readiness, blockers, dependency-driven ordering, and the reducer-owned distinction between ready, claimed, active, stale-lease, and claimable waves
- `Control`
  rerun intents, replay/proof state, and operator actions

The important constraint is that `Queue` and `control status` are not separate truths. They are projections from the same control-plane model. If the queue says a wave is blocked, the status output and the TUI should agree on the blocker until the underlying state changes.

The current cutover is covered in-tree by projection-parity fixtures:

- `cargo test -p wave-projections`
  proves the reducer-backed planning bundle, operator snapshot inputs, and control-status helpers come from one `ProjectionSpine`
- `cargo test -p wave-app-server`
  proves the transport snapshot carries the projection-owned queue rows and control-status payload without re-deriving them, including active/blocked/completed row labels from the same readiness state the reducer emitted
- `cargo test -p wave-tui`
  proves queue rows and control status items render from the app-server snapshot payload, including the active-row case where blocker strings exist but the projection-owned readiness state still says `active`
- `cargo test -p wave-cli`
  proves `wave control status` assembles its JSON report directly from the same projection spine contract

Compatibility-backed launcher inputs are still allowed to feed those reducers in this stage, but dry-run and preflight refusal paths stay read-only with respect to durable run truth:

- `cargo test -p wave-runtime`
  proves dry-run launch and preflight refusal keep rerun intents intact and do not write durable run or trace records that would perturb queue truth
- `cargo test -p wave-trace`
  proves stored compatibility run/trace paths normalize back to the same repo-local state during replay and readback

The proof-lifecycle parity fixtures for Wave 12 are intentionally narrow and shared across surfaces:

- `cargo test -p wave-runtime persist_agent_result_envelope_writes_canonical_result_path`
  proves runtime persistence writes the normalized canonical envelope path under `.wave/state/results/` through the `wave-results` store boundary instead of treating `wave-trace` as the result writer
- `cargo test -p wave-gates compatibility_run_input_prefers_structured_result_envelope_markers`
  proves closure-gate input resolves final-marker truth from the stored envelope before falling back to the explicit `wave-results` compatibility adapter
- `cargo test -p wave-app-server build_run_detail_prefers_structured_result_envelope_for_proof`
  proves app-server proof snapshots recompute from the stored envelope instead of inferring proof only from compatibility run markers, and that proof views still resolve a non-active latest relevant run
- `cargo test -p wave-cli proof_report_falls_back_to_latest_completed_run`
  proves `wave control proof show` resolves the latest relevant run for a wave and preserves the same structured-envelope proof source the app-server snapshot exposes
- `cargo test -p wave-results`
  proves structured closure verdicts can fall back to the owned integration and cont-QA artifacts when the terminal summary is incomplete, and that synthetic marker evidence no longer falsely claims a `last-message.txt` source line

This is especially relevant for the future TUI dependency: the UI should consume the same structured queue/status truth, not re-derive planning state from ad hoc terminal-specific logic.

The right-side operator panel is the built-in dashboard surface in the shipped shell. It renders the repo's runtime truth today, rather than a separate dashboard app or a placeholder for later UI work. Trace and replay state appear there as recorded evidence, not as transient debug output.
In narrow terminals, the shipped shell degrades to a text-summary fallback that shows the same operator snapshot in condensed form instead of attempting to preserve the split-panel layout.

## Closure And Marker Baseline

Wave closure is staged even in this bootstrap slice:

1. implementation agents land owned changes and proof
2. optional `E0` eval runs before integration when the wave declares it
3. optional specialist review such as `A6` design review may run before integration when the wave declares it
4. `A8` integration determines whether the slices reconcile
5. `A9` documentation records the doc closure state
6. `A0` cont-QA makes the final gate decision

Marker ownership is fixed:

- implementation agents emit `[wave-proof]`, `[wave-doc-delta]`, and `[wave-component]`
- optional `A6` emits `[wave-design]`
- optional `E0` emits `[wave-eval]`
- `A8` emits `[wave-integration]`
- `A9` emits `[wave-doc-closure]`
- `A0` emits `[wave-gate]`

Those markers are part of the authored-wave schema and lint contract, not just reporting convention.

## Skill Catalog Baseline

Skills are already first-class authoring inputs in this repo:

- agents attach repo-owned skill ids directly in `### Skills`
- `wave lint` rejects unknown skill ids in authored waves
- `wave doctor` validates the local catalog under `skills/`
- repo-specific bundles carry reusable rules for workspace layout, control-plane behavior, Codex runtime work, TUI work, and marker discipline

Implementation agents should normally attach:

- `wave-core`
- a role skill such as `role-implementation`
- `runtime-codex`
- one narrow repo-specific bundle
- `repo-wave-closure-markers` when final markers are required

Closure agents typically need `wave-core`, their role skill, and `repo-wave-closure-markers`. They should only pull in a subsystem bundle when their closure assignment genuinely depends on that surface.

This keeps wave prompts focused on the concrete assignment while pushing durable repo operating rules into versioned bundles.

The repo-specific bundle split is now part of the practical baseline:

- `repo-rust-workspace` for crate layout, config/spec work, and CLI/bootstrap changes
- `repo-rust-control-plane` for doctor, lint status, queue, readiness, and planning projections
- `repo-codex-orchestrator` for launcher, app-server actions, and project-scoped Codex state
- `repo-ratatui-operator` for right-panel operator behavior and terminal rendering
- `repo-wave-closure-markers` whenever exact marker output matters

## Fail-Closed Expectations

The bootstrap implementation is intentionally fail closed on authoring gaps. Current checks cover:

- missing heading, commit message, deploy environments, component promotions, or Context7 defaults
- missing frontmatter fields such as wave id, dependencies, or validation commands
- missing closure agents `A0`, `A8`, or `A9`
- missing executor, file ownership, final markers, deliverables, components, capabilities, or exit contract sections
- weak prompts that omit required headings or omit final-marker instructions
- prompt/file-ownership mismatches and overlapping ownership between agents
- missing role-prompt files for closure roles
- closure agents claiming implementation markers or implementation agents omitting their marker set
- malformed or unknown skill references
- authored-wave and compiled-prompt drift that would make runtime execution weaker than the checked-in wave spec

For `dark-factory`, the launcher adds one more invariant: a wave that fails preflight does not proceed into mutation. Operators should read that as a hard stop, not a warning path.

If a future wave changes the contract, update the parser, lint rules, queue/status surfaces, and repo guidance in the same slice.

## Command Map In This Slice

- `wave`
  Opens the interactive operator TUI on interactive terminals and falls back to a textual summary otherwise.
- `wave project show [--json]`
  Prints the parsed `wave.toml`, including the canonical authority roots.
- `wave doctor [--json]`
  Verifies config loading, wave parsing, role-prompt paths, canonical authority roots, skill-catalog health, queue state, and upstream metadata presence.
- `wave lint [--json]`
  Validates wave files and dark-factory requirements.
- `wave control status [--json]`
  Shows dependency-driven wave readiness, closure coverage, blocker state, and skill-catalog state.
- `wave control show|task|agent|rerun|close|proof|orchestrator`
  Exposes wave detail, active tasks, MAS agent state, rerun/manual-close state, proof/replay state, and orchestrator detail from the same operator snapshot used by the TUI.
- `wave delivery status [--json]`
  Exposes release, acceptance-package, risk, debt, and delivery-signal truth from the repo-local delivery catalog.
- `wave delivery initiative|release|acceptance show --id <id> [--json]`
  Shows one delivery object in the same projection family as the TUI and app-server.
- `wave draft`
  Compiles a wave bundle and writes per-agent prompt files under `.wave/state/build/specs/`.
- `wave launch [--dry-run]`
  Runs a single ready wave through the Codex-backed launcher.
- `wave autonomous [--dry-run]`
  Runs the current ready queue through the same launcher contract.
- `wave adhoc plan|run|list|show|promote`
  Uses repo-local adhoc state under `.wave/state/adhoc/` and can promote a planned run back into `waves/`.
- `wave trace latest|replay`
  Shows compatibility run and trace outputs and validates replay semantics against stored artifacts in `.wave/traces/runs/`.

One area is still intentionally incomplete:

- `wave dep`

## Self-Host Runbook

This repo now dogfoods the Rust operator on itself through the surfaces that already exist. The intended local loop is:

1. Confirm the repo state and roots with `wave project show --json`.
2. Validate the authoring and control-plane contract with `wave doctor --json`, `wave lint --json`, and `wave control status --json`.
3. Compile the active wave with `wave draft` so the runtime prompt bundle under `.wave/state/build/specs/` matches the checked-in spec.
4. Inspect readiness and queued work with `wave control show --wave <id> --json` and `wave control task list --wave <id> --json`.
5. Run `wave launch --wave <id> --dry-run --json` before any live local mutation.
6. If the dry run is clean, run `wave launch --wave <id> --json`.
7. Watch the attempt through `wave trace latest --json` and `wave trace replay --json`.
8. Open `wave` in an interactive terminal to inspect `Overview`, `Agents`, `Queue`, `Proof`, and `Control` in the built-in Ratatui operator shell.

This is a real self-host loop, but it is local and repo-scoped. It uses the launcher, queue, TUI, and trace surfaces already shipped in this slice; it does not claim live-host deployment or remote fleet control.

Remaining gaps stay explicit:

- `wave dep` is still a stub.
- The operator shell is the built-in TUI, not a separate dashboard product.
- Queue and replay visibility are evidence surfaces, not a guarantee that every future orchestration feature has shipped.

## Live State Roots

The current Rust runtime writes durable local state here:

- `.wave/state/build/specs/`
  Compiled wave bundles and per-agent prompts.
- `.wave/state/events/control/`
  Canonical append-only control-event logs for Wave 0.2 authority state.
- `.wave/state/events/coordination/`
  Canonical append-only coordination records for contradictions, facts, citations, and human-input state.
- `.wave/state/results/`
  Canonical authority root for result-envelope storage and attempt-scoped structured outputs. The live runtime now writes one normalized agent result envelope per attempted agent here through `wave-results`, and keeps the compatibility run record only as the explicit legacy adapter path.
- `.wave/state/derived/`
  Canonical authority root reserved for reducer-backed derived state once later cutover waves stop emitting compatibility-only queue outputs.
- `.wave/state/projections/`
  Canonical authority root reserved for reducer-backed queue, control, and operator projections in later cutover waves.
- `.wave/state/traces/`
  Canonical authority root reserved for canonical trace state, replay v2, and attempt-scoped provenance after the compatibility trace path is retired.
- `.wave/state/runs/`
  Compatibility recorded run output for live and dry-run launches. Proof surfaces and closure gates now treat this as an adapter input when a structured result envelope is missing, but replay still depends on it.
- `.wave/state/control/reruns/`
  Operator-written rerun intents.
- `.wave/traces/runs/`
  Compatibility trace bundles and replay inputs for completed runs until later cutover waves replace them with canonical trace-state readers. These bundles capture the recorded run, normalized artifact paths, and replay inputs that `wave trace replay` checks without mutating runtime state.
- `.wave/codex/`
  Project-scoped Codex auth, config, sqlite state, and session logs. The launcher must not write into the user's global Codex home.

## Launcher Assumptions

The Codex-backed launcher slice depends on a few concrete assumptions that should stay visible in the docs and config:

- compiled prompts already exist under `.wave/state/build/specs/<run-id>/`
- the launcher reads its runtime roots from `wave.toml`
- `CODEX_HOME` and `CODEX_SQLITE_HOME` are both pinned to `.wave/codex/`
- `last-message.txt` is the per-agent terminal artifact for the final assistant message
- the launcher writes a structured result envelope under `.wave/state/results/` for each completed agent attempt and also writes the compatibility run and trace artifacts needed by replay
- the launcher writes that structured result envelope through the `wave-results` boundary, so stored proof state, app-server proof snapshots, and closure-gate input all read the same normalized envelope truth for new runs
- launcher execution is a runtime substrate only; it is not a claim that autonomous queue behavior or any future TUI scheduling logic has shipped in this wave
- preflight refusal is part of the shipped launch contract, so missing requirements should surface before any live mutation begins
- the self-host flow is repo-local dogfood, not live-host mutation or fleet orchestration
- the shipped self-host loop is `project show`, `doctor`, `lint`, `draft`, `launch`, `control`, `trace`, and the built-in TUI on the same repo-scoped state roots
- planning, queue, and control-status projections are reducer-backed read models over compatibility inputs in this wave, while proof and closure surfaces are now envelope-first, legacy proof adaptation is isolated to `wave-results`, and replay remains compatibility-backed evidence rather than a promise that every future orchestration feature has shipped

Keep these assumptions aligned with the launcher code. If one changes, update the config and the reference docs in the same wave.

## Operator Shell

The built-in TUI is now the live operator shell for this repo. It is no longer just a narrow right-side dashboard panel plus a few queue actions.

The shipped shell contract is:

- left side: header, transcript, and composer
- right side: stable `Overview`, `Agents`, `Queue`, `Proof`, and `Control` dashboard
- explicit `head`, `wave`, and `agent` scopes
- operator and autonomous modes on the same durable control path
- reducer/projection-backed queue, proof, autonomy, and recovery visibility
- shell-local transcript search and compare views
- explicit `wave tui --alt-screen auto|always|never` and `wave tui --fresh-session`

The TUI remains a consumer of control-plane truth, not an independent planner. Queue, proof, recovery, and control state still come from the same reducer/projection/app-server path the CLI uses. Replay is still an explicit compatibility boundary here: it compares normalized run, trace, and result-envelope references, but it still ratifies the compatibility run and trace artifacts rather than a final canonical replay v2 surface.

The live interaction model now includes:

- `Tab` / `Shift+Tab` to cycle transcript, composer, and dashboard focus
- `[` / `]` to cycle dashboard tabs
- `j` / `k` or arrows to scroll transcript or move dashboard selection
- `r` / `c` for rerun request and clear
- `m` / `M` for manual close apply and clear
- `u` / `x` for operator-action approval or rejection
- slash commands for scope/mode/launch/rerun/MAS control/search/compare/help

If broader shell behavior is proposed later, it should build on this operator-shell contract rather than describe the TUI as a passive dashboard again.

## Self-Host Evidence

The intended self-host loop for this repository is the same one the code already exposes:

1. `wave project show --json` confirms the workspace-local roots and parsed config.
2. `wave doctor --json`, `wave lint --json`, and `wave control status --json` check the authoring and queue surfaces.
3. `wave draft` compiles the active wave into runtime prompts under `.wave/state/build/specs/`.
4. `wave launch --wave <id> --dry-run --json` writes the preflight report before mutation.
5. `wave launch --wave <id> --json` runs the local operator slice when the dry run is clean.
6. `wave control show --wave <id> --json`, `wave control proof show --wave <id> --json`, `wave control task list --wave <id> --json`, `wave trace latest --json`, and `wave trace replay --json` expose queue, proof, and trace evidence for the latest relevant run. Proof state is recomputed from the current stored result envelopes first, with compatibility run records used only through the explicit `wave-results` legacy adapter while `wave-trace` stays limited to persisted envelope loading and replay over compatibility-backed artifacts with normalized envelope references.
7. `wave` on an interactive terminal shows the same state in the built-in operator shell, and `wave tui --help` exposes the explicit shell startup controls.

This is dogfood evidence, not a claim that live-host deployment, remote fleet control, or a separate dashboard product has landed. Wave 13 is now the landed scheduler-authority and serial lease-enforcement checkpoint. True parallel-wave execution and per-wave worktree isolation remain future work in Wave 14-class follow-through.

## Narrow-Terminal Fallback

When the terminal is too narrow for the split layout, the shell does not try to force the right-side panel into a broken view. It falls back to the same text-summary surface used for non-interactive runs.

That fallback is part of the shipped behavior. It preserves the same control-plane truth by rendering condensed `Run`, `Agents`, `Queue`, and `Control` sections from the same operator snapshot, but it does not expose the full right-side dashboard tabs until there is enough room for the TUI layout.
In other words, narrow terminals get truthful status output, not a degraded split-pane rendering.
