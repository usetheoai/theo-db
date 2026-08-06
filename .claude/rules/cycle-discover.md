# Cycle: DISCOVER

Source of Truth for the discovery cycle. Skills consume this; do not duplicate content into SKILL.md.

## Purpose

Measure a hypothesis against **our own system** and either prove it or kill it. Outputs an opportunity — evidence, blast radius, and a way to verify the fix — never code.

This cycle inverts its ancestor. In Cycle, DISCOVER studied **how others solved a problem** and produced a blueprint of external patterns; it explicitly forbade looking at your own code. The Squad maintains a running ecosystem, and for that work the question *"how did project X do it?"* is the wrong one — it produces imitation, not maintenance. The question here is *"what is actually true about our code and our runtime, and is it worth changing?"*

The rename from **blueprint** to **opportunity** is deliberate. A blueprint is a design to copy. An opportunity is a measured gap in something we already run.

**Killing an item is a successful outcome of this cycle, not a failure of it.** A run that measures honestly and finds nothing has protected the plan cycle from work that would have been justified by a hunch. That is the job.

## Pre-conditions

Invoke `/discover --mode {review|live-test|bug|evolve}` when EITHER:

- A `B-NNN` item in `BACKLOG.md` has `status: raw` and is unclaimed, OR
- You are sweeping a domain for findings nobody has filed (`--sweep {domain}`).

Do NOT trigger DISCOVER for:

- **Studying how another project solved a problem.** This is the removed capability, not an oversight. The Squad justifies work by what is true in our system (`cycle-backlog.md` § Hard gates, G5). Reading someone else's code to learn a technique is fine and normal — it is simply not this cycle, and it is never the evidence.
- Locating a symbol in our own code. Use Grep/Glob.
- Anything answerable by reading our own `README.md` / `CLAUDE.md`.
- An item already `triaged`, `planned` or `shipped`. Re-measuring a closed item is how duplicate work enters.

## Chain

```
/discover --mode {mode} B-NNN            (or: --sweep {domain})
     ↓
/discover-plan {slug}
     ↓ (what will be measured, where, with which tool — the measurement plan)
/discover-edge-cases {slug}
     ↓ (what could make this measurement LIE — absorbed as MUST-FIX)
/discover-plan-confidence {slug}
     ↓ (gate on the measurement plan itself; INVALID returns to /discover-plan)
/discover-execute {slug}
     ↓ (runs the measurement → knowledge-base/discoveries/opportunities/{slug}-opportunity.md)
     │                        └─ or → ITEM_KILLED, and the B-NNN block records kill_reason
/discover-confidence {slug}
     ↓ (scores the opportunity; INVALID returns to /discover-plan)
/discover-improve {slug}   [optional — only when the score is NEEDS_REVISION]
```

An opportunity is the terminal artifact. Distilling one into a reusable skill is out of cycle and optional: invoke the standalone `/skill-creator` on demand.

### Proportionality — the fast lane

The full six-phase chain is calibrated for `evolve` and for `--sweep`, where what will be measured is genuinely open. It is disproportionate for a reproduced bug.

**Fast lane** — `--mode bug` where a failing test already exists: phases 1–3 collapse. A test that fails on the current state **is** a stronger measurement plan than any document describing one, and it is verifiable by execution rather than by review. Enter at `/discover-execute` with the repro and the failing test as the plan.

The fast lane is unavailable when the repro is not yet a test. "I can reproduce it by hand" is a plan, not a measurement, and it goes through the full chain like anything else.

## Modes

Every mode measures **our** system. They differ in what counts as a measurement.

| Mode | Finds | Evidence contract (all mandatory) | Primary lie risk |
|---|---|---|---|
| `review` | A defect or violation visible in our code | `file:line` · the rule or principle violated · why it matters *here*, not in general | The code is dead, the caller never existed, or the shape is deliberate |
| `live-test` | Behaviour wrong in the running system | `METHOD URL -> status` · console output · trace id where available · timing · screenshot for UI | **Environment vs product** — a dev-environment fault reported as a product defect |
| `bug` | A reproduced defect | Numbered repro · **a test that FAILS on the current state** | The repro depends on local state nobody else has |
| `evolve` | Measured cost of the status quo | The measurement itself (N round-trips, N duplicated call sites, N ms, N manual steps) | The cost is real and trivial |

**`bug` has a hard floor: no failing test, no bug.** A defect nobody can express as a failing test is not yet understood well enough to fix, and the test is what proves the fix later. This is the same discipline `cycle-implement.md` enforces at RED — brought forward, because writing it here is what makes the plan honest.

**`live-test` refuses on a domain with no block in `rules/live-target.txt`.** Six of the eight domains have none, by design — a Go library, a Postgres extension and a Terraform module have no surface a browser can probe. Refusing is correct; improvising a probe to look thorough produces theatre.

`live-test` carries one obligation the others do not: **name the uncertainty between environment and product.** `app-dev.usetheo.dev` is a dev environment, and dev environments break for reasons that have nothing to do with the code. An opportunity that cannot yet distinguish the two says so, in those words, rather than picking the more interesting explanation.

### Mode is reclassifiable

`suggested_mode` on the backlog item is the filer's guess (`rules/cycle-backlog.md` § Item schema). If measurement shows the hypothesis is a different shape — a "bug" that is really a micro-evolution, a "review" finding only observable at runtime — **reclassify and continue**. Record the reclassification and its reason in the opportunity. Forcing a measurement into the mode someone guessed at intake defeats the purpose of measuring.

## Phase contracts

| Phase | Input | Output | Hard gate |
|---|---|---|---|
| plan | `B-NNN` or domain + mode | measurement plan: what, where, which tool, what would falsify it | the plan names a tool that exists and a target that resolves |
| edge-cases | measurement plan | annotated plan with MUST-FIX items | every MUST-FIX has an answer or a stated open question |
| plan-confidence | annotated plan | score + verdict on the plan | no fabricated target; falsification criterion non-empty |
| execute | scored plan | opportunity **or** `ITEM_KILLED` | every evidence pointer resolves (G-E) |
| confidence | opportunity | score + verdict | INVALID returns to plan |
| improve (opt.) | NEEDS_REVISION opportunity | revised opportunity | bumped verdict on re-score |

## The four corners

Every opportunity populates four corners. An empty corner caps the score.

| Corner | Content |
|---|---|
| **Evidence** | The measurement, in the mode's contract above. Pointers must resolve. |
| **Constraint relation** | Does this **explore**, **subordinate**, **elevate** the declared constraint — or is it **local optimisation**? Cites `rules/current-constraint.md`. |
| **Blast radius** | What else across the ecosystem touches this. A change in `theo-contracts` reaches everything downstream; a change in `theokit-app` reaches nothing. |
| **Verification** | How we will know the fix worked — tied to the item's `dod` — and where the limit plausibly moves next. |

**The Constraint relation corner is advisory and may be answered `unknown`.** We do not instrument flow across the ecosystem, and a corner that demanded a constraint claim against data that does not exist would be answered by assertion — the exact defect G5 refuses at intake. `unknown` is honest and complete; it neither weakens the opportunity nor creates debt. See `rules/current-constraint.md` for why this is a lens rather than a gate.

The **Blast radius** corner is the one that earns its place in this ecosystem specifically. The repos form a dependency graph with `theo-contracts` at the stable base, and a maintenance change is dangerous in proportion to how far up that graph it sits.

## Verdicts

| Verdict | Meaning | Downstream |
|---|---|---|
| `SHIPPABLE` | Opportunity is measured, complete, and its pointers resolve | `/to-plan` |
| `SHIPPABLE_WITH_CAVEATS` | Complete, with stated open questions | `/to-plan`, caveats carried into the plan |
| `NEEDS_REVISION` | Recoverable via `/discover-improve` | loop |
| `INVALID` | Structural — a fabricated pointer, or an empty corner | back to `/discover-plan` |
| `ITEM_KILLED` | Measured honestly; the hypothesis did not hold | Item → `killed` + `kill_reason`. **Chain ends. This is success.** |

`ITEM_KILLED` is orthogonal to the other four: they grade a document, it reports an outcome. A killed item produces no opportunity to score.

## Hard gates

| # | Gate | Blocks on |
|---|---|---|
| G-E | **Evidence pointers resolve** | A cited `file:line` that does not exist, a URL never actually fetched, a trace id never observed, a test asserted to fail but never run. Fabricated evidence is the one unrecoverable defect in this cycle: everything downstream trusts it. |
| G-M | **Mode contract satisfied** | The mode's mandatory evidence is incomplete — most often `bug` without a failing test. |
| G-L | **Live target declared** | `--mode live-test` on a domain with no block in `rules/live-target.txt`. |
| G-C | **Corners populated** | Any of the four corners empty. `unknown` populates Constraint relation; it is an answer, not a blank. |
| G-K | **Kill is reasoned** | `ITEM_KILLED` without a `kill_reason` naming what was measured and what it showed. An unexplained kill is indistinguishable from an abandoned run. |

## Stop conditions

- Verdict `INVALID` → return to `/discover-plan` (the measurement plan was wrong, not necessarily the hypothesis).
- 3 consecutive iterations with no confidence improvement → escalate to a human.
- Measurement cannot be run at all (target unreachable, credential absent, tool missing) → **stop and ask the human.** Do not substitute a weaker measurement, do not reason about what the measurement would probably have shown, and do not record `ITEM_KILLED` — nothing was measured, so nothing was disproved.
- Either halt-loop emits BLOCKED → the cycle pauses; `/discover-confidence` must not honour the artifact.

## Halt-loop contracts

Two phases drive autonomous halt-loops via `ralph-loop:ralph-loop`, following the template in `rules/cycle-implement.md`: pre-flight guard against concurrent loops, formal stop conditions, post-promise sanity check, and an honest BLOCKED report over a false PASS.

- **`/discover-execute`** — completion promise `<promise>OPPORTUNITY_COMPLETE</promise>`, asserting that every plan question is `done` or `blocked` with a reason, every evidence pointer resolves on disk or in a recorded observation, all four corners are populated, and the mode contract is satisfied. The post-promise check re-verifies pointer integrity. Never emit on a partial state. `ITEM_KILLED` is emitted instead of the promise, with its `kill_reason`.
- **`/discover-improve`** — completion promise `<promise>OPPORTUNITY_IMPROVED</promise>`, asserting a re-run of the scorer in the emitting iteration reaches the target verdict. Partial improvement does not justify the promise.

## Anti-patterns

- **Discovery that turns into implementation.** The output is a document. An opportunity that already contains the patch has pre-empted the plan cycle and skipped its gates.
- **Fabricated evidence.** A plausible `file:line` nobody opened; a status code nobody requested; a test asserted to fail but never executed. This is the cycle's cardinal sin — everything downstream treats it as measured fact.
- **Prior art smuggled in as evidence.** "Project X does it this way" is not a measurement of our system. It may be true, useful, and the reason someone had the idea — it is still not evidence, and it cannot fill the Evidence corner.
- **Reporting a dev-environment fault as a product defect.** `app-dev.usetheo.dev` breaks for its own reasons. Name the uncertainty instead of resolving it toward the more interesting answer.
- **Refusing to kill.** Sunk cost after a long measurement makes a weak finding look shippable. A run that kills an item did its job; a run that ships a hunch it failed to confirm did the opposite.
- **Filling Constraint relation with a confident claim nobody measured.** `unknown` is the honest default while `current-constraint.md` is undeclared.
- **Improvising a live probe on a domain with no declared target.** Produces the appearance of runtime evidence with none of the substance.
- **Sweeping without registering.** A `--sweep` finding that stays in the run's output and never reaches `BACKLOG.md` is the orphaned-finding failure the single registry exists to prevent.

## Output

- `knowledge-base/discoveries/plans/{slug}-plan.md` — the measurement plan
- `knowledge-base/discoveries/opportunities/{slug}-opportunity.md` — the terminal artifact
- `BACKLOG.md` — the `B-NNN` block updated: `status` → `triaged` with `evidence`, or `killed` with `kill_reason`. A `--sweep` appends new blocks with `source: discover-{mode}`.

The study zone the ancestor cycle used (`knowledge-base/references/`, seeded at project inception and governed by a provenance rule) is **retired**: it existed to hold other people's code for imitation, which is the practice this cycle removed.

## Rollback

An opportunity that turns out wrong is simply not consumed downstream — supersede or delete the file under `knowledge-base/discoveries/opportunities/`. The `B-NNN` item returns to `raw` so it can be re-measured, with a note recording that the first measurement was withdrawn and why. Do not silently reset it: an item that was measured, believed, and then withdrawn carries information a fresh-looking `raw` item does not.

## Cross-references

- Schema for cycle rules: `rules/cycle-rule-schema.md`
- Upstream (intake): `rules/cycle-backlog.md` — supplies the `B-NNN` hypothesis this cycle measures
- Downstream: `rules/cycle-plan.md` — consumes `triaged` items and their opportunities
- Live environment declaration: `rules/live-target.txt`
- Constraint lens (advisory): `rules/current-constraint.md`
- Skills: `skills/discover-plan/SKILL.md`, `skills/discover-edge-cases/SKILL.md`, `skills/discover-plan-confidence/SKILL.md`, `skills/discover-execute/SKILL.md`, `skills/discover-confidence/SKILL.md`, `skills/discover-improve/SKILL.md`
- Halt-loop template: `rules/cycle-implement.md`
- Optional skill distillation (out of cycle): `skills/skill-creator/SKILL.md`
