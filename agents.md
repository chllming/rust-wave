# Agent Guidance

This repository is the in-progress Rust rewrite of Wave, rebuilt around Codex OSS.

## Current State

- The Rust workspace is the active implementation surface.
- `wave.toml` is the current project config and keeps durable repo-local state rooted under `.wave/`.
- `waves/` is the current implementation backlog and sequencing source.
- Each active wave is a rich multi-agent authored spec with mandatory closure agents `A0`, `A8`, and `A9`.
- The authored-wave contract is already enforced by parsing, linting, doctor checks, and planning status. Do not treat wave sections as optional prose.
- The Codex-backed launcher is live and uses project-scoped state under `.wave/codex/`.
- The operator shell is live: `wave` opens a Ratatui TUI on interactive terminals and falls back to a text summary otherwise.
- `wave project show --json` is the quickest way to confirm the parsed config and state roots.
- The app-server snapshot path is live and feeds the TUI/control surfaces from authoritative repo state.
- Trace persistence and replay validation are live for recorded runs.
- Rerun intents are stored under `.wave/state/control/reruns/` and already affect queue projections.
- The seeded `docs/` tree is reference material from upstream Wave, not the canonical runtime implementation for this repo.
- `wave adhoc` and `wave dep` exist as command entry points, but they currently short-circuit with not-implemented messages.

## Self-Host Flow

When this repository is used to dogfood the Rust system on itself, follow the shipped surfaces in this order:

1. Confirm the repo roots and runtime state with `wave project show --json`.
2. Check authoring and control-plane health with `wave doctor --json`, `wave lint --json`, and `wave control status --json`.
3. Compile the active wave with `wave draft` so the runtime prompt bundle under `.wave/state/build/specs/` matches the checked-in spec.
4. Run `wave launch --wave <id> --dry-run --json` before any live local mutation.
5. If the dry run is clean, run `wave launch --wave <id> --json` and watch the run through `wave control show --wave <id> --json`, `wave control task list --wave <id> --json`, `wave trace latest --json`, and `wave trace replay --json`.
6. Use the built-in TUI on an interactive terminal to inspect `Run`, `Agents`, `Queue`, and `Control` from the same control-plane snapshot.

The self-host loop is local-first and repo-scoped. It uses the launcher, queue, TUI, and trace surfaces that already exist in this tree, and it should not be described as a live-host mutation workflow.
Treat `.wave/codex/`, `.wave/state/`, and `.wave/traces/` as the only runtime roots involved in the dogfood loop.

Keep the guidance honest about gaps:

- `wave adhoc` and `wave dep` are present as entry points, but they are still stubs.
- The built-in TUI is the shipped operator shell, not a separate dashboard product.
- Trace replay and queue snapshots are evidence surfaces; they do not imply remote fleet control or host mutation beyond this worktree.

## Source Of Truth

- Read `README.md` first for repo purpose and current status.
- Read `docs/implementation/rust-codex-refactor.md` for the accepted architecture baseline.
- Read `wave.toml` for project-scoped defaults and paths, then confirm the parsed view with `cargo run -p wave-cli -- project show --json`.
- Read `waves/*.md` for the implementation order and exit criteria.
- Read the owning agent section inside the relevant wave for exact files, deliverables, Context7, skills, and final markers.
- Read `docs/context7/bundles.json` for the approved external library bundles used by this repo's waves.
- Read `skills/README.md` and `docs/reference/skills.md` for the current skill model and repo-specific bundles.
- Treat `third_party/codex-rs/UPSTREAM.toml` and `third_party/agent-wave-orchestrator/UPSTREAM.toml` as the reviewed upstream pins.

## Working Commands

- `cargo test`
- `cargo run -p wave-cli --`
- `cargo run -p wave-cli -- project show --json`
- `cargo run -p wave-cli -- doctor --json`
- `cargo run -p wave-cli -- lint --json`
- `cargo run -p wave-cli -- draft`
- `cargo run -p wave-cli -- control status --json`
- `cargo run -p wave-cli -- control show --wave <n> --json`
- `cargo run -p wave-cli -- control task list --wave <n> --json`
- `cargo run -p wave-cli -- control rerun list --json`
- `cargo run -p wave-cli -- control proof show --wave <n> --json`
- `cargo run -p wave-cli -- launch --wave <n> [--dry-run] --json`
- `cargo run -p wave-cli -- autonomous [--limit <n>] [--dry-run] --json`
- `cargo run -p wave-cli -- trace latest --json`
- `cargo run -p wave-cli -- trace replay --json`

Do not assume `wave adhoc` or `wave dep` are implemented yet. They are present as stubs that currently short-circuit with not-implemented messages.

## Authored-Wave Contract

Treat `waves/*.md` as production inputs. The current repo expects each active wave to include:

- a `+++` frontmatter block with `id`, `slug`, `title`, `mode`, `owners`, `depends_on`, `validation`, `rollback`, and `proof`
- a markdown heading plus commit message
- component promotions and deploy environments
- wave-level Context7 defaults
- closure-agent sections with role-prompt paths, closure-only ownership, skills, and closure markers
- implementation-agent sections with deliverables, file ownership, skills, Context7, components, capabilities, exit contract fields, and final markers

Implementation agents should expect these sections to exist and agree with each other:

- `### Executor`
- `### Context7`
- `### Skills`
- `### Components`
- `### Capabilities`
- `### Exit contract`
- `### Deliverables`
- `### File ownership`
- `### Final markers`
- `### Prompt`

Closure agents should expect:

- `### Role prompts`
- `### Executor`
- `### Context7`
- `### Skills`
- `### File ownership`
- `### Final markers`
- `### Prompt`

Implementation prompts must be structured enough for the linter to parse. At minimum, keep these sections present and non-empty inside `### Prompt`:

- `Primary goal`
- `Required context before coding`
- `Specific expectations`
- `File ownership (only touch these paths)`

The owned-path list inside the prompt must match the declared `### File ownership` section. If those drift, lint should fail and you should fix the wave rather than working around it.

Before you start implementation work, re-check your agent block for:

- `### Executor`
- `### Context7`
- `### Skills`
- `### Components`
- `### Capabilities`
- `### Exit contract`
- `### Deliverables`
- `### File ownership`
- `### Final markers`
- `### Prompt`

If one of those is missing or mismatched, treat that as authoring debt and fix the wave contract first.

If you are editing a wave definition itself, also run `cargo run -p wave-cli -- draft` so the compiled prompt output under `.wave/state/build/specs/` reflects the same contract the linter reads.

## Practical Workflow

Before coding inside your owned files:

1. Read `README.md`, `docs/implementation/rust-codex-refactor.md`, and the exact wave/agent block that owns your task.
2. Re-read `skills/README.md` and `docs/reference/skills.md` if the wave changes skills, closure expectations, or common bundle guidance.
3. Confirm the owned-path list in `### Prompt` matches `### File ownership`.
4. If you need to confirm the active project config, run `cargo run -p wave-cli -- project show --json`.
5. If the wave contract changed, run `cargo run -p wave-cli -- draft` and `cargo run -p wave-cli -- lint --json` before treating the wave as ready.
6. Only then implement inside your owned paths and leave proof that matches the exit contract.

For self-host dogfooding, keep the operational loop concrete:

- `wave launch` is the launcher entrypoint
- `wave control status|show|task|rerun|proof` is the queue and proof surface
- `wave trace latest|replay` is the recorded evidence surface
- `wave` on an interactive terminal is the TUI view of the same state
- `.wave/codex/`, `.wave/state/`, and `.wave/traces/` are the repo-local roots that make the flow self-hostable

## Closure Roles

The repo uses the same closure model across parser, lint, and queue status:

- `A6` is an optional report-only design reviewer and owns `[wave-design]` when a wave includes operator-surface or TUI ergonomics review
- `A7` is an optional security reviewer and owns `[wave-security]` when a wave includes security review
- `A8` is the integration steward and owns `[wave-integration]`
- `A9` is the documentation steward and owns `[wave-doc-closure]`
- `A0` is final cont-QA and owns `[wave-gate]`
- `E0` is optional cont-EVAL and owns `[wave-eval]` when a wave includes evaluation work

Implementation agents own the actual repo changes and normally end with:

- `[wave-proof]`
- `[wave-doc-delta]`
- `[wave-component]`

Do not assume closure agents will absorb missing implementation work. Their job is to reconcile and judge the landed slices.

Practical closure boundaries:

- `A6` reviews landed operator-facing UX against `docs/implementation/design.md`, records exact requested fixes plus approved deviations, and routes those fixes without taking implementation ownership.
- `A7` reviews security posture and routes exact security fixes without taking implementation ownership.
- `A8` reconciles landed slices and integration claims. It should not take implementation ownership through the side door.
- `A9` updates shared-plan or closure documentation only.
- `A0` judges the wave and records the final gate result, but does not finish missing implementation work.

Closure order is fixed:

1. implementation agents land owned work and proof
2. optional `E0` eval closes if the wave declares it
3. optional specialist reviewers such as `A6` design review or `A7` security review publish their report-only verdicts if the wave declares them
4. `A8` decides whether the slices are ready for doc closure
5. `A9` records documentation closure
6. `A0` makes the final gate decision

## Editing Rules

- Prefer touching the Rust crates under `crates/` over editing seeded Node-era config and docs.
- Keep the command surface aligned with the accepted plan: `wave` remains the primary binary.
- Preserve the clean-break direction. Do not reintroduce compatibility work for the old JS runtime layout unless explicitly requested.
- Keep dark-factory assumptions explicit. Validation, rollback, proof, and closure expectations should stay machine-readable where possible.
- Keep the interactive shell and non-interactive CLI aligned around the same control-plane state. Do not add TUI-only truth.
- Keep Context7 selections narrow and task-shaped. Do not widen a bundle when a wave-specific query or a new small bundle would do.
- Treat skill ids and final markers as part of the contract, not optional embellishment.
- Only touch the paths owned by your agent prompt. If a required change lands outside your ownership, route it instead of editing through it.
- Keep `wave.toml` path roots workspace-local and consistent with `.wave/codex/`, `.wave/state/`, and `.wave/traces/`.
- Keep deliverables concrete. If the wave says a file should exist or a command surface should change, land that artifact and leave proof.
- If `wave lint` reports a weak prompt, missing closure role, or ownership mismatch, fix the authored wave instead of compensating with undocumented assumptions in code.
- When adding a new subsystem, land it in the existing target crate path instead of inventing a new top-level structure.
- Update `README.md`, `docs/implementation/rust-codex-refactor.md`, or the relevant `waves/*.md` file when behavior or sequencing changes.

## Validation Expectations

- Run `cargo fmt` and the relevant `cargo test` targets for the crates you touch.
- If you change parsing, linting, or control-plane logic, update or add unit tests in the same crate.
- If you add a new command surface, verify it through `cargo run -p wave-cli -- ...`.
- If you change the TUI, start it under a PTY and confirm it exits cleanly with `q`.
- Run `cargo run -p wave-cli -- draft` when you change authored-wave structure or prompt-compilation expectations.
- Run `cargo run -p wave-cli -- lint --json` when you change authored-wave structure, lint rules, or docs that describe the enforced authoring contract.
- Run `cargo run -p wave-cli -- doctor --json` when you change skill-catalog expectations or operator guidance around closure coverage and planning status.
- Run `cargo run -p wave-cli -- trace replay --json` when you change recorded run-state or replay semantics.

## Implementation Priorities

- Keep moving through the committed wave order unless a blocking design issue forces a resequence.
- Prefer finishing live operator/runtime slices over scattering partial framework code across all crates.
- Focus remaining work on dark-factory enforcement, autonomous promotion, deeper trace semantics, and self-host dogfooding.

## Skill Selection

Prefer attaching the smallest bundle set that explains the work:

- `wave-core` for shared ownership, proof, and closure protocol
- one role skill such as `role-implementation` or `role-documentation`
- one runtime skill such as `runtime-codex`
- one or more repo-specific bundles for the subsystem being touched
- `repo-wave-closure-markers` whenever final markers are part of the exit contract

In this repo's active waves, implementation agents should assume `repo-wave-closure-markers` is part of the default stack because implementation markers are part of the enforced contract.

Use the repo-specific bundle that matches the surface you are changing:

- `repo-rust-workspace` for config/spec/CLI/crate-topology work
- `repo-rust-control-plane` for doctor, lint status, queue reducers, and planning projections
- `repo-codex-orchestrator` for launcher, app-server action, and project-scoped Codex state work
- `repo-ratatui-operator` for TUI rendering and operator-shell behavior

Do not pad waves with broad or redundant skill lists. If guidance is not reusable across waves, it belongs in the wave prompt instead.
