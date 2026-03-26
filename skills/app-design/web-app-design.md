# Web App Design Reference

This reference is for modern browser-based applications: SaaS products, dashboards, admin tools, internal apps, multi-entity CRUD surfaces, and responsive web application shells.

It is informed by current guidance patterns from sources such as Nielsen Norman Group, Material Design 3, and Shopify Polaris.

## What makes web app design different

Web apps are not just websites with forms.

They usually involve:

- repeated use
- stateful workflows
- multiple entities and views
- responsive behavior across device sizes
- asynchronous actions and background processing

That means they need stronger rules around layout, navigation, feedback, and state than content-first marketing sites.

## Core principles

### 1. Build an app shell, not a page collage

The shell should define:

- top-level navigation
- persistent context
- where actions live
- where detail appears

Users should not feel like each screen is a different product.

### 2. Responsive navigation must adapt, not degrade randomly

Navigation should change predictably by viewport:

- compact screens: navigation bar or simplified menu
- medium screens: rail or compact sidebar
- wide screens: drawer or sidebar

Keep the information architecture stable even when the component changes.

### 3. Complex apps need obvious interaction states

For any interactive control, the user should understand:

- default
- hover or focus
- active
- disabled
- loading
- error
- success

Feedback should be subtle but clear, not decorative.

### 4. Resource-heavy views need strong scanning design

For tables, queues, or indexes:

- keep columns meaningful
- make sort and filter visible
- keep row actions discoverable
- separate bulk actions from row actions

Never make users guess why a list changed.

## Recommended layout patterns

### Dashboard plus list plus detail

A strong default for serious web apps:

- navigation or context pane
- primary list or workspace
- detail or inspector pane

This works well for:

- operations products
- admin tools
- project management
- data review

### Resource index

For collections of items:

- clear title
- visible filters
- visible sort
- visible saved views where relevant
- row selection model if bulk actions exist

### Settings layout

Group settings by topic, not by backend model.

Use:

- section headings
- helpful descriptions
- clear save or autosave behavior

## State and feedback rules

### Asynchronous actions

Web apps frequently queue work in the background.

Design must distinguish:

- request submitted
- processing
- complete
- failed

Do not collapse all of those into one toast.

### Non-disruptive feedback

Use lightweight feedback for:

- save success
- copied value
- background request accepted

Use stronger inline or blocking feedback for:

- validation failures
- permission issues
- destructive confirmation
- workflow-blocking errors

### Validation

Good web app forms:

- validate inline where useful
- summarize blocking issues clearly
- preserve user input on failure
- explain the fix, not just the problem

## Navigation and wayfinding

Users should always know:

- current section
- current object or context
- current filters or scope
- whether they are editing, viewing, or creating

Good wayfinding tools:

- page titles
- breadcrumbs when hierarchy is real
- selected nav state
- visible filters and tabs

Bad wayfinding:

- relying on URL shape
- changing page titles too late
- hidden state in modals or drawers

## Responsiveness rules

Responsive web app design is not just shrinking cards.

Preserve:

- task priority
- navigation clarity
- action visibility
- readable density

Adapt:

- nav component
- pane arrangement
- column count
- inspector placement

Do not:

- hide essential actions behind unexplained overflow
- convert a desktop workflow into an unusable stacked maze

## Interaction quality

Modern web apps should feel:

- predictable
- fast
- keyboard-accessible
- consistent across repeated flows

Good interaction rules:

- focus states are visible
- keyboard path exists for core actions
- dialogs are used sparingly
- toasts are not the only record of outcome
- status changes appear near the affected object

## Accessibility baseline

Web apps must support:

- keyboard navigation
- semantic labels
- contrast
- reduced motion
- clear error messaging
- touch-friendly hit targets where needed

Do not rely on:

- hover only
- color only
- icon-only controls without labels or tooltips

## Modern anti-patterns

Reject designs that:

- make the sidebar do all the cognitive work
- hide state in tiny badges with no explanation
- use infinite drawers, sheets, and modals instead of clear page structure
- treat tables as a dumping ground for all metadata
- show loaders with no explanation of scope
- confirm background success before verification

## How to use this in reviews

When reviewing a web app surface, check:

- is the app shell coherent?
- is navigation adaptive but stable?
- are interaction states clear?
- can users scan lists and dashboards efficiently?
- are async actions honest?
- do forms prevent avoidable errors?
- does the layout remain usable across sizes?
