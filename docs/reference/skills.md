# Skills Reference

Skills are repo-owned instruction bundles used by authored waves. In this Rust/Codex rewrite, the active surface is simple and explicit:

- every agent can declare `### Skills`
- `wave lint` validates the referenced ids
- `wave doctor` validates the local skill catalog
- `wave draft` compiles the resolved wave plus skill guidance into per-agent prompts under `.wave/state/build/specs/`

The runtime-side projection model is still planned. The authoring and validation model is live now.

For current repo work, read skills as enforced authoring inputs:

- the wave names exact skill ids per agent
- lint rejects unknown ids inside `waves/*.md`
- doctor checks that the local catalog is well-formed enough for future waves to rely on
- repo guidance should mention the bundles future agents are expected to attach for common work

## Canonical Bundle Layout

Each bundle lives under `skills/<skill-id>/` and requires:

- `skill.json`
- `SKILL.md`

Current validation also expects:

- the directory name matches `skill.json.id`
- `SKILL.md` exists beside the manifest

## `skill.json`

Required fields:

- `id`
- `title`
- `description`
- `activation.when`

Useful optional fields already used in this repo:

- `activation.roles`
- `activation.runtimes`
- `termination`
- `permissions`
- `trust`
- `evalCases[]`

Minimal example:

```json
{
  "id": "repo-rust-control-plane",
  "title": "Repo Rust Control Plane",
  "description": "Repository-specific guidance for queue state and operator-facing projections.",
  "activation": {
    "when": "Attach when work changes Wave state or queue projections.",
    "roles": ["implementation", "integration", "cont-qa"],
    "runtimes": ["codex"],
    "deployKinds": []
  }
}
```

## `SKILL.md`

`SKILL.md` is the canonical procedure. Keep it:

- reusable across many waves
- explicit about ownership and proof
- short enough to stay readable
- free of wave-specific deliverable lists

If a detail is specific to one wave, it belongs in the wave prompt instead.

## Current Attachment Model

Today, this repo attaches skills directly from the authored wave:

```md
### Skills
- wave-core
- role-implementation
- runtime-codex
- repo-rust-control-plane
- repo-wave-closure-markers
```

This is the current source of truth. There is no separate `wave.config.json` routing layer in the active Rust implementation yet.

## Skills In The Wave Lifecycle

Skills are one layer of the authored-wave contract, not a parallel system:

1. the wave declares exact skill ids under each agent's `### Skills`
2. `wave lint` rejects unknown ids and still rejects the wave if the prompt, ownership, closure role, or marker contract is weak
3. `wave doctor` validates that the local catalog is usable for future waves
4. `wave draft` compiles the same wave and skill set into the prompt bundle the runtime will execute

Because the toolchain reads the same declarations end to end, a valid skill list is never enough by itself.

## Current Validation

`wave lint` enforces authored-wave usage:

- referenced skill ids must exist
- every agent, including closure agents, must still declare skills at all
- closure agents must still satisfy their own role-prompt and marker contracts
- weak authored-wave prompts are rejected even if skill ids are valid
- prompt/file-ownership mismatches and other fail-closed authored-wave gaps still block the wave

Skills do not override the wave contract. A valid bundle list does not excuse a missing `### Deliverables` section, a missing closure role, or the wrong final markers.

Skills also do not own:

- deliverables
- file ownership
- components or capabilities
- exit contract fields
- final-marker selection

Those stay in the authored wave and remain fail closed at lint time.

`wave doctor` validates the skill catalog itself:

- skills directory is readable
- `skill.json` parses
- `SKILL.md` exists
- manifest ids are unique
- manifest ids match directory names
- required manifest fields such as `id`, `title`, `description`, and `activation.when` are present

## Repo-Specific Bundles

The Rust/Codex rewrite adds repo-specific bundles so waves do not have to restate the same rules in every prompt.

### Workspace And Parsing

- `repo-rust-workspace`
  For workspace layout, crate boundaries, typed config/spec work, and CLI bootstrap slices.
  Attach this for parser, config, CLI, or crate-shape work.

### Control Plane

- `repo-rust-control-plane`
  For queue state, readiness blockers, closure coverage, and operator-facing status fields.
  Attach this when work changes `wave doctor`, `wave control status`, or planning projections.

### Codex Runtime

- `repo-codex-orchestrator`
  For the Codex-first launcher, project-scoped Codex state, and app-server/control-plane action work.
  Attach this for launcher, Codex home/state roots, or orchestration runtime work.

### TUI

- `repo-ratatui-operator`
  For the right-side operator panel, status tabs, and narrow-terminal fallback behavior.
  Attach this for `ratatui` operator surfaces and status rendering behavior.

### Closure

- `repo-wave-closure-markers`
  For final marker discipline across implementation, integration, documentation, cont-QA, and cont-EVAL roles.
  Attach this whenever marker wording or plain-line closure output matters.
  Closure-role markers are structured, not bare tokens. For example, documentation closure must end with `[wave-doc-closure] state=<closed|no-change|delta> paths=<...> detail=<...>`.
  Use `state=closed` when doc updates landed completely, `state=no-change` when none were required, and reserve `state=delta` for incomplete documentation closure that should block the wave.

Do not attach all repo-specific bundles by default. Pick the narrowest one that explains the subsystem you are actually changing.

## Core Skills

- `wave-core`
  Global ownership, proof, and closure protocol.
- `repo-coding-rules`
  Repo-specific editing and validation norms.
- `runtime-codex`
  Terminal-first Codex execution behavior.

Most implementation agents in this repo should start from:

- `wave-core`
- one role skill
- one runtime skill
- one repo-specific subsystem skill
- `repo-wave-closure-markers` when final markers are required

## Role Skills

- `role-implementation`
- `role-integration`
- `role-documentation`
- `role-cont-qa`
- `role-cont-eval`

These remain reusable role procedures. The authored wave provides the exact owned files, deliverables, Context7 query, and final markers.

## Practical Attachment Defaults

For most implementation work, start with:

- `wave-core`
- `role-implementation`
- `runtime-codex`
- one repo-specific subsystem bundle
- `repo-wave-closure-markers`

Common pairings in this repo:

- parser, config, CLI, or crate-shape work: add `repo-rust-workspace`
- queue, doctor, lint status, or planning work: add `repo-rust-control-plane`
- launcher, app-server actions, or project-scoped Codex state: add `repo-codex-orchestrator`
- operator shell and status tabs: add `repo-ratatui-operator`

Closure agents usually stay narrower:

- integration: `wave-core`, `role-integration`, `repo-wave-closure-markers`
- documentation: `wave-core`, `role-documentation`, `repo-wave-closure-markers`
- cont-QA: `wave-core`, `role-cont-qa`, `repo-wave-closure-markers`

Add more only when the closure task actually needs subsystem-specific procedure.

## When To Add Or Change A Skill

Create or update a repo-owned skill only when the same instruction needs to survive across multiple waves. Good candidates are:

- reusable repo operating rules
- subsystem-specific proof or boundary guidance
- runtime-specific execution constraints
- closure or marker discipline shared across many waves

Do not create a skill for:

- one wave's private deliverables
- one wave's owned paths
- one-off Context7 queries
- temporary coordination notes

Those belong in the wave prompt instead.

## Best Practices

- Use narrow repo-specific bundles instead of bloating the wave prompt.
- Keep skills procedural and reusable; keep assignments in the wave.
- Pair skills with a precise Context7 bundle/query for the current slice.
- Add `repo-wave-closure-markers` whenever final markers are part of the exit contract.
- Keep the skill list minimal but sufficient. Repeated repo rules belong in a bundle; one-off instructions belong in the wave prompt.
- When a new repo-specific bundle becomes a standard dependency for future work, update `README.md`, `agents.md`, and `skills/README.md` in the same change.
- Update `README.md`, `agents.md`, or `docs/implementation/rust-codex-refactor.md` when a new skill changes how future agents should work.
- Treat `wave lint` and `wave doctor` as contract enforcement, not style feedback. If either rejects a skill or wave, fix the repo state rather than documenting around it.
