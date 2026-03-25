# Current State

## Repo Baseline

- This repository is on the Rust/Codex rewrite baseline, not the older npm/package launcher baseline.
- The only live runtime in the Rust workspace today is Codex. Multi-runtime material elsewhere in the docs should be read as target-state or upstream reference unless this file says it is live.
- `wave.toml` is the project config for the current implementation and is loaded into a typed project-config model.
- `waves/*.md` is the canonical authored-wave source directory and is parsed directly by the Rust crates into typed wave and agent models.
- `wave-domain`, `wave-events`, and `wave-coordination` now define the typed authority-core baseline for task seeds, control events, and coordination records.
- Wave 12 is the current result-envelope and proof-lifecycle landing: planning status, queue/control JSON, app-server status inputs, and TUI queue/control truth now flow through reducer-backed projections over canonical scheduler authority plus compatibility run records, while proof and closure surfaces are envelope-first for the active run and the latest completed or failed run and replay ratification still uses compatibility run and trace artifacts.
- The next executable architecture work is now the post-authority follow-through: scheduler authority is landed, so Wave 13-class work moves to true parallel admission and execution mechanics rather than paper-only scheduler modeling.
- Wave 14 remains the targeted mid-wave checkpoint and retry design point, and Wave 19 remains the planner-emitted invariants plus staged gate-plan design point.
- The repo-local operator/runtime surface now extends through the Codex-backed launcher and agent lifecycle manager, TUI, autonomous scheduling, dependency-aware queue gating, and replay-aware traces.
- The live TUI operator surface includes the right-side panel as the direct queue/control dashboard, not just a passive status view.
- Operators can directly inspect run, agent, queue, and control truth from the shell without switching to a separate CLI status path first, and they can act on queue selection and rerun intents in-place.
- `wave adhoc` and `wave dep` remain planned-only command surfaces; the rest of the repo-local operator slice described here is live, even while replay ratification remains compatibility-backed.

## Shipped CLI Surface

- `wave`
- `wave project show [--json]`
- `wave doctor [--json]`
- `wave lint [--json]`
- `wave control status [--json]`
- `wave control show|task|rerun|proof`
- `wave draft`
- `wave launch`
- `wave autonomous`
- `wave trace latest|replay`

## Live Runtime Surfaces

- `wave` opens the interactive Ratatui operator shell on an interactive terminal and falls back to a text summary otherwise.
- The right-side panel exposes live `Run`, `Agents`, `Queue`, and `Control` tabs from the current repo-local Wave state, with `Queue` and `Control` serving as the operator's direct planning and rerun surfaces through reducer-backed projections over compatibility run records.
- The shell is an operator panel with actionable queue/control affordances, not merely a terminal summary of state.
- The launcher writes compiled prompts under `.wave/state/build/specs/`, compatibility run state under `.wave/state/runs/`, rerun intents under `.wave/state/control/reruns/`, compatibility trace bundles under `.wave/traces/runs/`, and project-scoped Codex state under `.wave/codex/`.
- Canonical authority roots now exist under `.wave/state/events/control/`, `.wave/state/events/coordination/`, `.wave/state/events/scheduler/`, `.wave/state/results/`, `.wave/state/derived/`, `.wave/state/projections/`, and `.wave/state/traces/`.
- Planning status, queue visibility, blocker reporting, closure-coverage summaries, and operator queue/control truth are now reducer-backed read models over scheduler claims, leases, and budgets plus compatibility run records. Operators can now see ready vs claimed vs active vs stale-lease states through projections, while proof and closure surfaces read `.wave/state/results/` first through `wave-results` for the active run and the latest completed or failed run, explicit legacy adaptation remains visible only through `wave-results` for legacy attempts, `wave-trace` now fail-closes without a stored envelope, and replay ratification still depends on `.wave/state/runs/` plus `.wave/traces/runs/` until the later cutover waves land.
- The launcher contract is project-scoped: it keeps Codex auth, sqlite state, and session logs under `.wave/codex/` and records each agent's final assistant message in the per-run bundle.
- Autonomous queueing, dependency-aware scheduling, and replay validation are live repo-local features on top of the same reducer-backed planning state plus compatibility-backed replay artifacts, so later waves can prove recorded outcomes without needing live-host mutation proof.

## Target-State Boundaries

- True parallel waves are not live yet. The current runtime still executes one selected wave at a time and one agent at a time inside that wave.
- Durable claims and leases are now live authority in the reducer and canonical scheduler event stream, but they do not yet power true parallel execution. The current runtime still uses them to describe serial admission truth honestly.
- Claude is not a live Rust runtime yet. Any Claude runtime docs in this repo should be read as target-state/reference until the Rust executor boundary actually lands.
- Runtime-aware skill projection is still target-state. The live Rust surface still relies on explicit per-agent `### Skills`.
- Contradiction-aware closure and human-input workflow state exist in the domain model and docs direction, but they are not yet the dominant live execution path.
- Per-wave worktree isolation is not live yet. One worktree per active parallel wave remains the target isolation model for the later parallel-runtime phase.

## Authored-Wave Contract Now Live

- Every active wave starts with a `+++` frontmatter block carrying `id`, `slug`, `title`, `mode`, `owners`, `depends_on`, `validation`, `rollback`, and `proof`.
- Every active wave must also declare a commit message, component promotions, deploy environments, and wave-level Context7 defaults in the markdown body.
- Every active wave must include at least one implementation agent; closure-only waves do not satisfy the current contract.
- Mandatory closure agents are `A0`, `A8`, and `A9`. `E0` is optional and only belongs in waves that explicitly need eval work.
- Implementation agents must declare `### Executor`, `### Context7`, `### Deliverables`, `### File ownership`, `### Skills`, `### Components`, `### Capabilities`, `### Exit contract`, `### Final markers`, and a structured `### Prompt`.
- Closure agents stay lighter but must still declare `### Role prompts`, `### Executor`, `### Context7`, `### Skills`, `### File ownership`, `### Final markers`, and a structured `### Prompt`.
- `### Skills` is required for every agent, including `A0`, `A8`, and `A9`; `wave lint` now fails closed on empty closure-agent skill lists as well as unknown ids.
- Implementation agents may not declare closure-only `### Role prompts`, and closure agents may not declare implementation-only sections such as deliverables, components, capabilities, or exit contracts.
- The `### Prompt` must include `Primary goal`, `Required context before coding`, `Specific expectations`, and `File ownership (only touch these paths)`.
- The owned-path list inside the prompt must restate the same paths declared in `### File ownership`.
- If an architectural seam requires manifest or dependency-edge edits, those manifest files must be in the same implementation agent's ownership slice.
- Deliverables must stay inside the owned-path slice, and duplicate owned paths, deliverables, or skill ids are rejected.
- The `Specific expectations` block must explicitly instruct the agent to emit its final markers as plain last-line output.
- Marker success is only valid when the owned architectural seam is actually closed; ownership handoff notes do not substitute for landed proof.
- Closure agents must point at the correct role prompt files and only emit the marker set they own.

## Validation And Status Surfaces

- `wave lint` rejects missing frontmatter metadata, missing shared wave sections, waves with no implementation agents, missing deliverables/components/capabilities/exit-contract fields, role-section drift between implementation and closure agents, weak prompts, missing plain-line marker instructions, duplicate owned paths/deliverables/skills, deliverables outside ownership, overlapping ownership, missing closure agents, missing role-prompt files, missing skill declarations on any agent, unknown skills, and weak Context7 declarations.
- `wave doctor` verifies config loading, wave loading, configured role-prompt paths, canonical authority roots under `.wave/state/`, skill-catalog health under `skills/`, upstream metadata pins, and the typed planning-status projection used by status reporting.
- `wave control status` exposes queue readiness, per-wave agent counts, closure totals, blocker categories, and skill-catalog health from the same reducer-backed planning projection that feeds `wave doctor`; compatibility run records remain adapter inputs at this stage.
- `wave control proof show` and app-server proof snapshots now resolve stored result envelopes first for the active run or the latest completed or failed run; explicit `compatibility-adapter` fallback remains only through `wave-results` for legacy attempts, and replay ratification stays on compatibility artifacts.
- The committed authored-wave backlog currently lints cleanly and has complete closure coverage across the wave set.
- Wave 12 shared-plan docs now record the result-envelope and proof-lifecycle landing, keep the remaining replay compatibility boundary explicit, move the next migration to Wave 13 runtime breakup plus post-agent gates, and keep Wave 14 checkpoint/retry plus Wave 19 planner invariants and staged gate plans explicit; Wave 12 cont-QA closure is not claimed here because that final gate still belongs to `A0`.
- Wave 9's repo-local self-host dogfood loop and durable evidence remain baseline proof surfaces; Wave 11 does not reopen that proof slice.
- Wave 5's direct shell control remains baseline behavior without changing closure sequencing or planning-status semantics.
- Dark-factory remains an enforced execution profile at launch, so later queue and dogfood waves must be authored with complete preflightable contract data before they are considered ready.
- Wave 7's autonomous queue selection and dependency-aware gating remain baseline assumptions, so later waves should assume queue claimability is computed from typed control-plane state rather than operator guesswork.

## Skills And Context7

- Skills are repo-owned bundles under `skills/<skill-id>/` and require both `skill.json` and `SKILL.md`.
- `wave lint` validates skill references inside `waves/*.md`.
- `wave doctor` validates the local skill catalog itself.
- The current repo-specific bundles cover workspace layout, control-plane behavior, Codex runtime work, TUI work, and closure-marker discipline.
- Context7 defaults are required in each wave, but Context7 remains non-canonical external context; repository code and docs stay authoritative.

## Closure And Marker Baseline

- Closure order is fixed: implementation proof, optional `E0`, `A8` integration, `A9` documentation, then `A0` cont-QA.
- Implementation agents emit `[wave-proof]`, `[wave-doc-delta]`, and `[wave-component]`.
- Optional `E0` emits `[wave-eval]`.
- `A8` emits `[wave-integration]`.
- `A9` emits `[wave-doc-closure]`.
- `A0` emits `[wave-gate]`.
- Those markers are part of the authored-wave schema and lint contract, not a reporting convention.

## Safe Assumptions For Later Waves

- Later waves may rely on typed parsing of frontmatter, shared wave sections, skills, components, capabilities, exit contracts, closure agents, prompt-owned-path restatements, and final markers from markdown.
- Later waves may rely on the authority-core crates and canonical state roots already existing in the repo config, including scheduler claims, task leases, and scheduler-budget events, even while queue and replay still consume compatibility run records and trace bundles.
- Later waves may rely on Wave 10 having already moved the project contract onto typed authority roots and shared authority-domain types before the reducer cutover begins.
- Later waves may rely on fail-closed lint to require non-empty skills on both implementation and closure agents, keep deliverables inside owned paths, preserve role-section boundaries, and enforce plain-line final-marker instructions before runtime work begins.
- Later waves may rely on `wave doctor` and `wave control status` sharing one reducer-backed planning projection for queue readiness, claim ownership, lease visibility, blocker-wave reporting, per-wave agent counts, closure coverage, queue visibility, and skill-catalog health, while still treating compatibility run records as queue/control adapter inputs until canonical attempt and result state replaces them.
- Later waves may rely on the Codex launcher, the right-side TUI panel, direct queue selection, rerun intents, autonomous queueing, dependency-aware gating, and replay validation being live in the repo-local runtime, while treating proof and closure as envelope-first for the active run and the latest completed or failed run and replay ratification as compatibility-backed until the later cutover waves land.
- Later waves may rely on autonomous queue claimability being computed from typed dependencies, run state, and rerun intents rather than manual operator arbitration.
- Later waves may rely on trace bundles, replay validation, and the wave 9 dogfood evidence as durable local evidence for recorded outcomes.
- Later queue and dogfood waves should assume the shell already exposes direct queue selection and rerun-intent control, so they do not need a separate operator surface to reason about those actions.
- Later queue and dogfood waves should also assume dark-factory launch refusal is fail-closed: if the authored contract is incomplete, the wave is malformed and should not be framed as launch-time fixup work.
- Later waves must not assume `wave adhoc`, `wave dep`, or live-host deployment proof until those slices are explicitly landed.
