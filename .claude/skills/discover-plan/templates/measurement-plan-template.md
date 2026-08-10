# Measurement Plan: {Title}

**Item:** B-{NNN}
**Repo:** {repo}
**Mode:** {review|live-test|bug|evolve}
**Slug:** `{kebab-case-slug}`
**Owner:** {name or handle}
**Created:** {YYYY-MM-DD}

<!--
  Every header line above is checked by check_plan_completeness.py. `Mode` must be one of
  the four literal tokens.

  This template FAILS its own checker until filled in — measured, not assumed: 8/10
  sections, plus `falsification_missing`. `Item` and `Mode` hold placeholders that are
  not a B-number and not a mode token, and the Falsification body is a TBD comment the
  checker strips before measuring.

  Deliberate in both cases. A placeholder shaped like a valid value would let a forgotten
  `B-000` pass the gate and point the plan at an item that does not exist; a pre-filled
  falsification sentence would be a commitment nobody made, which is worse than none.
-->

## Context

<!-- TBD: what the B-NNN item claims, and what state of the system made someone file it.
     One paragraph. Not the evidence — the item is a hypothesis and nothing is measured
     yet. That is the point of this document. -->

## Hypothesis

<!-- TBD: one sentence, stated so that it CAN be wrong.

     "The trace explorer is slow" cannot be wrong — slow compared to what?
     "The listing endpoint issues one query per span, so a 200-span trace costs 200
     round-trips" can be wrong, and measurement will say.

     A hypothesis nobody can refute produces a measurement that cannot fail. -->

## Falsification

<!-- TBD: what result KILLS this hypothesis. Write it BEFORE measuring.

     This section replaced the ancestor's >=2 ADR requirement, and it is the load-bearing
     part of the document. An ADR here would be premature — nothing has been measured, so
     there is nothing to decide. What makes a measurement plan honest is committing in
     advance to what would make you drop it. Without that commitment, any observation can
     be reinterpreted afterwards as confirming what was already believed.

     Be specific enough that /discover-execute can check it every iteration:

       "If the endpoint issues one query per request regardless of span count, the
        hypothesis is dead and B-NNN is killed with that result as its kill_reason."

     Also state what does NOT rescue it. A partial result is usually not a rescue: if the
     claim is about a set, finding one interesting member does not make the claim true. -->

## Measurement Questions

<!-- 3-10 questions, max 3 per corner, min 1 per corner OR a DEFER-CORNER marker.

     Tool and Target are located by HEADER TEXT, so keep those column names. Every row
     needs both: a question with no tool is a wish, and a tool with no target is a tool
     pointed at nothing.

     Verify each Target resolves BEFORE writing it here. A path that does not open caps
     the plan at 49 — and a plan that names paths nobody can open will produce fabricated
     evidence downstream. -->

| # | Question | Corner | Tool | Target | Expected answer shape |
|---|---|---|---|---|---|
| Q1 | {what will be measured} | evidence | {Read\|Grep\|Bash\|chrome-devtools} | `{path/ or URL}` | {shape of the answer} |
| Q2 | {what else touches this} | blast_radius | {tool} | `{path/}` | {caller list with file and key} |
| Q3 | {how we will know it worked} | verification | {tool} | `{path/}` | {pass/fail criterion} |

<!-- DEFER-CORNER: constraint | rules/current-constraint.md is status=undeclared and flow
     is not instrumented across the ecosystem, so the constraint relation is recorded as
     unknown in the opportunity rather than guessed here.

     This is the EXPECTED path for the constraint corner while the constraint is
     undeclared — not an exception. Inventing a constraint question to fill the slot is
     padding. Delete this marker and write real questions once a constraint is declared. -->

## Halt-loop Checkpoints

<!-- What must hold before /discover-execute can mark a question done. -->

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | Every cited target opens | Mark Qx BLOCKED with the path that failed |
| After answering Qx | The answer carries a pointer that resolves, line included | Re-iterate Qx, one retry |
| Every iteration | Re-read `## Falsification` — is it already satisfied? | Stop measuring; kill the item with its reason |
| Before promising complete | All four corners covered or explicitly deferred | Refuse the promise, keep iterating |

## Acceptance Criteria

<!-- Observable conditions for "this measurement is done". Note that a KILLED item
     satisfies this document too — the plan succeeds when the question is settled, not
     when the hypothesis survives. -->

- [ ] Every question is answered or BLOCKED with a stated reason
- [ ] Each answer carries a pointer that resolves — file exists and the line is within it
- [ ] The falsification criterion was evaluated explicitly, with its result recorded
- [ ] {mode-specific: `bug` — the failing test was RUN and its output recorded}
- [ ] {mode-specific: `live-test` — the environment-vs-product uncertainty is named}
- [ ] `/discover-confidence` on the resulting opportunity reaches SHIPPABLE_WITH_CAVEATS or better
