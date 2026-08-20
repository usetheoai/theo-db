# Cycle: BACKLOG

Source of Truth for the intake cycle. Skills consume this; do not duplicate content into SKILL.md.

## Purpose

Register **one unit of maintenance work** against the Theo ecosystem, cheaply and before anyone has measured anything. Outputs a numbered item in `BACKLOG.md` — never a plan, never code, never evidence.

This is phase 0 of the Squad chain. It exists because the downstream cycle (`cycle-discover`) demands measured evidence for everything it accepts, and that demand, applied at intake, would silence the most valuable signal a maintenance team has: the hunch. *"The theo-lens trace explorer feels slow"* is a legitimate thing to record and an illegitimate thing to plan against. BACKLOG separates the two — it takes the hunch, and hands DISCOVER the job of proving or killing it.

A backlog item is a **hypothesis with an owner and a closing criterion**. It is not a commitment.

## Pre-conditions

Invoke `/backlog-item {slug}` when ALL of:

- `BACKLOG.md` exists at the root of the governed scope — the umbrella when repos live below it, the repository itself when it is autonomous (created once by `/backlog-init`).
- There is one concrete thing to improve, fix, verify, or evolve in a repo that exists in the umbrella inventory.
- It maps to exactly one registered domain (see § Domain routing). Work spanning two domains is two items.

Do NOT trigger BACKLOG for:

- Work already in flight. Grep `BACKLOG.md` first — the dedup gate is mandatory, not advisory.
- A finding the sweep already produced. `/discover --sweep` registers its own items with evidence attached; re-registering them by hand creates the duplicate the single-registry rule exists to prevent.
- "Project X does it this way." That is not an item. See § Hard gates, G5.
- A question about how our own code works. Read the code.

## Chain

```
/backlog-item {slug}                         ← phase 0 · INTAKE (human, cheap, hypothesis)
     ↓ (produces: B-NNN in BACKLOG.md · status: raw · evidence: none-yet)
/discover --mode {review|live-test|bug|evolve} B-NNN
     ↓ (measures against OUR code/runtime)
     ├── evidence found  → status: triaged · evidence: <pointer>  → /to-plan
     └── nothing found   → status: killed   · kill_reason: <why>  → chain ends here
```

The second producer writes into the same registry without passing through this cycle:

```
/discover --mode {review|live-test} --sweep {domain}     ← no prior item
     ↓ (registers findings directly)
B-NNN · source: discover-review · evidence: <file:line> · status: triaged
```

One file, one schema, two entry paths. A sweep finding skips intake because it arrives with the evidence intake is not allowed to require.

## Phase contracts

| Phase | Input | Output | Hard gate |
|---|---|---|---|
| intake | one-sentence description + slug | `B-NNN` block in `BACKLOG.md`, status `raw` | G1–G5 all pass |
| (handoff) | `B-NNN` | item claimed by `/discover` | item is `raw` and unclaimed |

## Item schema

Every item is one `## B-NNN` block. Ids are monotonic, never reused, never renumbered — a killed item keeps its number so the audit trail survives.

```markdown
## B-014 — Reduce the theo-lens trace explorer p95   [ ]

domain: data-plane-ts
repo: theo-lens
suggested_mode: live-test
source: human
evidence: none-yet
why_now: the dashboard started loading a 30d trace window by default in 2026-07
status: raw
dod:
  - listing endpoint p95 below 800ms with a 30d window
  - regression covered by a test that fails on the current state
```

| Field | Required | Notes |
|---|---|---|
| `domain` | yes | routes to the specialist; must be a registered domain (G1) |
| `repo` | yes | must exist in the umbrella inventory (G1) |
| `suggested_mode` | yes | **a suggestion, not a decision** — DISCOVER may reclassify |
| `source` | yes | `human` \| `discover-review` \| `discover-live-test` \| `discover-bug` \| `discover-evolve` \| `live-incident` |
| `evidence` | yes | `none-yet` at intake; a pointer once DISCOVER measures |
| `why_now` | yes | what changed **in our system**; subject to G5 |
| `status` | yes | `raw` \| `triaged` \| `planned` \| `shipped` \| `killed` |
| `dod` | yes | ≥ 1 verifiable criterion (G4) |
| `kill_reason` | when `killed` | why the measurement did not support the hypothesis |

`suggested_mode` being non-binding is deliberate. A hunch filed as a `bug` that measurement reveals to be a `evolve` must change mode without leaving the backlog — reclassification is a DISCOVER outcome, not a re-intake.

### Status transitions

```
raw ──/discover measures──┬──> triaged ──/to-plan──> planned ──/release──> shipped
                          └──> killed (kill_reason mandatory)
```

`raw → planned` is forbidden. Nothing reaches a plan without passing DISCOVER's measurement.

## Domain routing

`domain` is what assigns the item to a specialist. **This table is this project's own**, written by hand on
2026-08-20 per the decisions in `knowledge-base/grills/registry-ownership-model-grill.md`. It replaced the
ecosystem-wide table the kit ships, which named 16 repos of which this machine has two — the exact failure the
paragraph below records for `theokit-sdk`.

`skills/backlog-init/scripts/detect_domains.py` emits a table from the directory layout, and the header it
generates says to edit it by hand when ownership does not follow that layout. **Here it does not**: the unit of
ownership in this project is the **pillar** (vector, lexical, columnar, …), not the repository. Two repos hold
eight pillars between them, so a repo-shaped table would send 66 of 80 items to one generic owner.

A consumer that keeps a table it did not write inherits a map of repos it does not have, and gate G1 then
refuses every item it files — correctly, since it genuinely cannot tell who owns the work. Measured on
`theokit-sdk` (2026-08-18): 88 items with measured `file:line` evidence, all `BLOCKER/unroutable_repo`. Measured
here on 2026-08-20 before this rewrite: `theodb-bench` `UNROUTED` (13 items) and `theo-db` `BROKEN ROUTE` to a
specialist that was never written.

**Read the `Repos` column as "which repo routes here by DEFAULT" — not "which repos this pillar touches".**
The distinction is load-bearing and is the reason this table needs no change to `route_domain.py`. Every repo
appears in exactly one row, so resolution by repo is deterministic; a pillar with an empty cell is reached
through the item's own `domain:` field, never by repo. Listing a repo under every pillar it touches would make
`route()` return whichever row comes first — it takes the first match and does not object — which is the
ambiguity this shape avoids by construction rather than by code.

| Domain | Repos (default route) | Specialist |
|---|---|---|
| `engine-pgrx` | `theo-db` | `agents/theo-pgrx.md` |
| `arnes` | `theodb-bench` | `agents/arnes.md` |
| `vetorial` | — | `agents/theo-recall.md` |
| `lexical` | — | `agents/theo-lexical.md` |
| `colunar` | — | `agents/theo-columnar.md` |
| `hot-path` | — | `agents/theo-hotpath.md` |
| `ai-surface` | — | `agents/theo-ai-surface.md` |
| `acervo` | — | `agents/theo-wiki.md` |
| `metodo` | — | `agents/theo-auditor.md` |
| `governanca` | — | `agents/governanca.md` |

**Why `theo-db` defaults to `engine-pgrx`.** It is the pillar holding the most items filed against that repo
(25 of the 60). The default is a starting point, not a verdict: `/backlog-item` asks for the `domain:` in the
grill, and a governance or vector item filed against `theo-db` takes its own pillar there. The default only
decides what `route_domain theo-db` answers when nobody said otherwise.

**`theo-concurrency.md` deliberately has no domain.** No item has been filed against it, and inventing a domain
so an existing agent has a row would put a routing target where the registry has no work — the same emptiness
gate G1 exists to refuse. The agent stays directly invocable.

### Repos this scope governs, and the ones it does not

Two, both in the table above and both verified on disk on 2026-08-20 (`git -C <repo> rev-parse HEAD`):
`theo-db` (the extension and engine) and `theodb-bench` (the measurement harness, a sibling checkout at
`../theodb-bench`). No item may name anything else — the table is the whole inventory.

The kit's shipped version of this section listed `theo-contextify`, `theo-gateway`, `theo-sandboox`,
`theokit-app` and `theo-itself` as named-but-not-cloned. **None of them belongs to this project**, and the
paragraph survived here only because the ecosystem-wide table did. It is removed rather than kept, because a
routing document that lists repos nobody here will ever file against teaches the reader to skim it.

What the removed paragraph got right, and this project just paid for, is worth keeping: **a routing table
describes disk, and disk drifts.** Re-verify before trusting it, rather than trusting the date on it.

## Verdicts

| Verdict | Meaning | Downstream action |
|---|---|---|
| `ITEM_REGISTERED` | Item written to `BACKLOG.md` as `raw` | Available for `/discover` |
| `ITEM_MERGED` | Dedup gate matched an open item; the new context was folded into it | No new id; the existing `B-NNN` proceeds |
| `ITEM_REJECTED` | Outside the ecosystem, or G5 refused it | Nothing written; the reason is surfaced to the human |

There is no "with caveats" band: an item is either in the registry or it is not.

## Hard gates

| # | Gate | Blocks on |
|---|---|---|
| G1 | **Domain + repo resolve** (executado por `skills/backlog-item/scripts/check_intake_gates.py`, que delega a `scripts/route_domain.py`) | `domain` not in the registered set, or `repo` not in the umbrella inventory. An item nobody owns is an item nobody does. |
| G2 | **Dedup search ran** (mesmo script; rodá-lo É a evidência) | No search of `BACKLOG.md` performed before writing. A collision on an open item forces `ITEM_MERGED`. |
| G3 | **Single domain** | The description spans two domains. Split it; one item, one specialist. |
| G4 | **Verifiable DoD** | Zero `dod` bullets, or every bullet unfalsifiable ("melhorar a performance"). Without a closing criterion the item never closes. |
| G5 | **No prior-art justification** | `why_now` justifies the item by what another project does rather than by something that changed in our system. This is the Squad signature rule (Unbreakable Rule: evidence is ours or it is not evidence). Reject and ask for the local reason. |

G1 e G2 são mecanizáveis e passaram a ser mecanizados; G3, G4 e G5 são julgamento e seguem conversacionais, cobertos pela bateria de evals da skill — automatizá-los produziria vereditos sobre linguagem que nenhuma medição sustenta.

G5 does not forbid *knowing* how others solved a problem — it forbids that knowledge from being the **justification** for the work. "We need caching because project X has it" is rejected. "We need caching because the endpoint makes 4 round-trips per request" is accepted, whether or not project X inspired the look.

Intake deliberately has **no evidence gate**. Requiring evidence here would collapse BACKLOG into DISCOVER and lose the hunch.

## Anti-patterns

- **Intake that turns into planning.** The output is a registry block. Solution design belongs downstream; an item that already prescribes the fix has pre-empted the measurement.
- **Evidence theatre at intake.** Inventing a plausible `file:line` so the item "looks solid". `evidence: none-yet` is the honest and correct value for a hunch — DISCOVER fills it in or kills the item.
- **Renumbering.** Reusing the id of a killed item, or resequencing after a purge. The number is the audit trail; a killed `B-007` stays `B-007` forever.
- **Registering the sweep's output by hand.** Duplicates what `--sweep` already wrote, with weaker evidence.
- **Multi-domain items.** "Improve ecosystem observability" is a program, not an item. It routes to nobody and closes never.
- **`dod` that restates the title.** "DoD: the trace explorer being faster" is the title again, not a criterion.
- **Treating `suggested_mode` as binding.** It is the filer's guess. Locking DISCOVER to it defeats the purpose of measuring.

## The index that opens the registry

`BACKLOG.md` MUST carry an `## Index` section immediately before the item blocks, listing **every**
item — one row each, linked to its own detail block — grouped into three buckets:

| Bucket | Statuses | The question it answers |
|---|---|---|
| **Open** | `raw`, `triaged` | registered, measured or not, but nothing is being built |
| **In flight** | `planned` | a plan exists; work is under way |
| **Closed** | `shipped`, `killed` | the chain ended — and `killed` is a *successful* ending |

`triaged` sits under **Open** deliberately. Measurement has run, but no plan exists, so nothing is
in flight; folding it into the in-flight count would make that number answer a different question
than the one people ask of it.

**The index is generated, never written.** `skills/backlog-review/scripts/backlog_index.py --write`
derives it from the blocks; `--check` exits 1 when it has drifted. `check_backlog_structure.py`
reports `index_stale` (major) when the file's index does not match the one the generator would
produce, and treats **absent as stale** — otherwise a registry opts out of the check by never
having the section, which is how the four registries in this ecosystem reached 592 items with
zero index rows.

This is not ceremony. A summary that disagrees with the items below it is worse than no summary:
a reader stops at the summary, so a wrong one reports absence where evidence exists — the same
failure `rules/knowledge-base-location.md` records for a split knowledge-base. Nothing forces the
index and the items to move together, so the gate is what keeps them honest.

## Output

- `BACKLOG.md` at the umbrella root — the single registry, spanning all repos in the inventory.
- `knowledge-base/backlog/{slug}-intake.md` — the intake grill log (one entry per answered question, with the G5 decision recorded).

The registry lives at the root of the governed SCOPE and not scattered below it, because a maintenance team asking "what is pending?" must have exactly one place to look. Per-directory backlogs inside one scope re-create the orphaned-findings problem the single-registry rule exists to solve.

What this never meant is "an umbrella is required". An autonomous repository is its own scope and keeps its own registry — `theokit-sdk` holds 88 items about `theokit-sdk`, and asking it to file them in a parent directory that is nobody's repository would put the registry outside the thing it governs.

## Rollback

An item registered in error is marked `status: killed` with a `kill_reason` — never deleted, never renumbered. If it was already `triaged`, the evidence DISCOVER attached stays on the block: knowing that something was measured and then dropped is worth more than a clean file.

## Cross-references

- Schema for cycle rules: `rules/cycle-rule-schema.md`
- Skill: `skills/backlog-item/SKILL.md`
- Bootstrap (once, at adoption): `skills/backlog-init/SKILL.md`
- Live environment declaration consumed by `/discover --mode live-test`: `rules/live-target.txt`
- Downstream: `rules/cycle-discover.md` — measures the hypothesis and flips the item to `triaged` or `killed`
- Then: `rules/cycle-plan.md` — consumes `triaged` items
- Branching contract for the registry commit: `rules/git-safety.md`
