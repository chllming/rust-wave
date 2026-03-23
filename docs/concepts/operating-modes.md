# Oversight, Dark-Factory, And Human Feedback

Wave now has an explicit planning vocabulary for execution posture.

Today that posture is captured in project profile memory, planner output, wave specs, and launch preflight. `dark-factory` is no longer just a label: the runtime now treats it as a fail-closed execution profile.

## The Two Postures

- `oversight`
  Human review and intervention are expected parts of the operating model for risky work.
- `dark-factory`
  The goal is end-to-end execution without routine human intervention.

These values are stored in `.wave/project-profile.json` and emitted into planner-generated specs and wave markdown.

## What Ships Today

Today the runtime ships:

- project-profile memory for default oversight mode
- planner prompts that ask for oversight mode
- generated specs and waves that record the chosen mode
- deploy-environment memory that helps infra and release planning
- orchestrator-first clarification handling and human feedback queueing
- launch preflight reports written into the compiled bundle directory
- launch refusal before runtime mutation when required contracts are missing
- operator-visible diagnostics for the refusal path

The runtime now enforces `dark-factory` at launch time. If a wave is missing required authoring or launch data, launch stops before mutation and surfaces a preflight report instead of downgrading behavior.

## How To Interpret The Modes Right Now

Treat them as execution posture:

- `oversight`
  Default when a human operator should expect to inspect progress, answer questions, or approve risky transitions.
- `dark-factory`
  Use when the wave is authored to satisfy the fail-closed contract and should not proceed without complete machine-checkable launch data.

## Dark-Factory At Authoring Time

`dark-factory` waves must already carry the contract that launch will enforce:

- explicit deploy environments
- concrete validation commands
- rollback or recovery guidance
- proof artifacts
- closure and marker expectations
- file ownership and prompt structure that the linter can verify

If those fields are weak or missing, the wave is malformed for dark-factory authoring, not something to “fill in later” during launch.

## Dark-Factory At Launch Time

Launch now performs a preflight gate before runtime mutation:

1. the compiled bundle writes a `preflight.json` report
2. the launcher checks the report before any launch-side mutation
3. if the report is not satisfied, launch refuses closed and returns diagnostics
4. if the report is satisfied, execution continues with the compiled prompt bundle

That means dark-factory launch behavior is diagnostic first and mutation second. Operators should expect refusal when the authored contract is underspecified.

## Human Feedback Is Not The Same Thing

Human feedback is a runtime escalation mechanism, not an operating mode.

The launcher flow is:

1. agent emits a clarification request or blocker
2. the orchestrator tries to resolve it from repo state, policy, ownership, or targeted rerouting
3. only unresolved items become human feedback tickets
4. those tickets stay visible in ledgers, summaries, and traces until resolved

That means even `oversight` mode still tries to keep routine clarification inside the orchestration loop before escalating to a human.

## Oversight Mode Best Fit

Choose `oversight` when:

- deploy or infra mutation is live and risky
- the environment model is incomplete
- rollback steps are still implicit
- legal, compliance, or release decisions need explicit human sign-off
- the repo is still shaping its skills and closure rules

## Dark-Factory Best Fit

Choose `dark-factory` only when all of these are already true:

- deploy environments are typed and explicit
- runtime and credential expectations are known
- validation commands are concrete
- rollback or recovery posture is documented
- closure evidence is machine-checkable or strongly operator-visible
- missing context would be treated as a planning failure, not something to improvise live
- the wave already satisfies the authoring contract the linter and launcher expect

## Best Practice

Default to `oversight` until the repo has earned `dark-factory`.

That usually means:

- stable skills for deploy and infra work
- consistent deploy-environment naming
- strong validation commands
- reliable docs and trace review habits
- low ambiguity about who owns live mutation
- preflight failures are treated as contract failures, not launch surprises
