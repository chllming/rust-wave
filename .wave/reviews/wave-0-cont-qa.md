# Wave 0 cont-QA

## Result

FAIL. The rich authored-wave contract itself is landed and verified, but the repo still presents two incompatible operator stories, so this wave cannot be called honestly closed at `repo-landed`.

## Blocking findings

1. Shared-plan docs still describe launcher, TUI, autonomous, and trace/replay surfaces as future or planned-only work. `docs/plans/current-state.md:8,56` says the live surface is only the bootstrap CLI plus authored-wave validation and queue visibility, while `docs/plans/component-cutover-matrix.md:29-42,60-62` keeps launcher, TUI, autonomous, trace, and replay components at `contract-frozen` and says they must not be treated as shipped. `docs/plans/component-cutover-matrix.json` mirrors the same levels. That conflicts with `README.md:18-29,61-76,152-176,195`, `agents.md:12-16,42-45`, and `docs/implementation/rust-codex-refactor.md:3-13,22-28,49-64,111-175`, all of which describe those surfaces as live today. The executable surface matches the live-story docs, not the shared-plan docs: `crates/wave-cli/src/main.rs:50-95,244-342` exposes `launch`, `autonomous`, `trace`, `draft`, and the richer `control` actions; `crates/wave-runtime/src/lib.rs:252-459` implements draft, launch, and autonomous execution with run-state and trace writes; `crates/wave-tui/src/lib.rs:92-155,382-538` implements the interactive right-side panel with `Run`, `Agents`, `Queue`, and `Control` tabs plus `q`/`Tab`/`Shift+Tab`/`j`/`k`/`r`/`c`; `crates/wave-trace/src/lib.rs:163-220` validates replay state. Because this wave explicitly requires parser, linter, control status, and operator guidance to agree on the same model, that documentation split blocks PASS.
2. The recorded documentation closure claim is therefore not honest as written. `cargo run -p wave-cli -- control show --wave 0 --json` reports `A9` closed with `[wave-doc-closure] ... detail=shared-plan docs now match authored-wave baseline`, but the owned shared-plan docs above still diverge from the repo's own current guidance and executable surface.

## Verified positives

- `cargo run -p wave-cli -- lint --json` returned `[]`.
- `cargo run -p wave-cli -- doctor --json` and `cargo run -p wave-cli -- control status --json` matched exactly: 10 waves, complete closure coverage, 0 skill-catalog issues, and 0 lint-error waves.
- `cargo test -p wave-spec -p wave-dark-factory -p wave-control-plane -p wave-cli` passed.
- The authored-wave parser, linter, and control-status surfaces do agree on the rich schema itself: mandatory closure agents, Context7 requirements, skill validation, prompt sections, owned-path restatement, and marker expectations.

## Required follow-up

- Align `docs/plans/current-state.md`, `docs/plans/component-cutover-matrix.md`, and `docs/plans/component-cutover-matrix.json` with the same current-state claims already made in `README.md`, `agents.md`, `docs/implementation/rust-codex-refactor.md`, and the live CLI/runtime code, or roll the higher-level runtime claims back so every operator-facing surface tells the same story.
- Re-run `cargo run -p wave-cli -- doctor --json` and `cargo run -p wave-cli -- control status --json` after that doc fix, then re-open cont-QA.

[wave-gate] architecture=pass integration=concerns durability=pass live=blocked docs=blocked detail=shared-plan-docs-contradict-live-runtime-claims
Verdict: FAIL

## 2026-03-23 Recheck

## Result

BLOCKED. The earlier shared-plan/runtime-story split is now resolved, but Wave 0 still does not honestly land at `repo-landed` because the authored-wave model is inconsistent on closure-agent skill requirements.

## Resolved since the prior report

- The shared-plan docs now match the live repo-local runtime story. `docs/plans/current-state.md:5-72`, `docs/plans/master-plan.md:5-32`, `docs/plans/migration.md:5-42`, `docs/plans/component-cutover-matrix.md:21-42`, and `docs/plans/component-cutover-matrix.json` now align with `README.md`, `agents.md`, `docs/implementation/rust-codex-refactor.md`, and the executable CLI/runtime surface.
- `A9`'s closure note is now honest. `.wave/build/specs/wave-00-1774255193343/agents/A9/last-message.txt` closes the exact shared-plan paths it owns, and those paths now match the repo-local baseline it claims.

## Blocking finding

1. Closure-agent skill resolution is still weaker than the authored-wave contract the repo claims to enforce. The parser models `skills` for every agent (`crates/wave-spec/src/lib.rs:92-105`, `crates/wave-spec/src/lib.rs:382-397`), and repo guidance still describes closure agents as carrying `### Skills` (`README.md:117-125`, `docs/plans/current-state.md:35-40`, `waves/00-architecture-baseline.md:40-43`, `waves/00-architecture-baseline.md:62-65`). But the linter only rejects empty skills for implementation agents: it validates unknown skill ids for all agents, then returns early for closure agents before the empty-skills check (`crates/wave-dark-factory/src/lib.rs:390-405`). `docs/reference/skills.md:92-99` matches that weaker enforcement by saying only implementation agents must declare skills at all. Because `wave-control-plane` and the CLI status surfaces only project lint findings plus skill-catalog health (`crates/wave-control-plane/src/lib.rs:294-307`, `crates/wave-control-plane/src/lib.rs:370-455`, `crates/wave-cli/src/main.rs:445-547`, `crates/wave-cli/src/main.rs:658-719`), `wave doctor --json` and `wave control status --json` both stay green even if a closure agent drops its `### Skills` section. Wave 0 explicitly says to treat weak skill resolution as blocking, so this mismatch still blocks closure.

## Verified positives

- `cargo run -q -p wave-cli -- lint --json` returned `[]`.
- `cargo run -q -p wave-cli -- doctor --json` returned `ok: true` with `10` waves, `60` agents, `0` lint-error waves, complete closure coverage, and `0` skill-catalog issues.
- `cargo run -q -p wave-cli -- control status --json` matched the doctor projection exactly.
- `cargo run -q -p wave-cli -- control show --wave 0 --json` shows the current run `wave-00-1774255193343` at `A0`, with `A1`, `A2`, `A3`, `A8`, and `A9` succeeded and the expected implementation/integration/doc-closure markers observed.
- `cargo test -q -p wave-spec -p wave-dark-factory -p wave-control-plane -p wave-cli` passed.
- `jq empty docs/plans/component-cutover-matrix.json` passed.

## Required follow-up

- Decide the intended contract for closure-agent skills and make every surface match it in the same slice.
- If closure-agent skills are truly required, add a fail-closed lint rule for empty `agent.skills` on closure agents and add coverage proving `wave lint`, `wave doctor`, and `wave control status` surface that gap.
- If closure-agent skills are optional, roll back the stronger wording in `README.md`, `docs/plans/current-state.md`, and any other guidance that currently presents them as mandatory contract fields.

[wave-gate] architecture=blocked integration=blocked durability=concerns live=pass docs=blocked detail=closure-agent-skill-requirement-not-unified
Verdict: BLOCKED

## 2026-03-23 Final rerun

## Result

PASS. This supersedes the earlier FAIL and BLOCKED entries. The authored-wave parser, fail-closed lint, doctor/control projection, and operator guidance now describe and enforce the same rich authored-wave model, including closure-agent skill requirements, so Wave 0 honestly lands at `repo-landed`.

## Resolved since the prior report

- Wave `0` still requires cont-QA to block weak skill resolution (`waves/00-architecture-baseline.md:62-65`), and the parser still models `skills`, closure-agent identity, expected role prompts, and expected final markers directly on every agent (`crates/wave-spec/src/lib.rs:92-136`, `crates/wave-spec/src/lib.rs:403-418`).
- `wave-dark-factory` now fails closed on both missing implementation coverage and empty `skills` lists before the closure-agent branch, so closure agents no longer bypass skill enforcement (`crates/wave-dark-factory/src/lib.rs:229-235`, `crates/wave-dark-factory/src/lib.rs:440-475`). Regression coverage explicitly asserts `agent-skills-required` for `A0`, `A8`, and `A9` (`crates/wave-dark-factory/src/lib.rs:1142-1242`).
- `wave-control-plane` still derives `lint_errors`, closure coverage, blockers, and ready state from one typed planning model (`crates/wave-control-plane/src/lib.rs:443-549`), and `wave-cli` surfaces that same model in `doctor` (`crates/wave-cli/src/main.rs:499-548`). That means a closure-agent skill regression now becomes a lint blocker that `wave doctor` and `wave control status` will both report through the shared status projection.
- Operator guidance is aligned on the same authored-wave model. `README.md:96-180`, `agents.md:50-91`, `docs/implementation/rust-codex-refactor.md:30-74`, `docs/implementation/rust-codex-refactor.md:142-154`, `docs/reference/skills.md:93-124`, `docs/plans/current-state.md:31-76`, `docs/plans/master-plan.md:5-35`, `docs/plans/migration.md:11-38`, and `docs/plans/component-cutover-matrix.md:21-42` now agree on mandatory closure agents, structured prompt sections, non-empty skills for every agent, owned-path restatement, plain-line marker instructions, and one shared doctor/control status model.
- The recorded documentation-closure claim is honest in the active rerun. `A9`'s last message names the exact shared-plan files it owns and the stricter assumptions they now carry (`.wave/build/specs/wave-00-1774256739953/agents/A9/last-message.txt:1-5`), and the active run record shows `A1`, `A2`, `A3`, `A8`, and `A9` succeeded with their expected markers before cont-QA (`.wave/state/runs/wave-00-1774256739953.json:14-120`).

## Verified positives

- `cargo run -q -p wave-cli -- lint --json` returned `[]`.
- `cargo run -q -p wave-cli -- doctor --json` returned `ok: true` with `10` waves, `60` agents, `0` lint-error waves, complete closure coverage, and `0` skill-catalog issues.
- `cargo run -q -p wave-cli -- control status --json` matched the doctor projection exactly, including queue totals, blocker summaries, closure totals, and skill-catalog health.
- `cargo run -q -p wave-cli -- control show --wave 0 --json` showed rerun `wave-00-1774256739953` at `A0`, with `A1`, `A2`, `A3`, `A8`, and `A9` already succeeded and their expected markers observed.
- `cargo test -q -p wave-spec -p wave-dark-factory -p wave-control-plane -p wave-cli` passed.
- `jq empty docs/plans/component-cutover-matrix.json` passed.

## Blocking findings

None.

[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=authored-wave-model-and-closure-skill-contract-align
Verdict: PASS
