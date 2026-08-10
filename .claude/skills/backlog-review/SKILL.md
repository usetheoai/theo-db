---
name: backlog-review
version: 0.1.0
requires: []
description: Report what has rotted in BACKLOG.md — duplicate or renumbered ids, triaged items with no evidence, raw items that were measured and never advanced, killed items with no reason, repos that route to nobody, vague or missing DoD, stale items, probable duplicates. Use this whenever someone asks whether the backlog is trustworthy or messy, before running the maintenance loop, after a sweep registers a batch of findings, or periodically — a registry nobody reviews is a registry nobody trusts. Read-only.
user-invocable: true
allowed-tools: Read Glob Grep Bash
argument-hint: "[path to BACKLOG.md]"
---

# `/backlog-review` — structural review of the registry

Read `BACKLOG.md` and report what is wrong with it. Writes nothing.

A registry is not a document that decays visibly. Its failure mode is quiet: statuses that stopped tracking reality, items nobody can act on, duplicates that split attention. By the time it looks wrong, people have already stopped consulting it. This skill makes that state observable while it is still cheap to fix.

## Cycle contract

Companion to [`rules/cycle-backlog.md`](../../rules/cycle-backlog.md) (the registry and its intake) and [`rules/cycle-maintenance.md`](../../rules/cycle-maintenance.md) (the loop that consumes it). Both are the source of truth for the schema, the status transitions and the gates. This skill only reports divergence from them.

## When NOT to invoke

- Before a `cycle-maintenance` run, so the loop does not select a broken item.
- After a `--sweep` registers a batch of findings — bulk writes are where duplicates and missing fields arrive.
- Periodically. A registry nobody reviews is a registry nobody trusts.
- Before handing the backlog to someone else.

Do NOT invoke to add or change items — that is `/backlog-item`. This skill is read-only, deliberately: a reviewer that also edits cannot be trusted to report what it found.

## What it checks

### Deterministic — the machine is sure

| Check | Severity | Why it matters |
|---|---|---|
| `duplicate_id` | blocker | Two blocks sharing a `B-NNN` destroy the audit trail |
| `renumbered` | blocker | Ids are never reused or reordered; a reused id makes every earlier reference ambiguous |
| `illegal_status` | blocker | A status outside the declared set means the loop cannot route the item |
| `triaged_without_evidence` | blocker | Triaged means measured. Without evidence the status is a claim nobody made |
| `unroutable_repo` | blocker | A repo in no domain routes to nobody (gate G1) |
| `raw_with_evidence` | major | Measurement happened and the status was never advanced — the rot the loop exists to prevent |
| `killed_without_reason` | major | Indistinguishable from an abandoned run (gate G-K) |
| `missing_field` | major | A required field absent |
| `invalid_mode` | major | `suggested_mode` outside the four |

### Heuristic — a human decides

| Check | Severity | Honest limitation |
|---|---|---|
| `thin_dod` | major | Zero DoD bullets. Nothing states when the item is done, so it never closes |
| `vague_dod` | minor | Word-list matching. A bullet carrying a number or a backticked artifact is treated as falsifiable even when it also reads as vague — "p95 below 800ms" is a criterion, and flagging it would train people to ignore the check |
| `stale_raw` | minor | 90 days is a convention, not a measurement. It asks a question rather than asserting a defect |
| `possible_duplicate` | minor | Title-word overlap ≥ 0.6 between OPEN items. Closed items are excluded — a shipped item and a new one in the same area is a follow-up, not a duplicate |

Every finding carries `kind: deterministic | heuristic`. A reader must be able to tell "this is certainly wrong" from "someone should look" without knowing the implementation.

## Verdict

Derived from the findings, never asserted — the same discipline the confidence scorers follow.

| Verdict | Condition | Exit |
|---|---|---|
| `SHIPPABLE` | no findings | 0 |
| `SHIPPABLE_WITH_CAVEATS` | minors only | 0 |
| `NEEDS_REVISION` | at least one major | 3 |
| `INVALID` | at least one blocker | 1 |

## Usage

```bash
python3 skills/backlog-review/scripts/check_backlog_structure.py BACKLOG.md
python3 skills/backlog-review/scripts/check_backlog_structure.py --json
```

Read the output and report it. Do not edit `BACKLOG.md`.

## When routing cannot be checked

The report carries `routing_table_read`. When it is `false`, the routing table was unreachable and **`unroutable_repo` did not run** — no repo was judged.

Say so in the report. Reporting every repo as unroutable from missing data would assert a violation the evidence does not support, and a clean report that silently skipped a check is worse than a report that says which check it could not run.

## Anti-patterns

- **Editing the backlog.** Read-only. A reviewer that edits cannot be trusted to report what it found.
- **Treating a heuristic finding as certain.** `possible_duplicate` and `vague_dod` ask questions. The human answers.
- **Ignoring `raw_with_evidence`.** It is the most informative finding here: someone measured and nobody advanced the status, which means the loop is being bypassed.
- **Silencing `stale_raw` by killing items in bulk.** A kill needs a `kill_reason` naming what was measured. Killing to clear a count produces exactly the unexplained kills gate G-K exists to prevent.
- **Reporting a clean verdict without saying `routing_table_read` was false.**

## Related

- The registry and its intake: [`rules/cycle-backlog.md`](../../rules/cycle-backlog.md)
- The loop that consumes it: [`rules/cycle-maintenance.md`](../../rules/cycle-maintenance.md)
- Routing: `scripts/route_domain.py`
- Bootstrap: [`skills/backlog-init/SKILL.md`](../backlog-init/SKILL.md)
- Intake: [`skills/backlog-item/SKILL.md`](../backlog-item/SKILL.md)
