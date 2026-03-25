# Master Plan

## Goals

- Keep `waves/*.md` as the canonical execution contract for the Rust/Codex rewrite.
- Keep the docs explicit about what is live in the Rust repo versus what is target-state architecture or upstream/package reference.
- Keep parser, lint, doctor, queue-status, and shared-plan docs aligned whenever the authored-wave contract changes.
- Keep repo-owned operating rules in `skills/` and repo docs, while reserving Context7 for narrow external-library truth.
- Keep the live repo-local operator/runtime surface honest about what is already executable in this worktree versus what still needs dogfood proof, while recognizing that wave 9 now adds repo-landed self-host evidence on top of trace bundles and replay validation.
- Keep the operator-facing TUI story aligned with the live right-side panel so docs do not split queue/control truth between the shell and CLI surfaces, including direct queue navigation and rerun-intent control.
- Keep later queue and dogfood waves authored against the fail-closed dark-factory profile, so missing launch contracts are treated as authoring errors rather than execution-time fixes.
- Keep later waves aligned with autonomous queue selection and dependency-aware gating now that wave 7 has landed those scheduler paths in the repo-local runtime and made claimability typed instead of manual.
- Keep the Wave 0.2 cutover honest about the current compatibility boundary, so planning status, queue/control JSON, and operator-facing status truth stay described as reducer-backed over compatibility run records, proof and closure surfaces stay described as envelope-first through `wave-results` for the active run and the latest completed or failed run, and replay ratification stays compatibility-backed until later waves retire the run/trace adapters.
- Fold control-discipline hardening into later waves, with Wave `13` owning runtime breakup plus post-agent gates, Wave `14` owning targeted mid-wave checkpoints and retry, Wave `19` owning planner-emitted invariants and staged gate plans, and later waves carrying contradiction tracking plus non-authoritative telemetry.
- Keep the intended parallel-wave multi-runtime architecture documented even while the live Rust runtime remains Codex-only and serial.
- Keep the target harness full-cycle, not implementation-only: design/spec/product loops first, implementation second, and verification/hardening/rollout after, all on the same reducer-backed substrate.

## Landed Baseline

- Waves `0` through `12` are now code-landed in the current worktree:
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
- This shared-plan landing does not claim Wave `12` cont-QA closure; that final verdict still belongs to `A0`.
- The next executable work on paper is to make the docs and architecture boundary explicit: research parity, live-vs-target-state honesty, and a documented scheduler-plus-executor abstraction for later Rust waves.
- The target architecture should now explicitly absorb the full-cycle wave model: design loops, synthesis gates, implementation packets, and post-implementation hardening all belong in the same harness plan.
- The next implementation-oriented architecture wave remains Wave `13`: break up runtime orchestration and add mandatory post-agent gate foundations in the launcher/supervisor path while keeping the repo-local envelope-first proof boundary honest.
- After that cutover, keep later work honest against the repo-landed dogfood evidence and close any future gaps against the same live surface.

## Next Waves

1. Wave `13`: break up runtime orchestration and add mandatory post-agent gate foundations so implementation slices stop, validate, and only then advance.
2. Wave `13` should be framed as runtime breakup plus scheduler foundation, not only as crate splitting, because true parallel waves require durable ownership and late-bound runtime adapters.
3. The architecture and wave planner should model full-cycle work explicitly: spec, architecture, product/design, synthesis, implementation, verification, hardening, and rollout waves should all sit on the same control-plane substrate.
4. Wave `14`: add targeted mid-wave checkpoints plus selective retry so failed slices can be isolated before full closure.
5. Waves `15` through `20`: add contradiction-aware repair loops, replay parity, Wave `19` planner-emitted invariants and staged gate plans, and local-first telemetry over the same authority model.
6. As waves execute, correct any gap between the shared-plan story, the code, and the operator-visible runtime state before promoting any later evidence.

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
- Do not describe live-host deployment proof as landed until the matching component reaches `repo-landed` in the component cutover matrix.
- Do not author later queue or dogfood waves as if dark-factory is optional or merely descriptive; once the profile is selected, the wave must already satisfy the launch-time contract that preflight enforces.
- Treat `docs/plans/component-cutover-matrix.json` as the canonical doc-parity declaration for cutover claims in README, current-state, and runtime-reference docs.
- When later waves advance control-plane behavior, require the authored contract to name the architecture sections in scope, the invariants it must preserve, and any staged gate expectations the launcher or closure roles must enforce.
- When the planner or architecture docs describe later waves, include the full-cycle role of the wave: design loop, synthesis gate, implementation slice, or post-implementation hardening/rollout closure.
