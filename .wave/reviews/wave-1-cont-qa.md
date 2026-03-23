## Wave 1 cont-QA

- `cargo metadata` matches the root workspace manifest and the crate layout in `Cargo.toml`.
- `wave --help` exposes the landed command surface, including `project`, `doctor`, `lint`, `draft`, `control`, `launch`, `autonomous`, `dep`, `trace`, and `adhoc`.
- `README.md`, `agents.md`, and `docs/implementation/rust-codex-refactor.md` all describe the same crate set and bootstrap scope.
- `cargo test -p wave-cli -p wave-config -p wave-runtime -p wave-tui -p wave-app-server -p wave-trace` passed.
- `dep` and `adhoc` are explicit stubs, and their runtime messages match the docs instead of pretending the commands are finished.

No findings.

[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=workspace manifest, CLI help, bootstrap docs, and validation all agree on the landed crate layout
Verdict: PASS
