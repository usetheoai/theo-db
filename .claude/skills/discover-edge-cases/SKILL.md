---
name: discover-edge-cases
version: 0.2.0
requires: [discover-plan]
description: Find what could make a measurement LIE — a target that resolves but is stale, a method that observes a proxy, an environment fault read as a product defect, a hypothesis nothing could refute. Use this after /discover-plan and before running any measurement, and whenever someone is about to trust a result that would be expensive to get wrong. An unfalsifiable hypothesis found here is always a blocker.
user-invocable: true
allowed-tools: Read Glob Grep Bash
argument-hint: "[plan-slug|plan-file-path]"
---

# Discover Edge Case Review

Analyze a measurement plan and ask one question: **what could make this measurement lie?**

That is the shift from the ancestor. It asked what the *research* might miss — a corner unstudied, a reference project unread. Missing information is a gap you notice. A measurement that runs cleanly and produces a confident wrong answer is not: it looks exactly like a measurement that worked, and everything downstream treats it as measured fact.

Sibling of `/edge-case-plan` — same philosophy, same output format, different scope: risks of the **measurement**, not of the implementation.

## Cycle contract

This skill is **phase 2** of [`cycle-discover`](../../rules/cycle-discover.md). The cycle rule is the source of truth for the four modes and their evidence contracts, chain order, hard gates and rollback. **Read `cycle-discover.md` before invoking this skill.** This SKILL.md retains the checklist, rubric and report format.

## Argument

Slug or path, via free text:

- Slug (`theo-lens-trace-latency`): `Glob` under `knowledge-base/discoveries/plans/*{slug}*.md`
- A `.md` path: use directly
- No hint: most recent file under `knowledge-base/discoveries/plans/` by mtime

## Philosophy

**You are NOT the agent that complicates things.** You are the agent that asks: *"suppose this measurement runs perfectly and still tells us something false — how?"*

Golden rules:

1. **Only flag what can actually happen during `/discover-execute`** — not theoretical scenarios.
2. **Never expand scope.** The fix for an edge case is an extra checkpoint or a different tool, never a new phase.
3. **KISS prevails.** If the fix requires rewriting the plan, document the risk and move on.
4. **Every flagged case comes with a fix in ≤1 line of plan change.**
5. **Combined edges only when realistic** — "the file moved in the July refactor so the line number is stale" is realistic; "the filesystem lies about line counts" is not.

## Process

### Step 1 — Read the measurement plan

```bash
ls knowledge-base/discoveries/plans/*${ARGUMENTS}* 2>/dev/null || ls -t knowledge-base/discoveries/plans/*.md | head -5
```

Read it fully. Note the `**Mode:**`, the hypothesis, the falsification criterion, every question with its Tool and Target, and the halt-loop checkpoints.

### Step 2 — Map the boundaries of the measurement

For each question:

- **What will actually be observed** — which tool, against which target, in which state of the repo or environment?
- **What the answer will be trusted to mean** — a `file:LINE` becomes a claim about behaviour; a `-> 500` becomes a claim about the product.
- **Order dependencies** — does Q4 assume Q1 already answered? Does it break if the order flips?

The lies live at those joints: between what is observed and what it is taken to mean.

### Step 3 — Apply the pragmatic checklist

Walk each question. ✅ if the plan covers it, ❌ if not. **Skip what does not apply** — a `review` plan has no environment risk, an `evolve` plan usually has no flakiness.

```
TARGETS
  [ ] Does every path target resolve today?
  [ ] Could it resolve and still be STALE — moved in a refactor, so the line means
      something else now?
  [ ] Is the target the thing the question is about, or merely near it?

METHOD
  [ ] Does the tool observe what the question asks, or a proxy for it?
  [ ] Would a passing result be indistinguishable from the tool not running at all?
  [ ] Does the method need state the plan never sets up (seeded data, auth, a built
      artifact)?

FALSIFICATION
  [ ] Is the criterion written, specific, and checkable mid-run?
  [ ] Could ANY plausible result be read as confirming the hypothesis? If so it is
      unfalsifiable — the single most damaging defect at this phase.
  [ ] Does the plan say what does NOT rescue the hypothesis?

MODE — review
  [ ] Does the plan check whether the code is DEAD before calling it a defect?
  [ ] Does it check whether the caller ever existed?
  [ ] Does it check whether the shape is DELIBERATE (git history, comments)?

MODE — live-test
  [ ] Is the target declared in rules/live-target.txt for this domain?
  [ ] Does the plan say how ENVIRONMENT fault will be told apart from PRODUCT defect?
  [ ] Is a single observation being trusted where the behaviour could be intermittent?
  [ ] Is the probe non-destructive?

MODE — bug
  [ ] Will the failing test actually be RUN, or only written?
  [ ] Does the repro depend on local state nobody else has?
  [ ] Would the test also fail for an unrelated reason (already-broken suite)?

MODE — evolve
  [ ] Will the cost be COUNTED, or estimated and presented as counted?
  [ ] Is the measurement taken under conditions that resemble real use?

BLAST RADIUS
  [ ] Does the plan look for consumers OUTSIDE the item's own repo?
  [ ] Would a cross-repo dependency be invisible to the planned method?

SCOPE
  [ ] Is there a clear stop criterion, or can the loop drift into a wider audit?
```

### Step 4 — Classify and report

| Level | Meaning | Action |
|---|---|---|
| **MUST FIX** | Will break `/discover-execute` or produce a confident wrong answer | Edit the plan and absorb it |
| **SHOULD TEST** | Unlikely but dangerous | Add a halt-loop checkpoint |
| **DOCUMENT** | Risk consciously accepted | Record it in `## Context` |
| **IGNORE** | Theoretical, or the fix costs more than the problem | Leave it out |

**An unfalsifiable hypothesis is always MUST FIX.** Every other defect here produces a wrong answer someone might catch; this one produces a measurement that *cannot* fail, and its output will be believed precisely because it ran cleanly.

### Step 5 — Save the report

```
knowledge-base/reviews/{plan-slug}-edge-cases-{YYYY-MM-DD}.md
```

Create `reviews/` if absent. The report is the audit trail before `/discover-execute` runs.

**This skill does NOT edit the plan.** The human reads the report and bumps the plan, absorbing each MUST FIX. Then `/discover-plan-confidence` scores the revised plan.

## Report Format

```markdown
# Discover Edge Case Review — {plan}

Date: YYYY-MM-DD
Plan analyzed: knowledge-base/discoveries/plans/{slug}-plan.md
Mode: {review|live-test|bug|evolve}
Questions analyzed: N
Edge cases found: N (MUST FIX: N, SHOULD TEST: N, DOCUMENT: N)

## MUST FIX

### EC-{N}: {short description}
- **Affected question:** Q{N}
- **Family:** Target / Method / Falsification / Mode / Blast radius / Scope
- **How the measurement lies:** {what it would report, and why that would be believed}
- **Impact:** {what downstream decides wrongly}
- **Suggested fix:** {≤1 sentence of plan change}

## SHOULD TEST

### EC-{N}: {short description}
- **Affected question:** Q{N}
- **Suggested halt-loop checkpoint:** {assertion to add}

## DOCUMENT

### EC-{N}: {short description}
- **Accepted risk:** {why it is OK not to address now}

## Summary

| Question | Edges | MUST FIX | SHOULD TEST | DOCUMENT |
|---|---|---|---|---|
| Q1 | N | N | N | N |

**Verdict:** MEASUREMENT PLAN OK / MEASUREMENT PLAN NEEDS ADJUSTMENT
```

## Anti-Patterns You NEVER Commit

1. **Suggesting a wider investigation** — "while we're here, also check X" → NO. An edge case lives inside what was planned.
2. **Speculating about future states** — "what if the endpoint changes next quarter?" → NO. Analyze the system that exists.
3. **Target paranoia** — demanding every path in the repo be verified → NO. Flag the targets the plan actually depends on.
4. **Turning a risk into a phase** — "add retry + fallback + caching to the loop" → NO. One clear method plus a stop criterion covers most of it.
5. **Proposing prior art as the fix** — "project X handles this by…" → NO. Not evidence, and not a fix for our measurement.
6. **Passing an unfalsifiable hypothesis as SHOULD TEST.** It is MUST FIX, every time.

## Integration

- Runs AFTER `/discover-plan`, BEFORE `/discover-plan-confidence`
- Analyzes **plans before execution**; for the produced opportunity, use `/discover-confidence`
- Chain and gates: [`rules/cycle-discover.md`](../../rules/cycle-discover.md)
- Live environment declaration: [`rules/live-target.txt`](../../rules/live-target.txt)
