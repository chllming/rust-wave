# Wave 4 cont-QA

## Result

PASS. The Codex-backed launcher, app-server gate, and project-scoped Codex state roots land as one fail-closed runtime slice.

## Verified positives

- `README.md` and `docs/implementation/rust-codex-refactor.md` both describe Codex as the operator/runtime substrate, not a placeholder adapter.
- `wave.toml` pins `project_codex_home = ".wave/codex"` plus repo-local run and trace roots, so launcher state stays project-scoped.
- `docs/reference/runtime-config/codex.md` matches the live launch shape: `codex exec`, compiled bundle prompts, `last-message.txt`, and `CODEX_HOME`/`CODEX_SQLITE_HOME` under `.wave/codex/`.
- `.wave/integration/wave-4.md` and `.wave/integration/wave-4.json` both report `ready-for-doc-closure` with zero claims, conflicts, or blockers.
- `.wave/state/runs/wave-04-1774260427455.json` shows `A1`, `A2`, `A3`, `A8`, and `A9` succeeded with expected markers observed.

## Blocking findings

None.

[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=codex-first-launcher-and-repo-local-state-roots-align
Verdict: PASS
