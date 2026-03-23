# Rust Wave 0.3 Notes

This document captures carry-forward notes from executing the Wave 10 and Wave 11 slices against the 0.2 architecture target. It is not the 0.3 architecture spec yet. It records what the repo and wave process proved, where the current control-plane boundary actually sits, and what 0.3 should harden.

## What The Current Process Proved

- Authored waves plus closure agents can keep implementation aligned with architecture, not just file delivery.
- Architecture-aware closure reviews work when they explicitly check `docs/implementation/rust-wave-0.2-architecture.md` and the canonical repo docs boundary.
- The Wave 11 integration failure was useful. The run stopped because canonical docs still described the pre-cutover planning model.
- The repo-local worktree remains the real authority. Any remote control surface should stay optional and non-authoritative.

## What The Current Process Still Misses

- Most architecture checks still happen late, near integration or closure, instead of continuously during implementation.
- Mid-wave regressions can still enter the worktree briefly, including source-build breakage.
- Docs drift can accumulate while code moves, then show up only at A8 or A9.
- The current `wave-control-plane` crate name overstates what exists today. The live crate is a shim over `wave-projections`, not the final semantic control plane.
- The 0.2 authority model is still only partially landed. Events, coordination records, and result envelopes are not yet the universal source of truth.

## Current Control-Plane Readout

- Today the repo has stronger projection control than semantic control.
- The current process is boundary control, not continuous semantic control.
- `wave-projections` owns reducer-backed planning, queue, and control read models.
- `wave-control-plane` is a compatibility forwarding layer.
- Closure gates can stop dishonest landing claims, but they do not yet prevent every temporary regression.
- The real 0.2 end-state control plane still requires canonical control events, coordination records, result envelopes, reducer state, gates, and derived projections.

## 0.3 Carry-Forward Goals

1. Move architecture enforcement earlier than closure.
2. Make architecture boundaries machine-checkable.
3. Make docs parity a first-class gate instead of cleanup work.
4. Keep the repo-local authority model explicit.
5. Continue the cutover from compatibility-backed runs and marker-first closure to event, envelope, and reducer authority.

## 0.3 Guardrails To Add

### Mandatory Post-Agent Gate

- After every implementation agent, run workspace build and test plus `wave doctor` and `wave control status`.
- Do not advance to the next agent while the source workspace is broken, even if the current agent emitted proof markers.
- Treat "the previously compiled binary still works" as a monitoring fallback, not as proof of correctness.

### Architecture Invariant Checks

- Add machine checks for rules that are currently only stated in docs or review prompts.
Examples:
- `wave-control-plane` must stay shim-only while `wave-projections` owns human-facing read models.
- CLI, app-server, and TUI must consume the same projection bundle or operator snapshot path.
- Compatibility run records may remain adapter inputs, but they must not become the hidden semantic planner again.

### Executable Doc-Parity Gate

- Make `docs/plans/component-cutover-matrix.json` the canonical declaration of cutover state.
- Fail closure if `README.md`, `docs/plans/*.md`, or runtime-reference docs disagree with that declared boundary.
- Promote doc drift from "cleanup" to "blocking mismatch" earlier in the run.

### Mid-Wave Integration Checkpoint

- Add an `A8-lite` or equivalent checkpoint after implementation agents and before full doc closure.
- Use it to compare reducer output, projection output, CLI status, app-server snapshot, and TUI surfaces before documentation agents start.
- Catch architecture drift while implementation context is still warm.

### Architecture-Scoped Review Inputs

- Require each wave to name the architecture sections it is advancing.
- Require A8 and A0 to review against the architecture doc and component matrix, not just the authored wave file.
- Record explicit "still compatibility-backed" versus "now authoritative" statements in closure artifacts.

### Local-First Control Service Boundary

- Keep remote control or telemetry services append and query only.
- Do not let hosted services mutate queue truth, rerun truth, closure truth, or scheduler truth.
- The repo-local event and reducer state must remain authoritative.

## 0.3 Architecture Priorities

- Finish canonical control-plane events.
- Finish durable coordination state.
- Land structured result envelopes and envelope-first closure.
- Add targeted post-agent and mid-wave gates on top of reducer and projection state.
- Move replay and proof lifecycle onto the same authority model.
- Only then call the control plane complete.

## Natural Placement In The 0.2 Waves

- Wave `12` is the natural home for machine-readable closure evidence. Result envelopes and proof lifecycle are where doc deltas, proof claims, and closure inputs should stop being plain-text-only markers and start becoming typed gate inputs.
- Wave `13` is the natural home for mandatory post-agent gates. The launcher and supervisor split should make the orchestration layer responsible for stopping after each implementation slice, running build and status checks, and refusing to advance on a broken source workspace.
- Wave `14` is the natural home for mid-wave checkpoints and selective retries. Once task graph state, retry planning, invalidation scope, and reuse rules exist, an `A8-lite` checkpoint can fail only the implicated owners instead of collapsing the whole wave into a blunt rerun.
- Wave `15` is the natural home for architecture contradictions and clarification flow. If canonical docs, component state, or implementation reality disagree, that should become durable contradiction and clarification state rather than an informal review note.
- Wave `16` is the natural home for stronger parity replay. Replay should ratify reducer state, projections, gate outcomes, and closure evidence, not just artifact presence.
- Wave `18` is the natural home for enforcing the final authority boundary. By hard cutover time, compatibility run artifacts and marker-first closure should no longer be able to re-enter as hidden authority paths.
- Wave `19` is the natural home for the planner-facing half of these ideas. That is where the planner should generate wave specs, draft packets, and ad-hoc plans that already encode the stronger architecture and gate expectations.
- Wave `20` is the natural home for historical visibility. Telemetry can record how often gates, doc-parity checks, architecture contradictions, and mid-wave checkpoints fail without making any remote service authoritative.

## Planner Strengthening From A Wave Perspective

- The planner should emit explicit architecture intent for each authored wave, including which 0.2 or 0.3 architecture sections the wave is advancing and which compatibility boundary must remain honest.
- The planner should emit expected invariants, not just validation commands. Example invariants include shim-only crates, single projection-consumer paths, and the requirement that canonical docs match the declared cutover boundary.
- The planner should emit staged gate plans. A wave spec should be able to say "run implementation checks after each implementation agent", "run integration checkpoint before doc closure", and "allow targeted retry only for the owners implicated by the failed checkpoint".
- The planner should treat the component matrix as a planning input, not only as reporting output. If a wave claims a promotion, the planner should require the matching docs and gate expectations up front.
- The planner should let project profile memory carry architecture posture defaults, such as local-first authority, required doc-parity checks, and whether a lane defaults to strict post-agent gating.
- The planner should generate closure-agent prompts that name the architecture doc and component matrix sections they must reconcile, rather than only naming the wave markdown.
- The planner should stay a declaration and gate-plan producer, not a second execution engine. The launcher, reducer, and gates should still enforce the generated contract.

## Suggested Split Between Planner And Orchestration

Planner owns:
- wave intent
- architecture sections in scope
- expected invariants
- staged gate declarations
- doc-parity expectations
- retry policy preferences at the declaration level

Orchestration owns:
- enforcing post-agent gates
- computing retry scope
- deciding which owners rerun
- evaluating contradictions and closure state
- persisting events, results, and projections

- The key rule is that the planner should describe the required control discipline in the wave contract, while the launcher and control-plane stack should enforce it.

## 0.3 Non-Goals

- Do not make the hosted control service authoritative.
- Do not treat projections as the final authority model.
- Do not allow docs to describe a more advanced state than code and gates enforce.
- Do not rely on closure-time review alone to maintain architectural integrity.
