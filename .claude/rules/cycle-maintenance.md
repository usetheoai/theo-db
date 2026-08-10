# Cycle: MAINTENANCE (macro super-loop)

Source of Truth for the macro super-loop that runs from a `BACKLOG.md` item all the way back to the same `BACKLOG.md` with the item's status advanced. Sits **above** `cycle-auto-plan`: where `cycle-auto-plan` orchestrates one item end-to-end, `cycle-maintenance` orchestrates the ongoing work — item by item — for as long as the ecosystem is maintained.

## Purpose

Close the feedback loop between **something someone noticed** (`BACKLOG.md`) and **what actually shipped** (`RELEASED` from `cycle-release`). Without the loop the registry rots: items get fixed but their status never advances, killed hypotheses are re-filed, and within weeks nobody trusts the backlog enough to look at it.

The cycle produces no new files of its own. It produces **status transitions on `B-NNN` blocks** and **traceability** between every released change and the item that motivated it.

### There is no `MAINTENANCE_COMPLETE`

The ancestor loop ended: `ROADMAP_COMPLETE` when every milestone was `[x]`, because a roadmap is a finite scope someone declared. **A backlog is not a scope, and an empty one is not an achievement.** An ecosystem under maintenance always has something worth measuring; an empty registry means nobody has looked recently, not that nothing is wrong.

So the empty state is `BACKLOG_EMPTY`, and it is a **prompt to sweep**, not a terminal verdict. Treating it as completion is how a maintenance system quietly stops working while reporting success.

## Pre-conditions

- `BACKLOG.md` exists at the umbrella root (created once by `/backlog-init`).
- At least one item is `raw` or `triaged`.
- The working branch is `workspace` (per `rules/git-safety.md` § 1 — `develop` integrates, `main` is release-only).

Do NOT trigger when:

- `BACKLOG.md` is missing — run `/backlog-init` first.
- Every item is `shipped` or `killed` — emit `BACKLOG_EMPTY` and recommend `/discover --sweep {domain}`.
- The human is mid-item on something else. One item in flight at a time; concurrency here means two loops editing the same registry.

## Chain

```
SELECT next item:
     ↓ read BACKLOG.md
     ↓ filter status ∈ {raw, triaged}
     ↓ rank: triaged before raw (measured beats unmeasured)
     ↓       then by age (oldest first — a registry that always works the
     ↓       newest item starves the rest and stops being a backlog)
     ↓ pick the first
     ↓
     ↓ if NO eligible item → emit BACKLOG_EMPTY (a prompt to sweep, not a finish line)
     ↓
ROUTE:
     ↓ scripts/route_domain.py <repo> → domain + specialist
     ↓ unroutable → ITEM_UNROUTABLE, surface to the human (gate G1)
     ↓
LOCK item:
     ↓ record knowledge-base/maintenance-runs/{B-NNN}-{date}.md (status: in_progress)
     ↓
DELEGATE:
     ↓ status raw     → /discover --mode {suggested_mode} B-NNN
     ↓                  ├── opportunity → status triaged → continue below
     ↓                  └── ITEM_KILLED → status killed → LOOP BACK to SELECT
     ↓ status triaged → /auto-plan B-NNN
     ↓                  (cycle-plan → implement → code-quality → review → release)
     ↓
ADVANCE:
     ↓ RELEASED → status shipped, with the release artifact linked
     ↓ blocked  → status unchanged, blocker surfaced, LOOP BACK to SELECT
     ↓
LOOP BACK to SELECT
```

## Phase contracts

| Phase | Input | Output | Hard gate |
|---|---|---|---|
| select | `BACKLOG.md` | one `B-NNN`, or `BACKLOG_EMPTY` | exactly one item in flight |
| route | the item's `repo` | domain + specialist | the repo resolves (G1) |
| lock | `B-NNN` | run record under `knowledge-base/maintenance-runs/` | no other run `in_progress` |
| delegate | `B-NNN` + status | opportunity, killed item, or release | the sub-cycle's own gates |
| advance | sub-cycle verdict | updated `B-NNN` block | status transition is legal |

## Verdicts

| Verdict | Meaning | Next |
|---|---|---|
| `ITEM_SHIPPED` | The item reached `RELEASED` and its block says `shipped` | Loop back to SELECT |
| `ITEM_KILLED` | Measurement refuted the hypothesis | Loop back to SELECT. **A successful outcome** |
| `ITEM_IN_FLIGHT` | Paused at a human-approval gate | Resume when the human answers |
| `ITEM_BLOCKED` | A sub-cycle blocked, recoverably | Surface, then loop back to SELECT — other items still move |
| `ITEM_UNROUTABLE` | `repo` is in no domain | Surface. The item cannot proceed until the repo is cloned or the routing table names it |
| `BACKLOG_EMPTY` | Nothing `raw` or `triaged` | **Run `/discover --sweep {domain}`.** Not a finish line |

There is no verdict for "the ecosystem is done".

## Ranking — why triaged outranks raw, and age outranks everything else

**Triaged before raw** because a triaged item already carries measured evidence. Its cost to finish is known; a raw item's is not. Working measured items first also keeps evidence fresh — an opportunity measured months ago describes a system that has since moved.

**Then oldest first.** A registry that always works the newest item starves the rest, and the starved items are exactly the ones nobody feels urgency about — which is not the same as the ones that do not matter. Age ordering is what keeps a backlog from becoming a list of whatever was mentioned most recently.

Neither rule outranks a human saying "do this one". The ranking exists so the loop can run unattended, not to override judgement.

## Anti-patterns

- **Treating `BACKLOG_EMPTY` as completion.** It means nobody has looked recently. Sweep.
- **Two items in flight.** Two loops editing `BACKLOG.md` collide on `B-NNN` allocation, and the ids are the audit trail.
- **Advancing status without the sub-cycle's verdict.** `shipped` set by hand means the checkbox stopped meaning anything — the exact rot this loop exists to prevent.
- **Re-selecting a killed item.** It carries `kill_reason` for a reason. Re-filing needs a new id with `supersedes:`, per `cycle-backlog.md § Step 2`.
- **Working only what is loud.** The ranking is there precisely because urgency and importance are not the same signal.
- **Selecting an item whose repo has no checkout.** It routes nowhere; `ITEM_UNROUTABLE` says so instead of pretending.

## Output

- `BACKLOG.md` — status transitions on `B-NNN` blocks
- `knowledge-base/maintenance-runs/{B-NNN}-{date}.md` — one record per run: what was selected, why, which specialist, what the sub-cycles returned

The run record is what makes the loop auditable after the fact: which items were picked, in what order, and what happened. Without it, a backlog whose items all say `shipped` cannot be distinguished from one somebody edited.

## Rollback

An item advanced in error is moved back with a note recording the advance and why it was withdrawn — never silently reset. An item whose `shipped` was withdrawn carries information a fresh-looking `triaged` item does not.

## Cross-references

- Schema for cycle rules: `rules/cycle-rule-schema.md`
- The registry and its intake: `rules/cycle-backlog.md`
- Measurement: `rules/cycle-discover.md`
- Orchestrator this delegates to: `rules/cycle-auto-plan.md`
- Routing: `scripts/route_domain.py`
- Specialists: `agents/README.md`
- Branching contract: `rules/git-safety.md`
