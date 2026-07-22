# TheoDB Evolution — dimensions, checklists, and contracts (depth)

Loaded on demand by `theodb-evolution/SKILL.md`. This is the exhaustive layer: the full per-dimension question
lists, the seven compatibility levels, the Phase-6 validation taxonomy, the performance-metric catalog, and the
per-phase output contracts. Use it when designing or reviewing a specific capability; the SKILL.md is the framework.

## Table of contents

1. The seven dimensions — full question lists
2. The seven compatibility levels
3. The performance-metric catalog
4. The Phase-6 validation taxonomy
5. Per-phase output contracts (Phases 1–8)
6. Worked example — the lexical engine

---

## 1. The seven dimensions — full question lists

### Functional
New capability surfaces: vector, lexical, hybrid, columnar analytics, graph, embeddings, reranking, NL→SQL,
observability, automatic administration. The risk is **accumulating superficially-integrated features**. The
correct criterion is *not feature count* but:
- consistency of the SQL interface across capabilities;
- integration *between* capabilities (do they compose in one query / one transaction?);
- transactional behavior of the new surface;
- ease of operation;
- reuse of the same infrastructure (not a parallel stack);
- **absence of parallel incompatible paths**.

The declared differentiation is keeping vector + lexical + graph + analytics in the **same transactional database
and the same SQL surface** — a new capability that forks the interface or the transaction model erodes the moat.

### Storage
Covers: persistent formats, pages, WAL, caches, auxiliary files, index structures, compaction, GC, VACUUM,
checkpoints, crash recovery. Every persistent change must answer:
- Does the new version read old data?
- Does the old version still read the data?
- Is there a migration?
- Can the migration be interrupted?
- What happens on a crash mid-migration?
- Can the upgrade be repeated (idempotent)?
- How is corruption detected?
- How is rollback performed?

Because a **own `ALTER EXTENSION ... UPDATE` chain already exists** (M137), format-evolution and
extension-evolution are **one subsystem**, not secondary SQL scripts. Treat a page/format change with the same
rigor as an engine change.

### Transactional
Every new structure must preserve: MVCC, snapshots, isolation, commit/abort, WAL, recovery, replication, VACUUM,
tuple visibility, concurrent DDL. A lexical engine is *not* done when it returns correct BM25 — it must be correct
when:
- a row is updated;
- a transaction is aborted;
- two transactions change the same document;
- a process dies during indexing;
- the server restarts;
- VACUUM removes old versions;
- an index is rebuilt;
- a checkpoint occurs;
- an extension is updated;
- a query uses an old snapshot.

### Performance
Never a final step. Each change records: **baseline → hypothesis → change → controlled benchmark → systemic impact
→ decision.** Metric catalog in § 3. The rule (README / `public-copy.md`): performance targets are proven by
reproducible benchmarks in `docs/benchmarks/`, never asserted.

### Compatibility
The project declares PostgreSQL compatibility, so every evolution verifies against: drivers, types, operators,
casts, functions, planner, extensions, `pg_dump`, `pg_restore`, physical backup, migration, admin tools,
observability, PostgreSQL versions. "Compatible" must mean a **declared level** (§ 2), not "speaks the protocol".

### Operational
Moving from "works in a benchmark" to "stays healthy for months" requires: metrics; structured logs; query
identification; tracing; internal statistics; alerts; integrity checks; backup/restore; corruption diagnosis;
growth control; memory limits; defense against adversarial workloads; incident documentation. The 30-day dogfood
(M141) is worth more here than several isolated optimizations.

### Maturity
See the ladder in SKILL.md. Each release declares the stage each capability is at and the evidence that earns it.

---

## 2. The seven compatibility levels

"Compatible with PostgreSQL" is not one thing. Each release declares **which levels were tested**:

| Level | Scope | How it is tested |
|---|---|---|
| 1 — Protocol | wire protocol, connection | any PG driver connects and runs a query |
| 2 — SQL & types | SQL surface, types, operators, casts, functions | regression suite; `pg_regress`-style |
| 3 — Tools & drivers | psql, ORMs, language drivers | representative driver/tool smoke |
| 4 — Backup & restore | `pg_dump`/`pg_restore`, physical backup, PITR | dump→restore round-trip byte/behaviour check |
| 5 — Transactional behavior | MVCC, isolation, visibility, DDL concurrency | isolation suite (the § 4 concurrency/crash tests) |
| 6 — Extensibility | coexistence with other extensions, own AMs | install alongside; `CREATE EXTENSION` matrix |
| 7 — Operation & upgrade | `ALTER EXTENSION UPDATE`, cross-version, ops tooling | the M137 upgrade harness (`scripts/test-upgrade.sh`) |

A capability that passes Level 2 but was never checked at Level 4 or 7 is **not** "PostgreSQL-compatible" for a
user who relies on `pg_restore` or in-place upgrade — say so honestly.

---

## 3. The performance-metric catalog

Report the subset that the change can plausibly move — and always the ones it *risks regressing*:

- throughput (ops/sec, QPS);
- latency **p50, p95, p99, p99.9** (tail matters most in production);
- memory per record / per entity;
- CPU per operation;
- bytes written / bytes read;
- **write amplification / read amplification**;
- disk space (index size, total relation size);
- index build time;
- recovery time (after crash);
- VACUUM impact;
- concurrency impact (throughput under N clients);
- **out-of-RAM behavior** (the number that inverts wins);
- steady-state performance (not just cold/first-run).

Anti-self-deception: measure ≥ 3 runs, report mean ± std dev, matched-recall for ANN, note hardware + methodology,
never cherry-pick a recall/latency point. (Consistent with `council-benchmark`'s lens: *você mediu ou está supondo?*)

---

## 4. The Phase-6 validation taxonomy

A capability that touches persistent/transactional state runs the applicable subset — and states which it ran:

- **unit tests** — pure logic (the pgrx-free core is testable standalone; the M139 pattern);
- **property-based tests** — invariants over generated inputs;
- **SQL regression** — the compatibility Level-2 surface;
- **concurrency tests** — the transactional dimension (two writers, old snapshot, DDL concurrency);
- **crash tests** — `SIGABRT` + WAL replay (the `isolation/crash*.sh` / `m139-lexical-crash-smoke.sh` method);
- **fault injection** — errors at boundaries (I/O failure mid-flush, allocation failure);
- **fuzzing** — malformed inputs / adversarial documents;
- **differential tests** — same query vs a reference (e.g. own vs pg_textsearch, own-vector vs pgvector);
- **upgrade tests** — `ALTER EXTENSION UPDATE` from every released version (the M137 harness);
- **downgrade tests** — the rollback story is real, not aspirational;
- **benchmarks** — the § 3 metrics, reproducible in `docs/benchmarks/`;
- **memory tests** — per-entity overhead, leak checks, out-of-RAM;
- **long-running tests** — steady state, growth, VACUUM over time (feeds Phase-7 dogfood).

"Total absence when the plan promised them" is a hard fail (mirrors the implement test-obligation gate). A generic
green suite that never exercises abort/crash/upgrade is not proof of the transactional dimension.

---

## 5. Per-phase output contracts

### Phase 1 — Discovery
Outputs: **problem statement**, measured **baseline**, representative **workload**, **failure model**,
constraints, hypotheses, references. Answer: is this problem really the database's? Is there a simpler solution?
(The parsimony ladder applies before any structure is designed.)

### Phase 2 — Invariants
Declare the properties the change must preserve *before* architecture — from the invariant catalog (SKILL.md).
Concrete, testable statements ("an aborted txn never leaves rows in the index"), not vibes.

### Phase 3 — Architecture (ADR)
The ADR MUST contain: alternatives; trade-offs; interaction with PostgreSQL; interaction with WAL; interaction
with MVCC; persistent format; compatibility (which levels); test plan; instrumentation; benchmark plan; rollout;
**rollback**. An ADR without alternatives + rollback is incomplete (caps plan-confidence at 70).

### Phase 4 — Falsifiable spike
Goal: answer the biggest uncertainty fast, not produce final code. Typical uncertainties: does the PG API allow
this? is the performance physically possible? is the memory acceptable? can the format be recovered? does it work
out of RAM? is there an architectural incompatibility? Ends with a **verdict**: viável / viável-com-restrições /
não-viável / inconclusivo. (M139 is the template: 4 gates measured → GO, in one session, behind a feature flag.)

### Phase 5 — Incremental implementation (order is load-bearing)
1. metrics + logs (observe first — you cannot operate the invisible);
2. reference model (the correct-but-slow oracle to diff against);
3. core structure;
4. experimental path;
5. recovery;
6. concurrency;
7. upgrade;
8. optimization (only after correctness);
9. planner integration;
10. public surface (last — do not expose what is not yet proven).

### Phase 6 — Validation
The § 4 taxonomy. State which ran and which are N/A (with reason).

### Phase 7 — Dogfooding
Real use ≥ 30 days; measure growth, stability, errors, resource use, slow operations, VACUUM behavior, backup
time, restore time, upgrade impact, diagnosability. This is the M141 gate — the only path to a Stage-4 claim.

### Phase 8 — Consolidation
Remove temp flags; simplify APIs; eliminate duplicate paths; **freeze the persistent format**; document
limitations; define support; record permanent regressions. The capability is now a stable part of the surface,
not an experiment.

---

## 6. Worked example — the lexical engine (M139 → M140.x)

Answering the five questions for the in-PG BM25 engine, to show the shape:

- **Capability:** transactional BM25 lexical search *inside* PostgreSQL, on the same SQL surface as vector + hybrid.
- **Guarantee:** respects MVCC, commit, abort, VACUUM, recovery, `ALTER EXTENSION UPDATE`.
- **Evidence:** crash test (`m139-lexical-crash-smoke.sh`) + cross-session MVCC test + upgrade test +
  reproducible nDCG@10 vs `pg_textsearch` in `docs/benchmarks/`.
- **Costs:** cache memory; write amplification; ingest cost; the buffer-then-flush discipline (no PG calls from
  Tantivy's worker threads — `panic=unwind` is a safety prerequisite, #153).
- **Uncertainties:** behavior on datasets ≫ RAM; high-update-rate workloads; flush-consistency under background
  merge at scale (deferred to M140.4, tracked in #153).

The spike (M139) proved viability across four gates (index+search, MVCC, crash-real, cost) and stopped there
honestly — production hardening (cache, merge, VACUUM at scale, the pgrx-free core) is the sequenced M140.1–M140.4,
each a milestone that must re-answer the five questions and prove its invariants. That is the methodology in motion.
