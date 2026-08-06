---
name: discover-plan
version: 0.2.0
requires: []
description: 'Turn a raw B-NNN item into a measurement plan: what will be measured in OUR system, with which tool, against which target, and what result would kill the hypothesis. Use this whenever someone wants to check whether a suspicion about our code or runtime is actually true, before writing any fix, and whenever an item needs to move from hunch to evidence. Never a study of how other projects solved it — prior art cannot be evidence here.'
user-invocable: true
allowed-tools: Read Glob Grep Bash Write
argument-hint: "B-NNN [--mode {review|live-test|bug|evolve}]"
---

# Discover-Plan — Plan a Measurement

Turns a `raw` backlog item — a hypothesis nobody has measured — into a **measurement plan**: what will be measured, where, with which tool, and what result would kill the hypothesis. The output is the input for `/discover-execute`.

Sibling of `/to-plan` — same backbone, different output. `to-plan` produces implementation plans; this produces measurement plans whose deliverable is an **opportunity** (or a killed item).

The ancestor of this skill planned an investigation into *other people's* code. This one plans an investigation into ours. That is the whole difference, and it changes every step below.

## When NOT to invoke

Invoke `/discover-plan B-NNN` when:

- The item is `status: raw` in `BACKLOG.md` and unclaimed.
- You are about to measure and want the measurement to be falsifiable before it starts.

Do NOT invoke when:

- The item is already `triaged`, `planned` or `shipped` — re-measuring a closed item is how duplicate work enters.
- **`--mode bug` and a failing test already exists.** Take the fast lane straight to `/discover-execute`: a test that fails on the current state is a stronger measurement plan than any document describing one, and it is verifiable by execution rather than by review. The fast lane is unavailable when the repro is not yet a test — "I can reproduce it by hand" is a plan, not a measurement.

## Cycle contract

This skill is **phase 1** of [`cycle-discover`](../../rules/cycle-discover.md). The cycle rule is the **source of truth** for the four modes and their evidence contracts, chain order, hard gates, stop conditions, anti-patterns and rollback. **Read `cycle-discover.md` before invoking this skill.** This SKILL.md retains only the planning protocol below.

## Process

### Step 0 — Read project rules (MANDATORY)

```bash
ls rules/
```

Read them. The plan SHALL cite at least one — `architecture.md` for boundaries a fix must respect, `testing.md` for the pyramid a new test must fit, `error-handling.md` for what a defect in error paths means here.

Also read `rules/current-constraint.md`. If it is `status = declared` and inside its `review_on`, the constraint corner gets real questions. If it is undeclared or expired, the corner is deferred — see Step 3.

### Step 1 — Read the item and inventory what is known

1. The `B-NNN` block in `BACKLOG.md` — `domain`, `repo`, `suggested_mode`, `why_now`, `dod`.
2. Prior opportunities under `knowledge-base/discoveries/opportunities/` touching the same repo. A hypothesis measured and killed three months ago deserves to know that before being measured again.
3. The repo's own `README.md` / `CLAUDE.md` and its build manifest — the plan must name tools that repo actually has.

### Step 2 — Confirm or reclassify the mode

`suggested_mode` is the filer's guess, not a decision. Confirm it against what the item actually claims:

| The item claims | Mode |
|---|---|
| Something in our code is wrong, visible by reading it | `review` |
| Something behaves wrong in the running system | `live-test` |
| A specific defect that reproduces | `bug` |
| The status quo costs something worth measuring | `evolve` |

Record a reclassification and its reason in `## Context`. Two constraints apply immediately:

- **`live-test` requires a declared target.** Check `rules/live-target.txt` for this domain. No block, no plan — six of the eight domains have no live surface by design. Reclassify to `review` or return the item; do not invent a probe.
- **`bug` requires that a failing test be *writable*.** If nobody can express the defect as a test that fails, the defect is not understood well enough to plan against. Say so and return the item rather than planning around it.

### Step 3 — Declare the four corners (MANDATORY — a decision step, not a checkbox)

Pause here and decide the corner coverage BEFORE drafting questions.

| Corner | What the questions must settle |
|---|---|
| **Evidence** | The measurement itself, in this mode's contract |
| **Constraint Relation** | Does this explore, subordinate, elevate the declared constraint — or is it local optimisation? |
| **Blast Radius** | What else across the ecosystem touches this |
| **Verification** | How we will know a fix worked, and where the limit moves next |

**The constraint corner is normally deferred, and that is the expected path.** While `rules/current-constraint.md` is `status = undeclared`, write:

```
<!-- DEFER-CORNER: constraint | current-constraint.md is undeclared and flow is not instrumented -->
```

Inventing a constraint question to fill the slot is padding. The corner is a lens, not a gate.

**Blast Radius is the corner that earns its place in this ecosystem.** The repos form a dependency graph with `theo-contracts` at the stable base; a maintenance change is dangerous in proportion to how far up that graph it sits. A question here is rarely wasted.

#### Question budget

- **Total 3-10.** Below 3 the plan is not a plan; above 10 the loop exhausts. The floor is 3, not the ancestor's 5 — five was sized for a prior-art survey across several projects, and a floor that forces padding produces questions written to satisfy a counter.
- **Max 3 per corner**, **min 1 per corner** or a `DEFER-CORNER` marker.
- **Each question maps to exactly one corner.** Split the ones that span.

#### Pre-validate every target (MANDATORY)

Open each path before writing it into the table. A target that does not resolve caps the plan at 49 — and, worse, a plan naming paths nobody can open is a plan that will produce fabricated evidence downstream, where everything treats it as measured fact.

For a plan, a **directory is a legitimate target**: the plan points at what it intends to open, so no line number is expected yet. That is the deliberate difference from the opportunity, where evidence is `file:LINE` and the line must exist because measurement already happened.

Live URLs are checked against `rules/live-target.txt`. A host no domain declares is a probe the cycle refuses to run.

### Step 4 — Write the falsification criterion (MANDATORY)

**Write this before drafting the questions, and never after seeing results.**

State what result kills the hypothesis, specifically enough that `/discover-execute` can check it every iteration. State also what does *not* rescue it: if the claim is about a set, one interesting member does not make it true.

This replaced the ancestor's `>= 2 ADRs` requirement. An ADR here is premature — nothing has been measured, so there is nothing to decide. What makes a measurement plan honest is committing in advance to what would make you drop it. Without that commitment, any observation can be reinterpreted afterwards as confirming what was already believed, and the measurement becomes a ritual that cannot fail.

A plan whose falsification criterion is empty or placeholder caps at 70.

### Step 5 — Write the plan

Use `templates/measurement-plan-template.md`. Save to:

```
knowledge-base/discoveries/plans/{slug}-plan.md
```

`{slug}` is kebab-case and prefixed by the repo when the problem shape recurs across repos (`theo-lens-trace-latency`, not `latency`). The registry spans 21 repos.

## Quality Rules

Non-negotiable for every measurement plan:

1. **Every question names a Tool and a Target.** A question with no tool is a wish; a tool with no target is a tool pointed at nothing.
2. **Every target resolves**, verified by opening it, not by plausibility.
3. **The falsification criterion is written before measuring**, and is specific enough to be checked.
4. **Question budget respected** — 3-10 total, max 3 per corner, min 1 or deferred.
5. **The mode's contract is planned for**: `bug` plans a test that will be *run*; `live-test` plans to name the environment-vs-product uncertainty; `evolve` plans to produce a number, not an adjective.
6. **No premature conclusions.** A measurement plan ASKS. Answers come from `/discover-execute`. A plan that already states the finding has decided the outcome before measuring it.
7. **No prior art as justification.** "Project X does it this way" is not a reason to measure ours. The reason is in the item's `why_now`, and gate G5 already refused it at intake if it was not.

## What this skill does NOT do

- Run the measurement — `/discover-execute`.
- Review edge cases in the plan — `/discover-edge-cases`.
- Score the plan — `/discover-plan-confidence`.
- Fix anything, or write into a governed repo. Planning is reading.

## Anti-patterns

1. **"Let's just look around and see what we find."** No plan, no falsification, no gate. The measurement then confirms whatever was already believed.
2. **Writing a target without opening it.** The single most common source of fabricated evidence downstream.
3. **Filling the constraint corner with a confident claim nobody measured.** Defer it.
4. **Unfalsifiable hypotheses.** "The trace explorer is slow" cannot be wrong. "The listing endpoint issues one query per span" can.
5. **Planning a `live-test` on an undeclared domain.** Produces the appearance of runtime evidence with none of the substance.
6. **Padding to reach the question floor.** Three real questions beat six written for a counter.

## Related

- Upstream: [`rules/cycle-backlog.md`](../../rules/cycle-backlog.md) — supplies the `B-NNN` hypothesis
- Next: `/discover-edge-cases` (what could make this measurement lie)
- Then: `/discover-plan-confidence` (scores this plan), `/discover-execute` (runs it)
- Live environment declaration: [`rules/live-target.txt`](../../rules/live-target.txt)
- Constraint lens: [`rules/current-constraint.md`](../../rules/current-constraint.md)
- Template: `templates/measurement-plan-template.md`
