# TUI Design Reference

This reference is for terminal-native product design: operator shells, dashboards, workflow cockpits, and text-first live system control.

Use this when the product surface lives in a terminal and must work through keyboard throughput, dense information architecture, and explicit state visibility.

## What TUI design optimizes for

A TUI is for:

- live operation
- fast observation
- low-friction intervention
- keyboard-only throughput
- constrained environments such as SSH or remote shells

It is not just a CLI with boxes.

## Core rules

### 1. Operational honesty is non-negotiable

The operator must be able to see:

- what is live
- what is stale
- what is partial
- what is pending
- what failed

Never imply:

- success before confirmation
- freshness without a freshness signal
- safety without evidence

### 2. Focus must always be obvious

At all times, the user should know:

- which pane has focus
- which row or entity is selected
- where keyboard input will land

Streaming data must never steal focus.

### 3. Keyboard flow must be consistent

A serious TUI needs:

- navigation keys
- action keys
- search or command entry
- reliable escape and cancel paths

Good defaults:

- tab or shift-tab to move focus
- slash to search
- question mark for help
- escape to back out

### 4. Stable layout beats clever layout

The screen should not jump, flicker, or reflow constantly.

Prefer:

- stable pane layout
- stable columns
- clear grouping
- one-keystroke drill-down

### 5. Recovery-first UX

The user needs:

- cancel
- pause
- retry
- back
- a visible path to understand errors

## Good TUI layout patterns

### Three-part shell

Strong default:

- global context header
- main workspace
- focused detail or inspector

### Summary first

The default view should answer:

- what changed
- what is broken
- what needs action

Details should be one step away.

### Narrow terminal fallback

When width is constrained:

- stack
- trim columns
- keep semantics stable
- prefer truthful condensed output over broken split panes

## State and feedback

### Streaming

Show whether the interface is:

- live
- replaying
- paused
- lagging
- catching up

### Actions

Separate:

- requested
- running
- applied
- verified
- failed

### Errors

Errors should explain:

- what failed
- where
- what was affected
- what the operator can do next

## Visual rules

- use color semantically, not decoratively
- never rely on color alone
- avoid ambiguous-width glyphs for critical alignment
- keep symbols simple and conservative

## Anti-patterns

Reject TUIs that:

- flicker
- hide scope
- hide freshness
- overload color
- stream in ways that move the screen under the user
- provide no reliable cancel path

## How to use this in reviews

When reviewing a TUI, check:

- is the state honest?
- is focus obvious?
- is navigation consistent?
- is the layout stable?
- are streaming and action states legible?
- can the user recover from failure?
