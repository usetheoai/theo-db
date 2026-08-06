# Discover-Plan — Portable Installation

This skill works in **any project** that uses Claude Code and maintains a `BACKLOG.md` registry of maintenance items (see `rules/cycle-backlog.md`).

It has no reference-clones requirement. The ancestor of this skill planned investigations into *other people's* code under `knowledge-base/references/`; this one plans measurements of **your own system**, so the only inputs it needs are your repo, your rules, and the item being measured.

## What you get

- `/discover-plan B-NNN` — writes a measurement plan to `knowledge-base/discoveries/plans/{slug}-plan.md`
- A template (`templates/measurement-plan-template.md`) that bakes in the falsification criterion, Tool + Target per question, the four corners, and halt-loop checkpoints

The skill produces no output until invoked. It is instructions plus a template.

## Quick install

### 1. Copy the skill directory

```bash
cp -r /path/to/source/skills/discover-plan .claude/skills/
```

The skill walks UP from the saved plan path to find your project root (`.claude/` or `.git/` marker), so both the standalone (`rules/`) and plugin (`.claude/rules/`) layouts work.

### 2. Ensure the consumer chain exists

The full chain:

```
/discover-plan → /discover-edge-cases → /discover-plan-confidence → /discover-execute → /discover-confidence → (/discover-improve if needed)
```

`/discover-plan` alone gives you planning, but its references to downstream phases stay unactionable until the rest are installed.

## Project requirements

### Required

- **`rules/` (or `.claude/rules/`)** with at least `architecture.md` and `testing.md`, or your equivalents. Step 0 mandates reading them. Missing rules degrade the plan to a generic one.
- **`BACKLOG.md`** at the workspace root. The skill takes a `B-NNN` item as its input — the hypothesis it plans to measure. Without a registry there is nothing to plan against; run `/backlog-init` first.
- **`knowledge-base/discoveries/plans/`** — created on first use if absent.

### Required for `--mode live-test` only

- **`rules/live-target.txt`** declaring the domain's live surface. The skill **refuses** to plan a live probe against an undeclared domain rather than improvising one. If your project has no live surface, this file is unnecessary and `live-test` simply never applies.

### Optional but recommended

- **`rules/current-constraint.md`** — the constraint lens. Absent or undeclared, the constraint corner is deferred, which is the expected path rather than a failure.
- Prior opportunities under `knowledge-base/discoveries/opportunities/` — Step 1 reads them so a hypothesis killed three months ago is not silently re-measured.

## What happens out of the box

1. Reads `rules/` to internalize project principles, plus `current-constraint.md`
2. Reads the `B-NNN` block and prior opportunities touching the same repo
3. Confirms or reclassifies the mode (`review` / `live-test` / `bug` / `evolve`)
4. Declares the four corners, normally deferring the constraint corner
5. Writes the falsification criterion **before** the questions
6. Pre-validates every target by opening it
7. Writes the plan

## Customizing for your project

### Domains and repo routing

The eight domains in `rules/cycle-backlog.md § Domain routing` are specific to the Theo ecosystem. For your own, edit that table — the skill reads `domain` and `repo` from the item and does not hardcode names.

### Question budget

Total 3-10, max 3 per corner, min 1 per corner or a `DEFER-CORNER` marker. The floor is 3 because maintenance items are small; raising it forces padding, and a question written to satisfy a counter measures nothing. Both bounds live in `skills/discover-plan-confidence/scripts/check_plan_completeness.py` (`MIN_QUESTIONS` / `MAX_QUESTIONS`) — change them there, not only in prose, or the gate and the guidance will disagree.

### The four corners

Evidence / Constraint Relation / Blast Radius / Verification are tuned for maintaining a running system. Changing them means editing **three** places or the pieces drift apart:

1. `SKILL.md § Step 3`
2. `templates/measurement-plan-template.md`
3. `skills/discover-plan-confidence/scripts/check_corner_coverage.py` (`CORNERS`) **and** its sibling in `skills/discover-confidence/`

Blast Radius is the corner most worth keeping. It earns its place wherever repos form a dependency graph: a change is dangerous in proportion to how far up that graph it sits.

## What's portable, what's project-specific

| Element | Portable? | Notes |
|---|---|---|
| `SKILL.md` protocol (Steps 0-5) | ✅ | Generic; the rules lookup handles both layouts |
| Falsification-first discipline | ✅ | Domain-agnostic and the core of the phase |
| Question budget (3-10, max 3/corner) | ✅ | Heuristic, adjustable in the checker |
| Template structure | ✅ | Matches the deterministic checker |
| The four corners | ⚠️ | Portable in shape; edit all three places if you change them |
| Domain routing table | ❌ | The eight Theo domains. Replace with yours. |
| `rules/live-target.txt` contents | ❌ | Your environments, your credentials-by-name |

## Limitations (known)

- **The template fails its own checker until filled in** — 8/10 sections plus `falsification_missing`, because `Item`, `Mode` and the falsification body hold placeholders. Deliberate: a placeholder shaped like a valid value would let a forgotten `B-000` pass the gate and point the plan at an item that does not exist. Do not "fix" this by pre-filling them.
- **No automated tests for this skill** — unlike `discover-plan-confidence` (36 pytest tests), `discover-plan` is exercised only by real invocations. Its output, however, *is* gated deterministically by that sibling.
- **Weak Step 0 fallback** — with no `rules/`, the skill notes the absence but generates no fallback rule set.
- **Target pre-validation is existence, not faithfulness** — Step 3 proves a path opens; it does not prove the path contains what the question assumes. That is `/discover-edge-cases`'s "resolves but stale" check, and it is a review by a reader, not a deterministic gate.

## Self-validation

```bash
# The protocol steps are present
grep -E '^### Step [0-5]' .claude/skills/discover-plan/SKILL.md

# The template carries the mandatory headers the checker looks for
grep -E '^\*\*(Item|Repo|Mode):\*\*|^## (Context|Hypothesis|Falsification|Measurement Questions)' \
  .claude/skills/discover-plan/templates/measurement-plan-template.md

# The gate agrees with the guidance about the question floor
grep -n 'MIN_QUESTIONS' .claude/skills/discover-plan-confidence/scripts/check_plan_completeness.py
```

## Related

- Upstream: `rules/cycle-backlog.md` — supplies the `B-NNN` hypothesis
- Next: `/discover-edge-cases` → `/discover-plan-confidence` → `/discover-execute`
- Template: `templates/measurement-plan-template.md`
- Sibling: `/to-plan` (same architecture, different output — implementation plans)
