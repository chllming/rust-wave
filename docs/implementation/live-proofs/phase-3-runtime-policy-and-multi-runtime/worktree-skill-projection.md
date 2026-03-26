# Worktree Skill Projection Proof

This note explains the execution-root proof embedded in [runtime-boundary-proof.json](./runtime-boundary-proof.json).

## Setup

The proof fixture intentionally uses two different skill catalogs:

- repo-root fixture:
  `/home/coder/codex-wave-mode/.wave/state/live-proofs/phase-3-runtime-policy-and-multi-runtime-fixture/.wave/state/live-proofs/runtime-policy-root`
- execution-root fixture:
  `/home/coder/codex-wave-mode/.wave/state/live-proofs/phase-3-runtime-policy-and-multi-runtime-fixture/.wave/state/live-proofs/runtime-policy-worktree`

The authored agent declares:

- `repo-only`
- `worktree-only`

The selected runtime is `codex`.

## Expected Behavior

Because runtime skill projection now resolves from the execution root:

- `repo-only` must be dropped
- `worktree-only` must remain projected
- `runtime-codex` must be auto-attached if it exists in the execution root

## Observed Result

The recorded runtime detail shows:

- declared skills: `repo-only`, `worktree-only`
- projected skills: `worktree-only`, `runtime-codex`
- dropped skills: `repo-only`
- auto-attached skills: `runtime-codex`

The overlay preview in [runtime-boundary-proof.json](./runtime-boundary-proof.json) also points at execution-root paths only:

- `/home/coder/codex-wave-mode/.wave/state/live-proofs/phase-3-runtime-policy-and-multi-runtime-fixture/.wave/state/live-proofs/runtime-policy-worktree/skills/worktree-only/SKILL.md`
- `/home/coder/codex-wave-mode/.wave/state/live-proofs/phase-3-runtime-policy-and-multi-runtime-fixture/.wave/state/live-proofs/runtime-policy-worktree/skills/runtime-codex/SKILL.md`

There is no projected path back to the repo-root-only bundle.

## Conclusion

Wave 15 no longer leaves repo-root skill projection split from wave-local execution. The runtime overlay, projected skill set, and actual runtime execution root now agree on one filesystem view.
