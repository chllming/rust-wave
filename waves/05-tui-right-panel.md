+++
id = 5
slug = "tui-right-panel"
title = "Build the right-side operator panel in the TUI"
mode = "dark-factory"
owners = ["tui", "operator"]
depends_on = [3, 4]
validation = ["cargo test -p wave-tui"]
rollback = ["Hide the panel behind a feature flag until the layout and subscriptions are stable."]
proof = ["crates/wave-tui/src/lib.rs", "waves/05-tui-right-panel.md"]
+++
## Goal
Turn the current textual shell into the Codex-backed Wave operator TUI with a right-side dashboard for run, agent, queue, and control state.

## Deliverables
- Right-side panel tabs for Run, Agents, Queue, and Control.
- Live subscriptions from the control-plane status model.
- Operator actions that open the path to task, rerun, and proof control.

## Closure
- The panel renders correctly in wide terminals.
- Queue and agent state are sourced from authoritative Wave data.
- Narrow-terminal fallback is covered by snapshot tests.
