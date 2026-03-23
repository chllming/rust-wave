Wave 6 cont-QA review

Findings:
- Dark-factory is enforced as a fail-closed launch profile, not just a label. `crates/wave-runtime/src/lib.rs` writes `preflight.json` before mutation and returns `LaunchPreflightError` when `preflight.ok` is false.
- The CLI surfaces refusal diagnostics instead of downgrading silently. `crates/wave-cli/src/main.rs` prints `launch refused for wave ...` plus the failing contract diagnostics.
- The repo guidance and mode docs now match the runtime behavior. `docs/concepts/operating-modes.md`, `docs/implementation/rust-codex-refactor.md`, and `docs/reference/repository-guidance.md` all state that preflight runs before mutation and that incomplete contracts stop launch closed.

Assessment:
- architecture: pass
- integration: pass
- durability: pass
- live: pass
- docs: pass

[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=launch preflight rejects underspecified waves before runtime mutation
Verdict: PASS
