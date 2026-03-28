# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                                  # Build entire workspace
cargo test                                   # Run all tests
cargo test -p wave-trace                     # Run tests for a single crate
cargo test -p wave-results -- test_name      # Run a single test by name

cargo run -p wave-cli -- project show --json # Verify parsed config
cargo run -p wave-cli -- doctor --json       # Repo health checks
cargo run -p wave-cli -- lint --json         # Authored-wave validation
cargo run -p wave-cli -- control status --json
cargo run -p wave-cli -- control show --wave 18 --json
cargo run -p wave-cli -- control orchestrator show --wave 18 --json
cargo run -p wave-cli -- tui --help          # Operator shell flags
cargo run -p wave-cli --                     # Interactive operator shell
```

Toolchain: Rust stable, edition 2024, resolver 2.

## Architecture

Codex Wave Mode is a Rust rewrite of Wave — a multi-agent orchestration operator framework built on Codex OSS. A **wave** is a discrete, authored unit of multi-agent work defined as executable markdown specs in `waves/*.md`. Each wave declares agents, file ownership, dependencies, validation gates, and closure requirements that are consumed by parsers, linters, and the runtime.

### Crate Dependency Layers (bottom-up)

**Domain & Config** — foundational types, no business logic:
- `wave-domain` — typed string IDs (via `string_id!()` macro), enums (`TaskState`, `GateVerdict`, `ClosureDisposition`), wire-format records
- `wave-config` — parses `wave.toml`, resolves authority root paths under `.wave/state/`
- `wave-spec` — parses `waves/*.md` frontmatter + markdown into `WaveDocument` structs

**Authority Sources** — append-only JSONL logs and stored envelopes:
- `wave-events` — control-event and scheduler-event logs under `.wave/state/events/`
- `wave-coordination` — coordination records (claims, evidence, blockers, decisions)
- `wave-results` — result/proof/closure envelopes under `.wave/state/results/`
- `wave-trace` — run-record parsing and replay validation from `.wave/traces/runs/`
- `wave-gates` — planning gate verdicts and closure fact computation with compatibility adapters

**Projection & Planning** — reducer-backed read models:
- `wave-reducer` — pure reducer: gates + closure facts + lint findings → readiness state
- `wave-projections` — **authoritative projection spine**: planning, queue, control, and delivery read models (single source of truth for all status queries)
- `wave-control-plane` — re-export shim for naming compatibility during cutover

**Validation & Policy:**
- `wave-dark-factory` — fail-closed linting at authoring/draft/launch time (weak prompts, missing closure agents, bad skill IDs, marker drift)

**Runtime & Execution:**
- `wave-runtime` — launch, rerun, draft, replay, adhoc execution plumbing, MAS sandbox execution, recovery, and head control

**Presentation:**
- `wave-app-server` — assembles `OperatorSnapshot` from projection spine + run details
- `wave-tui` — Ratatui operator shell (left transcript/composer shell plus right-side `Overview`, `Agents`, `Queue`, `Proof`, and `Control`)
- `wave-cli` — `wave` binary entry point, Clap subcommands, routes to all other crates

### Key Architectural Patterns

- **Authority-driven**: canonical truth lives in append-only JSONL logs under `.wave/state/`, not computed on demand
- **Projection spine**: all read models funnel through `wave-projections`; CLI and TUI never query authority logs directly
- **Dark factory enforcement**: validation is fail-closed at three stages — lint (authoring), draft (compilation), launch (preflight)
- **Compatibility boundary**: live runs/traces feed the reducer via adapters in `wave-gates`, not direct truth — enables gradual migration
- **Closure agents**: three mandatory roles per wave — A0 (cont-qa), A8 (integration), A9 (documentation) — own gating logic
- **Operator shell**: `wave` / `wave tui` is the live operator surface, with persisted shell sessions, transcript search, compare mode, and operator/autonomous head control
- **MAS pilot**: Wave 18-era multi-agent execution is partially live for `execution_model = "multi-agent"` waves via per-agent sandboxes, recovery-required state, and reducer-backed operator views; the remaining gap is a real proof run that closes the pilot end to end

### State Roots

All operator state lives under `.wave/`:
- `.wave/state/events/control/` — control-event logs
- `.wave/state/events/coordination/` — coordination records
- `.wave/state/events/scheduler/` — scheduler authority
- `.wave/state/results/` — structured result envelopes
- `.wave/state/projections/` — reducer projections
- `.wave/state/traces/` — canonical traces
- `.wave/state/build/specs/` — compiled per-agent prompts (from `wave draft`)
- `.wave/codex/` — repo-local Codex state

### Conventions

- All domain IDs are string wrappers via the `string_id!()` macro in `wave-domain`
- Error handling: `anyhow::Result<T>` with `anyhow::Context` for chains; `thiserror` for custom error types
- Serialization: serde throughout; JSONL for append-only logs, JSON for snapshots and records
- The `services/wave-control-rust/` crate is a separate service, not part of the core operator flow
