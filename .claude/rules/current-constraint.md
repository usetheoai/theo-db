# Current constraint — a lens, not a gate

What currently limits the ecosystem's ability to ship value. Declared by a human, dated, with a review date.

## What this is for

A maintenance squad's characteristic failure is **local optimization**: shipping ten well-evidenced micro-evolutions into a stage that was never the limit, and mistaking the activity for throughput. This file exists so that `/discover` can ask *"where does this sit in the flow?"* while writing an opportunity — and so that the answer comes from a declaration someone made on the record, rather than from whatever the agent finds convenient in the moment.

## What this is NOT

**It is not a gate.** No item is blocked, deprioritized or rejected for failing to touch the constraint. `/discover` reads this file, states the relation in the opportunity's *Constraint relation* corner, and moves on.

The reason is measurement, and it is worth being blunt about: **we do not currently instrument flow across the ecosystem.** There is no per-stage lead time, no wait time, no WIP series, no cumulative flow diagram over the 21 repos. A hard gate asking *"does this touch the constraint?"* against data that does not exist would be answered by assertion — and an assertion dressed as a measurement is precisely what gate G5 of `cycle-backlog.md` exists to refuse. Building it into this corner would reproduce, one file over, the defect the system was designed to prevent.

So the corner is **advisory and may be answered `unknown`.** `unknown` is an honest, complete answer. It is not a finding, it is not debt, and it does not weaken the opportunity that carries it.

## Empirical identification — opportunistic, never required

The Squad's own cycles may surface real flow evidence as a by-product: a `--mode review` sweep noticing that one repo's PRs sit for days, a `live-test` run measuring that deploys lag merges by a week, a `--mode evolve` measuring a pipeline's duration. When that happens, record it here with its source and date.

This is **opportunistic**. No phase is required to produce it, no verdict depends on it, and no run is incomplete without it. If instrumenting flow properly is ever worth doing, it is worth doing as a backlog item measured like any other — not as a tax on every discovery.

## Declaration

```
status      = undeclared
declared_by =
declared_on =
review_on   =
constraint  =
evidence    =
kind        = physical | policy | external
```

### Field contract

| Field | Meaning |
|---|---|
| `status` | `undeclared` \| `declared` |
| `declared_by` | The human who made the call. A constraint with no name attached is a rumour. |
| `declared_on` | Absolute date. Constraints move; an undated one is unfalsifiable. |
| `review_on` | Absolute date this declaration must be revisited. Section 5.5 of the discipline: elevating a constraint relocates it, and a policy written for a constraint that moved outlives its own reason. |
| `constraint` | One sentence naming the limiting factor. |
| `evidence` | What supports it — measured, observed, or explicitly `qualitative judgement`. Say which. |
| `kind` | `physical` (capacity of people, pipelines, environments), `policy` (rules and approvals the organisation could change tomorrow), `external` (outside our control). |

## Current state

**`status = undeclared`.**

Nothing is declared yet, and the system works without it: `/discover` writes `Constraint relation: unknown — no constraint declared` and produces a complete, valid opportunity. Declaring one sharpens prioritisation; not declaring one costs nothing but that sharpening.

## Anti-patterns

- **Declaring a constraint to make the corner look filled.** `unknown` is the honest answer until someone actually decides. A fabricated declaration is worse than none: every opportunity after it inherits the fabrication as context.
- **Naming a team as the constraint.** Constraints are stages, policies, capacities and dependencies. "The backend team" is a stage described by its people, and describing it that way turns a flow problem into a performance conversation.
- **Letting the declaration outlive its `review_on`.** An expired declaration is stale context that reads as current. `/discover` surfaces the expiry rather than trusting the value.
- **Turning this into a gate.** The moment an item is refused for not touching the constraint, the corner starts being answered strategically instead of honestly.

## Cross-references

- Consumer: `rules/cycle-discover.md` — reads this while writing the *Constraint relation* corner
- Intake gate that this file must not become: `rules/cycle-backlog.md`
