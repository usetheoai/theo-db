# Cycle: BACKLOG

Source of Truth for the intake cycle. Skills consume this; do not duplicate content into SKILL.md.

## Purpose

Register **one unit of maintenance work** against the Theo ecosystem, cheaply and before anyone has measured anything. Outputs a numbered item in `BACKLOG.md` — never a plan, never code, never evidence.

This is phase 0 of the Squad chain. It exists because the downstream cycle (`cycle-discover`) demands measured evidence for everything it accepts, and that demand, applied at intake, would silence the most valuable signal a maintenance team has: the hunch. *"The theo-lens trace explorer feels slow"* is a legitimate thing to record and an illegitimate thing to plan against. BACKLOG separates the two — it takes the hunch, and hands DISCOVER the job of proving or killing it.

A backlog item is a **hypothesis with an owner and a closing criterion**. It is not a commitment.

## Pre-conditions

Invoke `/backlog-item {slug}` when ALL of:

- `BACKLOG.md` exists at the umbrella root (created once by `/backlog-init`).
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

`domain` is what assigns the item to a specialist. The registered set:

Verified on disk 2026-08-05 (`find -maxdepth 2 -name .git` + `git -C <repo> rev-list --count HEAD`), not copied from any inventory table.

| Domain | Repos (present on disk) | Specialist |
|---|---|---|
| `engine-go` | `theo` | `agents/engine-go.md` |
| `control-plane` | `theo-cloud`, `theo-traefik-mcp` | `agents/control-plane.md` |
| `data-plane-ts` | `theo-memory`, `theo-rag`, `theo-lens`, `theo-trust`, `theo-skills`, `theo-promptly` | `agents/data-plane-ts.md` |
| `theo-db` | `theo-db` | `agents/theo-db.md` |
| `infra-terraform` | `theo-infra-modules`, `theo-infra-live` | `agents/infra-terraform.md` |
| `contracts-auth` | `theo-contracts` | `agents/contracts-auth.md` |
| `frontend-dashboard` | `theo-cloud/dashboard` | `agents/frontend-dashboard.md` |
| `platform-cli` | `theo-cli`, `theo-storage` | `agents/platform-cli.md` |

**One repo, two domains — resolved by path, not by judgement.** `theo-cloud` holds both the Go control plane and the TypeScript dashboard (`theo-cloud/dashboard/package.json`, verified on disk). The `repo` field therefore takes `theo-cloud` for the Go half and `theo-cloud/dashboard` for the UI half. Listing the bare repo under both domains would make routing depend on iteration order — the same item routing differently on different runs, which works until it does not and nothing changed. `scripts/route_domain.py` enforces the one-repo-one-domain invariant, and `tests/test_route_domain.py::test_no_repo_belongs_to_two_domains` is what caught the ambiguity.

### Repos an inventory names but disk does not

`theo-contextify`, `theo-gateway`, `theo-sandboox`, `theokit-app` and `theo-itself` appear in the umbrella's `CLAUDE.md` and have **no checkout** as of 2026-08-05. They are listed here rather than deleted so that the divergence stays visible: an item filed against one of them routes nowhere until the repo is actually cloned, and `/backlog-item` gate G1 refuses it.

This is exactly why `skills/backlog-init/SKILL.md` mandates reading the inventory from disk. The umbrella's table claims it was "verified 2026-07-28" and states that a repo absent from it does not exist in the folder; a week later, five of its entries had no checkout. Documentation drifts, and a routing table that names a repo nobody has cloned sends work to a specialist who cannot open the code.

`theo-workspace` (a nested clone of the umbrella itself) takes no items.

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
| G1 | **Domain + repo resolve** | `domain` not in the registered set, or `repo` not in the umbrella inventory. An item nobody owns is an item nobody does. |
| G2 | **Dedup search ran** | No search of `BACKLOG.md` performed before writing. A collision on an open item forces `ITEM_MERGED`. |
| G3 | **Single domain** | The description spans two domains. Split it; one item, one specialist. |
| G4 | **Verifiable DoD** | Zero `dod` bullets, or every bullet unfalsifiable ("melhorar a performance"). Without a closing criterion the item never closes. |
| G5 | **No prior-art justification** | `why_now` justifies the item by what another project does rather than by something that changed in our system. This is the Squad signature rule (Unbreakable Rule: evidence is ours or it is not evidence). Reject and ask for the local reason. |

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

## Output

- `BACKLOG.md` at the umbrella root — the single registry, spanning all repos in the inventory.
- `knowledge-base/backlog/{slug}-intake.md` — the intake grill log (one entry per answered question, with the G5 decision recorded).

The registry lives at the umbrella root and not per-repo because a maintenance team asking "what is pending?" must have exactly one place to look. Per-repo backlogs re-create the orphaned-findings problem the single-registry rule exists to solve.

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
