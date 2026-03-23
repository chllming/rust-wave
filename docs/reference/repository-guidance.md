---
title: "Repository Guidance"
summary: "Default repository-level guidance for orchestrated wave work."
---

# Repository Guidance

Use this page as the default in-repo guidance for wave agents.

## Defaults

- Keep file ownership explicit in every agent prompt.
- Prefer small, reviewable changes over broad speculative edits.
- Update impacted docs when work changes interfaces, status, sequencing, or proof expectations.
- When the repo defines a component cutover matrix, keep wave promotions, agent ownership, and shared-plan status aligned with it.
- Run the relevant validation commands for touched workspaces.
- Record blockers, assumptions, clarifications, and handoffs with `wave coord post`; treat the markdown message board as the human-readable projection of that durable state.
- Treat external docs as non-canonical unless the task is specifically about third-party APIs or tooling behavior.

## Dark-Factory Rules

When a wave or task uses `dark-factory`, treat the mode as enforced, not aspirational.

At authoring time:

- state the deploy environment, validation commands, rollback posture, proof artifacts, and ownership boundaries explicitly
- keep the prompt/file-ownership slices closed and aligned
- include the exact marker family the agent is allowed to emit
- do not rely on runtime improvisation to repair missing contract fields

At launch time:

- expect preflight to run before any runtime mutation
- expect launch to refuse closed if the wave contract is incomplete
- treat preflight diagnostics as the source of truth for why execution stopped
- do not reclassify a failed preflight as a soft warning or a partial success

## Operator Rule

If the repo is using the dark-factory profile, operators should author for the failure mode they want the runtime to enforce. Missing launch data is a spec problem, not a runtime exception to work around.
