# Current State

## Repo Baseline

- This repository is on the Rust rewrite baseline, not the older npm/package launcher baseline.
- Wave 15 has now landed the runtime-policy and multi-runtime adapter slice: the live runtime boundary in the Rust workspace is runtime-neutral, Codex and Claude are sibling adapters behind that boundary, runtime selection and fallback are explicit records, and runtime-aware skill projection resolves from the wave-local execution root or worktree rather than the repo root.
- The product-factory branch now lands a delivery layer above waves plus repo-local `adhoc` execution: initiatives, releases, acceptance packages, risks, debts, per-wave delivery soft state, machine-facing signal output, shell wrappers, and `wave adhoc` plan or run or list or show or promote are now part of the executable local surface in this worktree.
- The control plane now has an explicit manual-close override and scoped rerun recovery path: closure overrides live under `.wave/state/control/closure-overrides/`, later dependency gates can accept an operator waiver without rewriting a failed latest run into synthetic success, and rerun intents now support `full`, `from-first-incomplete`, `closure-only`, and `promotion-only` scopes.
- `wave.toml` is the project config for the current implementation and is loaded into a typed project-config model.
- `waves/*.md` is the canonical authored-wave source directory and is parsed directly by the Rust crates into typed wave and agent models.
- `wave-domain`, `wave-events`, and `wave-coordination` now define the typed authority-core baseline for task seeds, control events, and coordination records.
- Wave 12 is the current result-envelope and proof-lifecycle landing: planning status, queue/control JSON, app-server status inputs, and TUI queue/control truth now flow through reducer-backed projections over canonical scheduler authority plus compatibility run records, while proof and closure surfaces are envelope-first for the active run and the latest completed or failed run and replay ratification still uses compatibility run and trace artifacts.
- Wave 13's scheduler-authority slice is code-landed, but the current workspace still does not have a canonical completed Wave 13 run: live queue state currently shows Wave 13 `ready` and `claimable` with `latest_run=null`.
- Wave 14's repo-local parallel-wave cutover is code-landed and proof-backed, but the current workspace still only has a dry-run-backed Wave 14 proof run and the wave remains blocked on Wave 13 in canonical queue state.
- Wave 15 is dependency-complete here through an explicit manual-close override after a failed promotion-conflict run, and Wave 17 is now the latest canonically completed wave in this workspace after the delivery-layer landing closed above Wave 16's decision-lineage and human-input substrate.
- Wave 19 remains the planner-emitted invariants plus staged gate-plan design point.
- The repo-local operator/runtime surface now extends through the Codex-backed launcher and agent lifecycle manager, TUI, autonomous scheduling, dependency-aware queue gating, and replay-aware traces.
- The live TUI operator surface includes the right-side panel as the direct queue/control dashboard, not just a passive status view.
- Operators can directly inspect run, agent, queue, control, autonomy, and recovery truth from the shell without switching to a separate CLI status path first, and they can act on queue selection, rerun intents, manual-close recovery, and MAS agent controls in-place.
- The shell now supports transcript search, compare mode, explicit alt-screen policy, and explicit fresh-session startup through `wave tui`.
- The shared MAS dependency contract is now compiled once and reused across lint, domain, and runtime: authored agent dependencies, artifact reads, and barrier-expanded upstreams now mean the same thing in task graphs, fail-closed lint, and runtime readiness.
- `wave dep` remains planned-only. `wave adhoc` and `wave delivery` are now live repo-local command surfaces in this worktree, even while replay ratification remains compatibility-backed.

## Shipped CLI Surface

- `wave`
- `wave tui [--alt-screen auto|always|never] [--fresh-session]`
- `wave project show [--json]`
- `wave doctor [--json]`
- `wave lint [--json]`
- `wave control status [--json]`
- `wave control show|task|agent|rerun|close|proof|orchestrator`
- `wave delivery status [--json]`
- `wave delivery initiative|release|acceptance show --id <id> [--json]`
- `wave draft`
- `wave launch`
- `wave autonomous`
- `wave adhoc plan|run|list|show|promote`
- `wave trace latest|replay`

## Live Runtime Surfaces

- `wave` opens the interactive Ratatui operator shell on an interactive terminal and falls back to a text summary otherwise.
- The right-side panel exposes live `Overview`, `Agents`, `Queue`, `Proof`, and `Control` tabs from the current repo-local Wave state, with `Queue`, `Proof`, and `Control` serving as the operator's direct planning, proof, rerun, and manual-close recovery surfaces through reducer-backed projections over compatibility run records.
- The shell is an operator panel with actionable queue/control affordances, not merely a terminal summary of state.
- The launcher writes compiled prompts under `.wave/state/build/specs/`, wave-scoped worktrees under `.wave/state/worktrees/`, compatibility run state under `.wave/state/runs/`, rerun intents under `.wave/state/control/reruns/`, closure overrides under `.wave/state/control/closure-overrides/`, compatibility trace bundles under `.wave/traces/runs/`, and runtime artifacts such as `runtime-prompt.md`, `runtime-skill-overlay.md`, and `runtime-detail.json` under each agent bundle; project-scoped Codex state remains under `.wave/codex/`.
- The repo-local delivery layer reads `docs/plans/delivery-catalog.json`, projects initiative or release or acceptance-package truth into CLI, TUI, and app-server surfaces, and merges that delivery soft-state overlay back onto per-wave planning status and machine-facing control signals.
- The repo-local adhoc lane now writes planned runs under `.wave/state/adhoc/runs/` and isolated adhoc execution state under `.wave/state/adhoc/runtime/`, with promotion writing a numbered wave back into `waves/`.
- Canonical authority roots now exist under `.wave/state/events/control/`, `.wave/state/events/coordination/`, `.wave/state/events/scheduler/`, `.wave/state/results/`, `.wave/state/derived/`, `.wave/state/projections/`, and `.wave/state/traces/`.
- Planning status, queue visibility, blocker reporting, closure-coverage summaries, and operator queue/control truth are now reducer-backed read models over scheduler claims, leases, budgets, worktree records, promotion records, scheduling records, recovery plans, and control events plus compatibility run records. Operators can now see ready vs claimed vs active vs stale-lease states, wave worktree identity, promotion state, merge blocking, scheduler phase, explicit waiting or preemption reasons, fairness rank, protected closure capacity, preemption evidence, selected runtime, directive delivery method, fallback count, per-agent runtime detail, and recovery-required state through projections and app-server transport, while proof and closure surfaces read `.wave/state/results/` first through `wave-results` for the active run and the latest completed or failed run, explicit legacy adaptation remains visible only through `wave-results` for legacy attempts, `wave-trace` now fail-closes without a stored envelope, and replay ratification still depends on `.wave/state/runs/` plus `.wave/traces/runs/` until the later cutover waves land.
- `wave control show`, app-server transport, and the TUI now surface manual-close override truth, rerun scope, last activity timestamps, and stalled-run hints directly instead of leaving recovery state or live-run health implicit; the TUI can now apply or clear manual-close overrides through confirm-first `m` and `M` actions rather than forcing operators back to the CLI.
- Manual-close application now preserves control-plane integrity instead of relying on best-effort ordering: override application validates or derives repo-relative evidence, clears rerun intent only inside the same locked mutation, and restores the previous rerun or override file state if the override write or audit event append fails.
- Dependency-handshake classification in operator transport is now typed workflow state on `HumanInputRequest`, with a legacy route-name fallback kept only for older records that predate the explicit field.
- The TUI no longer targets only the first actionable approval or escalation on a wave: `[` and `]` now move the selected operator action in the `Control` view before `u` or `x` applies to that selected item.
- Operator-shell targeting is now explicit and honest: plain-text guidance follows the shell target, while wave hotkeys and implicit wave commands act on the visibly selected wave in the dashboard.
- The shell now starts in `Dashboard` focus so documented hotkeys work immediately; free-text guidance requires explicitly moving into `Composer` focus.
- Repo-level `head` scope keeps `Control` as a visible cross-wave review queue, and `u` / `x` act on that selected visible row rather than a hidden per-wave action slot.
- `/follow run|agent|off` now has real behavior: `run` follows the active run wave and current agent, `agent` pins the selected MAS agent, and `off` preserves manual selection and transcript position.
- Narrow terminals no longer degrade to a blind summary surface; the shell now renders as a one-column layout with visible transcript, composer, and dashboard stack.
- The live shell command surface now includes `/rerun [full|from-first-incomplete|closure-only|promotion-only]`, `/pause`, `/resume`, `/rerun-agent`, `/rebase`, `/reconcile`, `/approve-merge`, and `/reject-merge`.
- The launcher contract is project-scoped: it keeps Codex auth, sqlite state, and session logs under `.wave/codex/` and records each agent's final assistant message in the per-run bundle.
- Autonomous queueing, dependency-aware scheduling, and replay validation are live repo-local features on top of the same reducer-backed planning state plus compatibility-backed replay artifacts, so later waves can prove recorded outcomes without needing live-host mutation proof.

## Target-State Boundaries

- True parallel-wave execution is now live in repo-local use for the Codex-backed runtime: by default the scheduler admits up to two non-conflicting waves concurrently, each active wave runs in its own wave-scoped worktree, FIFO fairness now orders claimable implementation admission by persisted waiting time, reserved closure capacity can defer new implementation admission ahead of that lane, and closure work can revoke a saturated implementation lease when the scheduler needs to protect closure progress.
- Wave `18` has now partially landed the intra-wave MAS slice for opt-in `execution_model = "multi-agent"` waves: the repo can now parse MAS-authored waves, persist MAS orchestration records, allocate per-agent sandboxes, compute a MAS ready set, launch parallel-safe agents concurrently, and surface sandbox, merge, invalidation, directive, and orchestrator detail in CLI, app-server, and TUI views.
- In those MAS waves, `reads_artifacts_from` is now a hard upstream dependency for both graph emission and runtime readiness, and `ClosureBarrier` expansion waits on the non-report-only, non-closure frontier instead of creating peer-to-peer closure cycles.
- Wave `18` is still not closed: MAS waves now have a live autonomous-head steering loop, broader durable control actions for pause or resume or rerun or rebase or reconcile or merge approval, reducer-backed recovery-required handling that preserves accepted sibling work, richer directive-delivery semantics, and a finished operator shell product. The remaining gap is one live pilot proof run showing concurrent launch, steering, targeted recovery, and honest closure end to end, plus later head-behavior expansion beyond the current safe action family.
- Durable claims and leases are live authority in the reducer and canonical scheduler event stream, and the current runtime now uses that authority for concurrent claim admission, task-lease renewal and expiry, and wave-scoped execution state.
- Claude is now a live Rust runtime adapter when the local `claude` CLI is available and authenticated; proof classification for a checked-in bundle may still be live, dry-run-backed, or fixture-backed and is recorded in the Wave 15 proof bundle.
- Runtime-aware skill projection is now live, but it remains late-bound and execution-rooted: explicit per-agent `### Skills` still express the semantic contract, and the runtime computes the final projected skill set from the wave-local execution root after final runtime selection and fallback.
- Wave 16 now lands durable question, assumption, decision, contradiction, human-input, dependency-handshake, and selective invalidation state in the live control plane; later waves still own the delivery/package layer above that boundary.
- Per-wave worktree isolation is live now, but it is intentionally wave-scoped rather than per-agent: one worktree per active wave, shared by every agent inside that wave, with promotion or conflict state recorded explicitly before closure and released only after the Git worktree is actually removed.
- MAS waves now sit above that baseline with per-agent sandboxes derived from the wave-local integration head, but the repo should still be read as mid-cutover rather than fully end-state: serial waves continue to use the shared wave worktree model, and the MAS pilot still needs closure proof before the docs can claim a fully landed intra-wave operating model.
- The live scheduler surface now exposes fairness rank, reserved closure capacity, protected closure state, and preemption evidence through reducer-backed projections, but the fairness rule is intentionally narrow to the claimable implementation lane and the later runtime-policy wave still owns richer policy controls and multi-runtime routing.

## Authored-Wave Contract Now Live

- Every active wave starts with a `+++` frontmatter block carrying `id`, `slug`, `title`, `mode`, `owners`, `depends_on`, `validation`, `rollback`, and `proof`.
- Every active wave must also declare a commit message, component promotions, deploy environments, and wave-level Context7 defaults in the markdown body.
- Every active wave must include at least one implementation agent; closure-only waves do not satisfy the current contract.
- Mandatory closure agents are `A0`, `A8`, and `A9`. `E0` remains optional for eval work, and optional specialist reviewers such as `A6` design review may be added when a wave needs report-only closure review beyond the mandatory gates.
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
- Manual close is now explicit operator metadata, not wording-only closure: a waived wave still keeps its failed latest run, but reducer-backed dependency gates and operator surfaces accept the active override record as authoritative completion for downstream readiness.
- The committed authored-wave backlog currently lints cleanly and has complete closure coverage across the wave set.
- Wave 12 shared-plan docs now record the result-envelope and proof-lifecycle landing and keep the remaining replay compatibility boundary explicit; the current workspace still needs canonical Wave 13 and Wave 14 completion state before the scheduler-authority and parallel-wave migrations can be described as a clean linear landed sequence here. Wave 12 cont-QA closure is not claimed here because that final gate still belongs to `A0`.
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

- Closure order is fixed: implementation proof, optional `E0`, optional specialist review such as `A6` design review, `A8` integration, `A9` documentation, then `A0` cont-QA.
- Optional specialist reviewers are report-only by default. They do not absorb implementation ownership, and their structured markers are advisory unless the wave explicitly treats a blocked review as stopping closure.
- Implementation agents emit `[wave-proof]`, `[wave-doc-delta]`, and `[wave-component]`.
- Optional `A6` design review emits `[wave-design]`.
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
- Later queue and dogfood waves should assume the shell already exposes direct queue selection, rerun-intent control, and manual-close override control, so they do not need a separate operator surface to reason about those actions.
- Later queue and dogfood waves should also assume that multi-item approvals and escalations are selectable in-shell and that dependency-handshake semantics arrive as typed workflow state rather than route-substring heuristics.
- Later waves may rely on explicit closure-override metadata and scoped rerun recovery instead of manual state surgery when an already-inspected failed predecessor must be waived or resumed locally.
- Later queue and dogfood waves should also assume dark-factory launch refusal is fail-closed: if the authored contract is incomplete, the wave is malformed and should not be framed as launch-time fixup work.
- Later waves may assume `wave adhoc` and `wave delivery` exist in this worktree. They must not assume `wave dep`, live-host deployment proof, or true intra-wave MAS until Wave `18` lands that pilot slice explicitly.
