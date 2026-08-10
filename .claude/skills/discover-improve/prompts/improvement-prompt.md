# Discover-Improve Halt-Loop Driver Prompt

You are mid-improvement, iteration {ITERATION}. The user invoked `/discover-improve {SLUG}` to lift an opportunity's `/discover-confidence` score to `{TARGET}`.

**Opportunity:** `{OPPORTUNITY_PATH}`
**Current verdict:** `{CURRENT_VERDICT}` (score `{CURRENT_SCORE}`)
**Target:** `{TARGET}`

You are improving how the finding is **argued**, never what it **claims**. The measurement is over; this phase does not get to add to it.

## The boundary — read before every edit

| Section | May you edit it? | Why |
|---|---|---|
| `## Corner 1 — Evidence` | **NO** | The record of a measurement. Editing it falsifies findings, and nothing downstream can tell. |
| `## Corner 2 — Constraint Relation` | Only formatting | `<!-- UNKNOWN: reason -->` is a complete answer. Do not "upgrade" it into a claim. |
| `## Corner 3 — Blast Radius` | Only from evidence already gathered | Never invent a consumer nobody looked for. |
| `## Corner 4 — Verification` | Yes | This is where vagueness actually lives, and where sharpening pays. |
| `## Recommendation` | Yes | An argument, not a record. |
| `## Context` | Yes | Background prose. |

If improving a corner would require knowing something nobody measured, **stop and report it as unfixable**. That is a correct outcome, not a failed iteration.

## Your contract for this iteration

1. **Re-read the current score report.** Work the largest detractor first — the caps ranked by how much score they hold down.

2. **Pick ONE fix.** One per iteration keeps the effect of each change measurable. A batch that lifts the score leaves you unable to say which edit did it.

3. **Apply it, respecting the boundary above.**

   Fixes that are usually available:

   - **Vague Recommendation** → scope it. Name what is IN and what is OUT and belongs to its own item. A recommendation that quietly widens is how a micro-evolution becomes a refactor nobody sized.
   - **Verification with no criterion** → tie it to the item's `dod` in `BACKLOG.md`. Prefer a test that fails against the current state.
   - **Verification with no successor limit** → name where the limit plausibly moves once this is fixed.
   - **Smell density** → weak imperatives and loopholes, in the editable sections only.
   - **Missing mandatory section** → add the header and fill it, but only if the content is already implied by what was measured. An empty header added to satisfy a counter is worse than a missing one: it converts a visible gap into an invisible lie.

4. **Re-score.** Run `/discover-confidence {SLUG}`. Record the new score.

5. **Halt check.** If the verdict on disk is at or above `{TARGET}`, emit `<promise>OPPORTUNITY_IMPROVED</promise>`. Otherwise STOP — the loop resumes.

## Inviolable rules

- **Never edit `## Corner 1 — Evidence`.** Not to fix a typo, not to strengthen a phrase, not to make it read better.
- **Never write `<!-- BLOCKED: ... -->` onto an unresolvable pointer.** A marked pointer leaves `fabricated` and enters `explicitly_blocked`, so `fabricated_evidence` stops firing. Writing one here is bypassing the cycle's most important hard cap by hand — the same reason the deterministic fixer had that behaviour removed.
- **Never add a claim about the system that was not measured.** Whatever corner it would improve.
- **Never substitute `may`/`might`** in descriptive prose. "The endpoint may return 500 under load" is a measured fact; "must return 500" is a different, false claim.
- **Never emit the promise on a partial improvement.** The score on disk in THIS iteration decides — not the direction of travel, not the expectation that the next edit will get there.
- **Never touch a governed repo.** This phase edits one document.

## Stop conditions

Report honestly and stop — do NOT emit the completion promise — when ANY of:

1. **The remaining cap is unfixable by editing.** `fabricated_evidence`, `empty_corner_evidence`, `empty_corner_blast_radius`, `no_adr_on_cross_repo_change`. Each needs a re-measurement or a human decision. Name the cap and the real fix.
2. **Three consecutive iterations with no score improvement.** The remaining distance is not editorial.
3. **The only path left would require inventing a claim.** Say so plainly.
4. **The score went DOWN.** Revert the last edit and report — an improvement loop that degrades the artifact has a defect in the loop, not in the artifact.

An honest "this cannot be lifted by editing" beats a promise emitted on a score that never reached the target.
