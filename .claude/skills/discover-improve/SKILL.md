---
name: discover-improve
version: 0.2.0
requires: [discover-confidence]
description: Lift a low-scoring opportunity toward its target verdict — improving how the finding is ARGUED, never what it CLAIMS. Use this when /discover-confidence returns NEEDS_REVISION and someone wants the artifact salvaged. Never rewrites the Evidence corner (that is the record of a measurement) and never annotates an unresolvable pointer (that would disarm the fabricated_evidence hard cap). Refuses outright when the score is capped by something only a re-measurement can fix.
user-invocable: true
allowed-tools: Read Glob Grep Bash Write Edit Skill
argument-hint: "{opportunity-slug} [--target SHIPPABLE_WITH_CAVEATS]"
---

# Discover-Improve — Iterative Opportunity Score Lifter

Reads an opportunity, scores it with `/discover-confidence`, applies fixes, re-scores, and repeats until it reaches the target verdict.

Sibling of `/plan-improve` — same architecture (ralph-loop halt-loop + deterministic Phase A + LLM Phase B), different fix categories.

**The governing distinction:** an opportunity is part **record** and part **prose**. `## Corner 1 — Evidence` records what was measured; `## Recommendation` argues what to do about it. This skill may improve the argument. It may never edit the record.

## Cycle contract

This skill is **phase 6** of [`cycle-discover`](../../rules/cycle-discover.md), invoked when `/discover-confidence` returns `NEEDS_REVISION`. The cycle rule is the source of truth for chain order, hard gates, stop conditions and rollback. **Read `cycle-discover.md` before invoking this skill.**

## When NOT to invoke

Invoke `/discover-improve {slug}` after `/discover-confidence` returned `NEEDS_REVISION` and the caps are ones this skill can actually close.

**Do NOT invoke when the score is capped by any of these** — none is fixable by editing text, and iterating on them burns a loop to arrive at the same number:

| Cap | Why this skill cannot fix it | What actually fixes it |
|---|---|---|
| `fabricated_evidence` | A pointer that does not resolve means someone invented it or the code moved | Re-measure, or a human replaces the pointer |
| `empty_corner_evidence` | Nothing was measured | Re-run `/discover-execute` |
| `empty_corner_blast_radius` | Nobody scoped the change | Answer it — a real question, not prose |
| `no_adr_on_cross_repo_change` | A decision for other repos' maintainers is missing | A human writes the ADR |

The skill checks the caps before starting and **refuses**, naming the cap and the real fix. A refusal here is cheaper than an honest failure four iterations later.

## Two-phase fixing

### Phase A — deterministic (`scripts/apply_fixes.py`)

Idempotent, zero LLM calls, `$0`. Scoped to `## Recommendation`:

- Weak imperatives: `should` → `must`, `could` → `can`
- Loopholes stripped: "if possible", "as appropriate", "when applicable", "where feasible"
- Code fences never touched

**`may` and `might` are deliberately NOT substituted.** In descriptive prose they are correct — *"the endpoint may return 500 under load"* is a measured fact about something intermittent, and *"must return 500"* is a different, false claim. No regex distinguishes description from prescription, so it does not try.

Exit codes: `0` fixed or nothing to fix, `2` file not found, **`3` unresolvable pointers present** — a signal for the halt-loop to stop rather than iterate on something it cannot fix.

#### What Phase A REPORTS but never touches

Unresolvable pointers are listed and left exactly as they are.

The ancestor annotated each one with `<!-- BLOCKED: path not found -->`. Measured against the current checker (2026-08-05): a marked pointer moves out of `fabricated` and into `explicitly_blocked`, and `fabricated_evidence` stops firing. That made the fixer an **automated bypass of the cycle's most important hard cap** — a script turning an INVALID opportunity into a passing one with nothing measured.

`test_apply_fixes.py::test_never_writes_a_blocked_marker` locks it shut. A failure there is a hole in the gate, not a formatting regression.

A `<!-- BLOCKED: -->` marker written by a human or by the measurement itself is respected and not re-reported. The difference is who decided.

### Phase B — semantic (LLM, inside the halt-loop)

What deterministic rules cannot do:

- Sharpen a vague **Recommendation** into a scoped one, naming what is out of scope
- Make the **Verification** corner concrete — tie it to the item's `dod`, prefer a test that fails against the current state
- Name the successor limit in Verification when it is missing
- Complete the **Blast Radius** corner from evidence already gathered — never by guessing at consumers nobody looked for

Phase B obeys the same boundary: it may argue better, never claim more. Any statement about the system that was not measured is out of bounds, whichever corner it would improve.

## Workflow

1. **Parse arguments.** `{slug}`, optional `--target` (default `SHIPPABLE_WITH_CAVEATS`).
2. **Score first.** Run `/discover-confidence {slug}`. If already at target, report and stop — an unnecessary loop is worse than none.
3. **Check the caps.** Any cap from the refusal table above → stop, name the cap and the real fix.
4. **Phase A.** Run `apply_fixes.py --json`. Exit 3 → surface the pointers and stop.
5. **Pre-flight guard.** Verify `.claude/ralph-loop.local.md` does not have `active: true` (`rules/loop-engine-convention.md § Anti-patterns`).
6. **Invoke ralph-loop** with `prompts/improvement-prompt.md`, `--completion-promise 'OPPORTUNITY_IMPROVED'`.
7. **Post-promise sanity check.** Re-run the scorer in the emitting iteration. **The verdict on disk is the verdict** — a promise emitted while the score is below target is a promise integrity violation.

## Anti-patterns

- **Rewriting the Evidence corner.** It is the record of a measurement. Editing it falsifies findings, and downstream cannot tell.
- **Annotating unresolvable pointers.** Disarms `fabricated_evidence`. Removed on purpose; see Phase A.
- **Adding claims to fill a corner.** A Blast Radius written from imagination is worse than an empty one: empty is visible, invented is not.
- **Looping on a cap this skill cannot close.** Four iterations to reach the same number, then a false report.
- **Emitting the promise on a partial improvement.** The score on disk decides, not the direction of travel.
- **Substituting `may`/`might` in descriptive prose.** Inverts measured facts.

## Related

- Upstream: `/discover-confidence` (produces the score this skill lifts)
- Re-score after: `/discover-confidence` — the verdict on disk is canonical
- Golden rule: [`rules/discover-opportunity-golden-rule.md`](../../rules/discover-opportunity-golden-rule.md)
- Script: `scripts/apply_fixes.py`
- Prompt: `prompts/improvement-prompt.md`
- Loop engine: `ralph-loop` plugin
