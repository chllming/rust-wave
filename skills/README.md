# Skills

Skills are repo-owned procedural bundles that authored waves attach to agents through each agent's `### Skills` section.

In this repository, skills are already part of the active authored-wave contract. They are not future metadata or optional prompt sugar.

## Current Model

- `waves/*.md` is the canonical assignment surface.
- Each agent names the skill ids it needs under `### Skills`.
- `wave lint` rejects unknown skill ids.
- `wave doctor` validates the local skill catalog (`skill.json` plus `SKILL.md`).
- `wave draft` writes compiled prompt bundles under `.wave/state/build/specs/` using the same wave and skill declarations.
- Runtime projection into the future launcher/TUI is planned, but the current repo already treats skills as real authoring inputs.
- Repo guidance should therefore describe the bundle families agents are expected to use for common Rust/Codex work.

Skills are therefore one enforced layer of the authored-wave contract:

1. the wave declares them
2. lint validates them
3. doctor validates the catalog behind them
4. draft compiles them into runtime prompt bundles

A correct skill id does not rescue a weak wave prompt or bad ownership split.

## Bundle Layout

Each skill lives under `skills/<skill-id>/` and requires:

- `skill.json`
- `SKILL.md`

The bundle directory name must match the manifest `id`.

`wave doctor` also expects the manifest itself to be usable:

- `id`
- `title`
- `description`
- `activation.when`

## What Belongs In A Skill

Keep `SKILL.md`:

- procedural
- reusable across many waves
- specific about proofs, ownership, and handoff behavior
- free of wave-specific file lists or one-off assignments

Put wave-specific detail in the authored wave:

- owned paths
- deliverables
- Context7 query
- final markers
- required context docs

Keep the split strict. If the same repo rule appears in several waves, move it into a skill. If it only applies to one wave, keep it in that wave's prompt.

## Fail-Closed Expectations

Treat skill validation as blocking:

- `wave lint` fails when a wave references an unknown skill id or omits `### Skills`
- `wave doctor` fails when `skill.json` is malformed, required fields are missing, ids collide, or `SKILL.md` is absent
- wave-level failures still win; a healthy skill catalog does not excuse missing closure agents, weak prompts, or mismatched ownership

When the authoring contract changes, update the skill docs and the repo guidance in the same slice.

## Bundle Families In This Repo

Core:

- `wave-core`
- `repo-coding-rules`
- `repo-wave-closure-markers`

Role:

- `role-implementation`
- `role-integration`
- `role-documentation`
- `role-design`
- `role-cont-qa`
- `role-cont-eval`
- `role-infra`
- `role-deploy`
- `role-research`

Runtime:

- `runtime-codex`
- `runtime-claude`
- `runtime-opencode`
- `runtime-local`

Repo-specific Rust/Codex bundles:

- `repo-rust-workspace`
- `repo-rust-control-plane`
- `repo-codex-orchestrator`
- `repo-ratatui-operator`
- `tui-design`

These are the standard repo-specific attachments for implementation work. Use the narrowest one that matches the subsystem you are changing.

Provider bundles:

- `provider-railway`
- `provider-aws`
- `provider-kubernetes`
- `provider-docker-compose`
- `provider-ssh-manual`
- `provider-custom-deploy`
- `provider-github-release`

## Validation

Run:

```bash
cargo run -p wave-cli -- doctor --json
cargo run -p wave-cli -- lint --json
```

Current validation checks:

- manifest ids resolve
- manifest ids match bundle directory names
- `SKILL.md` exists
- required manifest fields exist
- authored waves reference only known skill ids
- implementation agents declare skills at all
- closure-role marker and prompt expectations still have to pass at the wave level

`wave lint` is still fail closed at the wave level. A valid skill id does not rescue an underspecified prompt, missing closure agents, or mismatched ownership.

When a wave definition changes materially, also run:

```bash
cargo run -p wave-cli -- draft
```

That catches drift between the authored wave and the compiled prompt bundle the runtime will execute.

## Authoring Guidance

- Use these common implementation stacks instead of inventing ad hoc combinations:
  - parser, config, CLI, or crate-topology work: `wave-core`, `role-implementation`, `runtime-codex`, `repo-rust-workspace`, `repo-wave-closure-markers`
  - queue, doctor, or planning-status work: `wave-core`, `role-implementation`, `runtime-codex`, `repo-rust-control-plane`, `repo-wave-closure-markers`
  - launcher, app-server, or project-scoped Codex state work: `wave-core`, `role-implementation`, `runtime-codex`, `repo-codex-orchestrator`, `repo-wave-closure-markers`
  - TUI or operator-shell work: `wave-core`, `role-implementation`, `runtime-codex`, `repo-ratatui-operator`, `repo-wave-closure-markers`
- Closure-role defaults stay smaller:
  - `A8`: `wave-core`, `role-integration`, `repo-wave-closure-markers`
  - `A9`: `wave-core`, `role-documentation`, `repo-wave-closure-markers`
- optional design review: `wave-core`, `role-design`, `tui-design`, `repo-wave-closure-markers`
  - `A0`: `wave-core`, `role-cont-qa`, `repo-wave-closure-markers`
- Pair narrow skills with narrow Context7 queries. Do not use a broad skill to compensate for a weak query.
- Attach `repo-wave-closure-markers` anywhere final marker discipline matters.
- Prefer the repo-specific Rust/Codex bundles over repeating crate or runtime policy in every wave prompt.
- Keep closure-role skills and authored-wave prompts aligned: A0, A8, and A9 are mandatory closure agents in active waves in this repo.
- A practical implementation-agent default is: `wave-core`, one role skill, one runtime skill, one repo-specific subsystem skill, and `repo-wave-closure-markers`.
- Do not create a new repo-specific bundle for one wave's private instructions. Put one-off guidance in that wave's `### Prompt`.
- Revisit skill docs when lint or doctor rules change. The bundle catalog and the enforcement surface need to move together.

## When To Create A Bundle

Create a new repo-owned skill only when the rule is reusable across future waves. Typical reasons:

- a subsystem now has stable proof or ownership rules
- a runtime has repeatable execution constraints
- a closure workflow needs stable marker or artifact guidance

Do not create a bundle for:

- one wave's owned files
- one wave's deliverables
- one wave's temporary workaround
- a single Context7 query

## Next Step

When launcher/runtime projection lands, the same skill catalog will feed compiled task prompts and runtime overlays. Until then, the authored wave and the local skill files are the source of truth.
