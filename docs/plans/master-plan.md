# Master Plan

## Goals

- Keep `waves/*.md` as the canonical execution contract for the Rust/Codex rewrite.
- Keep parser, lint, doctor, queue-status, and shared-plan docs aligned whenever the authored-wave contract changes.
- Keep repo-owned operating rules in `skills/` and repo docs, while reserving Context7 for narrow external-library truth.
- Keep the live repo-local operator/runtime surface honest about what is already executable in this worktree versus what still needs dogfood proof, while recognizing that wave 9 now adds repo-landed self-host evidence on top of trace bundles and replay validation.
- Keep the operator-facing TUI story aligned with the live right-side panel so docs do not split queue/control truth between the shell and CLI surfaces, including direct queue navigation and rerun-intent control.
- Keep later queue and dogfood waves authored against the fail-closed dark-factory profile, so missing launch contracts are treated as authoring errors rather than execution-time fixes.
- Keep later waves aligned with autonomous queue selection and dependency-aware gating now that wave 7 has landed those scheduler paths in the repo-local runtime and made claimability typed instead of manual.

## Landed Baseline

- Waves `0` through `8` are now code-landed in the current worktree:
  - Wave `0` freezes the rich authored-wave schema, including frontmatter, shared wave sections, structured agent blocks, mandatory closure agents, non-empty skills for every agent, owned-path and deliverable constraints, role-section boundaries, and marker contracts.
  - Wave `1` lands the Rust workspace shape and bootstrap CLI entrypoints.
  - Wave `2` lands `wave.toml`, typed config loading, authored-wave parsing, and dark-factory lint.
  - Wave `3` lands planning-status, queue visibility, and doctor/control projections as the shared baseline for later queue and blocker reasoning.
  - Wave `4` lands the Codex-backed launcher, agent lifecycle manager, app-server snapshot, and project-scoped Codex state roots.
- Wave `5` lands the right-side TUI operator panel, status tabs, and direct operator visibility into queue/control state. Later waves should treat the shell as an operator control surface with direct queue navigation and rerun-intent actions, not just a read-only status view.
  - Wave `6` lands launch preflight and fail-closed runtime policy.
  - Wave `7` lands autonomous queueing and dependency-aware scheduling over the typed control-plane state.
  - Wave `8` lands recorded trace bundles and replay validation.
- The remaining execution goal is to keep later work honest against the repo-landed dogfood evidence and close any future gaps against the same live surface.

## Next Waves

1. Wave `9`: dogfood the Rust/Codex system on this repo and record durable proof.
2. As waves execute, correct any gap between the shared-plan story, the code, and the operator-visible runtime state before promoting any later evidence.

## Planning Rules

- Treat component promotions, deploy environments, Context7 defaults, owned paths, and final markers as contract fields, not planning prose.
- When a wave changes parser fields, skill semantics, closure order, or marker ownership, update code, repo guidance, shared-plan docs, and the component matrix in the same slice.
- When parser or lint rules change how waves must be authored, update the shared-plan assumptions and the component matrix together so the docs stay aligned with the executable contract.
- Later waves may rely on `wave doctor` and `wave control status` reading one typed planning projection for per-wave agent counts, blocker categories, closure totals, queue visibility, and skill-catalog health.
- Later waves may rely on the launcher substrate managing agent execution against project-scoped Codex home and per-agent run artifacts instead of mutating global operator state.
- Do not describe repo-local execution, TUI, autonomous scheduling, or replay behavior as future work when the executable surface is already present in the worktree.
- Do not describe trace-backed replay validation as future work when the executable surface already persists and validates recorded runs locally.
- Do not describe the right-side operator panel or its queue/control tabs as future work when those controls are already visible in the live shell.
- Do not describe operator access to queue, rerun, and planning truth as indirect when the live shell already exposes those tabs and keybindings.
- Do not describe the right-side panel as read-only when it already exposes direct queue navigation and rerun-intent controls.
- Do not describe queue admission as manual-only when autonomous queueing and dependency gating are already part of the typed control-plane flow and claimability decisions come from typed state.
- Do not treat later queue and dogfood waves as if they need a new TUI control plane before they can reason about queue selection or rerun intents.
- Do not describe live-host deployment proof as landed until the matching component reaches `repo-landed` in the component cutover matrix.
- Do not author later queue or dogfood waves as if dark-factory is optional or merely descriptive; once the profile is selected, the wave must already satisfy the launch-time contract that preflight enforces.
