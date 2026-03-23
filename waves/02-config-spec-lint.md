+++
id = 2
slug = "config-spec-lint"
title = "Implement typed config, wave parsing, and dark-factory lint"
mode = "dark-factory"
owners = ["implementation", "planner"]
depends_on = [1]
validation = ["cargo test -p wave-config -p wave-spec -p wave-dark-factory"]
rollback = ["Revert the parsing and lint crates if the new file formats prove unstable."]
proof = ["wave.toml", "crates/wave-config/src/lib.rs", "crates/wave-spec/src/lib.rs", "crates/wave-dark-factory/src/lib.rs"]
+++
## Goal
Make the new `wave.toml` and `waves/*.md` formats executable, and enforce the minimum dark-factory contract at lint time.

## Deliverables
- Typed config loader.
- Markdown wave parser with TOML front matter.
- Dark-factory lint rules for validation, rollback, proof, and closure sections.

## Closure
- `wave project show --json` prints the parsed config.
- `wave lint --json` returns an empty list for the committed waves.
- Broken sample inputs are rejected in unit tests.
