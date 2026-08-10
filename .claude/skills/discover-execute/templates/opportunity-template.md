# Opportunity: {Title}

**Item:** B-{NNN}
**Repo:** {repo}
**Mode:** {review|live-test|bug|evolve}
**Slug:** `{kebab-case-slug}`
**Source plan:** `knowledge-base/discoveries/plans/{slug}-plan.md`
**Generated:** {YYYY-MM-DD} via `/discover-execute`
**Confidence verdict:** PROVISIONAL (awaiting `/discover-confidence`)

<!--
  Every header line above is checked by check_opportunity_completeness.py. `Mode` must be
  one of the four literal tokens — a mode invented here reads as a missing section.

  This template FAILS its own checker until filled in (8/10 sections: `Item` and `Mode`
  hold placeholders that are not a B-number and not a mode token). That is deliberate.
  Shipping placeholders shaped like valid values would let a forgotten `B-000` pass the
  gate and point the opportunity at an item that does not exist — a silent wrong answer
  instead of a loud missing one.

  If the measurement reclassified the mode, record BOTH: put the final mode on the line
  above and explain the change under Context. The filer's guess at intake is not binding,
  but a silent switch loses the information that the guess was wrong.
-->

## Context

<!-- TBD: what the item claimed, and what state of the system prompted measuring it.
     One paragraph. This is not the evidence — it is why anyone looked. -->

## Corner 1 — Evidence

<!-- TBD: the measurement, in THIS MODE's contract. Nothing else belongs here.

     review     — `path/file.ext:LINE` + the rule or principle violated + why it matters
                  HERE, not in general. Every pointer is opened before it is written down.
     live-test  — `METHOD URL -> STATUS`, console output, trace id, timing, screenshot for
                  UI. Plus, mandatory: name the uncertainty between ENVIRONMENT and
                  PRODUCT. A dev environment breaks for its own reasons.
     bug        — the numbered repro AND a test that FAILS on the current state. No
                  failing test, no bug: a defect nobody can express as a failing test is
                  not understood well enough to fix.
     evolve     — the measured cost of the status quo (N round-trips, N duplicated call
                  sites, N ms, N manual steps). A number, not an adjective.

     A pointer that does not resolve — missing file, or a line past the end of one — caps
     this opportunity at 49 (INVALID). Everything downstream treats what is written here
     as measured fact.

     If a pointer genuinely cannot be produced, mark it `<!-- BLOCKED: reason -->`. That
     is an honest gap and is not counted as fabrication. Inventing a plausible path is. -->

## Corner 2 — Constraint Relation

<!-- TBD: does this EXPLORE, SUBORDINATE or ELEVATE the declared constraint — or is it
     LOCAL OPTIMISATION? Read `rules/current-constraint.md` before answering.

     If it is `status = undeclared`, or the declaration is past its `review_on`, the
     complete and correct answer is:

         <!-- UNKNOWN: reason -->

     `unknown` with a stated reason populates this corner. It is not debt, it does not
     weaken the opportunity, and it is the honest default while flow is uninstrumented.
     A confident constraint claim nobody measured is worse than `unknown`.

     This marker works HERE ONLY. In the other three corners it does not count. -->

## Corner 3 — Blast Radius

<!-- TBD: what else across the ecosystem touches this.

     Name the repos concretely — the checker reads this section to decide whether an ADR
     is required. Naming a repo other than the one above marks the change cross-repo and
     requires an `## ADRs` section below.

     Danger is proportional to position in the dependency graph: a change in
     `theo-contracts` reaches everything downstream; one in a leaf repo reaches nothing.
     "Repo-local" is a legitimate and common answer — say it plainly rather than padding. -->

## Corner 4 — Verification

<!-- TBD: two things.

     1. How we will know the fix worked — tied to the item's `dod` in BACKLOG.md, not a
        restatement of the title. Prefer a test that fails against the current state.
     2. Where the limit plausibly moves next. Elevating one constraint relocates it;
        naming the successor is what stops the next round from being a surprise. -->

## ADRs

<!-- OPTIONAL — required only when Corner 3 names a repo other than this one.

     A repo-local fix carries no ADR requirement; demanding one for a one-line change is
     ceremony. But a change that reaches other repos decides something for THEIR
     maintainers, and shipping that unrecorded is how a breaking change arrives
     unannounced.

     ### D1 — {decision}
     **Decision:** …
     **Rationale:** …
     **Alternatives considered:** …
     **Consequences for downstream repos:** … -->

## Recommendation

<!-- TBD: what to do, scoped. NOT the patch — an opportunity carrying the fix has
     pre-empted the plan cycle and skipped its gates.

     Say explicitly what is OUT of scope and belongs to its own item. A recommendation
     that quietly widens is how a micro-evolution becomes a refactor nobody sized. -->

## Blocked questions

<!-- Every question from the plan that ended `blocked`, with its reason. Delete the
     section if there were none.

     An empty list here while the progress file records blockers is drift, and it blocks
     handoff — the post-promise check compares the two. -->
