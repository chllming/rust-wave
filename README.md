# Codex Wave Mode

This repository is the in-progress Rust rewrite of Wave, rebuilt around Codex OSS as the operator and orchestration runtime.

The current state is an executable local operator slice:

- a Rust workspace with the target crate layout
- a project config in `wave.toml` with workspace-local roots for waves, Codex state, run state, and traces
- production-style human-authored waves under `waves/` with closure agents, owned paths, deliverables, Context7 defaults, and final markers
- a compileable `wave` CLI and interactive TUI shell
- repo-specific skill bundles for Rust workspace, control-plane, Codex runtime, TUI, and closure-marker work
- pinned and vendored upstream baselines for Codex OSS and the Wave control-plane reference branch
- project-scoped Codex state under `.wave/codex/`
- live run state under `.wave/state/runs/`, rerun intents under `.wave/state/control/reruns/`, and trace bundles under `.wave/traces/runs/`

## Status

Working now:

- `wave`
- `wave project show [--json]`
- `wave doctor [--json]`
- `wave lint [--json]`
- `wave control status [--json]`
- `wave launch`
- `wave autonomous`
- `wave draft`
- `wave control show|task|rerun|proof`
- `wave trace latest|replay`

Still pending:

- `wave adhoc`
- `wave dep`
- terminal-surface integration beyond the built-in TUI shell

`wave adhoc` and `wave dep` are present in the CLI surface, but they currently short-circuit with not-implemented messages.

## Repo Layout

- `crates/`
  Rust workspace crates for CLI, config, spec parsing, control-plane bootstrapping, runtime, TUI, app-server, dark-factory policy, and traces.
- `waves/`
  The implementation backlog for this refactor, expressed as rich multi-agent authored waves.
- `wave.toml`
  The new project-scoped config for the Rust implementation.
- `docs/implementation/rust-codex-refactor.md`
  The current architecture baseline for this repo.
- `third_party/codex-rs/`
  Reviewed and vendored Codex OSS baseline plus `UPSTREAM.toml`.
- `third_party/agent-wave-orchestrator/`
  Reviewed and vendored Wave control-plane reference baseline plus `UPSTREAM.toml`.
- `docs/`
  Seeded upstream Wave docs that remain useful as reference input during the rewrite.

## Getting Started

1. Install Rust stable.
2. Run `cargo run -p wave-cli -- project show --json` to confirm the parsed project config and state roots.
3. Run `cargo test`.
4. Run `cargo run -p wave-cli -- doctor --json`.
5. Run `cargo run -p wave-cli -- control status --json`.

If you want the interactive operator shell with the right-side status panel:

```bash
cargo run -p wave-cli --
```

In a non-interactive shell, the same command falls back to a text summary.

Useful live commands:

```bash
cargo run -p wave-cli -- launch --wave 0 --dry-run --json
cargo run -p wave-cli -- control show --wave 0 --json
cargo run -p wave-cli -- control rerun request --wave 4 --reason "operator request"
cargo run -p wave-cli -- trace replay --json
```

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
3. keep `A8`, `A9`, and `A0` present with closure-only owned files, role-prompt paths, and only the marker family each role owns
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
- upstream metadata pins
- closure coverage across waves
- queue and run-state visibility
- project-scoped Codex binary availability
- recorded run-state and active-wave visibility

## Operator Shell

On an interactive terminal, `wave` opens a Ratatui shell with a right-side panel.

The right-side panel currently exposes:

- `Run`
  Active wave, run id, elapsed time, proof counts, and declared proof artifacts.
- `Agents`
  Per-agent state, proof-marker completeness, and deliverables for the active run.
- `Queue`
  Wave readiness, blockers, and dependency-driven queue state.
- `Control`
  Rerun intents, replay/proof state, and the available operator keybindings.

Current keybindings:

- `q`
  Quit the shell.
- `Tab` / `Shift+Tab`
  Cycle the right-side panel tabs.
- `j` / `k`
  Move the selected wave.
- `r`
  Request a rerun for the selected wave.
- `c`
  Clear the selected wave's rerun intent.

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
