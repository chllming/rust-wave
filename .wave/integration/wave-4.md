# Wave 4 Integration Summary

## Scope

- Wave: `4` (`codex-launcher`)
- Agent: `A8` (`Integration Steward`)
- Generated at: `2026-03-23T00:00:00Z`

## Evidence

- `crates/wave-runtime/src/lib.rs` launches agents through `codex exec`, streams prompts from the compiled wave bundle, writes `last-message.txt`, and pins `CODEX_HOME` plus `CODEX_SQLITE_HOME` to `.wave/codex/`.
- `crates/wave-cli/src/main.rs` exposes `wave launch` and `wave autonomous` as Codex-gated entrypoints and refuses non-dry-run launch when the Codex binary is unavailable.
- `crates/wave-app-server/src/lib.rs` reports launch and autonomous actions as implemented only when the Codex binary is present, so the operator shell reflects the same runtime gate.
- `wave.toml` makes the project-scoped Codex home explicit and repo-local, and `third_party/codex-rs/UPSTREAM.toml` pins the vendored Codex baseline used by the launcher substrate.
- `docs/reference/runtime-config/codex.md` matches the live launcher shape and the repo-local state contract, including the build-bundle path and per-agent terminal artifact.

## Open Claims

None.

## Conflicts

None.

## Blockers

None.

## Deploy Risks

- `repo-local` only; no live host mutation.

## Doc Drift

None.

## Decision

Codex is clearly the primary runtime, the repo-local state contract is explicit, and the launcher/operator surfaces are coherent.

[wave-integration] state=ready-for-doc-closure claims=0 conflicts=0 blockers=0 detail=Codex-first launcher and repo-local state roots are aligned
