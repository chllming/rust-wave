---
name: app-design
description: Designs and reviews modern apps, web apps, dashboards, flows, and operator surfaces using distinct guidance for general app UX, browser-based web apps, and terminal-native interfaces. Use when the user asks about app design, UX, information architecture, product surfaces, web application design, or modern interface rules.
---

# App Design

Use this skill when the work is about modern interface design rather than one specific implementation framework.

## Surface triage

Pick the right reference before making design calls:

- `app-design.md`
  Use for general product/app UX across mobile, desktop, and software interfaces.

- `web-app-design.md`
  Use for browser-based apps, dashboards, admin tools, SaaS products, responsive layouts, and multi-pane web applications.

- `tui-design.md`
  Use for terminal-native interfaces, operator shells, keyboard-heavy live system control, and text-first operational surfaces.

## Working rules

- Start by naming the primary surface type.
- Design for the user’s real operating loop, not abstract prettiness.
- Prioritize clarity of hierarchy, navigation, state, and feedback before visual polish.
- Treat loading, empty, error, success, and destructive states as first-class design work.
- Prefer patterns that make scope, status, and next action obvious.
- Separate blocking design defects from polish debt.

## Review priorities

When reviewing a design:

1. confirm the correct surface category
2. check information architecture and navigation
3. check action hierarchy and state visibility
4. check trust, recovery, and error handling
5. check accessibility, responsiveness, and consistency

## Output guidance

For design review, prefer:

- surface type
- primary tasks
- key strengths
- blocking defects
- advisory improvements
- exact next changes

For design proposals, prefer:

- user and operating context
- information architecture
- navigation model
- screen or pane layout model
- state and feedback model
- accessibility and responsiveness constraints

## References

- [app-design.md](./app-design.md)
- [web-app-design.md](./web-app-design.md)
- [tui-design.md](./tui-design.md)
