+++
id = 6
slug = "dark-factory-enforcement"
title = "Make dark-factory an enforced execution profile"
mode = "dark-factory"
owners = ["runtime", "safety"]
depends_on = [2, 3, 4]
validation = ["cargo test -p wave-dark-factory -p wave-runtime"]
rollback = ["Fall back to planning-only dark-factory semantics if the hard gates block too broadly."]
proof = ["crates/wave-dark-factory/src/lib.rs", "crates/wave-runtime/src/lib.rs", "waves/06-dark-factory-enforcement.md"]
+++
## Goal
Stop treating dark-factory as a label and instead reject launches that lack explicit environment, validation, rollback, proof, or closure contracts.

## Deliverables
- Preflight checks for launch.
- Hard launch refusal when required dark-factory data is missing.
- Clear operator diagnostics for each missing contract.

## Closure
- A fully specified dark-factory wave launches.
- An under-specified dark-factory wave is rejected before any runtime mutation.
- Failures point back to concrete missing fields in the wave spec.
