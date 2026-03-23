# Migration

## Baseline Reset

- This repo no longer treats the older package-first starter scaffold as the active model.
- The live baseline is the Rust workspace plus rich authored waves under `waves/`.
- Until the runtime and adoption waves land, treat this repository as the reference implementation; do not assume an npm-style install or upgrade flow is the current path.

## Moving Older Waves To The Authored-Wave Contract

1. Move active wave definitions into `waves/*.md` and treat them as canonical execution inputs.
2. Add a `+++` frontmatter block with `id`, `slug`, `title`, `mode`, `owners`, `depends_on`, `validation`, `rollback`, and `proof`.
3. Add a top-level commit message, component promotions, deploy environments, and `## Context7 defaults`.
4. Keep at least one implementation agent in every active wave. Add mandatory closure agents `A0`, `A8`, and `A9`. Add `E0` only when eval work is explicitly in scope.
5. For each implementation agent, declare `### Executor`, `### Context7`, `### Deliverables`, `### File ownership`, `### Skills`, `### Components`, `### Capabilities`, `### Exit contract`, `### Final markers`, and a structured `### Prompt`.
6. For each closure agent, declare `### Role prompts`, `### Executor`, `### Context7`, `### Skills`, `### File ownership`, `### Final markers`, and a structured `### Prompt`.
7. Keep role boundaries strict: implementation agents do not declare `### Role prompts`, and closure agents do not declare implementation-only sections such as deliverables, components, capabilities, or exit contracts.
8. Inside each prompt, restate the exact owned-path list under `File ownership (only touch these paths)`, keep the prompt headings `Primary goal`, `Required context before coding`, and `Specific expectations`, and instruct the agent to emit its final markers as plain last-line output.
9. Keep every deliverable inside its owned-path slice and avoid duplicate owned paths, deliverables, or skill ids inside an agent contract.
10. Register every referenced skill under `skills/<skill-id>/` with both `skill.json` and `SKILL.md`, and keep `### Skills` non-empty for both implementation and closure agents.
11. Update shared-plan docs and the component cutover matrix in the same slice whenever the wave changes parser, skill, closure, promotion, planning-status, or queue-visibility assumptions.
12. When parser or lint rules change how waves are authored, update the typed config/parsing assumptions here as well as the matrix and current-state summaries.

## Validation Checklist

1. `cargo test -p wave-spec -p wave-dark-factory -p wave-control-plane -p wave-cli`
2. `cargo run -p wave-cli -- project show --json`
3. `cargo run -p wave-cli -- doctor --json`
4. `cargo run -p wave-cli -- lint --json`
5. `cargo run -p wave-cli -- control status --json`

## Current Fail-Closed Expectations

- Missing frontmatter metadata, commit message, component promotions, deploy environments, Context7 defaults, or an implementation agent fails lint.
- Missing closure agents `A0`, `A8`, or `A9` fails lint.
- Missing deliverables, components, capabilities, exit contracts, final markers, structured prompt headings, or plain-line marker instructions inside `Specific expectations` fails lint.
- Missing or mismatched owned paths, duplicate owned paths/deliverables/skills, deliverables outside ownership, missing role-prompt files, role-section drift, overlapping ownership, missing skill declarations on any agent, or unknown skills fails lint.
- Missing or malformed skill manifests fails doctor.
- A contract change is not complete until parser, lint, doctor/status surfaces, and shared-plan docs all agree on the same authored-wave model.

## Future Migration Work

- Cross-repo bootstrap automation, richer adoption flows, `wave adhoc`, `wave dep`, and live-host deployment workflows remain future work.
- The repo-local runtime waves are already executable in this worktree, so shared-plan docs should describe launcher, TUI, autonomous queueing, dependency-aware gating, replay, and queue-status projections as live local capabilities.
- Trace bundles plus replay validation mean later closure and dogfood waves can cite durable local evidence for recorded runs without waiting on live-host mutation proof.
- The right-side operator panel is part of that live local capability set and should be described as the direct queue/control dashboard, not a separate dashboard product.
- Later waves should assume the shell can expose operator actions and not just operator visibility when they define queue, rerun, or planning interactions.
- Wave 5 specifically means the shell already supports direct queue selection and rerun-intent control, so later docs should not describe those affordances as prospective.
- Dark-factory waves must already include the launch contract that preflight checks, so queue and dogfood authors should treat missing validation, rollback, proof, or environment detail as a planning failure instead of deferred runtime input.
- Queue and dogfood waves should treat the shell as the primary operator surface for selection and rerun actions, while CLI surfaces stay authoritative for JSON projections and automation.
- Queue admission for autonomous runs is dependency-gated and claimability-aware, so later waves should author against typed queue readiness rather than manual queue promotion.
- Launcher and agent-lifecycle docs should treat `.wave/codex/` plus per-run bundle artifacts as the authoritative runtime-state contract for later waves.
- Wave 9 now lands the self-host dogfood runbook and evidence as part of the repo baseline, so future migration notes should treat that evidence as shipped local state rather than a planned proof slice.
- When later waves change the contract, update this guide together with the parser, lint rules, queue/doctor surfaces, and component matrix.
