---
name: discover-execute
version: 0.2.0
requires: [discover-plan-confidence]
description: Run a measurement against OUR own code and runtime via halt-loop, routed by mode (review / live-test / bug / evolve), and produce an opportunity — or KILL the item when the falsification criterion is met. Use this whenever someone wants to find out whether a suspicion is real, to sweep a domain for findings nobody filed (--sweep), or to reproduce and prove a defect. Killing an item is a successful outcome, not a failed run.
user-invocable: true
allowed-tools: Read Glob Grep Bash Write Edit Skill
argument-hint: "{plan-slug} | --sweep {domain}"
---

# Discover-Execute — Halt-Loop Measurement Driver

Reads an approved measurement plan, then drives a halt-loop investigation **against our own system**, producing an opportunity — or killing the item.

**Architecture:** wraps the `ralph-loop` plugin's autonomous-iteration mechanism (Stop hook + state file) with a measurement-specific prompt. Each iteration answers one question, appends to the opportunity, and re-evaluates both the halt condition and the kill condition.

**Two terminal outcomes, both legitimate:**

- `<promise>OPPORTUNITY_COMPLETE</promise>` — the hypothesis held; the opportunity feeds `/discover-confidence`.
- `<promise>ITEM_KILLED</promise>` — the falsification criterion was met; the item is closed with its `kill_reason`. **This is success.** A run that measures honestly and finds nothing has protected the plan cycle from work that would have been justified by a hunch.

## Cycle contract

This skill is **phase 4** of [`cycle-discover`](../../rules/cycle-discover.md). The cycle rule is the source of truth for the four modes and their evidence contracts, chain order, hard gates (G-E/G-M/G-L/G-C/G-K), stop conditions, anti-patterns and rollback. **Read `cycle-discover.md` before invoking this skill.** This SKILL.md retains phase-specific detail (halt-loop workflow, mode routing, post-promise checks).

## When NOT to invoke

- **Fast lane:** `--mode bug` where a failing test already exists — enter here directly, skipping phases 1–3. A test that fails on the current state is a stronger measurement plan than any document describing one, and it is verifiable by execution rather than by review. Unavailable when the repro is not yet a test: "I can reproduce it by hand" is a plan, not a measurement.
- `--sweep {domain}` to measure a whole domain with no prior item.

## Mode routing

The mode comes from the plan's `**Mode:**` header (or `--sweep`'s flag). It decides what counts as evidence, and evidence from one mode does not satisfy another.

| Mode | The measurement | Refuses when |
|---|---|---|
| `review` | Open the file, read surrounding context, record `path:LINE` + the rule violated + why it matters here | — |
| `live-test` | Probe the declared target; record `METHOD URL -> STATUS`, console, trace id, timing, screenshot | The domain has no block in `rules/live-target.txt` |
| `bug` | Numbered repro **plus a test that fails on the current state, executed** | The failing test was never run |
| `evolve` | Count the cost of the status quo — a number, not an adjective | — |

`review` carries a discipline the others do not: rule out that the code is **dead**, that the caller **never existed**, and that the shape is **deliberate**. All three produce findings that look real and are not.

`live-test` carries the environment-vs-product obligation. A dev environment breaks for its own reasons; the opportunity states which explanation it believes and what would distinguish them, or states plainly that it cannot yet tell.

**Reclassification is expected.** `suggested_mode` on the backlog item is the filer's guess. If measurement shows a different shape, switch mode and record the switch with its reason. Forcing the measurement into the guessed mode defeats the purpose of measuring.

## Workflow

### Step 1 — Resolve inputs

1. Plan path: `knowledge-base/discoveries/plans/{slug}-plan.md`. Read it fully — extract `**Item:**`, `**Repo:**`, `**Mode:**`, the Measurement Questions table, and `## Falsification`.
2. Confirm the `B-NNN` item exists in `BACKLOG.md` and is `raw`. An item already `triaged`, `planned` or `shipped` is not re-measured — that is how duplicate work enters.
3. For `live-test`, confirm the domain has a block in `rules/live-target.txt`. **No block, no probe.**

### Step 2 — Initialize the opportunity

Create `knowledge-base/discoveries/opportunities/{slug}-opportunity.md` from `templates/opportunity-template.md`. Every section present, each corner holding a `<!-- TBD -->` placeholder mapped to its question.

Fill the header lines immediately — `**Item:**`, `**Repo:**`, `**Mode:**` are checked by `check_opportunity_completeness.py`, and a mode token outside the four reads as a missing section.

### Step 3 — Build the halt-loop prompt

Read `prompts/execute-mode-prompt.md` and substitute `{PLAN_SLUG}`, `{PLAN_PATH}`, `{OPPORTUNITY_PATH}`, `{ITEM}`, `{MODE}`.

### Step 4 — Pre-flight guard (concurrent-loop safety)

Verify `.claude/ralph-loop.local.md` (if present) does NOT have `active: true`. Concurrent ralph-loops on overlapping state is a documented anti-pattern (`rules/loop-engine-convention.md § Anti-patterns`). A stale state file observed `active` HALTS and surfaces to the human rather than spawning a second loop.

### Step 5 — Invoke ralph-loop

**Read `rules/loop-engine-convention.md § How to invoke ralph-loop:ralph-loop safely` BEFORE this step.** The positional argument is shell-evaluated; inlining a multi-section driver prompt (backticks, fenced blocks, `$(...)`) breaks loop startup with a bash parse error. Use the file-referenced pattern.

1. Write the substituted prompt to `.claude/halt-loop-prompts/discover-execute-{slug}.md` (gitignored).
2. Invoke `ralph-loop:ralph-loop` with:
   - Positional prompt (no shell metacharacters): `Read .claude/halt-loop-prompts/discover-execute-{slug}.md and follow its instructions for this halt-loop iteration.`
   - `--completion-promise 'OPPORTUNITY_COMPLETE'`

### Step 6 — Per-iteration contract

Enforced by the driver prompt. Each iteration: pick the next pending question → run the mode's measurement → write the answer into its corner with a resolving pointer → update `.progress-{slug}.json` → **re-check the kill condition** → re-evaluate the halt condition.

The kill check runs every iteration, not at the end. Recognising early that the falsification criterion is already met is what stops a long measurement from accumulating the sunk cost that makes a weak finding look shippable.

### Step 7 — Post-promise sanity check

After `<promise>OPPORTUNITY_COMPLETE</promise>`, run ONCE before reporting:

```bash
python3 - "{OPPORTUNITY_PATH}" <<'PY'
# Re-verify every code pointer: the file exists, the line is within it, and the line is
# PRINTED so you can confirm it says what the opportunity claims.
#
# Written in python3 rather than shell on purpose. The shell version used
# `$(wc -l < "$path")` for the bounds check, and when `wc` was unavailable that expanded
# to empty — every comparison failed and the check reported EVERY pointer as fabricated.
# An eval run hit exactly that: 23 real pointers, 23 false FABRICATED. A check that
# collapses to "everything is fake" when a tool is missing gets distrusted and then
# ignored, which is worse than one that fails loudly.
import re, sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
pat = re.compile(r"\b((?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+\.[A-Za-z0-9]{1,10}):(\d+)")
bad = 0
for path, line in sorted(set(pat.findall(text))):
    f, n = Path(path), int(line)
    if not f.is_file():
        print(f"FABRICATED (missing file): {path}:{n}"); bad += 1; continue
    lines = f.read_text(encoding="utf-8", errors="replace").splitlines()
    if not 1 <= n <= len(lines):
        print(f"FABRICATED (line {n} past EOF, file has {len(lines)}): {path}"); bad += 1; continue
    print(f"  ok {path}:{n} | {lines[n-1].strip()[:80]}")
print(f"\n{bad} fabricated" if bad else "\nall pointers resolve")
sys.exit(1 if bad else 0)
PY
```

Checking the line matters as much as checking the file. A pointer at line 400 of a 30-line file is evidence that moved, and the ancestor's check — which stripped the line suffix before testing the path — passed it as verified.

Any `FABRICATED:` line is a **PROMISE INTEGRITY VIOLATION**: re-mark the claim `<!-- BLOCKED: ... -->` and re-invoke the loop. Never accept the promise at face value.

On `<promise>OPPORTUNITY_BLOCKED</promise>` the check still runs, plus the blocked count in the report must match `.progress-{slug}.json`. Drift blocks handoff.

On `<promise>ITEM_KILLED</promise>`, verify instead that the `B-NNN` block carries a `kill_reason` naming what was measured and what it showed (gate G-K). An unexplained kill is indistinguishable from an abandoned run.

### Step 8 — Update `BACKLOG.md` and report

| Outcome | `B-NNN` becomes |
|---|---|
| `OPPORTUNITY_COMPLETE` | `status: triaged`, `evidence: <pointer>` |
| `ITEM_KILLED` | `status: killed`, `kill_reason: <what was measured>` |
| `OPPORTUNITY_BLOCKED` | unchanged — `raw`, with the blocker surfaced to the human |

Report: the opportunity path, iterations used, questions answered / blocked with reasons, pointers verified, runtime observations recorded, and any mode reclassification. Next step: `/discover-confidence {slug}` — except on `ITEM_KILLED`, where the chain ends and there is nothing to score.

### Step 9 — Sweep mode

`--sweep {domain}` measures a domain with no prior item. Each finding is registered directly in `BACKLOG.md` with `source: discover-{mode}`, its evidence attached, and `status: triaged` — sweep findings skip intake because they arrive with the evidence intake is not allowed to require.

Registration is not optional. A finding that stays in this run's output and never reaches the registry is exactly the orphaned-finding failure the single registry exists to prevent.

## Anti-patterns

- **Writing to a governed repo.** Discover produces a document. An opportunity carrying the patch has pre-empted the plan cycle and skipped every gate after it.
- **Fabricated evidence** — a plausible pointer nobody opened, a status code nobody requested, a test asserted to fail but never executed.
- **Prior art as evidence.** Not a measurement of our system, and it cannot fill the Evidence corner.
- **Improvising a live probe** on a domain with no declared target.
- **Refusing to kill.** After a long measurement, sunk cost makes a weak finding look shippable.
- **`ITEM_KILLED` when nothing was measured.** Target unreachable is not disproof — stop and ask the human.
- **Emitting a promise without the Step 7 check.**
- **Spawning concurrent ralph-loops** on overlapping state.
- **Sweeping without registering.**

## What this skill does NOT do

- Write the measurement plan — `/discover-plan`.
- Review edge cases — `/discover-edge-cases`.
- Score the opportunity — `/discover-confidence`.
- Refine a low-scoring opportunity — `/discover-improve`.
- Fix anything. Measuring is reading.

## Related

- Upstream: `/discover-plan`, `/discover-edge-cases`, `/discover-plan-confidence`
- Downstream: `/discover-confidence`, then `/discover-improve` if the score is low
- Intake contract: `rules/cycle-backlog.md`
- Live environment declaration: `rules/live-target.txt`
- Constraint lens: `rules/current-constraint.md`
- Template: `templates/opportunity-template.md`
- Prompt: `prompts/execute-mode-prompt.md`
- Loop engine: `ralph-loop` plugin (must be enabled in `~/.claude/settings.json`)
