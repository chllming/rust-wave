---
title: "Coordination Failure Review"
summary: "Repo-owned synthesis of the multi-agent coordination failures most relevant to the Wave architecture and the architectural responses this repo intends to use."
---

# Coordination Failure Review

This document is the repo-owned synthesis layer for the research bibliography in [agent-context-sources.md](./agent-context-sources.md).

It is not a paper cache, not operator runbook guidance, and not a claim that every response below is already implemented in the Rust workspace. Its job is to answer a narrower question:

**What failure modes matter most for a serious multi-agent coding harness, and what architecture does Wave need in response?**

## How To Read This

- Use this page for the architecture-level takeaways.
- Use [agent-context-sources.md](./agent-context-sources.md) for the full bibliography.
- Use [agent-context-cache.md](./agent-context-cache.md) when you want the cache crosswalk and links into cached paper/article copies or topic-grouped reading lists.

## The Core Failure Pattern

The strongest recurring theme across the bibliography is that multi-agent systems fail less from raw model quality and more from weak coordination structure:

- agents do not share the same facts at the same time
- agents make local progress without durable shared state
- orchestrators serialize work that should be parallel or parallelize work without safe ownership
- verification happens too late, after bad work has already propagated
- context, skills, and memory are injected ad hoc instead of being governed as first-class artifacts

For this repo, that means the target harness cannot just be “a launcher that starts more than one agent.”

It needs:

- canonical shared state
- reducer-backed execution truth
- explicit ownership and lease discipline
- late-bound runtime adapters
- post-slice verification gates
- explicit contradiction and human-input workflows

## Failure Families And Required Responses

## 1. Shared-State Failure

Typical failure:
- one agent discovers something important, but the rest of the system never absorbs it as durable state
- progress exists only in terminal logs, summaries, or ad hoc prose

Research themes:
- blackboard and shared-workspace systems
- distributed-information reasoning failures
- teammate-style coding-agent cooperation gaps

Wave response:
- keep canonical control events under `.wave/state/events/control/`
- keep canonical coordination state under `.wave/state/events/coordination/`
- keep result envelopes under `.wave/state/results/`
- keep boards, inboxes, summaries, dashboards, and queue views as projections only

This is why the Rust architecture keeps pushing toward reducer-backed authority instead of terminal-local truth.

## 2. Serial-Orchestrator Failure

Typical failure:
- the system has multiple agents, but the orchestrator still behaves like a serial queue runner
- parallelism is faked by narration or ad hoc process spawning rather than governed by a scheduler

Research themes:
- parallel planning and acting
- orchestration protocols
- benchmark evidence that simultaneous coordination is hard

Wave response:
- introduce a real scheduler layer
- make claims, leases, and concurrency budgets first-class state
- distinguish readiness from ownership
- allow true parallel waves only when ownership, dependencies, and budget allow it

This is the main gap between the current Rust runtime and the intended harness.

## 3. Late-Verification Failure

Typical failure:
- implementation appears complete until integration or final QA discovers hidden breakage
- the system advances because an agent emitted markers, not because the workspace stayed healthy

Research themes:
- plan-execute-verify-replan loops
- evaluation harnesses for long-running agents
- secure coding workflows with analyzer and tool feedback

Wave response:
- run mandatory post-slice gates after implementation agents
- stop advancement on broken workspace state
- make replay compare recomputed semantic state, not just artifact presence
- keep proof and closure machine-readable through result envelopes

This is why Wave 13 and later architecture notes emphasize post-agent verification and replay ratification.

## 4. Context And Skill Drift

Typical failure:
- repository instructions become bloated, stale, or inconsistent
- skill systems are treated as prompt fragments instead of maintained procedures
- runtime-specific guidance leaks into the core task model

Research themes:
- AGENTS.md and repository context files
- skills and procedural memory
- long-horizon context engineering and compaction

Wave response:
- keep repo-owned operating rules in versioned docs and `skills/`
- keep authored waves as the execution contract
- keep Context7 narrow and task-scoped
- resolve runtime-specific skill overlays late, after executor selection

This is why the intended harness needs one global abstraction for planning and skills above Codex and Claude adapters.

## 5. Contradiction Blindness

Typical failure:
- incompatible claims survive as prose instead of becoming actionable state
- integration failure is discovered, but not routed into a durable repair loop

Research themes:
- blackboard deliberation and contradiction handling
- distributed reasoning failure analyses
- human escalation and repair loops in real harnesses

Wave response:
- keep facts and contradictions as first-class authority objects
- let gates depend on unresolved contradictions
- let clarification and human-input requests stay inside durable workflow state
- route repair loops through reducer state instead of informal review notes

This is one of the places where the current domain model is ahead of the current runtime behavior.

## 6. Runtime-Coupling Failure

Typical failure:
- the orchestration model is really just one vendor runtime with some wrappers
- changing runtime means changing the task model, skill model, or queue semantics

Research themes:
- heterogeneous multi-agent assemblies
- protocol-driven orchestration
- MCP and context-aware server collaboration

Wave response:
- keep planning, task graph, gates, and reducer semantics runtime-agnostic
- isolate runtime-specific launch behavior in executor adapters
- persist runtime identity and fallback history as execution metadata, not planning truth
- make the same wave contract drive Codex and Claude

This is the central architectural rule for multi-runtime parity.

## Architecture Consequences For This Repo

The intended harness for this repository should therefore have the following shape:

1. `waves/*.md` stays the canonical declaration contract.
2. The reducer computes truth from declarations, events, coordination records, result envelopes, and later scheduler state.
3. A scheduler, not the runtime launcher, owns parallelism and leases.
4. A launcher and supervisor execute work chosen by the scheduler.
5. Codex and Claude sit behind a shared executor API.
6. The TUI, CLI, and app-server consume projections from the same reducer-backed state.

## What This Review Does Not Claim

This review does not claim that the Rust repo already ships:

- true parallel waves
- durable claim and lease coordination
- live Claude execution in the Rust runtime
- contradiction-aware closure and human-input routing end to end

Those are still target-state concerns, even though the docs and domain model already point in that direction.

## Most Relevant Reading Paths

If you want to go deeper by topic:

- shared workspaces and blackboard coordination:
  see `agent-context-cache.md` and then `agent-context-cache/topics/blackboard-and-shared-workspaces.md`
- planning and orchestration:
  see `agent-context-cache.md` and then `agent-context-cache/topics/planning-and-orchestration.md`
- repository context and evaluation:
  see `agent-context-cache.md` and then `agent-context-cache/topics/repo-context-and-evaluation.md`
- skills and procedural memory:
  see `agent-context-cache.md` and then `agent-context-cache/topics/skills-and-procedural-memory.md`
- long-running agents and compaction:
  see `agent-context-cache.md` and then `agent-context-cache/topics/long-running-agents-and-compaction.md`

## Bottom Line

The research supports the direction already visible in the Rust architecture docs:

- keep the control plane reducer-backed
- keep shared state explicit
- treat scheduling as a first-class subsystem
- keep runtime adapters at the edge
- make skills and context governed artifacts
- verify continuously, not only at the end

That is the shortest credible path from the current serial Rust launcher to a real parallel-wave multi-runtime harness.
