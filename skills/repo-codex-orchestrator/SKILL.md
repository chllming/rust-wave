# Repo Codex Orchestrator

Use this skill when work changes how this repo embeds or targets Codex OSS as the Wave operator runtime.

## Working Rules

- Codex is the first-class runtime for this rewrite. Do not preserve generic multi-runtime abstractions when they dilute the Codex-first implementation.
- Keep Codex state project-scoped under `.wave/codex/`. Do not depend on a user's global Codex home.
- Treat the right-side TUI panel and control-plane actions as part of the operator shell, not as an unrelated add-on.
- Prefer typed launcher/control actions over scraping chat output or terminal text.
- Keep non-interactive CLI and interactive TUI paths aligned around the same underlying control-plane actions.

## Integration Boundaries

- `third_party/codex-rs/UPSTREAM.toml` is the reviewed upstream pin.
- `crates/wave-runtime` owns launcher/runtime behavior.
- `crates/wave-app-server` owns app-server-facing control-plane glue.
- `crates/wave-tui` owns the operator shell and right-side panel rendering.
- `crates/wave-cli` owns operator command entrypoints and non-interactive status/reporting.

## Implementation Discipline

1. Start from the authoritative Wave state and launcher contract.
2. Thread project-scoped paths through config rather than hard-coding them.
3. Keep Codex-specific changes explicit in docs and tests so later rebases remain understandable.
4. Fail closed on missing state roots, missing proofs, or unsupported actions.
5. When a control-plane action is added, reflect it in both CLI and TUI planning docs.

## Control-Plane Bias

- The runtime should mutate authoritative state, not just print logs.
- Dashboard surfaces should subscribe to state changes rather than infer status from screen text.
- If a new Codex integration cannot produce durable state, treat it as incomplete.
