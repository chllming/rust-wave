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
| `dark-factory-lint` | `repo-landed` | Frontmatter, role-section boundaries, marker instructions, owned-path restatements, deliverable bounds, and skill gaps fail closed before runtime work begins. |
| `planning-status` | `repo-landed` | Queue readiness, per-wave agent counts, closure coverage, blocker reporting, and queue visibility come from the typed wave model. |
| `queue-json-surface` | `repo-landed` | Operators can inspect blocker-wave lists, closure totals, queue visibility, and skill-catalog state in text and JSON form from one projection. |
| `codex-launcher-substrate` | `repo-landed` | `wave launch` runs ready waves through the Codex-backed launcher, manages agent lifecycles, and records durable per-run state. |
| `project-scoped-codex-home` | `repo-landed` | Codex auth, config, sqlite state, and session logs live under `.wave/codex/` instead of the operator's global home, and launcher execution stays project-scoped. |
| `tui-right-side-panel` | `repo-landed` | The interactive `wave` shell renders the right-side operator panel against authoritative Wave state and exposes the direct operator dashboard with operator-visible queue selection and rerun controls. |
| `operator-status-tabs` | `repo-landed` | `Run`, `Agents`, `Queue`, and `Control` tabs are live, keyboard navigable, and share the same queue/control truth as the CLI surfaces, including in-place operator actions. Later waves can rely on these as the primary operator affordances for queue selection and rerun intent. |
| `dark-factory-preflight` | `repo-landed` | Launch writes preflight reports and fails closed before runtime mutation when contracts are underspecified, so later queue and dogfood waves must be authored with complete launch data up front. |
| `fail-closed-launch-policy` | `repo-landed` | Runtime launch stops on preflight or marker/proof failures rather than silently downgrading behavior. |
| `autonomous-wave-queue` | `repo-landed` | `wave autonomous` launches the ready queue from the same typed planning state and respects the scheduler's claimability decisions. |
| `dependency-aware-scheduler` | `repo-landed` | Queue promotion and blockage are derived from dependencies, run state, and rerun intents, so autonomy never bypasses typed gating. |
| `trace-bundle-v1` | `repo-landed` | Live and dry runs write trace bundles under `.wave/traces/runs/`. |
| `replay-validation` | `repo-landed` | `wave trace replay` validates stored run outcomes against the recorded artifacts. |
| `self-host-dogfood-runbook` | `repo-landed` | The repo-local self-host loop is now documented as a concrete operator runbook that stays repo-scoped and does not imply live-host mutation. |
| `dark-factory-dogfood-evidence` | `repo-landed` | The repo-local dogfood evidence slice now records the proven self-host loop and its durable local evidence surfaces. |

The dogfood-evidence components are now `repo-landed`: the repo has the executable runtime/operator surface plus the self-host evidence to describe it honestly.

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

## Usage

- Keep architecture docs descriptive and keep maturity claims here.
- `currentLevel` is the canonical post-wave state of the repo, not a future target.
- When a wave promotes a component, update `currentLevel` to the promoted target before documentation closure.
- Later waves may build on all `repo-landed` components directly, but they must treat the remaining dogfood components as planned-only until those waves land.
