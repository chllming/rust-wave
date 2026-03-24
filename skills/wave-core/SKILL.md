# Wave Core

<!-- CUSTOMIZE: Add project-specific coordination channels, artifact locations, or naming conventions below. -->

## Core Rules

- Re-read your authored-wave agent section before major work. `### Deliverables`, `### File ownership`, `### Skills`, and `### Final markers` are binding.
- Re-read the compiled shared summary, inbox, and board projection before major decisions and before final output.
- Treat file ownership, exit contracts, and structured markers as hard requirements.
- Map every deliverable and every closure claim to exact proof. Name the file path, command, marker, or artifact that proves it.
- Post coordination records for meaningful progress, blockers, decisions, and handoffs.
- Make gaps explicit with exact files, exact fields, and exact follow-up owners.
- Do not infer closure from intent alone. Closure requires proof artifacts and consistent shared state.
- Silence is not evidence. If a deliverable is not mentioned in landed artifacts, it is not done.
- When two sources conflict, prefer the one backed by landed code or durable proof over the one backed by prose.
- Later durable evidence supersedes earlier claims only when it addresses the same scope explicitly. Do not treat stale markers as current truth.

## Authored-Wave Expectations

- Active waves in this repo require closure agents `A0`, `A8`, and `A9`.
- Implementation agents are expected to finish with `[wave-proof]`, `[wave-doc-delta]`, and `[wave-component]` unless the wave says otherwise.
- Closure roles own only their closure artifacts and shared-plan surfaces. They do not silently absorb implementation work.
- Context7 defaults and skill ids in the authored wave are part of the operating contract, not optional hints.

## Coordination Protocol

1. Read the shared summary and your inbox at the start of every major step.
2. Post a coordination record when any of these occur:
   - meaningful progress on an exit contract deliverable
   - a blocker is discovered or resolved
   - a decision changes scope, ownership, or interface
   - a handoff to another agent is needed
   - a helper assignment is opened or resolved
   - a clarification is routed or answered
3. Each coordination record must include: agent id, timestamp context, topic, and actionable detail.
4. Do not batch coordination. Post records as events occur so downstream agents see them promptly.
5. When a record references another agent, name that agent explicitly.
6. Coordination records are append-only. Do not edit or delete previous records; post corrections as new records.
7. When you receive an inbox message that requires action, acknowledge it with a coordination record before proceeding.

## Ownership & Boundaries

- Only modify files you own. File ownership is declared in the wave definition under each agent.
- If you need a change in a file you do not own, open a follow-up request naming the owning agent, the exact file, and the exact change needed.
- Shared-plan docs (current-state.md, component matrix, roadmap) are owned by the documentation steward, not implementation agents.
- Implementation-specific docs (inline comments, subsystem READMEs) stay with the implementation owner.
- When ownership is ambiguous, post a coordination record requesting clarification before editing.
- Helper assignments create temporary cross-boundary access. They remain blocking until the linked follow-up resolves.
- Cross-lane dependencies require explicit dependency tickets. Do not assume another lane's state without a resolved ticket.

## Proof Requirements

- Every exit contract deliverable must have a corresponding proof artifact: a passing test, a generated file, a durable summary, or an explicit structured marker.
- Proof should be traceable line by line. When you claim an exit contract line is satisfied, name the exact artifact that satisfies that line.
- Generic claims ("tests pass", "works correctly") are not proof. Name the exact test file, command, or artifact.
- Component promotions require evidence that the component actually reached the declared level, not just that adjacent code landed.
- Runtime-facing proof must be real evidence (logs, health checks, build output), not future-work notes.
- Proof must be durable. Transient output (terminal scrollback, ephemeral logs) is not proof unless captured into a file.
- When a wave changes operator or proof surfaces, parity proof must name the authoritative producer and every touched consumer surface.
- When proof cannot be produced within the wave, record the gap explicitly with the reason and the follow-up owner.

## Closure Checklist

A wave is closable only when all nine conditions are satisfied:

1. **Exit contracts pass** -- every agent's declared exit contract deliverables are present and backed by proof artifacts.
2. **Deliverables exist within ownership** -- each deliverable lives in files owned by the agent that produced it.
3. **Component proof/promotions pass** -- promoted components reached their declared target level with evidence.
4. **Helper assignments resolved** -- every helper assignment posted during the wave has a linked resolution.
5. **Dependency tickets resolved** -- all inbound cross-lane dependency tickets are resolved or explicitly deferred.
6. **Clarification follow-ups resolved** -- every routed clarification chain has a linked follow-up that is closed.
7. **cont-EVAL satisfies targets** -- if the wave includes cont-EVAL, the eval marker shows `satisfied` with matching target and benchmark ids.
8. **Integration recommends closure** -- the integration marker shows `ready-for-doc-closure` and is not contradicted by later evidence.
9. **Documentation and cont-QA pass** -- doc closure marker is `closed` or `no-change`, and the cont-QA verdict is `PASS` with a matching gate marker.

If any condition is not met, the wave remains open. Do not approximate closure.

For waves that cut over operator status, queue, or proof surfaces, "Exit contracts pass" includes parity between the authoritative reducer or envelope truth and every user-facing consumer touched by the wave.

Closure runs in staged order:
1. Implementation and proof (all implementation agents).
2. cont-EVAL (if present) -- must report `satisfied` before integration runs.
3. Integration -- must report `ready-for-doc-closure` before documentation and cont-QA run.
4. Documentation -- must report `closed` or `no-change`.
5. cont-QA -- final verdict. Only PASS allows the wave to close.

Do not skip stages. Each stage depends on the prior stage completing.

## Structured Markers Reference

Emit markers exactly as shown. Parsers depend on the format.

| Marker | Format |
|---|---|
| `[wave-gate]` | `[wave-gate] architecture=<pass\|concerns\|blocked> integration=<pass\|concerns\|blocked> durability=<pass\|concerns\|blocked> live=<pass\|concerns\|blocked> docs=<pass\|concerns\|blocked> detail=<text>` |
| `[wave-eval]` | `[wave-eval] state=<satisfied\|needs-more-work\|blocked> targets=<n> benchmarks=<n> regressions=<n> target_ids=<csv> benchmark_ids=<csv> detail=<text>` |
| `[wave-integration]` | `[wave-integration] state=<ready-for-doc-closure\|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text>` |
| `[wave-doc-closure]` | `[wave-doc-closure] state=<closed\|no-change\|delta> paths=<comma-separated-paths> detail=<text>` |
| `[infra-status]` | `[infra-status] kind=<conformance\|role-drift\|dependency\|identity\|admission\|action> target=<surface> state=<checking\|setup-required\|setup-in-progress\|conformant\|drift\|blocked\|failed\|action-required\|action-approved\|action-complete> detail=<text>` |
| `[deploy-status]` | `[deploy-status] state=<deploying\|healthy\|failed\|rolled-back> service=<name> detail=<text>` |

- Every marker must appear on a single line.
- The `detail` field is free text but should be concise (under 120 characters).
- Only the role that owns the marker type should emit it. Do not emit markers for other roles.
- For `[wave-doc-closure]`, use `closed` when documentation updates are complete, `no-change` when nothing changed, and `delta` only when closure remains incomplete and the wave should fail.

Marker ownership:

| Marker | Emitted by |
|---|---|
| `[wave-gate]` | cont-QA role |
| `[wave-eval]` | cont-EVAL role |
| `[wave-integration]` | Integration role |
| `[wave-doc-closure]` | Documentation role |
| `[infra-status]` | Infra role |
| `[deploy-status]` | Deploy role |

When you encounter a marker in the coordination log, treat it as the authoritative state from that role. If a role emits multiple markers during a wave, the last one supersedes earlier ones.

<!-- CUSTOMIZE: Add project-specific marker types or extend existing formats here. -->

## Customization

<!-- CUSTOMIZE: Override or extend any section above. Common additions:
  - Project-specific coordination record format
  - Additional closure conditions beyond the nine listed
  - Custom marker types for project-specific workflows
  - Ownership rules for monorepo sub-packages
-->
