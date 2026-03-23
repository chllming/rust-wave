# Current State

## Repo Baseline

- This repository is on the Rust/Codex rewrite baseline, not the older npm/package launcher baseline.
- `wave.toml` is the project config for the current implementation and is loaded into a typed project-config model.
- `waves/*.md` is the canonical authored-wave source directory and is parsed directly by the Rust crates into typed wave and agent models.
- The repo-local operator/runtime surface now extends through the Codex-backed launcher and agent lifecycle manager, TUI, autonomous scheduling, dependency-aware queue gating, and replay-aware traces.
- The live TUI operator surface includes the right-side panel as the direct queue/control dashboard, not just a passive status view.
- Operators can directly inspect run, agent, queue, and control truth from the shell without switching to a separate CLI status path first, and they can act on queue selection and rerun intents in-place.
- Remaining planned-only work is the still-stubbed `wave adhoc` and `wave dep` commands; trace and replay semantics are already landed and dogfood evidence is now repo-landed.

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
- The right-side panel exposes live `Run`, `Agents`, `Queue`, and `Control` tabs from authoritative Wave state, with `Queue` and `Control` serving as the operator's direct planning and rerun surfaces.
- The shell is an operator panel with actionable queue/control affordances, not merely a terminal summary of state.
- The launcher writes compiled prompts under `.wave/build/specs/`, run state under `.wave/state/runs/`, rerun intents under `.wave/state/control/reruns/`, trace bundles under `.wave/traces/runs/`, and project-scoped Codex state under `.wave/codex/`.
- The launcher contract is project-scoped: it keeps Codex auth, sqlite state, and session logs under `.wave/codex/` and records each agent's final assistant message in the per-run bundle.
- Autonomous queueing, dependency-aware scheduling, and replay validation are live repo-local features on top of the same recorded run state, so later waves can prove recorded outcomes without needing live-host mutation proof.

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
- Deliverables must stay inside the owned-path slice, and duplicate owned paths, deliverables, or skill ids are rejected.
- The `Specific expectations` block must explicitly instruct the agent to emit its final markers as plain last-line output.
- Closure agents must point at the correct role prompt files and only emit the marker set they own.

## Validation And Status Surfaces

- `wave lint` rejects missing frontmatter metadata, missing shared wave sections, waves with no implementation agents, missing deliverables/components/capabilities/exit-contract fields, role-section drift between implementation and closure agents, weak prompts, missing plain-line marker instructions, duplicate owned paths/deliverables/skills, deliverables outside ownership, overlapping ownership, missing closure agents, missing role-prompt files, missing skill declarations on any agent, unknown skills, and weak Context7 declarations.
- `wave doctor` verifies config loading, wave loading, skill-catalog health under `skills/`, upstream metadata pins, and the typed planning-status projection used by status reporting.
- `wave control status` exposes queue readiness, per-wave agent counts, closure totals, blocker categories, and skill-catalog health from the same typed wave model that feeds `wave doctor`.
- The committed authored-wave backlog currently lints cleanly and has complete closure coverage across the wave set.
- cont-QA: closed; wave 9 landed the repo-local self-host dogfood loop and its durable evidence, so the remaining gaps are now limited to the still-stubbed `wave adhoc` and `wave dep` commands.
- cont-QA: closed; wave 5 landed direct shell control without changing closure sequencing or planning-status semantics.
- cont-QA: closed; dark-factory is now an enforced execution profile at launch, so later queue and dogfood waves must be authored with complete preflightable contract data before they are considered ready.
- cont-QA: closed; wave 7 added autonomous queue selection and dependency-aware gating, so later waves should assume queue claimability is computed from typed control-plane state rather than operator guesswork.

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
- Later waves may rely on fail-closed lint to require non-empty skills on both implementation and closure agents, keep deliverables inside owned paths, preserve role-section boundaries, and enforce plain-line final-marker instructions before runtime work begins.
- Later waves may rely on `wave doctor` and `wave control status` sharing one typed planning projection for queue readiness, blocker-wave reporting, per-wave agent counts, closure coverage, queue visibility, and skill-catalog health.
- Later waves may rely on the Codex launcher, the right-side TUI panel, direct queue selection, rerun intents, autonomous queueing, dependency-aware gating, and replay validation being live in the repo-local runtime.
- Later waves may rely on autonomous queue claimability being computed from typed dependencies, run state, and rerun intents rather than manual operator arbitration.
- Later waves may rely on trace bundles, replay validation, and the wave 9 dogfood evidence as durable local evidence for recorded outcomes.
- Later queue and dogfood waves should assume the shell already exposes direct queue selection and rerun-intent control, so they do not need a separate operator surface to reason about those actions.
- Later queue and dogfood waves should also assume dark-factory launch refusal is fail-closed: if the authored contract is incomplete, the wave is malformed and should not be framed as launch-time fixup work.
- Later waves must not assume `wave adhoc`, `wave dep`, or live-host deployment proof until those slices are explicitly landed.
- cont-QA: closed; wave 7 added autonomous queue selection and dependency-aware gating, so later waves should treat queue claimability as a typed runtime decision in repo-local execution.
- cont-QA: closed; wave 8 added trace bundles and replay validation, so later closure waves can use durable local artifacts to prove recorded behavior before dogfood evidence exists.
