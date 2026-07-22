---
name: theodb-evolution
description: How to conduct TheoDB's technical evolution — capability-driven, evidence-gated milestones that preserve PostgreSQL's invariants (MVCC, WAL, VACUUM, recovery, upgrade) while moving the database from pré-1.0 toward sustained production maturity. Use this WHENEVER planning or scoping the next milestone/capability, designing a storage/index/persistent-format or extension-upgrade change, deciding whether a capability is actually "done", reviewing whether a change kept PostgreSQL's transactional guarantees, choosing between features, or judging production-readiness/maturity — even when the user only says "what should we build next", "is this ready for production", "evolve the database", "add BM25/graph/columnar", or names a capability (lexical engine, columnar TableAM, graph, vector index). Complements /to-plan, /plan-confidence, /review and the ADR process with the seven evolution dimensions, the five-question milestone gate, the eight-phase capability process, the invariant catalog, and the maturity ladder. TheoDB is PostgreSQL 18 upstream (no fork) + the Rust extension theodb_rs — that architecture is what makes this methodology load-bearing rather than optional.
user-invocable: true
allowed-tools: Read Glob Grep Bash
argument-hint: "{capability-or-milestone-topic}"
---

# TheoDB Evolution — capability-driven, evidence-gated database evolution

TheoDB is **PostgreSQL 18 upstream (no fork) + the `theodb_rs` Rust extension** (own `vector` type + ANN AMs,
hybrid search, native graph, columnar TableAM). Because it is *composed onto a live transactional engine* rather
than built from zero, evolving it is not "add code" — it is **adding a verifiable capability without breaking the
transactional, recovery, and compatibility guarantees the host already gives users.** This skill is the Source of
Truth for *how* to select, design, prove, and land each evolution. It sits **above** the mechanical cycle skills
(`/to-plan`, `/plan-confidence`, `/implement`, `/review`) and feeds them: cite it in the plan's ADRs and in the
milestone DoD.

## The one rule everything else serves

**Delivery is not the merge. Delivery is the evidence that the desired behavior exists.** A milestone that merged
code but cannot show — with a reproducible artifact — that the capability holds under abort, crash, VACUUM,
upgrade, and out-of-RAM is **not done**. This is the same lesson the project already lives: *tests passing ≠
system works*; here, *code merged ≠ capability exists*. Everything below is machinery to make that rule operational.

## The five-question milestone gate (the central test)

Every non-trivial milestone MUST answer these five, in the plan and again in the implementation summary. If any
answer is "we don't know yet", the milestone is not scoped — return to discovery.

1. **What is now possible?** (the capability, in user-observable terms — not "added a cache" but "p99 of BM25 over
   larger-than-RAM datasets dropped from X to Y without breaking MVCC/recovery/VACUUM/upgrade").
2. **What guarantee was added?** (the invariant now provable — e.g. "an aborted txn never leaves rows in the index").
3. **What evidence demonstrates it?** (the reproducible artifact: crash test + concurrency test + upgrade test +
   `docs/benchmarks/` number — never prose, never "should work").
4. **What cost or trade-off was introduced?** (memory, write amplification, build time, a new failure mode — stated
   honestly, never hidden).
5. **What uncertainty remains?** (the honest edge — "behavior beyond N× RAM / under high update rate is unmeasured").

A milestone description that cannot fill all five is not FAANG-level rigor; it is code looking for a justification.

## The eight-phase capability process (maps onto the existing cycle)

Each significant capability walks these phases in order. They map onto the cycle skills — this skill supplies the
*content* the cycle mechanics carry. Skip a phase only with a recorded reason (an ADR line), never silently.

| Phase | What it produces | Cycle home |
|---|---|---|
| 1 — Discovery | problem statement, workload, **measured baseline**, failure model, constraints, hypotheses, references | `/discover-plan` → `/discover-execute` |
| 2 — Invariants | the properties the change MUST preserve, declared **before** architecture (see § Invariant catalog) | plan's `## ADRs` + DoD |
| 3 — Architecture (ADR) | alternatives, trade-offs, PG/WAL/MVCC interaction, persistent format, compat, test plan, instrumentation, benchmark, rollout, **rollback** | `docs/adr/NNNN-*.md` |
| 4 — Falsifiable spike | fast answer to the biggest uncertainty; ends with a verdict: **viável / viável-com-restrições / não-viável / inconclusivo** | `/to-plan` spike milestone (M139 is the template) |
| 5 — Incremental impl | in order: **metrics+logs → reference model → core structure → experimental path → recovery → concurrency → upgrade → optimization → planner integration → public surface** | `/implement` (halt-loop, TDD) |
| 6 — Validation | unit + property + SQL-regression + concurrency + crash + fault-injection + fuzz + differential + upgrade + downgrade + benchmark + memory + long-running (see references) | `/implement` gates + `/review` |
| 7 — Dogfooding | real use ≥ 30 days: growth, stability, resource use, VACUUM behavior, backup/restore time, upgrade impact, diagnosability | the dogfood anchor (M141) |
| 8 — Consolidation | remove temp flags, simplify APIs, kill duplicate paths, **freeze the persistent format**, document limits + support, record permanent regressions | release + `docs/adr/` |

**Order matters in Phase 5.** Metrics and logs go *first* — a capability you cannot observe is a capability you
cannot operate or debug when it breaks at 3 a.m. Optimization goes *late* — never optimize a structure whose
correctness (recovery, concurrency, upgrade) is not yet proven.

## The seven dimensions (analyze every evolution across all of them)

A change is evaluated in **seven dimensions simultaneously** — most production defects live in a dimension the
author never considered. The full per-dimension checklists are in
[`references/dimensions-and-checklists.md`](references/dimensions-and-checklists.md); the load-bearing summary:

| Dimension | The question it forces | Fatal miss |
|---|---|---|
| **Functional** | Does it integrate into the *same SQL surface + same transaction*, or is it a parallel incompatible path? | A feature bolted on beside the others (the anti-moat) |
| **Storage** | New reads old? Old reads new? Migration interruptible + repeatable + crash-safe? Corruption detectable? Rollback? | A persistent format with no upgrade/rollback story |
| **Transactional** | Correct under update / abort / concurrent writers / mid-index crash / restart / VACUUM / reindex / checkpoint / extension-update / old snapshot? | "Returns correct BM25" mistaken for "is done" |
| **Performance** | baseline → hypothesis → change → **controlled benchmark** → systemic impact → decision; incl. out-of-RAM + steady-state | A win at 2k rows that inverts out of RAM |
| **Compatibility** | Which of the seven compatibility levels were **tested** this release? (protocol → SQL/types → tools/drivers → backup/restore → transactional → extensibility → operation/upgrade) | "PostgreSQL-compatible" meaning only "speaks the wire protocol" |
| **Operational** | Metrics, structured logs, query id, tracing, integrity check, backup/restore, corruption diagnosis, growth/memory limits, adversarial-workload defense | "Works in benchmark" that dies after weeks |
| **Maturity** | Which stage (0–5) does this move the capability to, and is the evidence for that stage actually present? | Claiming a stage without its evidence |

The declared differentiation of TheoDB — **vector + lexical + graph + analytics in one transactional store, one
SQL surface** — is a *functional-dimension* property. A capability that regresses it (a parallel path, an
inconsistent interface) is a strategic loss even if it ships fast.

## The invariant catalog (declare, then prove — Phase 2 + Phase 6)

Any change that touches persistent state or transaction visibility MUST declare which of these it preserves, and
Phase 6 MUST prove each with a test. These are the PostgreSQL guarantees users already rely on — breaking one
silently is the worst class of defect this project can ship.

- **Aborted-txn purity** — an aborted transaction never leaves anything visible in the structure.
- **Committed durability** — a committed transaction is still visible after restart (WAL replay).
- **Snapshot correctness** — a reader never sees a version invisible to its snapshot (MVCC).
- **VACUUM safety** — reclaiming dead versions never corrupts or loses live data.
- **Crash safety** — a mid-operation `SIGABRT` + WAL replay yields a consistent structure (the `isolation/crash*.sh` method; the M136 cassert-CI is the net).
- **Upgrade safety** — `ALTER EXTENSION ... UPDATE` is total, idempotent, interruptible-and-resumable, and never leaves the cluster unable to start. Format-evolution and extension-upgrade are **one subsystem** (the M137 chain), not secondary SQL scripts.
- **Recall floor** (approximate structures) — an ANN/lexical result respects the declared minimum recall; "approximate" is a *bounded* promise, not an excuse.

Precedent to reuse (Rule 9, don't reinvent): `theodb_rs/src/am/page/` (blob-over-pages + `GenericXLog`),
`am/columnar.rs` (MVCC-via-heap-catalog, M99), `isolation/crash*.sh` (#46/#47 durability harness),
`m139-lexical-crash-smoke.sh` (crash proof of the lexical spike).

## The maturity ladder (name the stage; earn it with evidence)

Evolution is a change of **stage**, not just of code. Name the stage a milestone targets and require the stage's
evidence — do not claim a stage you cannot back.

| Stage | Name | Evidence that earns it |
|---|---|---|
| 0 | Protótipo | functional demo, local tests, format still mutable |
| 1 | Experimental | documented invariants, automated regression, reproducible benchmarks, known-bugs logged |
| 2 | Preview | upgrade tested, crash-safety proven, compat levels declared, basic observability, first real workloads |
| 3 | Beta operacional | sustained use, backups actually restored, incidents analyzed, predictable perf, rollback procedures |
| 4 | Production-ready (delimitado) | explicit production scope, SLOs, compatibility matrix, operating history, security policy, stable release cadence |
| 5 | Plataforma madura | safe multi-generation upgrades, third-party extensions, ecosystem, full ops docs, long-term predictability |

TheoDB is honestly **pré-1.0** and does not claim production-readiness before sustained evidence — the 30-day
dogfood (M141) is the gate to even *begin* the Stage-4 claim, and by the `dogfood-golden-rule` no volume of
benchmarks substitutes for it. A single well-run dogfood milestone advances maturity more than several isolated
optimizations.

## The North Star (what "better" means here)

Not "more features than other databases", and not "beat every competitor on every benchmark". The sustainable
North Star is:

> **Build the most integrated *open* PostgreSQL platform for transactional + AI-native + analytical workloads,
> with demonstrable guarantees of correctness, compatibility, portability, and operation.**

This is grounded in a measured finding, not a slogan: **beating ScaNN on raw vector throughput inside a permissive
PostgreSQL extension was measured as structurally infeasible** (paradigm gap: anisotropic AH-LUT + not paying the
MVCC/WAL tax — M73/M74, `docs/adr/0033`/`0035`). That is not failure; it is engineering: *measure, acknowledge the
limit, reposition the differentiation where a real structural advantage exists* — the integrated, transactional,
open surface. Every proposed evolution should be checked against this North Star: does it deepen the integrated
open platform with a demonstrable guarantee, or just add surface area?

## How to use this skill

- **Selecting the next evolution:** phrase the candidate as a *capability with a measured baseline and a success
  metric* (the five questions), not as a code task. If it cannot be phrased that way, it is not ready — run Phase 1.
- **Designing a capability:** walk Phases 1–4; the ADR (Phase 3) must carry the invariant list (Phase 2) and the
  rollback plan. A storage/format/upgrade change is a *subsystem* — treat it with the M137 rigor.
- **Deciding "is it done":** run the five-question gate + the invariant proofs (Phase 6). Correct results are
  necessary, not sufficient — abort/crash/VACUUM/upgrade/out-of-RAM behavior is the rest of "done".
- **Reviewing a change:** the seven dimensions are the review checklist; a change silently strong in one dimension
  and unconsidered in another (typically storage or transactional) is the flag.
- **Judging production-readiness:** locate the capability on the maturity ladder and demand the stage's evidence;
  the honest answer is usually a lower stage than the enthusiasm suggests.

Read [`references/dimensions-and-checklists.md`](references/dimensions-and-checklists.md) when you need the
exhaustive per-dimension question lists, the seven compatibility levels, the Phase-6 test taxonomy, or the
per-phase output contracts. Keep this SKILL.md as the framework; the reference is the depth.

## Anti-patterns

- **"It merged, so it's done."** The merge is the start of proof, not the end. (Violates the one rule.)
- **Phrasing a milestone as a code task** ("implement the cache") instead of a measured capability. (Fails the five questions.)
- **Optimizing before proving correctness** — a fast structure that loses data on crash is worthless. (Phase-5 order.)
- **Treating a persistent-format change as a side SQL script** instead of the upgrade *subsystem* (M137 lesson).
- **"PostgreSQL-compatible" as a vibe** — no declared, tested compatibility levels.
- **Claiming a maturity stage without its evidence** — especially "production-ready" before the 30-day dogfood.
- **Chasing a benchmark the North Star already ruled structurally infeasible** (vector-QPS-vs-ScaNN) instead of
  deepening the integrated open surface where the advantage is real.

## Cross-references

- Cycle mechanics this skill feeds: `.claude/rules/cycle-discover.md`, `cycle-plan.md`, `cycle-implement.md`, `cycle-review.md`; skills `/to-plan`, `/plan-confidence`, `/implement`, `/review`.
- Gates that enforce pieces of it: `.claude/rules/testing.md` (§ 4.1 edge vs negative), `error-handling.md`, `code-quality-golden-rule.md`, `.github/workflows/cassert-sql-safety.yml` (crash-net), `scripts/test-upgrade.sh` (upgrade subsystem).
- Precedent capabilities: M99 (columnar TableAM), M108+ (graph), M135 (PG18), M136 (quality gates), M137 (upgrade chain), M139 (lexical spike — the Phase-4 template), M140.1–M140.4 (the sequenced lexical engine), M141 (dogfood).
- North Star + measured limits: `docs/adr/0002`, `0033`, `0035`, `0036`; `CLAUDE.md` (Esforço ≠ Complexidade; TheoDB rules 1/5/7).
- Reference depth: [`references/dimensions-and-checklists.md`](references/dimensions-and-checklists.md).
