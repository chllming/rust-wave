# Rust/Codex Refactor Baseline

This repository now has a working local operator/runtime slice of the refactor:

- a Rust workspace with the target crate layout
- `wave.toml` as the new project config
- `waves/` as the new rich authored-wave source directory
- a compileable `wave` CLI and interactive Ratatui shell
- repo-specific skills for Rust workspace, control-plane, Codex runtime, TUI, and closure-marker work
- pinned and vendored upstream baselines for Codex OSS and the Wave control-plane docs branch
- live Codex-backed launcher state under the repo-local `.wave/codex/`
- recorded run state under `.wave/state/runs/`
- replay-aware trace bundles under `.wave/traces/runs/`

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
- non-closure agents must declare skills, components, capabilities, deliverables, exit contracts, and the implementation marker set
- closure agents must include the correct role-prompt files and only the markers they own
- authored waves are expected to be strong enough that `wave draft`, `wave lint`, `wave doctor`, and queue status all describe the same task model

The practical authoring shape for future waves is:

1. frontmatter for repo-level sequencing and validation
2. shared markdown sections for commit, components, deploy target, and Context7 defaults
3. closure-agent sections that declare role prompts, owned closure artifacts, and exactly one closure marker family
4. implementation-agent sections that declare executor, skills, subsystem identity, exit contract, owned files, and the implementation marker set
5. a prompt block that restates owned paths and gives coding instructions in the required headings the linter knows how to read

Treat that shape as an executable schema, not a docs convention. Future waves should be authored as if the parser, linter, and operator shell are all first-order consumers of the same markdown.

## Wave Lifecycle Through The Toolchain

The current bootstrap already uses one authored-wave contract across authoring, validation, and runtime surfaces:

1. `waves/*.md` provides the canonical human-authored spec
2. `wave-spec` parses the rich markdown into typed wave and agent models
3. `wave-dark-factory` rejects underspecified waves, ownership drift, weak prompts, marker drift, and bad skill references
4. `wave-control-plane` and `wave doctor` project closure coverage, skill-catalog health, and queue readiness from the same typed model
5. `wave draft` writes compiled per-agent prompts under `.wave/state/build/specs/`
6. `wave launch` and `wave autonomous` write `preflight.json`, refuse closed on missing requirements, and only then use the compiled prompts with the project-scoped Codex runtime under `.wave/codex/`
7. `wave-app-server` and `wave-tui` render operator state from authored waves, lint findings, rerun intents, recorded runs, and replay state

That end-to-end flow is why repo guidance must stay practical and synchronized with enforcement. If the docs describe a looser model than the toolchain accepts, operators will author broken waves.

That same model feeds the operator surfaces today:

- `wave-spec`
  parses the rich markdown structure into typed wave and agent models
- `wave-dark-factory`
  rejects underspecified or inconsistent authored waves
- `wave-control-plane`
  computes queue readiness, closure coverage, and missing-agent status from the typed wave model
- `wave-cli`
  exposes the model through `wave`, `wave doctor`, `wave lint`, `wave control ...`, `wave launch`, `wave autonomous`, and `wave trace ...`
- `wave-app-server`
  builds authoritative operator snapshots from waves, lint findings, recorded runs, rerun intents, and replay state
- `wave-tui`
  renders the right-side operator panel for `Run`, `Agents`, `Queue`, and `Control`

Trace data is part of the same operator truth. `wave trace latest` surfaces the durable record for each completed wave, including the trace path and replay verdict, and `wave trace replay` validates that the stored trace bundle or legacy run record still matches the live run state and artifact inventory.

The repo guidance docs should therefore describe the same concrete contract that these crates enforce, not a looser future-state summary.

## Planning Status Model

Planning status is now a first-class control-plane concern in this refactor. The documented status surface should stay aligned with the queue and operator snapshot rather than with any UI-specific interpretation.

The current status model should be read as:

- `Run`
  the active wave, run id, elapsed time, proof counts, and declared proof artifacts
- `Agents`
  per-agent state, proof or marker completeness, and deliverables
- `Queue`
  readiness, blockers, dependency-driven ordering, and whether a wave is claimable
- `Control`
  rerun intents, replay/proof state, and operator actions

The important constraint is that `Queue` and `control status` are not separate truths. They are projections from the same control-plane model. If the queue says a wave is blocked, the status output and the TUI should agree on the blocker until the underlying state changes.

This is especially relevant for the future TUI dependency: the UI should consume the same structured queue/status truth, not re-derive planning state from ad hoc terminal-specific logic.

The right-side operator panel is the built-in dashboard surface in the shipped shell. It is the place where the repo's runtime truth is rendered today, rather than a separate dashboard app or a placeholder for later UI work. Trace and replay state appear there as recorded evidence, not as transient debug output.
In narrow terminals, the shipped shell degrades to a text-summary fallback that shows the same operator snapshot in condensed form instead of attempting to preserve the split-panel layout.

## Closure And Marker Baseline

Wave closure is staged even in this bootstrap slice:

1. implementation agents land owned changes and proof
2. optional `E0` eval runs before integration when the wave declares it
3. `A8` integration determines whether the slices reconcile
4. `A9` documentation records the doc closure state
5. `A0` cont-QA makes the final gate decision

Marker ownership is fixed:

- implementation agents emit `[wave-proof]`, `[wave-doc-delta]`, and `[wave-component]`
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
  Prints the parsed `wave.toml`.
- `wave doctor [--json]`
  Verifies config loading, wave parsing, skill-catalog health, queue state, and upstream metadata presence.
- `wave lint [--json]`
  Validates wave files and dark-factory requirements.
- `wave control status [--json]`
  Shows dependency-driven wave readiness, closure coverage, blocker state, and skill-catalog state.
- `wave control show|task|rerun|proof`
  Exposes wave detail, active tasks, rerun intents, and proof/replay state from the same operator snapshot used by the TUI.
- `wave draft`
  Compiles a wave bundle and writes per-agent prompt files under `.wave/state/build/specs/`.
- `wave launch [--dry-run]`
  Runs a single ready wave through the Codex-backed launcher.
- `wave autonomous [--dry-run]`
  Runs the current ready queue through the same launcher contract.
- `wave trace latest|replay`
  Shows recorded run state and validates replay semantics against stored artifacts in `.wave/traces/runs/`.

Two areas are still intentionally incomplete:

- `wave adhoc`
- `wave dep`

And the operator shell should still be treated as planning-only where it depends on bootstrap state:

- it may render the current queue/status truth
- it does not replace the underlying control-plane model
- it should not be documented as if the live runtime has already gained every future dashboard affordance

## Live State Roots

The current Rust runtime writes durable local state here:

- `.wave/state/build/specs/`
  Compiled wave bundles and per-agent prompts.
- `.wave/state/runs/`
  Recorded run state for live and dry-run launches.
- `.wave/state/control/reruns/`
  Operator-written rerun intents.
- `.wave/traces/runs/`
  Stored trace bundles and replay inputs for completed runs. These bundles capture the recorded run, agent artifact presence, and replay inputs that `wave trace replay` checks without mutating runtime state.
- `.wave/codex/`
  Project-scoped Codex auth, config, sqlite state, and session logs. The launcher must not write into the user's global Codex home.

## Launcher Assumptions

The Codex-backed launcher slice depends on a few concrete assumptions that should stay visible in the docs and config:

- compiled prompts already exist under `.wave/state/build/specs/<run-id>/`
- the launcher reads its runtime roots from `wave.toml`
- `CODEX_HOME` and `CODEX_SQLITE_HOME` are both pinned to `.wave/codex/`
- `last-message.txt` is the per-agent terminal artifact for the final assistant message
- launcher execution is a runtime substrate only; it is not a claim that autonomous queue behavior or any future TUI scheduling logic has shipped in this wave
- preflight refusal is part of the shipped launch contract, so missing requirements should surface before any live mutation begins

Keep these assumptions aligned with the launcher code. If one changes, update the config and the reference docs in the same wave.

## Operator Shell

The built-in TUI is the current dashboard surface for this repo. It uses a right-side operator panel instead of a separate browser/dashboard app, and the non-interactive fallback is a text summary rather than a second dashboard mode.
This panel is the shipped operator surface, not a future affordance.

The live right-side panel contract is intentionally narrow:

- `Run`
  active wave, run id, elapsed time, proof counts, and declared proof artifacts
- `Agents`
  per-agent state, marker completeness, and deliverables
- `Queue`
  readiness, blockers, dependency state, and claimability
- `Control`
  rerun intents, replay/proof state, and operator keybindings

Only the tab-switching, wave-navigation, rerun-intent, and quit bindings are live today. Other dashboard affordances mentioned in older docs should be treated as planned until the TUI actually ships them.

The TUI is a consumer of control-plane truth, not an independent planner. Any future terminal-surface changes should preserve that dependency so the queue view, `wave control status`, and replay/proof surfaces remain consistent.
When there are multiple active runs, the `Run`, `Agents`, and `Control` tabs should bind to the currently selected wave rather than drifting to an unrelated first-active-run snapshot.

Only these actions are shipped today:

- `Tab` / `Shift+Tab` to cycle tabs
- `j` / `k` to move the selected wave
- `r` to request a rerun for the selected wave
- `c` to clear a rerun intent
- `q` to quit

If a broader dashboard interaction is proposed later, it should be documented as planned until the implementation lands.

## Narrow-Terminal Fallback

When the terminal is too narrow for the split layout, the shell does not try to force the right-side panel into a broken view. It falls back to the same text-summary surface used for non-interactive runs.

That fallback is part of the shipped behavior. It preserves the same control-plane truth by rendering condensed `Run`, `Agents`, `Queue`, and `Control` sections from the same operator snapshot, but it does not expose the full right-side dashboard tabs until there is enough room for the TUI layout.
In other words, narrow terminals get truthful status output, not a degraded split-pane rendering.
