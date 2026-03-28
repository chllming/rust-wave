# Codex Wave Mode

This repository is the in-progress Rust rewrite of Wave, rebuilt around Codex OSS as the operator and orchestration runtime.

The current state is an executable local operator slice:

- a Rust workspace with the target crate layout
- a project config in `wave.toml` with workspace-local roots for waves, Codex state, typed authority roots under `.wave/state/`, and compatibility run and trace outputs retained until later cutover waves replace them
- production-style human-authored waves under `waves/` with closure agents, owned paths, deliverables, Context7 defaults, and final markers
- a compileable `wave` CLI and interactive TUI shell
- repo-specific skill bundles for Rust workspace, control-plane, Codex runtime, TUI, and closure-marker work
- pinned and vendored upstream baselines for Codex OSS and the Wave control-plane reference branch
- project-scoped Codex state under `.wave/codex/`
- canonical authority roots under `.wave/state/events/control/`, `.wave/state/events/coordination/`, `.wave/state/events/scheduler/`, `.wave/state/results/`, `.wave/state/derived/`, `.wave/state/projections/`, and `.wave/state/traces/`
- compatibility outputs under `.wave/state/runs/` and `.wave/traces/runs/`, plus rerun intents under `.wave/state/control/reruns/`

## Status

Working now:

- `wave`
- `wave tui [--alt-screen auto|always|never] [--fresh-session]`
- `wave project show [--json]`
- `wave doctor [--json]`
- `wave lint [--json]`
- `wave control status [--json]`
- `wave control show|task|agent|rerun|close|proof|orchestrator|repair`
- `wave delivery status [--json]`
- `wave delivery initiative|release|acceptance show --id <id> [--json]`
- `wave launch`
- `wave autonomous`
- `wave draft`
- `wave adhoc plan|run|list|show|promote`
- `wave trace latest|replay`

Still pending:

- `wave dep`
- the real Wave 18 proof run that ratifies concurrent MAS execution, targeted recovery, and honest closure end to end

`wave dep` is still present in the CLI surface but currently short-circuits with a not-implemented message.

## Repo Layout

- `crates/`
  Rust workspace crates for CLI, config, spec parsing, control-plane bootstrapping, runtime, TUI, app-server, dark-factory policy, and traces.
- `waves/`
  The implementation backlog for this refactor, expressed as rich multi-agent authored waves.
- `wave.toml`
  The new project-scoped config for the Rust implementation.
- `docs/plans/current-state.md`
  The live baseline for the current repo-local runtime, operator shell, and MAS cutover state.
- `docs/implementation/rust-codex-refactor.md`
  The bootstrap and cutover record for how the Rust baseline replaced the older package-era stack.
- `third_party/codex-rs/`
  Reviewed and vendored Codex OSS baseline plus `UPSTREAM.toml`.
- `third_party/agent-wave-orchestrator/`
  Reviewed and vendored Wave control-plane reference baseline plus `UPSTREAM.toml`.
- `docs/`
  Seeded upstream Wave docs that remain useful as reference input during the rewrite.

## Getting Started

The current tagged release is `v0.1.0`.

Wave is not published to crates.io yet, but you can install the released CLI directly from the Git tag:

```bash
cargo install --git https://github.com/chllming/rust-wave.git --tag v0.1.0 wave-cli --locked
wave project show --json
wave doctor --json
wave control status --json
```

If you prefer to work from a checked-out release tag:

```bash
git clone --branch v0.1.0 https://github.com/chllming/rust-wave.git
cd rust-wave
cargo install --path crates/wave-cli --locked
wave project show --json
```

For active development on the repo itself, build or install from source:

Prerequisites:

1. Install Rust stable with `rustup`.
2. Ensure `cargo` is on your `PATH`.
3. Clone this repository locally.

Build and run from the workspace:

```bash
cargo build --release -p wave-cli
./target/release/wave project show --json
./target/release/wave doctor --json
./target/release/wave control status --json
```

If you prefer a local per-user install from your clone without managing the `target/release/` path directly:

```bash
cargo install --path crates/wave-cli --locked
wave project show --json
wave doctor --json
wave control status --json
```

If you are working on the repo itself, the editable dev path is still:

```bash
cargo run -p wave-cli -- project show --json
cargo test
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- control status --json
```

To update after pulling new changes:

```bash
git pull
cargo build --release -p wave-cli
```

If you installed from your clone with `cargo install --path`, rerun:

```bash
cargo install --path crates/wave-cli --locked --force
```

If you installed from the tagged Git release, rerun the same command with `--force`:

```bash
cargo install --git https://github.com/chllming/rust-wave.git --tag v0.1.0 wave-cli --locked --force
```

If you want the interactive operator shell:

```bash
./target/release/wave
```

In a non-interactive shell, the same command falls back to a text summary.

Useful shell entry points:

```bash
cargo run -p wave-cli -- tui --help
cargo run -p wave-cli -- tui --alt-screen never
cargo run -p wave-cli -- tui --fresh-session
```

Useful live commands:

```bash
cargo run -p wave-cli -- launch --wave 0 --dry-run --json
cargo run -p wave-cli -- control show --wave 0 --json
cargo run -p wave-cli -- delivery status --json
cargo run -p wave-cli -- adhoc plan --task "investigate scheduler drift" --json
cargo run -p wave-cli -- control rerun request --wave 4 --reason "operator request"
cargo run -p wave-cli -- trace replay --json
```

## Railway MCP

This repo includes a repo-local Railway MCP launcher so Codex, Claude, and Cursor can all talk to the same Railway project from the same checkout.

- launcher: `.codex-tools/railway-mcp/start.sh`
- shared MCP config: `.mcp.json`
- Cursor MCP config: `.cursor/.mcp.json`
- Codex project config: `.codex/config.toml`
- Claude project settings: `.claude/settings.json`
- Railway project id: `b2427e79-3de9-49c3-aa5a-c86db83123c0`

One-time local checks:

```bash
./.codex-tools/railway-mcp/node_modules/.bin/railway whoami
./.codex-tools/railway-mcp/node_modules/.bin/railway link --project b2427e79-3de9-49c3-aa5a-c86db83123c0
codex mcp list
```

## Self-Host Runbook

This repo is meant to dogfood the Rust operator on itself. The practical local loop is:

1. Verify the repo-scoped surfaces first:

```bash
cargo run -p wave-cli -- project show --json
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- lint --json
cargo run -p wave-cli -- control status --json
```

2. Review the active wave and its compiled prompts:

```bash
cargo run -p wave-cli -- draft
```

3. Inspect the planned run before any mutation:

```bash
cargo run -p wave-cli -- launch --wave <id> --dry-run --json
```

4. Start the live local operator slice only when the dry run is clean:

```bash
cargo run -p wave-cli -- launch --wave <id> --json
```

5. Watch the same run through the current compatibility queue and trace surfaces:

```bash
cargo run -p wave-cli -- control show --wave <id> --json
cargo run -p wave-cli -- control task list --wave <id> --json
cargo run -p wave-cli -- trace latest --json
cargo run -p wave-cli -- trace replay --json
```

6. Open the TUI on an interactive terminal to inspect `Overview`, `Agents`, `Queue`, `Proof`, and `Control` from one reducer-backed snapshot:

```bash
cargo run -p wave-cli --
```

The runbook is intentionally repo-local. It relies on `.wave/codex/`, `.wave/state/`, and `.wave/traces/`, and it does not mutate a live host outside this worktree.

Current gaps remain explicit:

- `wave dep` still short-circuits with a not-implemented message.
- The TUI is the built-in operator shell, not a separate dashboard app.
- Self-host dogfooding is local-first; it does not imply live-host deployment or remote fleet control.
- Wave 0.2 now lands authority-core plus reducer-backed planning, queue, and control projections over compatibility run inputs; proof and closure surfaces are envelope-first through stored result envelopes, with legacy proof adaptation isolated to `wave-results`, while replay ratification still depends on compatibility run and trace artifacts until later waves retire those adapters.
- Wave 18 is partially live, not fully ratified: MAS waves now have per-agent sandboxes, recovery-required state, runtime-backed head control, and the finished shell product, but the live proof run is still outstanding.

## Context7

- Precise per-wave Context7 defaults now live in `waves/*.md`.
- Each implementation and closure agent also carries its own narrow Context7 override.
- The bundle catalog lives in `docs/context7/bundles.json`.
- The local API key should live in `.env.local`. A checked-in template lives in `.env.local.example`.
- The real key is intentionally not documented in tracked files.

## Project Config

`wave.toml` is the bootstrap config for this repo. The parsed project surface currently points at:

- `waves_dir = "waves"`
- `project_codex_home = ".wave/codex"`
- `state_dir = ".wave/state"`
- `trace_dir = ".wave/traces"`
- `state_events_control_dir = ".wave/state/events/control"`
- `state_events_coordination_dir = ".wave/state/events/coordination"`
- `state_results_dir = ".wave/state/results"`
- `state_derived_dir = ".wave/state/derived"`
- `state_projections_dir = ".wave/state/projections"`
- `state_traces_dir = ".wave/state/traces"`

The canonical Wave 0.2 authority-root contract is the set of `.wave/state/` paths above:

- control events: `.wave/state/events/control/`
- coordination records: `.wave/state/events/coordination/`
- structured results: `.wave/state/results/`
- derived state: `.wave/state/derived/`
- projections: `.wave/state/projections/`
- canonical traces: `.wave/state/traces/`

At the current Wave 0.2 stage, those roots are typed, resolved, and doctor-checked. Planning, queue, and control projections are now reducer-backed read models over compatibility run inputs, while proof and closure surfaces read stored result envelopes first, legacy proof adaptation lives only in `wave-results`, and replay ratification still relies on compatibility run and trace artifacts until the later replay wave lands.

`state_runs_dir` and `trace_runs_dir` remain present as compatibility outputs while later cutover waves replace the current run-record and trace-bundle path. `state_control_dir` continues to house rerun intents for the current operator slice.

Use `cargo run -p wave-cli -- project show --json` to verify the loaded config, and keep any new repo-local state rooted under those paths.

## Skills

- Repo-owned skills live under `skills/`.
- Rich authored waves attach skills per agent through `### Skills`.
- `cargo run -p wave-cli -- doctor --json` validates the local skill catalog shape.
- `cargo run -p wave-cli -- lint --json` validates authored-wave skill references and rejects unknown ids.
- Standard repo-specific bundles are `repo-rust-workspace`, `repo-rust-control-plane`, `repo-codex-orchestrator`, `repo-ratatui-operator`, and `repo-wave-closure-markers`.

## Authored-Wave Baseline

`waves/*.md` is the canonical execution contract for this repo. Active waves are not loose planning notes. They are the inputs that the parser, linter, doctor surface, and planning queue all read.

Every active wave has two layers:

- a `+++` frontmatter block with `id`, `slug`, `title`, `mode`, `owners`, `depends_on`, `validation`, `rollback`, and `proof`
- a markdown body with the commit message, component promotions, deploy environments, Context7 defaults, and one block per agent

Every active wave also keeps closure agents `A8`, `A9`, and `A0` present. `E0` is optional and only appears when the wave explicitly carries eval work.

Implementation agents are expected to carry practical execution sections, not freeform prose:

- `### Executor`
- `### Deliverables`
- `### File ownership`
- `### Skills`
- `### Context7`
- `### Components`
- `### Capabilities`
- `### Exit contract`
- `### Final markers`
- a `### Prompt` with `Primary goal`, `Required context before coding`, `Specific expectations`, and `File ownership (only touch these paths)`

Closure agents stay lighter, but they are still structured:

- `### Role prompts`
- `### Executor`
- `### Context7`
- `### Skills`
- `### File ownership`
- `### Final markers`
- a `### Prompt` with the same required subheadings

Closure roles stay distinct:

- `A6` is optional design review and ends with `[wave-design]` when the wave includes operator-surface or TUI ergonomics review
- `A7` is optional security review and ends with `[wave-security]` when the wave includes threat-model-first review
- `A8` integrates and ends with `[wave-integration]`
- `A9` closes documentation and ends with `[wave-doc-closure]`
- `A0` is final cont-QA and ends with `[wave-gate]`
- `E0` is optional eval coverage and ends with `[wave-eval]` when the wave includes it
- closure agents keep closure-only ownership; they do not silently absorb implementation work or code ownership

Implementation agents end with:

- `[wave-proof]`
- `[wave-doc-delta]`
- `[wave-component]`

The repo treats those markers and owned paths as contract fields, not stylistic suggestions.

For new implementation waves, start from a practical default stack:

- `wave-core`
- one role skill
- `runtime-codex`
- one narrow repo-specific subsystem skill
- `repo-wave-closure-markers`

When the subsystem is clear, keep the repo-specific attachment narrow:

- parser, config, CLI, or crate-shape work: `repo-rust-workspace`
- doctor, lint status, queue, or planning work: `repo-rust-control-plane`
- launcher, app-server, or project-scoped Codex state: `repo-codex-orchestrator`
- TUI or operator-shell behavior: `repo-ratatui-operator`

## Authoring New Waves

For future waves, write the spec as if `wave lint` were the first reviewer:

1. declare the wave-level frontmatter, commit message, component promotions, deploy environments, and Context7 defaults
2. give each implementation agent concrete deliverables, a closed file-ownership slice, components, capabilities, exit contract fields, skills, and implementation markers
3. keep `A8`, `A9`, and `A0` present with closure-only owned files, role-prompt paths, and only the marker family each role owns; add optional specialist reviewers such as `A6` design review only when the wave actually needs them
4. restate the exact owned paths inside each agent's `### Prompt`; the prompt and `### File ownership` section must match
5. run `cargo run -p wave-cli -- draft`, `cargo run -p wave-cli -- lint --json`, and `cargo run -p wave-cli -- doctor --json` before treating the wave as ready

## Fail-Closed Validation

`wave lint` rejects authored waves that leave the contract underspecified. That includes:

- missing frontmatter metadata, commit messages, deploy environments, or component promotions
- missing closure agents `A0`, `A8`, or `A9`
- missing owned paths, deliverables, exit contracts, components, capabilities, or final markers
- prompts that omit the required sections or fail to restate the owned paths
- closure agents that omit their role-prompt files or claim the wrong marker set
- missing or weak Context7 declarations
- unknown skill ids
- overlapping file ownership between agents

`wave doctor` checks the repo surfaces that authored waves depend on:

- config and wave loading
- skill-catalog health under `skills/`
- role-prompt path availability under `docs/agents/`
- canonical authority roots under `.wave/state/`
- upstream metadata pins
- closure coverage across waves
- queue and compatibility run-state visibility
- project-scoped Codex binary availability
- recorded compatibility run-state and active-wave visibility

## Operator Shell

On an interactive terminal, `wave` opens a Ratatui operator shell.

The shell is split into:

- a left-side operator shell with header, transcript, and composer
- a right-side dashboard with `Overview`, `Agents`, `Queue`, `Proof`, and `Control`

The shell supports:

- `head`, `wave`, and `agent` scopes
- operator and autonomous mode
- reducer-backed queue, proof, control, autonomy, and recovery visibility
- transcript search and compare mode
- explicit `--alt-screen` and `--fresh-session` startup controls via `wave tui`
- dashboard focus by default on startup so shell hotkeys work immediately
- plain text guidance bound to the current shell target
- wave hotkeys and implicit wave commands bound to the visibly selected wave
- `follow run|agent|off` tracking for active runs, pinned agents, or manual selection
- a visible cross-wave review queue in repo-level `head` scope, with `u` / `x` bound to the selected review row
- a one-column shell layout in narrow terminals, with visible transcript, composer, and dashboard instead of hidden input

Current keybindings:

- `q`
  Quit the shell.
- `Tab` / `Shift+Tab`
  Cycle transcript, composer, and dashboard focus. Composer capture is explicit; startup focus is `Dashboard`.
- `[` / `]`
  Cycle the right-side dashboard tabs.
- `j` / `k`
  Scroll transcript or move dashboard selection, depending on focus.
- `r`
  Request a rerun for the selected wave.
- `c`
  Clear the selected wave's rerun intent.
- `m` / `M`
  Apply or clear manual close with confirmation.
- `u` / `x`
  Approve or reject the selected operator action.
- `?`
  Open shell help.

Live shell commands include:

- `/wave <id>`
- `/agent <id>`
- `/scope head|wave|agent`
- `/mode operator|autonomous`
- `/launch [wave-id]`
- `/rerun [full|from-first-incomplete|closure-only|promotion-only]`
- `/clear-rerun`
- `/pause`, `/resume`, `/rerun-agent`, `/rebase`, `/reconcile`
- `/approve-merge`, `/reject-merge`
- `/open overview|agents|queue|proof|control`
- `/follow run|agent|off`
- `/search <text>`, `/clear-search`
- `/compare wave <id> | /compare agent <id>`, `/clear-compare`

When authoring or updating a wave, keep the docs and the executable contract aligned in the same slice.

## Implementation Model

The refactor is being driven by the tool itself. The committed waves in `waves/` are the execution sequence:

1. freeze the architecture and repo shape
2. bootstrap the Rust workspace and command surface
3. implement config, spec parsing, and dark-factory lint
4. bootstrap control-plane status
5. add the Codex-backed launcher
6. add the right-side TUI panel
7. enforce dark-factory as a runtime policy
8. add autonomous queueing
9. add trace and replay
10. dogfood the Rust system on this repo

The repo has already moved past the earlier planning-only bootstrap. The remaining work is now about hardening and closing the later waves against the live operator/runtime surface that exists in this worktree.

## Upstreams Reviewed

- Codex OSS: `https://github.com/openai/codex`
- Wave control-plane reference: `https://github.com/chllming/agent-wave-orchestrator/tree/docs/wave-positioning-refresh`

The exact reviewed commits are recorded in the `third_party/*/UPSTREAM.toml` files.
