# How to use the Squad ecosystem

A pipeline for taking a maintenance item from **hunch → measurement → plan → code → merge**, with Claude Code as the active agent at every phase. Each phase has hard gates, anti-patterns, rollback and an audit trail documented in `rules/cycle-*.md`.

```
BACKLOG → DISCOVER → PLAN → IMPLEMENT → CODE-QUALITY → REVIEW → RELEASE
   ↓          ↓         ↓        ↓            ↓           ↓        ↓
 B-NNN     measures   plans/   commits +   dead-code/   gate    develop→main
 (hunch)   OUR code            tests       fabrication/ tighter  PR + semver
            ↓                              wiring
       ITEM_KILLED ✔
       (chain ends — a successful outcome)
```

Each arrow is an **unbreakable chain** — you do not skip a phase, and you do not advance past an INVALID verdict. Unlike a roadmap pipeline, this one has no end state: `cycle-maintenance` loops for as long as the ecosystem is maintained.

## Which phase, when

| Question | Cycle | Entry point |
|---|---|---|
| "First time — there is no registry yet" | (one-shot bootstrap) | `/backlog-init` |
| "I noticed something worth looking at" | `cycle-backlog` | `/backlog-item {slug}` |
| "Is this hunch real?" | `cycle-discover` | `/discover --mode {review\|live-test\|bug\|evolve} B-NNN` |
| "Sweep a domain for things nobody filed" | `cycle-discover` | `/discover --sweep {domain}` |
| "Advance the next item end-to-end autonomously" | `cycle-maintenance` → `cycle-auto-plan` | `/auto-plan` (no arg) or `/auto-plan B-NNN` |
| "The measurement holds — design the fix" | `cycle-plan` | `/to-plan B-NNN` |
| "Requirements are still vague" | `cycle-plan` phase 0 | `/grill-me {slug}` |
| "Build it per the plan" | `cycle-implement` | `/implement {plan-slug}` |
| "Audit dead code + fabricated APIs post-implement" | `cycle-code-quality` | `/code-quality` |
| "Review before merge" | `cycle-review` | `/review {plan-slug}` |
| "Cut a release (develop → main + tag)" | `cycle-release` | `/release [bump-level]` |
| "Check the released thing works for its user" | `cycle-acceptance` | `/acceptance B-NNN` |
| "What has rotted in the registry?" | auxiliary | `/backlog-review` |
| "Which specialist owns this repo?" | auxiliary | `python3 scripts/route_domain.py {repo}` |
| "Just locate something in the code" | (no cycle) | Glob/Grep, or `/ast-grep` for structural queries |
| "Is this operation CP or AP?" | auxiliary | `/cap-theorem-specialist` |
| "The queue never drains / we OOM under load" | auxiliary | `/backpressure-specialist` |
| "One slow service took the whole site down" | auxiliary | `/resilience-specialist` |

## Quick start

### 1. Create the registry (once)

```bash
/backlog-init
```

Inventories the repos **from disk** (`find` + `git rev-list`), never from a documentation table. Builds the domain routing table, names the exclusions with their reasons, and seeds **zero items** — an item nobody filed has no `why_now`, no DoD and no owner.

### 2. Register a hunch

```bash
/backlog-item theo-lens-trace-latency
```

Four questions, one per turn: what changed in **our** system; which repo and therefore which domain; which discover mode looks right (a guess, not a decision); and the verifiable Definition of Done.

**No evidence is requested.** `evidence: none-yet` is the honest value for a hypothesis. Gate G5 rejects a `why_now` justified by what another project does.

### 3. Measure

```bash
/discover --mode live-test B-014
```

Runs `discover-plan → edge-cases → plan-confidence → execute → confidence`, measuring against our code or runtime. Two legitimate endings:

- **Opportunity** → the item becomes `triaged` with its evidence attached.
- **`ITEM_KILLED`** → the falsification criterion was met. The item closes with a `kill_reason` and the chain ends. **This is success** — the run stopped work that would have been justified by a hunch.

**Fast lane:** for `--mode bug` where a failing test already exists, enter directly at `/discover-execute`. A test that fails on the current state is a stronger measurement plan than any document describing one.

### 4. Ship it

```bash
/auto-plan B-014
```

Chains plan → implement → code-quality → review → release, pausing at each gate.

## The four modes

| Mode | Measures | Refuses when |
|---|---|---|
| `review` | A defect visible in our code | — |
| `live-test` | Behaviour in the running system | The domain has no block in `rules/live-target.txt` |
| `bug` | A reproduced defect | The failing test was never **run** |
| `evolve` | The cost of the status quo | — |

`review` carries a discipline the others do not: before recording a finding, rule out that the code is **dead**, that the caller **never existed**, and that the shape is **deliberate**. All three produce findings that look real and are not.

`live-test` carries the environment-vs-product obligation: a dev environment breaks for its own reasons, and an opportunity that cannot yet tell the two apart says so rather than picking the more interesting explanation.

**Mode is reclassifiable.** `suggested_mode` is the filer's guess. If measurement shows a different shape, switch and record why.

## Where things live

| Path | What |
|---|---|
| `BACKLOG.md` | The single registry, at the umbrella root |
| `knowledge-base/discoveries/plans/` | Measurement plans |
| `knowledge-base/discoveries/opportunities/` | Opportunities (the terminal artifact) |
| `knowledge-base/maintenance-runs/` | One record per macro-loop run |
| `knowledge-base/reviews/` | Edge-case reports |
| `rules/cycle-*.md` | The contracts. Source of truth for every phase |
| `agents/*.md` | The eight domain specialists |

## The specialists

Routing is deterministic: the item declares `repo`, and a repo belongs to exactly one domain.

```bash
python3 scripts/route_domain.py theo-lens
# repo   : theo-lens
# domain : data-plane-ts
# agent  : agents/data-plane-ts.md
```

A repo the routing table does not know **does not route** — gate G1 refuses the item rather than sending it to a specialist who cannot open the code. Read `agents/README.md` before assuming a build command: each specialist carries commands verified on disk, and documentation drifts.

## Unbreakable principles

- **Evidence is ours or it is not evidence.** "Project X does it this way" cannot fill the Evidence corner.
- **A pointer resolves, line included.** A cited line past the end of a file is evidence that moved.
- **Killing an item is success.** But never kill when the measurement could not run — target unreachable is not disproof. Stop and ask.
- **`unknown` is complete** for the constraint corner, and only there.
- **Ids are never reused.** A killed `B-007` stays `B-007`.
- **Measuring is reading.** Discover writes a document, never a patch.

## Common questions

### "The item is a one-line fix. Do I still file it?"

Yes. Squad's ancestor refused hotfixes and refactors at intake and routed them elsewhere; those are the core workload of a maintenance team, and the door is open for them here.

### "The measurement found nothing. Did I waste the run?"

No — that is the cycle working. Record the `kill_reason` naming what was measured and what it showed. An unexplained kill is indistinguishable from an abandoned run.

### "Can I skip `/discover-edge-cases`?"

It is where you ask what could make the measurement **lie** — a target that resolves but is stale, a method that observes a proxy, an environment fault read as a product defect. An unfalsifiable hypothesis found there is always MUST FIX: every other defect produces a wrong answer someone might catch; that one produces a measurement that cannot fail.

### "`/discover-confidence` returned INVALID. Now what?"

Read the caps. `fabricated_evidence` and `empty_corner_evidence` mean re-measure — `/discover-improve` refuses them by design, because no amount of editing fixes a pointer that does not resolve.

### "The backlog is empty. Are we done?"

No. `BACKLOG_EMPTY` means nobody has looked recently. Run `/discover --sweep {domain}`. There is no `MAINTENANCE_COMPLETE`: a backlog is not a scope.

### "How do I adapt this to another ecosystem?"

Replace the domain routing table in `rules/cycle-backlog.md`, write one specialist per domain in `agents/`, and declare your live environments in `rules/live-target.txt`. The phases, gates and evidence contracts are ecosystem-agnostic; the routing table and the specialists are not.

## Maintenance notes

- `python3 scripts/check_xrefs.py` validates that every cross-reference resolves.
- `python3 scripts/test_e2e_smoke.py` validates cycle-rule structure and skill frontmatter.
- `bash scripts/run_slice_tests.sh` runs each slice in its own process — slices ship colliding module basenames, so a single wide pytest process is unsound.
- Adding a domain? The routing table, the specialist file and `tests/test_route_domain.py` must agree; two guards fail loudly if they do not.
