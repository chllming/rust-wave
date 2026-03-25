# Component Cutover Matrix

This matrix is the canonical place to record the authored-wave rewrite components and the highest maturity the repo can honestly claim for each one.

## Levels

- `inventoried`
- `contract-frozen`
- `repo-landed`
- `baseline-proved`
- `pilot-live`
- `qa-proved`
- `fleet-ready`
- `cutover-ready`
- `deprecation-ready`

## Current Repo Levels

| Component | Current level | Current safe assumption |
| --- | --- | --- |
| `authored-wave-schema` | `repo-landed` | Rust parses frontmatter, shared wave fields, structured agent sections, and prompt-owned-path restatements directly from `waves/*.md`. |
| `closure-contracts-and-skill-catalog` | `repo-landed` | Lint and doctor enforce closure-agent presence, role-prompt wiring, non-empty skills for every agent, marker ownership, and repo skill ids/manifests. |
| `rust-workspace` | `repo-landed` | The target Rust workspace shape is stable for future waves. |
| `wave-cli-bootstrap` | `repo-landed` | `wave`, `project show`, `doctor`, `lint`, and `control status` exist as the bootstrap CLI surface. |
| `wave-config-and-spec` | `repo-landed` | `wave.toml` is loaded through a typed config model and authored waves are parsed into typed wave and agent structures. |
| `authority-core-domain` | `repo-landed` | `wave-domain` maps authored waves into typed task seeds and shared authority primitives for later reducer work. |
| `authority-event-log` | `repo-landed` | `wave-events` provides append/query control-event logs under `.wave/state/events/control/`. |
| `authority-coordination-log` | `repo-landed` | `wave-coordination` provides append/query coordination records under `.wave/state/events/coordination/`. |
| `typed-authority-roots` | `repo-landed` | `wave.toml`, `wave project show`, and `wave doctor` now expose canonical authority roots under `.wave/state/` while `.wave/state/runs/` and `.wave/traces/runs/` remain explicit compatibility surfaces for reducer inputs and replay ratification; new-run proof and closure lifecycle now read `.wave/state/results/` first. |
| `dark-factory-lint` | `repo-landed` | Frontmatter, role-section boundaries, marker instructions, owned-path restatements, deliverable bounds, and skill gaps fail closed before runtime work begins. |
| `reducer-state-spine` | `baseline-proved` | `wave-reducer` now computes deterministic lifecycle, blocker, closure, and readiness state for planning status, queue/control status, and operator projections from authored declarations plus compatibility-backed run-record adapters; structured result envelopes are not yet the canonical reducer input. |
| `gate-verdict-spine` | `baseline-proved` | `wave-gates` now owns typed gate and closure verdict helpers over reducer-backed state, with new-run closure consumers reading stored structured envelopes first, explicit compatibility adapters remaining visible for legacy attempts, and replay ratification still resolving through compatibility run and trace artifacts in this stage. |
| `planning-status` | `baseline-proved` | Queue readiness, per-wave agent counts, closure coverage, blocker reporting, queue visibility, and operator-facing planning status now flow through reducer-backed projections over compatibility run records; proof and closure are envelope-first for the active run and the latest completed or failed run, and replay ratification remains later work. |
| `queue-json-surface` | `baseline-proved` | Operators can inspect blocker-wave lists, closure totals, queue visibility, and skill-catalog state in text and JSON form from the same reducer-backed planning and operator-status projections, while proof surfaces read stored envelopes first for the active run and the latest completed or failed run and replay evidence still comes from compatibility run records and trace bundles. |
| `result-envelope-lifecycle` | `repo-landed` | New-run agent results persist canonical structured envelopes under `.wave/state/results/` through `wave-results`, while legacy marker ingestion stays inside explicit compatibility adapters for older attempts. |
| `proof-lifecycle-spine` | `repo-landed` | CLI and app-server proof views resolve stored envelopes first for the active run and the latest completed or failed run, expose `compatibility-adapter` fallback only for legacy attempts, and keep replay ratification on compatibility artifacts. |
| `envelope-first-closure` | `repo-landed` | Runtime acceptance and closure-gate input now validate stored envelope closure state first for new runs, with legacy marker interpretation confined to explicit compatibility adapters and replay-backed ratification still left to later waves. |
| `codex-launcher-substrate` | `repo-landed` | `wave launch` runs ready waves through the Codex-backed launcher, manages agent lifecycles, writes structured result envelopes under `.wave/state/results/`, and still records the compatibility run artifacts that queue/control and replay projections consume. |
| `project-scoped-codex-home` | `repo-landed` | Codex auth, config, sqlite state, and session logs live under `.wave/codex/` instead of the operator's global home, and launcher execution stays project-scoped. |
| `tui-right-side-panel` | `repo-landed` | The interactive `wave` shell renders the right-side operator panel against current repo-local Wave state and exposes the direct operator dashboard with operator-visible queue selection and rerun controls over reducer-backed queue/control projections. |
| `operator-status-tabs` | `repo-landed` | `Run`, `Agents`, `Queue`, and `Control` tabs are live, keyboard navigable, and share the same reducer-backed queue/control truth as the CLI surfaces, including in-place operator actions. Later waves can rely on these as the primary operator affordances for queue selection and rerun intent. |
| `dark-factory-preflight` | `repo-landed` | Launch writes preflight reports and fails closed before runtime mutation when contracts are underspecified, so later queue and dogfood waves must be authored with complete launch data up front. |
| `fail-closed-launch-policy` | `repo-landed` | Runtime launch stops on preflight or marker/proof failures rather than silently downgrading behavior. |
| `autonomous-wave-queue` | `repo-landed` | `wave autonomous` launches the ready queue from the same reducer-backed planning state and respects the scheduler's claimability decisions while compatibility run inputs remain adapters. |
| `dependency-aware-scheduler` | `repo-landed` | Queue promotion and blockage are derived from dependencies, compatibility run state, and rerun intents, so autonomy never bypasses typed gating. |
| `trace-bundle-v1` | `repo-landed` | Live and dry runs write compatibility trace bundles under `.wave/traces/runs/`. |
| `replay-validation` | `repo-landed` | `wave trace replay` validates stored compatibility run outcomes against the recorded compatibility artifacts. |
| `self-host-dogfood-runbook` | `repo-landed` | The repo-local self-host loop is now documented as a concrete operator runbook that stays repo-scoped and does not imply live-host mutation. |
| `dark-factory-dogfood-evidence` | `repo-landed` | The repo-local dogfood evidence slice now records the proven self-host loop and its durable local evidence surfaces. |

The dogfood-evidence components are now `repo-landed`: the repo has the executable runtime/operator surface plus the self-host evidence to describe it honestly.
Wave `10` makes the authority-core components `repo-landed`, and Wave `11` raises the reducer/projection slice to `baseline-proved`: planning status, queue/control JSON, and operator-facing status truth are now reducer-backed over compatibility run records.
Wave `12` lands `result-envelope-lifecycle`, `proof-lifecycle-spine`, and `envelope-first-closure`: new-run persistence flows through `wave-results`, proof surfaces are envelope-first for the active run and the latest completed or failed run, legacy compatibility adapters stay explicit for legacy attempts, and replay mismatches compare normalized or semantic envelope references while replay ratification remains later work.
Wave `13` is now the landed scheduler-authority cutover: queue/control truth remains reducer-backed, local claim admission is exclusive, and live leases renew and expire in canonical scheduler authority while execution remains serial.
The next executable cutover is Wave `14`: true parallel-wave execution, wave-scoped worktree isolation, and merge discipline without regressing the current queue/control or replay authority boundary.
Wave `15` remains the runtime-policy and multi-runtime adapter cutover, and Wave `19` remains the planner-emitted invariants plus staged gate-plan cutover.
Wave `11` only promotes the four reducer/projection components listed below. Wave `12` only promotes the three envelope/proof lifecycle components listed below. Gate verdict helpers, launcher substrate, and other consumers reflect the landed boundary but keep their existing levels until later waves promote them.
Wave `12` is recorded here for shared-plan parity, but final cont-QA closure still belongs to `A0` and is not claimed by this matrix.

## Promotions By Wave

- Wave `0`: `authored-wave-schema`, `closure-contracts-and-skill-catalog`
- Wave `1`: `rust-workspace`, `wave-cli-bootstrap`
- Wave `2`: `wave-config-and-spec`, `dark-factory-lint`
- Wave `3`: `planning-status`, `queue-json-surface`
- Wave `4`: `codex-launcher-substrate`, `project-scoped-codex-home`
- Wave `5`: `tui-right-side-panel`, `operator-status-tabs`
- Wave `6`: `dark-factory-preflight`, `fail-closed-launch-policy`
- Wave `7`: `autonomous-wave-queue`, `dependency-aware-scheduler`
- Wave `8`: `trace-bundle-v1`, `replay-validation`
- Wave `9`: `self-host-dogfood-runbook`, `dark-factory-dogfood-evidence`
- Wave `10`: `authority-core-domain`, `authority-event-log`, `authority-coordination-log`, `typed-authority-roots`
- Wave `11`: `reducer-state-spine`, `gate-verdict-spine`, `planning-status`, `queue-json-surface`
- Wave `12`: `result-envelope-lifecycle`, `proof-lifecycle-spine`, `envelope-first-closure`

## Usage

- Keep architecture docs descriptive and keep maturity claims here.
- `currentLevel` is the canonical post-wave state of the repo, not a future target.
- When a wave promotes a component, update `currentLevel` to the promoted target before documentation closure.
- Do not promote consumer surfaces beyond their current level just because they now read reducer-backed projections; only the components explicitly promoted by the wave move.
- `repo-landed` or `baseline-proved` does not imply compatibility artifacts are retired; use the safe-assumption column to record whether queue/control is reducer-backed while proof is envelope-first for the active run and the latest completed or failed run and replay still depends on compatibility state.
- Later waves may build on all `repo-landed` and `baseline-proved` components directly, but they must still treat replay ratification as compatibility-backed and must not overstate structured result envelopes beyond the envelope-first proof/closure boundary that is live today for the active run and the latest completed or failed run.
- Treat `component-cutover-matrix.json` as the canonical doc-parity input when README, current-state, master-plan, and runtime-reference docs describe the cutover boundary.
