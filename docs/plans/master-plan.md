# Master Plan

## Goals

- Keep `waves/*.md` as the canonical execution contract for the Rust/Codex rewrite.
- Keep the docs explicit about what is live in the Rust repo versus what is target-state architecture or upstream/package reference.
- Keep parser, lint, doctor, queue-status, and shared-plan docs aligned whenever the authored-wave contract changes.
- Keep repo-owned operating rules in `skills/` and repo docs, while reserving Context7 for narrow external-library truth.
- Keep the live repo-local operator/runtime surface honest about what is already executable in this worktree versus what still needs dogfood proof, while recognizing that wave 9 now adds repo-landed self-host evidence on top of trace bundles and replay validation.
- Keep the operator-facing TUI story aligned with the live right-side panel so docs do not split queue/control truth between the shell and CLI surfaces, including direct queue navigation and rerun-intent control.
- Keep `docs/implementation/design.md` as the detailed TUI UX and operator-ergonomics spec for later operator-surface work, so future waves do not stop at broad control-plane surfaces without defining concrete interaction behavior.
- Keep later queue and dogfood waves authored against the fail-closed dark-factory profile, so missing launch contracts are treated as authoring errors rather than execution-time fixes.
- Keep later waves aligned with autonomous queue selection and dependency-aware gating now that wave 7 has landed those scheduler paths in the repo-local runtime and made claimability typed instead of manual.
- Keep the Wave 0.2 cutover honest about the current compatibility boundary, so planning status, queue/control JSON, and operator-facing status truth stay described as reducer-backed over compatibility run records, proof and closure surfaces stay described as envelope-first through `wave-results` for the active run and the latest completed or failed run, and replay ratification stays compatibility-backed until later waves retire the run/trace adapters.
- Fold control-discipline hardening into later waves without reopening the landed scheduler-authority slice: Wave `14` owns true parallel execution plus worktree isolation, Wave `15` owns runtime policy and multi-runtime adapters, Wave `19` owns planner-emitted invariants and staged gate plans, and later waves carry contradiction tracking plus non-authoritative telemetry.
- Keep the intended parallel-wave multi-runtime architecture documented even while richer policy controls and later delivery layers remain unfinished.
- Keep the target harness full-cycle, not implementation-only: design/spec/product loops first, implementation second, and verification/hardening/rollout after, all on the same reducer-backed substrate.
- Treat execution isolation as a hard architectural requirement for true parallel waves: one worktree per active parallel wave, not one worktree per agent and not shared root-workspace mutation across parallel implementation waves.

## Landed Baseline

- Waves `0` through `15` are now code-landed in the current worktree:
  - Wave `0` freezes the rich authored-wave schema, including frontmatter, shared wave sections, structured agent blocks, mandatory closure agents, non-empty skills for every agent, owned-path and deliverable constraints, role-section boundaries, and marker contracts.
  - Wave `1` lands the Rust workspace shape and bootstrap CLI entrypoints.
  - Wave `2` lands `wave.toml`, typed config loading, authored-wave parsing, and dark-factory lint.
  - Wave `3` lands planning-status, queue visibility, and doctor/control projections as the shared baseline for later queue and blocker reasoning.
  - Wave `4` lands the Codex-backed launcher, agent lifecycle manager, app-server snapshot, and project-scoped Codex state roots.
  - Wave `5` lands the right-side TUI operator panel, status tabs, and direct operator visibility into queue/control state. Later waves should treat the shell as an operator control surface with direct queue navigation and rerun-intent actions, not just a read-only status view.
  - Wave `6` lands launch preflight and fail-closed runtime policy.
  - Wave `7` lands autonomous queueing and dependency-aware scheduling over the typed control-plane state.
  - Wave `8` lands recorded trace bundles and replay validation.
  - Wave `9` lands the repo-local self-host runbook and durable dogfood evidence.
  - Wave `10` lands the authority-core domain, durable control and coordination logs, and typed authority roots in `wave.toml`, while leaving queue, blocker, closure, operator, and replay truth on compatibility run records and trace bundles under `.wave/state/runs/` and `.wave/traces/runs/` until the reducer/projection cutover lands.
  - Wave `11` lands the reducer/projection spine: planning status, queue/control JSON, and operator-facing status surfaces now derive from reducer-backed projections over compatibility run records.
  - Wave `12` lands structured result envelopes and proof lifecycle for new runs: runtime persistence flows through `wave-results`, proof and closure surfaces resolve stored envelopes first for the active run and the latest completed or failed run, explicit legacy adapters stay visible for legacy attempts, and replay mismatches compare normalized or semantic envelope references while replay ratification remains compatibility-backed through the run and trace adapters.
  - Wave `13` lands scheduler authority as live serial control-plane truth: readiness is no longer ownership, local claim admission is exclusive under concurrent launchers, and live leases renew and expire through canonical scheduler events while true parallel execution and per-wave worktrees remain future work.
  - Wave `14` lands true parallel-wave execution with one wave-local worktree per active wave, explicit promotion state, fairness, reserved closure capacity, and preemption evidence.
  - Wave `15` lands the runtime-policy and multi-runtime adapter boundary: Codex and Claude are sibling adapters behind one runtime-neutral execution plan, runtime identity and fallback are durable, and runtime-aware skill projection resolves from the wave-local execution root.
- Post-Wave `15` control-plane hardening is now also landed in the current worktree: manual-close overrides, scoped rerun recovery, closure artifact scaffolding, and active-run stall visibility are explicit operator surfaces. A manual close never rewrites a failed latest run into synthetic success; it is separate durable control metadata that later dependency gates may honor.
- This shared-plan landing does not claim Wave `12` cont-QA closure; that final verdict still belongs to `A0`.
- The next executable work on paper is Wave `16`: move above the landed runtime boundary into durable question, assumption, decision, contradiction, human-input, dependency-handshake, and invalidation semantics while keeping the docs and architecture boundary explicit.
- The target architecture should now explicitly absorb the full-cycle wave model: design loops, synthesis gates, implementation packets, and post-implementation hardening all belong in the same harness plan.
- The target architecture should also assume wave-level execution isolation: each active parallel wave owns its own worktree, while agents inside that wave share the same wave-local filesystem view.
- The next implementation-oriented architecture wave is Wave `16`: durable workflow semantics above the landed scheduler and runtime boundary while keeping the repo-local envelope-first proof boundary honest.
- After that cutover, keep later work honest against the repo-landed dogfood evidence and close any future gaps against the same live surface.

## Next Waves

1. Wave `16`: land question, assumption, decision, contradiction, human-input, dependency-handshake, and invalidation semantics as durable workflow state. This is where design-first loops stop leaking back into prose. Live proof for this wave should show an unresolved question or superseded decision reopening or blocking the correct downstream wave and invalidating the right proofs without broad manual judgment.
2. Wave `17`: land the portfolio, release, and acceptance-package layer above waves. Initiatives, milestones, release trains, outcome contracts, rollout readiness, ship decisions, known risks, and outstanding debt should become first-class delivery truth instead of summary prose. Live proof for this wave should show one initiative or release object aggregating multiple waves into a coherent ship or no-ship state.
3. After Wave `17`, add a dedicated TUI and control-plane ergonomics wave that implements `docs/implementation/design.md` directly: keyboard model, operator action lifecycle, blocker triage, per-agent live status, orchestrator approvals, proof drill-down, and multi-wave concurrency visibility should become first-class UX work rather than incidental fallout from backend slices.
4. After that UX wave, continue with richer replay parity, planner-emitted invariants and staged gate plans, telemetry, benchmarks, and any remaining package-parity work, but only once the scheduler, runtime, delivery, isolation, and operator-ergonomics foundations above are proven in repo-local use.
5. As waves execute, correct any gap between the shared-plan story, the code, the live-proof artifacts, and the operator-visible runtime state before promoting any later evidence.

## Planning Rules

- Treat component promotions, deploy environments, Context7 defaults, owned paths, and final markers as contract fields, not planning prose.
- Treat manifest and dependency-edge ownership as contract fields when an architectural seam requires them; do not split a required seam across owners that cannot complete it.
- When a wave changes parser fields, skill semantics, closure order, or marker ownership, update code, repo guidance, shared-plan docs, and the component matrix in the same slice.
- When parser or lint rules change how waves must be authored, update the shared-plan assumptions and the component matrix together so the docs stay aligned with the executable contract.
- Shared-plan docs may record a landing before `A0` runs, but only the `A0` gate closes cont-QA; do not mark that state closed in plan docs ahead of the gate verdict.
- For Wave `11`, only `reducer-state-spine`, `gate-verdict-spine`, `planning-status`, and `queue-json-surface` move to `baseline-proved`; consumers that read those projections keep their own existing maturity until their own cutover waves promote them.
- Later waves may rely on `wave doctor` and `wave control status` reading one typed planning projection for per-wave agent counts, blocker categories, closure totals, queue visibility, and skill-catalog health.
- Later waves may rely on the launcher substrate managing agent execution against project-scoped Codex home and per-agent run artifacts instead of mutating global operator state.
- Do not describe repo-local execution, TUI, autonomous scheduling, or replay behavior as future work when the executable surface is already present in the worktree.
- Do not describe trace-backed replay validation as future work when the executable surface already persists and validates recorded runs locally.
- Do not describe the right-side operator panel or its queue/control tabs as future work when those controls are already visible in the live shell.
- Do not describe operator access to queue, rerun, and planning truth as indirect when the live shell already exposes those tabs and keybindings.
- Do not describe the right-side panel as read-only when it already exposes direct queue navigation and rerun-intent controls.
- Do not describe queue admission as manual-only when autonomous queueing and dependency gating are already part of the typed control-plane flow and claimability decisions come from typed state.
- Do not treat later queue and dogfood waves as if they need a new TUI control plane before they can reason about queue selection or rerun intents.
- Do not describe proof surfaces as active-run-only when the live boundary already resolves envelope-first proof for the active run and the latest completed or failed run, with explicit compatibility adapters only for legacy attempts or replay.
- Do not describe structured result envelopes as the universal reducer/control authority, or replay ratification as fully event/envelope backed, until the later cutover waves actually land; the live boundary today is envelope-first proof/closure for the active run and the latest completed or failed run with compatibility-backed replay.
- Do not describe a manually closed or waived wave as succeeded; keep the failed latest run visible and record the waiver as explicit control-plane metadata if a dependent wave is allowed to proceed.
- Do not describe live-host deployment proof as landed until the matching component reaches `repo-landed` in the component cutover matrix.
- Do not author later queue or dogfood waves as if dark-factory is optional or merely descriptive; once the profile is selected, the wave must already satisfy the launch-time contract that preflight enforces.
- Treat `docs/plans/component-cutover-matrix.json` as the canonical doc-parity declaration for cutover claims in README, current-state, and runtime-reference docs.
- When later waves advance control-plane behavior, require the authored contract to name the architecture sections in scope, the invariants it must preserve, and any staged gate expectations the launcher or closure roles must enforce.
- When the planner or architecture docs describe later waves, include the full-cycle role of the wave: design loop, synthesis gate, implementation slice, or post-implementation hardening/rollout closure.
- When later waves change what operators can see, approve, or control directly, update `docs/implementation/design.md` in the same slice so the detailed TUI UX spec stays aligned with the code and shared-plan story.
