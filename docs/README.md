# Wave Documentation

This tree mixes two kinds of material:

- repo-specific Rust rewrite guidance for the CLI and operator surfaces that ship in this worktree
- broader package-era Wave concepts and reference docs that remain useful background, but do not describe the full live Rust CLI surface

For this repo, start with the Rust-specific docs first and treat the generic package docs as supporting context.

## Suggested Structure

- `docs/concepts/`
  Mental models and architecture. Read these first if you want to understand what a wave is, how runtime-agnostic execution works, or how Context7 differs from skills.
- `docs/guides/`
  Task-oriented workflows. Use these when you need to set up the planner, choose an operating mode, or decide how to run tmux and terminal surfaces.
- `docs/reference/`
  Exact command, config, and file-format details. Use this when you need precise key names, runtime options, or bundle structure.
- `docs/plans/`
  Starter plan docs, runbooks, roadmap, and current-state pages that ship with the package and seed adopting repositories.
- `docs/research/`
  Source index for the external papers and articles that informed the harness design. Hydrated caches stay local and ignored.

## Start Here

- Current repo baseline:
  Read [implementation/rust-codex-refactor.md](./implementation/rust-codex-refactor.md) for the shipped Rust/Codex operator slice, the self-host loop, and the current compatibility boundary.
- Rust 0.2 target:
  Read [implementation/rust-wave-0.2-architecture.md](./implementation/rust-wave-0.2-architecture.md) for the post-bootstrap authority model and the later reducer cutover plan.
- Rust 0.3 carry-forward notes:
  Read [implementation/rust-wave-0.3-notes.md](./implementation/rust-wave-0.3-notes.md) for lessons from executing Wave 10 and Wave 11, the current control-plane boundary, and the extra guardrails that later architecture work should add.
- Runtime config and authority roots:
  Read [reference/runtime-config/README.md](./reference/runtime-config/README.md) for the live `wave.toml` surface, typed authority roots under `.wave/state/`, and the compatibility outputs that queue and trace commands still read today.
- Repo commands and local operator loop:
  Read [../README.md](../README.md) for the current CLI command map, self-host runbook, and repo-local constraints.
- Looking for concrete example waves:
  Read [reference/sample-waves.md](./reference/sample-waves.md) for authored-wave examples that match the current structured wave surface.
- Want the broader Wave concepts:
  Read [concepts/what-is-a-wave.md](./concepts/what-is-a-wave.md), [concepts/runtime-agnostic-orchestration.md](./concepts/runtime-agnostic-orchestration.md), and [concepts/context7-vs-skills.md](./concepts/context7-vs-skills.md) as background reference. Treat them as conceptual material, not as the current Rust CLI runbook.
- Tuning runtime behavior:
  Read [reference/runtime-config/README.md](./reference/runtime-config/README.md) and [reference/skills.md](./reference/skills.md).
- Want the research framing behind the design:
  Read [research/coordination-failure-review.md](./research/coordination-failure-review.md) and [research/agent-context-sources.md](./research/agent-context-sources.md) as supporting research input rather than operator instructions.

## Package vs Repo-Owned Material

- Generic runtime and concept docs live here under `docs/`.
- The current repo-owned operating contract lives in:
  - `wave.toml`
  - `waves/*.md`
  - `README.md`
  - `docs/implementation/*.md`
  - `skills/`
  - the repository source itself

Some docs under `docs/guides/` and `docs/plans/` describe broader or older package surfaces that the Rust CLI in this repo does not fully ship yet. Use them as background only unless the Rust-specific docs above point you there directly.
