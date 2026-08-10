---
type: defaults-bundle
created_at: 2026-08-05
purpose: Fallback corner definition when the project has no `rules/discover-opportunity-*` files
---

# Default Corner Coverage (FALLBACK ONLY)

**This document is FALLBACK.** If `rules/discover-opportunity-golden-rule.md` exists, it is
the source of truth and these defaults are IGNORED.

`/discover-confidence` checks `rules/` FIRST. Only when the relevant rule files are missing
does it fall back to this file plus `../templates/rubric-opportunity.md` and
`../templates/discover-opportunity-thresholds.example.txt`.

## Four-corner coverage

Every opportunity produced by `/discover-execute` MUST populate all four corners:

| # | Corner | Required H2 section | Empty triggers |
|---|---|---|---|
| 1 | Evidence | `## Corner 1 — Evidence` | `empty_corner_evidence` hard cap (<=49) |
| 2 | Constraint Relation | `## Corner 2 — Constraint Relation` | `empty_corner_constraint` hard cap (<=49) |
| 3 | Blast Radius | `## Corner 3 — Blast Radius` | `empty_corner_blast_radius` hard cap (<=49) |
| 4 | Verification | `## Corner 4 — Verification` | `empty_corner_verification` hard cap (<=49) |

A corner is "populated" when the H2 section exists and carries non-placeholder content
(not `<!-- TBD -->`, not empty, not only headings).

## The one scoped exception

`<!-- UNKNOWN: reason -->` populates **Corner 2 only**. The constraint is a lens, not a
gate (`rules/current-constraint.md`): with no flow instrumentation across the ecosystem,
demanding a constraint claim would be answered by assertion rather than measurement.

It is deliberately NOT honoured in corners 1, 3 and 4. An opportunity whose Evidence
corner is `unknown` measured nothing; one whose Blast Radius is `unknown` is a change
nobody scoped. A global escape hatch would become the cheapest way to pass, and every
other gate would be decoration.

`<!-- DEFERRED: ... -->`, honoured by the ancestor checker, is **not honoured anywhere**.

## What each corner answers

| Corner | Question |
|---|---|
| Evidence | What was measured, in the mode's contract — `file:line`, `METHOD URL -> status`, a failing test, or a counted cost |
| Constraint Relation | Does this explore, subordinate, elevate the declared constraint — or is it local optimisation? |
| Blast Radius | What else across the ecosystem touches this |
| Verification | How we will know the fix worked (tied to the item's DoD), and where the limit plausibly moves next |
