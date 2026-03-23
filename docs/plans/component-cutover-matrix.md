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
| `typed-authority-roots` | `repo-landed` | `wave.toml`, `wave project show`, and `wave doctor` now expose canonical authority roots under `.wave/state/` while `.wave/state/runs/` and `.wave/traces/runs/` remain explicit compatibility surfaces for reducer inputs, proof lifecycle, and replay ratification. |
| `dark-factory-lint` | `repo-landed` | Frontmatter, role-section boundaries, marker instructions, owned-path restatements, deliverable bounds, and skill gaps fail closed before runtime work begins. |
| `reducer-state-spine` | `baseline-proved` | `wave-reducer` now computes deterministic lifecycle, blocker, closure, and readiness state for planning and operator projections from authored declarations plus compatibility-backed adapter inputs; structured result envelopes are not yet the canonical reducer input. |
| `gate-verdict-spine` | `baseline-proved` | `wave-gates` now owns typed gate and closure verdict helpers over reducer-backed state, but proof lifecycle and replay ratification still resolve through compatibility artifacts in this stage. |
| `planning-status` | `baseline-proved` | Queue readiness, per-wave agent counts, closure coverage, blocker reporting, and queue visibility now flow through reducer-backed planning status over compatibility run inputs; structured result envelopes and proof lifecycle remain later work. |
| `queue-json-surface` | `baseline-proved` | Operators can inspect blocker-wave lists, closure totals, queue visibility, and skill-catalog state in text and JSON form from the same reducer-backed planning projection, while proof and replay evidence still come from compatibility artifacts. |
| `codex-launcher-substrate` | `repo-landed` | `wave launch` runs ready waves through the Codex-backed launcher, manages agent lifecycles, and records the compatibility run artifacts that current queue, proof-lifecycle, and replay projections still consume. |
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
Wave `10` makes the authority-core components `repo-landed`, and Wave `11` raises the reducer/projection slice to `baseline-proved`: planning, queue, and control/operator truth are now reducer-backed over compatibility run inputs, while structured result envelopes remain later work and proof lifecycle plus replay ratification remain compatibility-backed.
The next executable cutover is Wave `12`: land structured result envelopes and proof lifecycle while keeping legacy compatibility adapters explicit.
Wave `11` is recorded here for shared-plan parity, but final cont-QA closure still belongs to `A0` and is not claimed by this matrix.

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

## Usage

- Keep architecture docs descriptive and keep maturity claims here.
- `currentLevel` is the canonical post-wave state of the repo, not a future target.
- When a wave promotes a component, update `currentLevel` to the promoted target before documentation closure.
- `repo-landed` or `baseline-proved` does not imply compatibility artifacts are retired; use the safe-assumption column to record whether queue/control is reducer-backed while proof or replay still depend on compatibility state.
- Later waves may build on all `repo-landed` and `baseline-proved` components directly, but they must still treat structured result envelopes as not yet authoritative and treat proof lifecycle plus replay ratification as compatibility-backed until the later cutover waves land.
- Treat `component-cutover-matrix.json` as the canonical doc-parity input when README, current-state, master-plan, and runtime-reference docs describe the cutover boundary.
