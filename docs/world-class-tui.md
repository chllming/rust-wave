# World-Class TUI Design for Serious Systems

## What makes a TUI fundamentally different

**Core UX Philosophy**  
A production TUI sits in a weird, powerful middle ground: it’s more stateful and *instrument-like* than a typical CLI, but it’s more constrained, composable, and failure-prone than a GUI. Those constraints aren’t a handicap; they’re the whole point.

**How TUIs differ from CLIs**  
CLIs are optimized for *one-shot execution* and composition (pipes, scripts). TUIs are optimized for *operating a live system* in a tight feedback loop: triage, steer, observe, intervene, verify, repeat. That’s why a TUI needs a stable information architecture, visible state, continuous feedback, and interruptibility—not just flags and output.

A practical smell test: if a user runs the tool and then keeps it open for minutes/hours while the world changes, you’re in TUI-land.

**How TUIs differ from GUIs**  
GUIs buy you pixels, typography, and pointing devices. TUIs buy you:
- “Always there” availability (SSH, serial consoles, minimal environments).
- Predictable keyboard throughput (hands never leave the keys).
- Low-bandwidth remote operation.
- Logs/commands/data in one place without context switching.

But TUIs cost you:
- Fragile rendering surfaces (terminal capability variance).
- Limited layout + typography.
- Hard accessibility problems (colors, glyph widths, screen readers).
- Harder discoverability (unless you design it deliberately).

### When TUI is the right abstraction vs CLI vs GUI

**Choose TUI when**:
- The task is *interactive operations* over a changing system (observe → decide → act), not just “run command → read output.”  
- You need *continuous situational awareness* (dashboards, live logs, queue state, cluster state).
- Users benefit from *fast navigation across many entities* without retyping commands.
- You need *streaming and partial results* as first-class UX (long-running jobs, agents, workflows).  
- You must work well over SSH or constrained environments.

**Prefer CLI when**:
- The dominant use case is automation/scripting, not interactive steering.
- The best UX is “do one thing well” with clean stdout/stderr semantics.
- The workflow is a linear pipeline and the user wants composability.

**Prefer GUI/web when**:
- Users need multi-dimensional visualizations, rich forms, drag/drop, heavy spatial tasks, or collaborative workflows.
- You need robust accessibility features that the terminal ecosystem can’t provide reliably.

### Principles that actually matter in TUIs

**Latency perception**  
You don’t get to hide latency behind animations. People interpret “nothing happened” as “it’s broken.” Classic response-time thresholds still map cleanly onto TUIs: ~0.1s feels instantaneous; ~1s preserves flow; >10s demands progress visibility and a way to interrupt. citeturn15view0

**Trust and “operational honesty”**  
A TUI used for infrastructure or agentic systems must earn trust by being explicit about:
- What it *knows* (data freshness, last update time, source).
- What it’s *doing* (current action, where it is in a workflow).
- What it *assumes* (filters, scopes, namespaces, auth identity).
- What it *cannot guarantee* (eventual consistency, partial connectivity, stale cache).

If your TUI ever implies success while the backend failed, operators will stop believing *all* success states.

**Observability baked into UX**  
Treat the TUI as an observability surface, not a skin. Dashboards should answer the “golden signals” style questions—latency, traffic, errors, saturation—because that’s how humans quickly assess system health under pressure. citeturn16view0  
Even if you aren’t building an SRE dashboard, the mental model carries: show demand, delay, failure, and capacity.

**Cognitive load management**  
Terminals encourage density. Density is not clarity. High-quality TUIs achieve “dense but legible” via:
- strong grouping,
- stable layout,
- progressive disclosure,
- and ruthless control of what changes per frame.

### Good TUI vs bad TUI heuristics

A TUI is probably *good* if:
- The screen is stable; the user’s eyes learn where to look.
- Every destructive action is hard to do accidentally, easy to audit after.
- There is always a way out: cancel, back, quit, detach.
- Streaming output never lies; partial results are clearly marked as partial.
- The tool behaves sensibly when not in a real terminal (TTY detection, no animation spam). citeturn17view0turn8search2

A TUI is probably *bad* if:
- It flickers, jumps, or reflows constantly (layout thrash).
- It overloads color as decoration instead of meaning.
- It hides scope (namespace/project/cluster) anywhere.
- It requires memorizing 40 hotkeys with no in-product help.
- It blocks on network calls without visible progress or cancellation. citeturn15view0

## Interaction model that scales to power users

**Interaction Model**  
World-class TUIs are *keyboard-first, focus-driven, and interruptible*. If the user ever has to “hunt the cursor” or wonder “where am I typing?”, you’ve already lost.

### Keyboard-first design (mandatory patterns)

A serious TUI should have four input “lanes” that never fight each other:

1. **Navigation keys** (move focus, move selection, switch panes)  
2. **Action keys** (operate on the selected entity)  
3. **Command entry** (palette / prompt / “:” mode / search box)  
4. **Global escapes** (help, quit, cancel, back, detach)

You can implement this with different paradigms, but the separation must stay intact.

Practical guidance:
- **Reserve `Esc` and `Ctrl+C` semantics**: `Esc` should back out of transient states; `Ctrl+C` should cancel the current operation (and if nothing is running, offer to quit). Terminal line discipline maps Ctrl-C to SIGINT by default; users expect it to work. citeturn8search3turn2search1
- **Ship an in-app keybinding help overlay** (e.g., `?`) that is context-sensitive, not a static cheat sheet. Tools like K9s expose `?` for help and show context actions; that pattern works because it makes discoverability cheap. citeturn6search0turn6search17
- **Provide a “list key bindings” view** (again, `?` is common). tmux does this (`C-b ?`) and it’s one of the reasons its learning curve is survivable. citeturn14view0

### Navigation paradigms (and when they win)

You’ll usually pick one primary paradigm and optionally add a command palette.

**Vim-style (hjkl, modes)**  
Great when:
- You expect heavy text manipulation, large tables, rapid scanning.
- Your audience already lives in modal editors.

Risk:
- Mode errors. If you choose modalities, you must make mode state unmissable.

Readline’s vi-mode is a nice cautionary reference: it explicitly switches between insert and command mode via `Esc`. That clarity is what you need if you go modal. citeturn1search6turn1search2

**Tab-based / multi-pane (like tmux mental model)**  
Great when:
- You have independent contexts (dashboard vs logs vs details).
- Users want to keep multiple views hot.

tmux demonstrates why a “prefix key” can scale a large command surface without collisions: a consistent leader (`C-b` by default) gates the command namespace. citeturn14view0  
You don’t have to copy tmux, but the design lesson is: **create a safe namespace for power actions**.

**Hierarchical (tree → list → detail)**  
Great when:
- The domain has natural containment (clusters → namespaces → workloads → pods, queues → jobs → attempts).
- You want progressive disclosure without losing orientation.

Risk:
- Deep nesting can turn into “breadcrumb hell” unless you provide jump/search.

**Command palette (VS Code model)**  
Great when:
- You have lots of actions and navigation targets.
- You want fuzzy search instead of memorization.

The key insight from entity["company","Microsoft","software company"]’s VS Code guidelines: the palette is “where all commands are found,” and naming + grouping matter because discovery is search-driven. citeturn2search0turn2search4

### Focus management and the user’s mental model

A TUI must make *focus* explicit:
- Which pane has focus?
- Which widget is active (table vs input vs log tail)?
- Where will keystrokes land?

Rules that work in practice:
- **Focus should never be ambiguous.** Use at least two signals: (1) border/underline/marker, (2) cursor presence or header highlight.
- **Tab/Shift-Tab cycling is not optional** for multi-pane TUIs. Power users will still use direct bindings, but Tab is the universal “unstick me” key.
- **Focus transitions must be stable under streaming updates.** Streaming should not steal focus or reset selection unless the user explicitly opts into “follow tail.”

### Modal vs modeless: be opinionated

**Modeless by default** is safer for ops: fewer “why didn’t my key work?” moments.  
**Modal with discipline** is faster for editing and batch actions.

If you use modes, follow two hard rules:
- Mode changes must be explicit and reversible (`Esc` to exit, visible mode indicator).
- Mode must constrain damage (e.g., “selection mode” can’t execute destructive commands until confirmed).

### Interruptibility and cancellation

For serious long-running systems, cancellation is not a feature. It’s *core UX integrity*.

Design for three cancellation levels:
- **Cancel local UI work** (stop rendering a heavy view, cancel search query).
- **Cancel remote request** (cancel HTTP/gRPC, stop streaming).
- **Cancel remote operation** (tell backend to stop the job, if supported).

Terminal users expect to interrupt: Nielsen explicitly calls out “a clearly signposted way for the user to interrupt” for long delays. citeturn15view0  
At the OS level, Ctrl-C maps to SIGINT (unless you suppress it), so you should route that to cancellation semantics users recognize. citeturn8search3turn2search1

## Information architecture for complex live systems

**Information Architecture**  
Your IA is the product. Widgets are just how it leaks to the screen.

### Structuring complex system state in a terminal

Think in three layers:

**Global state (always visible)**  
- Identity + scope: “who am I?” and “where am I operating?” (cluster/project/env/namespace/account).  
- Connectivity + freshness: connected/disconnected, last update time, lag/backpressure.  
- System status summary: errors, retries, warnings.

**Session state (sticky within the TUI instance)**  
- Current workspace/view.
- Applied filters and sort keys.
- Selected entity and “follow” mode.
- Background tasks initiated in-session.

**Local state (per pane/widget)**  
- Scroll position, selection, cursor location.
- Input buffer for search/filters/forms.
- Temporary UI states (dialogs, toasts).

Why this matters: without explicit hierarchy, you get “mystery filters” and “invisible scopes,” which is how accidents happen.

### Progressive disclosure vs density (don’t be dogmatic)

Terminals can show a lot. The trap is showing too much *at once*.

A pattern that works:
- **Default view answers “what’s wrong / what changed / what needs action.”**
- Extra detail is one keystroke away (enter → detail, `l` → logs, `e` → events, etc.).
- Historical context is available but not always on screen.

For monitoring-style surfaces, borrow the “dashboard should answer basic questions” framing from the entity["company","Google","tech company"] SRE book. citeturn16view0  
This pushes you toward summaries + drilldowns rather than a wall of metrics.

### Pane layouts, dashboards, drill-down flows

Common production layouts that scale:

**Dashboard + list + detail (3-pane)**  
- Left: navigation / resource types / saved views  
- Middle: list/table with sorting/filtering  
- Right: detail inspector (tabs inside detail: summary, events, logs, yaml/json)

This is the core layout of many successful ops TUIs because it minimizes context switching.

**List + inline preview (2-pane)**  
- Middle: list  
- Bottom/right: preview that updates with selection

This works well for log/event triage.

**Split + compare**  
When users compare two entities (two nodes, two runs, two deployments), provide explicit compare mode rather than forcing mental diff.

### Handling real-time vs historical data

Real-time data needs different UX contracts than historical:
- Real-time views must expose **update cadence** and **dropped frames / missed events**.
- Historical views must expose **time range** and **sampling/aggregation**.

Don’t blur them. If a user thinks they’re looking at “now” but you’re showing cached “5 minutes ago,” trust collapses.

### State + identity for long-running sessions

**Multi-session / Long-running UX**  
In long-lived TUIs, users detach, reconnect, and resume workflows. You need visible session continuity:
- session name/id
- whether the view is “live” or “replay”
- what was pending when disconnected

tmux is the canonical mental model: sessions can detach and reattach, including via external events like SSH disconnection. citeturn14view0  
Even if you don’t implement tmux-like multiplexing, “resume after interruption” must be a design goal, not a bug fix.

## Visual design system for terminals that holds up under stress

**Visual Design System**  
Terminals constrain you to a grid of cells with finicky color and Unicode behavior. Treat that as a real design system problem: define primitives and tokens, then stick to them.

### Typography and text constraints

You effectively have:
- one font (monospace),
- limited emphasis (bold/underline/inverse),
- and lots of environments where bold is just “bright color” instead of a heavier weight. citeturn8search9

So:
- Prefer **spacing + alignment + grouping** over “typographic hierarchy.”
- Use **case and punctuation** consistently (e.g., `ENV: prod`, `NS: payments`).
- Avoid relying on subtle emphasis differences (italic often unsupported).

### Unicode width, alignment, and why it bites

If your UI aligns columns, you must treat character width as a first-class concern:
- East Asian Width rules exist because some characters are inherently wide; ambiguous-width characters can render as 1 or 2 cells depending on environment. citeturn4search5turn4search1
- If you naïvely count Unicode code points, your table alignment will break in CJK locales and with emoji.

Design implication:
- Either restrict your “guaranteed alignment glyph set,” or implement width-aware layout using a wcwidth-like algorithm and accept that some terminals differ. citeturn4search1turn4search9

Also: terminals and tooling can be vulnerable to text-rendering tricks (e.g., bidirectional override characters). For operator tools, consider visibly marking suspicious control characters in logs and diffs. citeturn4search6turn4search18

### Color usage: semantic beats decorative

Use color with intention; if everything is colored, nothing is. citeturn17view0

A practical semantic palette (roles, not hues):
- **Critical** (data loss / outage / destructive action armed)
- **Warning** (degraded, risky, partial)
- **Success** (completed, applied)
- **Info** (context, metadata)
- **Muted** (secondary text, timestamps)
- **Focus** (active pane, current row)
- **Selection** (selected items, multi-select state)

Rules that keep you out of trouble:
- **Never encode meaning only in color.** Use icons/symbols + text labels as redundancy.
- **Respect no-color and non-TTY contexts** (especially if parts of your TUI can output to logs). Disabling color when not in a TTY is a widely adopted CLI norm. citeturn17view0turn8search2
- **Constrain yourself to a lowest-common-denominator theme** unless you can detect and manage truecolor/256-color support reliably. Terminals historically started at 8/16 colors; 256-color and 24-bit exist but are not universal, and palettes vary. citeturn8search9turn7search11

Accessibility baseline: contrast still matters in terminals. WCAG contrast guidance (e.g., 4.5:1 for normal text) is a useful target when you design themes, even if terminals aren’t “web.” citeturn8search0turn8search4

### Layout primitives: boxes, spacing, alignment

Pick a small set of primitives and use them everywhere:
- **Panels**: titled boxes with consistent padding
- **Lists/tables**: fixed header, scroll body, steady column alignment
- **Status line**: one-line global context + alerts
- **Toasts**: transient confirmations (non-blocking)
- **Dialogs**: blocking confirmations/forms (rare; only for high risk)

Boxes are not decoration—they are boundary markers for cognition. But don’t over-box: too many borders create visual noise and shrink usable space.

### Symbols, icons, and ASCII/Unicode

Unicode box drawing and symbols can massively improve scanability, but be disciplined:
- Use a **restricted icon set** (status dots, arrows, check/cross, warning triangle).
- Avoid emoji for core meaning; they widen unpredictably and render inconsistently. citeturn4search1turn4search5
- Provide ASCII fallbacks.

### Limited width and resizing

Resizing is constant in terminal life. Handle it like a first-class event:
- On window size changes, the OS signals SIGWINCH to the foreground process group. citeturn2search3
- Your layout must degrade gracefully: collapse columns, elide text, switch to stacked panels.

Never respond to a resize by corrupting the screen and hoping the user will redraw. Make it automatic.

## Feedback, trust signals, and recovery-first UX

**Feedback & State Visibility**  
This section is about operational honesty: what you show when things are slow, partial, or failing.

### Loading, streaming, and partial results

Use Nielsen’s timing model mechanically:
- **<0.1s**: just do it; no spinner. citeturn15view0
- **0.1–1s**: subtle “working” indicator if needed; don’t flash.
- **1–10s**: show activity + what is happening.
- **>10s**: show progress and provide interrupt/cancel. citeturn15view0

For unknown workloads, percent-done may be impossible; then show “work completed so far” (e.g., “Fetched 37/?? streams; last: node-12”). Nielsen explicitly recommends this style of progress when total work is unknown. citeturn15view0

For streaming data:
- Clearly distinguish **steady state** vs **catch-up** (replay).
- Show backpressure: “dropping updates” or “render throttled.”
- Offer “pause” / “freeze view” so users can read without the UI moving under them.

### Error states and recovery UX

Design for failure as normal:
- Networks drop.
- Auth expires.
- Backends lie (eventual consistency).
- Streams stall.
- Partial writes happen.

Your error UX must answer:
1. What failed?
2. What was the scope (what did it affect)?
3. Is the system safe right now?
4. What can the user do next (retry, rollback, inspect logs)?

Use the SRE framing: separate symptom (“what’s broken”) from cause (“why”). citeturn16view0  
A good TUI keeps those distinct in messaging and in UI surfaces (e.g., banner shows symptom; detail panel shows probable causes with evidence).

### Success confirmation patterns (and avoiding “fake success”)

Success UX must be proportional to consequence:
- For low-risk actions: toast “Saved” + last-updated marker.
- For high-risk actions: show a **receipt**: what changed, where, by whom, correlation id, and where to audit.

“Fake success” is deadly in ops tools. If a command is asynchronous, say so:
- “Delete requested” is not “Deleted.”
- “Applied config” is not “Rolled out.”

Make this visible in state: pending → in progress → verified.

### System trust signals: show work, not just claims

Trust signals that matter in production TUIs:
- **Freshness**: last update time; stream lag.
- **Source**: which API / cluster / agent provided the data.
- **Auth identity**: current user/role/context.
- **Consistency**: whether data is cached, eventual, or authoritative.
- **Audit hooks**: correlation IDs, links/hints to logs or traces.

Even in terminal space, hyperlinks are increasingly supported via OSC 8 (supported by VTE/iTerm2, etc.), so you can embed clickable “open in web” links where available. citeturn11view0turn4search4turn4search0  
But always provide a plain-text fallback.

### Error handling & recovery UX (retry, resume, replay)

You want three recovery patterns:

**Retry (same request)**  
- Offer immediate retry with exponential backoff only if the error indicates transient failure.

**Resume (continue an operation)**  
- If a long-running backend job continues after disconnect, the TUI must reconnect and reflect current job state (not restart blindly).

**Replay (reconstruct context)**  
- Provide a local event log panel (“what just happened”) so an operator can reconstruct actions and failures without external tooling.

This is where “logs as event streams” thinking is useful: treat logs/events as a stream you can view, filter, and ship elsewhere. citeturn3search1

### Operator error vs user error

Stop blaming the user. Categorize errors by who can fix them:

- **User-fixable**: validation, missing input, wrong scope (“namespace not selected”).
- **Operator-fixable**: permissions, quotas, cluster health.
- **System-fixable**: transient backends, timeouts.

Your copy and UX should reflect this (actionable steps vs “call support”).

## Performance and responsiveness engineering for TUIs

**Performance & Responsiveness**  
A TUI that “feels slow” is usually failing at *perceived latency* and *render stability*, not raw CPU.

### Perceived vs actual latency: what to optimize

Per Nielsen’s thresholds, your UI must provide **some** feedback within 0.1–1s to preserve flow. citeturn15view0  
This is often more important than shaving 50ms off backend calls.

Practical tactics:
- Optimistically render skeleton rows.
- Stream partial results (first 20 rows) while fetching full data.
- Defer expensive formatting until idle.

### Streaming vs batching

Batching is simpler; streaming wins for ops:
- Streaming supports early triage (“I already see the broken node”).
- Streaming reduces “is it hung?” anxiety. citeturn15view0

But streaming can create scroll chaos—so always:
- Pin selection.
- Support pause/follow modes.
- Rate-limit reflows (render throttling).

### Rendering strategies: full redraw vs diffing

Most serious TUIs should be built on a cell-based diffing model, not ad-hoc printing:
- curses-style libraries maintain a virtual screen and update the physical screen efficiently. citeturn0search10turn0search6
- Efficiency patterns like `wnoutrefresh` + `doupdate` exist specifically to batch multiple window updates into one terminal flush. citeturn0search10turn0search6turn0search14

Design implication:
- **Treat “frame render” as a transaction.** Build next frame in memory, diff, flush once.
- Separate “state updates” from “render updates.” Never let a backend callback directly print.

### Avoiding flicker and layout thrash

Flicker is usually caused by:
- clearing the screen too often,
- reflowing layouts on every small data change,
- printing variable-width lines without stable padding.

Rules:
- Keep column widths stable during a session (or change them only on explicit resize/layout mode).
- Use ellipsis + horizontal scrolling instead of constantly recomputing widths.
- Redraw at a fixed tick (e.g., 30–60 fps max, often less), independent of event arrival rate.

### Handling slow backends gracefully

Slow backends are normal. Your TUI must behave like a resilient client:
- Timeouts with clear messaging.
- Retry controls.
- Cached last-known-good views (marked as stale).
- Degraded mode: fewer columns, reduced refresh rate, paused expensive panels.

Also: if you use the alternate screen buffer, know what you’re trading away. In xterm, alternate screen disables scrollback and saved lines, which can harm debugging if the user expects to scroll back after exit. citeturn9view0  
So: use alternate screen for true full-screen apps, but provide explicit “export logs / copy view” affordances.

## Implementation guidance, advanced patterns, anti-patterns, and concrete examples

**Implementation Guidance**  
This is less about picking a library and more about picking a *model* that won’t collapse under streaming, concurrency, and operator pressure.

### Recommended architectural approaches (opinionated)

A world-class TUI architecture is typically:

**Event-driven + unidirectional data flow**
- Inputs (keys), backend events (streams), and timers produce messages.
- Messages update state.
- State renders to a frame.

This is why frameworks like Bubble Tea explicitly model themselves on The Elm Architecture, which enforces a clean loop for stateful TUIs. citeturn6search3turn6search16

This style scales because:
- you can test state transitions without a terminal,
- you can throttle rendering,
- you can keep cancellation consistent.

**Layer your state**
- Domain state (truth from backend, versioned, timestamped)
- View state (selection, filters, pane focus)
- Effect state (in-flight requests, retries, cancellation tokens)

**Make effects explicit**
Never let random code paths “just call the backend.” Effects should be tracked so the UI can show what’s running and let users cancel.

### Library selection (guidance, not a shopping list)

Pick libraries based on the rendering + input model you need:

- If you want classic diffed cell rendering + portability across terminals, lower-level terminal abstractions like tcell are built as “cell based view” systems and integrate terminfo. citeturn7search3turn7search11turn13search14  
- If you’re in Rust, libraries like Ratatui provide widget-driven TUIs; it explicitly positions itself as a lightweight widget toolkit for complex TUIs. citeturn3search16turn3search20  
- For Python, prompt_toolkit is designed for full-screen terminal apps with explicit layout + key bindings. citeturn6search2turn6search15  
- If you want modern terminal features (vivid color, more advanced rendering, even multimedia in capable terminals), Notcurses is explicitly built for “complex TUIs on modern terminal emulators,” pushing beyond classic curses constraints. citeturn3search6turn3search2

The decision is architectural:
- **Do you need a widget framework** (tables, panes, dialogs) or just terminal control?
- **Do you need async input and streaming** without blocking?
- **Do you need cross-platform Windows terminal behavior** (then pick libraries that explicitly support Windows terminals). citeturn7search2turn7search3

### Terminal semantics you must respect

**TTY detection**  
If the program isn’t attached to a terminal, don’t try to run a TUI. `isatty()` exists for a reason. citeturn8search2  
Fail fast with a clear message.

**Raw vs canonical mode**  
Full-screen TUIs usually need noncanonical input mode; canonical mode buffers until newline. termios documents this distinction explicitly. citeturn2search2

**Resize handling**  
SIGWINCH is not optional; window size changes generate it. citeturn2search3turn0search19  
Treat resize as a re-layout + re-render event, not a glitch.

**Bracketed paste**  
If you have text inputs, bracketed paste prevents paste from being misinterpreted as typed keystrokes; xterm’s control sequences describe how pasted text is bracketed (`ESC [ 200 ~ ... ESC [ 201 ~`). citeturn9view0  
Implement it or accept that paste UX will be brittle.

### Testing TUIs (how to make it real)

Do not rely on “it looks fine on my terminal.”

A serious approach:
- **Pure state reducer tests** (message → new state).
- **Golden frame tests**: render state to a buffer and diff against expected frames (with width variants).
- **Property tests for layout invariants** (no overlap, no negative widths, stable truncation).
- **Input fuzzing** for key sequences and focus transitions.
- **Integration tests in pseudo-terminals (pty)** to validate raw mode, resize, Ctrl-C, and escape sequence hygiene.

### Accessibility considerations (terminal reality, not wishful thinking)

Terminal accessibility is hard, but you can still do real work:
- Provide a **high-contrast theme** and honor contrast principles (WCAG contrast guidelines are a good target). citeturn8search0turn8search4
- Provide **no-color mode** and avoid color-only meaning. citeturn17view0
- Offer **text export**: copy current panel, dump selected rows as JSON, etc.
- Avoid ambiguous-width glyphs for critical alignment (Unicode width issues are real). citeturn4search5turn4search1

### Advanced patterns (that actually work)

**Command palette (VS Code–style, adapted to terminal)**  
Borrow the design, not the pixels:
- Fuzzy search over commands and targets.
- Clear command naming + categories matter because discovery is search-driven. citeturn2search0turn2search4
- Support “primary action” shortcuts (enter executes) and show keybinding hints.

For navigation/search inspiration, fzf shows how powerful fuzzy selection + key bindings can be in terminal workflows. citeturn12search0turn12search14

**Split views and dashboards**  
Use splits for *independent time domains*: e.g., left is stable inventory (pods), right is streaming logs. Never let the log stream reorder the inventory list.

**Logs vs structured views**  
Treat logs as an event stream and provide:
- raw tail (like `less` follow mode),
- structured extraction (fields, severity),
- jump-to-related entity.

Even `less` documents an explicit “follow” mode (`F`) and how to interrupt it, which maps cleanly to “tail but stoppable.” citeturn5search4turn15view0

**Search and filtering UX**
- Always show active filters in the global header.
- Make “clear filters” a single keystroke.
- Provide incremental search within tables (`/`) and persistent filter queries (like `F4` filter in htop). citeturn5search11

**Inline editing and forms**
- Keep forms narrow and explicit; avoid “spreadsheet editing” unless unavoidable.
- Validate as-you-type *without* being noisy.
- Make submit/cancel keys consistent across all forms.

### Anti-patterns (blunt, because these waste years)

**Anti-Patterns**

1. **Rainbow dashboards**: color everywhere, meaning nowhere. Users stop seeing it. citeturn17view0  
2. **Streaming that steals focus**: selection jumps because data refreshed. This is infuriating and dangerous.  
3. **Modal labyrinth**: 6 modes, no visible mode state, and `Esc` sometimes exits and sometimes deletes. If you do modes, be explicit. citeturn1search6  
4. **Fake success**: “Done!” while the backend job is queued or failed. This destroys trust permanently.  
5. **Silent failure**: errors logged nowhere, UI just stops updating. At minimum, surface “stale / disconnected / last update.”  
6. **Hard-coded width assumptions**: works at 120 cols, breaks at 80; breaks harder in CJK due to width ambiguity. citeturn4search5turn4search1  
7. **Full-screen abuse that nukes scrollback** without compensation (export/copy). Alternate screen buffers can disable scrollback in terminals like xterm. citeturn9view0  
8. **No escape hatch**: Ctrl-C doesn’t cancel; the only option is killing the terminal. Users will do exactly that and you’ll lose state. citeturn8search3turn15view0  
9. **Action keys that change per view with no help**: muscle memory breaks; misfires happen.  
10. **Over-clever glyph art**: fancy borders that don’t align everywhere, or icons that render double-width. “Looks cool” is not a UX requirement. citeturn4search1turn4search5

### Example patterns (concrete, copyable)

image_group{"layout":"carousel","aspect_ratio":"16:9","query":["htop terminal screenshot tree view","k9s terminal UI screenshot","lazygit terminal UI screenshot","tmux panes status line screenshot"],"num_per_query":1}

#### Good dashboard layout (3-pane, ops-ready)

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ SCOPE: prod / cluster-a / ns=payments     AUTH: oncall@...     LIVE ● 1.2s lag │
│ Alerts: 2 critical, 5 warn   Last refresh: 12:03:41   Stream: connected        │
├───────────────┬───────────────────────────────────┬─────────────────────────┤
│ Views         │ Workloads (sorted: errors desc)    │ Inspector               │
│ [ ] Pods      │ NAME          READY  CPU  ERR  AGE │ payments-api            │
│ [ ] Deploys   │ api-7f9...    2/3    61%   12  14m │ Status: Degraded ⚠      │
│ [x] Services  │ worker-3a1... 1/1    12%    0  2h  │ Restarts: 12 in 10m     │
│ [ ] Nodes     │ ...                               │ Recent events           │
│ Saved:        │ / filter: "api"   q clear filter  │ - probe failed ...      │
│  * Hot        │ enter open   a actions   l logs    │ - rollout in progress   │
├───────────────┴───────────────────────────────────┴─────────────────────────┤
│ F1 Help  / Search  : Command  Tab Next pane  Esc Back  Ctrl+C Cancel  q Quit │
└─────────────────────────────────────────────────────────────────────────────┘
```

Why it works:
- Scope/auth/freshness are *always visible* (trust surface).
- The list is stable (sorted + filtered explicitly shown).
- Actions are discoverable (footer hints + `a` actions + `:` palette).

#### Good log viewer (structured + tail control)

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Logs: payments-api  | level>=WARN | follow: ON | paused: OFF | buffer: 12k   │
├─────────────────────────────────────────────────────────────────────────────┤
│ 12:03:31.882 WARN  timeout calling billing svc  req_id=...  retry=1/3        │
│ 12:03:32.104 ERROR payment failed               req_id=...  code=DECLINED    │
│ 12:03:33.019 WARN  retrying...                  req_id=...  backoff=500ms    │
│ ...                                                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│ Space pause  f follow  / search  n next hit  F filter  c copy line  Esc back │
└─────────────────────────────────────────────────────────────────────────────┘
```

Why it works:
- “Follow/paused/buffer” are explicit (stream honesty).
- Search/filters are inline (fast triage).
- Copy is a first-class action (operators paste evidence constantly).

#### Good command interaction (palette + safe defaults)

```text
: > restart deployment…
  Restart deployment: payments-api
  Restart deployment: payments-worker
  Rollback deployment…
  Scale deployment…
  Open logs…
  Open events…
```

Design notes:
- Commands are verbs + objects.
- Dangerous commands are labeled clearly and can require confirmation.
- Fuzzy search makes discovery cheap (VS Code-style guidance: naming/grouping matters). citeturn2search0turn2search4

#### Good error display (actionable + scoped)

```text
┌────────────────────────────── Error ────────────────────────────────┐
│ Request failed: list pods (ns=payments)                              │
│ Cause: permission denied (403)                                       │
│                                                                      │
│ You can:                                                             │
│  • Switch namespace (n)                                              │
│  • Re-authenticate (r)                                               │
│  • View auth context (i)                                             │
│  • Retry (Enter)                                                     │
│                                                                      │
│ Ref: corr_id=7c1f…   time=12:03:44                                   │
└──────────────────────────────────────────────────────────────────────┘
```

Why it works:
- It tells you what failed and where (scope).
- It offers next actions, not just blame.
- It provides a correlation id for auditing.

#### Bad vs good comparison (what to stop doing)

Bad (lies, unstable, no escape):

```text
UPDATING.....DONE!
[screen flickers constantly, selection jumps, no scope shown]
```

Good (honest, stable, cancellable):

```text
Syncing workloads… 37 received (stream)   last=worker-3a1…   lag=1.2s
Ctrl+C cancel   Space pause   i details
```

This maps directly to response-time guidance: for longer operations, show progress and allow interruption. citeturn15view0