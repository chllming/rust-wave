# Agent Context Cache Guide

This page is the tracked guide to the cached research archive under `docs/research/agent-context-cache/`.

Use it when you need:

- a stable repo-owned link target for the cache
- the crosswalk between bibliography sections and cache topic pages
- the right entrypoint into cached papers, articles, and topic indexes

For the repo-owned synthesis and bibliography, start with:

- [coordination-failure-review.md](./coordination-failure-review.md)
- [agent-context-sources.md](./agent-context-sources.md)

## What The Cache Is

The cache is the browsing layer for the research archive.

It contains:

- cached papers and reports
- cached practice articles
- topic-grouped reading lists

The cache is supporting reference material. It is not canonical repo policy.

## Entry Points

- `agent-context-cache/papers/index.md`
  cached papers and reports with fit notes
- `agent-context-cache/articles/index.md`
  cached practice articles and vendor guidance
- `agent-context-cache/topics/index.md`
  topic-grouped reading lists across the archive

## Cache Status

The repo currently includes a checked-in cache snapshot for browsing convenience. If that changes later, update this page, `agent-context-sources.md`, and `docs/README.md` together so the docs do not drift again.

## Bibliography Crosswalk

| Bibliography section in `agent-context-sources.md` | Cache entrypoint |
| --- | --- |
| Practice Articles | `agent-context-cache/articles/index.md` and `agent-context-cache/topics/harnesses-and-practice.md` |
| Planning and Orchestration | `agent-context-cache/topics/planning-and-orchestration.md` |
| Harnesses, Context Engineering, and Long-Running Agents | `agent-context-cache/topics/harnesses-and-practice.md` and `agent-context-cache/topics/long-running-agents-and-compaction.md` |
| Skills and Procedural Memory | `agent-context-cache/topics/skills-and-procedural-memory.md` |
| Agent Context Files and Configuration | `agent-context-cache/topics/repo-context-and-evaluation.md` |
| Blackboard and Shared Workspaces | `agent-context-cache/topics/blackboard-and-shared-workspaces.md` |
| Multi-Agent Orchestration and Architecture | `agent-context-cache/topics/planning-and-orchestration.md` and `agent-context-cache/topics/blackboard-and-shared-workspaces.md` |
| Security and Secure Code Generation | `agent-context-cache/topics/security-and-secure-code-generation.md` |
| Security Benchmarks and Evaluation | `agent-context-cache/topics/security-and-secure-code-generation.md` |
| Multi-Agent Security | `agent-context-cache/topics/security-and-secure-code-generation.md` and `agent-context-cache/topics/blackboard-and-shared-workspaces.md` |
| Skill Practice and Open Standards | `agent-context-cache/articles/index.md` plus `agent-context-cache/topics/skills-and-procedural-memory.md` |
| Adjacent Memory and Prompting Research | `agent-context-cache/topics/long-running-agents-and-compaction.md` plus selected entries in `agent-context-cache/papers/index.md` |

## Notes

- Topic pages are browsing aids, not a normative taxonomy.
- Some bibliography items may be intentionally uncached; the bibliography remains the source of record.
- If the cache taxonomy changes, update the crosswalk here instead of making readers infer the mapping.
