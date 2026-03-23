# Repo Wave Closure Markers

Use this skill when a wave requires explicit final markers and plain-line closure evidence.

## Working Rules

- Emit only the markers your role owns.
- Put required markers on plain lines by themselves at the end of output unless the wave explicitly says otherwise.
- Do not bury required markers inside code fences, bullets, or prose paragraphs.
- Missing markers mean the attempt is incomplete even if files were changed correctly.
- Marker text must match the authored wave exactly. Do not invent synonyms.

## Marker Discipline

- Implementation agents end with:
  - `[wave-proof]`
  - `[wave-doc-delta]`
  - `[wave-component]`
- Integration steward ends with:
  - `[wave-integration] state=<ready-for-doc-closure|needs-more-work> claims=<n> conflicts=<n> blockers=<n> detail=<text>`
- Documentation steward ends with:
  - `[wave-doc-closure] state=<closed|no-change|delta> paths=<comma-separated-paths> detail=<text>`
  - use `closed` when shared-plan updates landed completely, `no-change` when none were required, and `delta` only when more documentation work remains and closure should fail
- cont-QA ends with:
  - `[wave-gate] architecture=<pass|concern|blocked> integration=<pass|concern|blocked> durability=<pass|concern|blocked> live=<pass|concern|blocked> docs=<pass|concern|blocked> detail=<text>`
- cont-EVAL ends with:
  - `[wave-eval] state=<pass|concern|blocked> detail=<text>`

Bare marker tokens are not sufficient for closure roles. If your role owns a structured marker, include the required attributes on the same plain final line.

## Before Closing

1. Re-read the authored wave's `### Final markers` section.
2. Confirm the markers match the role you are executing.
3. Confirm the marker content reflects the landed repo state, not intent.
4. Place the final markers after the substantive report body.
5. If a marker cannot honestly be emitted, state the blocker and leave the marker absent.
