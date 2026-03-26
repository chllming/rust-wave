# App Design Reference

This reference is for modern app UX in the broad sense: product interfaces, native-feeling software surfaces, desktop or mobile flows, and general application design beyond one specific platform.

It is grounded in current design guidance such as Nielsen Norman Group's usability heuristics for complex applications and Apple's Human Interface Guidelines around navigation and app structure.

## What app design should optimize for

Good app design helps people:

- understand where they are
- understand what the system is doing
- understand what actions are available
- recover from mistakes
- complete frequent tasks with low friction

The interface should not compete with the task. It should make the task legible and safe.

## Core principles

### 1. Clear hierarchy

Every screen should establish:

- primary purpose
- primary action
- secondary actions
- current state

If everything is emphasized, nothing is.

### 2. Navigation should feel obvious

Users should always know:

- where they are
- how they got there
- how to go back
- how to move sideways to peer areas

Prefer one clear path to a destination over multiple competing routes.

### 3. Recognition over recall

Do not make users remember:

- hidden actions
- invisible modes
- current filters
- system status

Make important context visible.

### 4. Feedback must be proportional

The app should respond differently for:

- instant actions
- background actions
- risky actions
- failed actions

Request accepted is not the same as task complete.

### 5. Recovery is part of the design

Support:

- undo where possible
- back and cancel
- retry for transient failures
- clear explanations for blocking errors

## Navigation guidance

Choose a navigation model that matches the product shape:

- hierarchical
  for drill-down flows, setup flows, and nested content
- sectional or flat
  for top-level product areas
- content-driven
  for immersive reading, media, or creation environments

Good navigation rules:

- keep top-level areas stable
- avoid moving primary navigation between screens
- label destinations with task language, not internal architecture

## State model

Every important screen should handle:

- empty state
- loading state
- success state
- error state
- disabled state
- destructive state

Each of those should answer:

- what happened
- what this means
- what the user can do next

## Forms and input

Good form design:

- asks only for what is needed
- groups related fields
- uses helpful defaults
- validates early without being noisy
- explains errors near the field and at the form level when necessary

Rules:

- do not overload placeholder text as the only label
- do not hide required context until after failure
- do not use vague errors

## Action hierarchy

The screen should clearly distinguish:

- primary action
- safe secondary actions
- destructive actions
- passive metadata

Destructive actions need:

- stronger visual separation
- clear wording
- confirmation when the impact is meaningful

## Information density

Modern apps often need to be dense without becoming chaotic.

Do this by:

- strong grouping
- stable spacing
- progressive disclosure
- concise labeling
- consistent action placement

Avoid:

- card soup
- decorative complexity without hierarchy
- mixing multiple unrelated workflows on one screen

## Trust signals

Users should be able to tell:

- what data is current
- whether work is saved
- whether something is syncing
- who or what changed something
- whether an action is pending or complete

Trust improves when status is explicit, not implied.

## Accessibility and inclusion

Modern app design must assume different input and perception modes.

Always account for:

- keyboard access
- screen-reader labeling
- contrast
- motion sensitivity
- touch target size
- understandable language

Never rely on:

- color alone
- hover alone
- precise pointer interactions alone

## Consistency rules

Use one consistent pattern for:

- success feedback
- destructive actions
- save behavior
- error placement
- navigation labels
- button hierarchy

Consistency beats novelty in repeated workflows.

## Modern anti-patterns

Reject designs that:

- hide key actions behind unexplained iconography
- replace clear navigation with clever animation
- use optimistic states without honest confirmation
- bury errors in toasts
- overload dashboards with equal-weight information
- force users to rediscover the same workflow on each screen

## How to use this in reviews

When reviewing an app design, check:

- is the user’s primary task obvious?
- is navigation coherent?
- is status visible?
- are actions prioritized?
- are failure and recovery paths explicit?
- is density controlled rather than chaotic?
