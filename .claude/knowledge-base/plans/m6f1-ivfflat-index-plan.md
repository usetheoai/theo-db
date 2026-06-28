---
slug: m6f1-ivfflat-index
milestone_id: M9
created_at: 2026-06-28
goal: Validate + benchmark the IVFFlat vector index in the recall harness, closing docs/features 03 and 04
---

# Plan: IVFFlat / IVF vector index — validate + benchmark (features 03/04)

> **Version 1.0** — Close `docs/features/03-indice-ivfflat.md` + `04-indice-ivf.md` with measured evidence: add
> IVFFlat as a first-class index in the recall@k harness (it ships in pgvector but was never exercised by us),
> benchmark it against HNSW on the same dataset with the same recall methodology, and assert recall via an
> integration test. Pure parsimony: the index already exists in pgvector (no new dependency) — this slice
> *validates + measures* it, mirroring the existing `_hnsw_spec`/`_diskann_spec` harness pattern.

## Goal

> Enable TheoDB to recommend IVFFlat on evidence by benchmarking it in the recall@k harness, measured by the
> IVFFlat integration test reporting recall@10 in [0,1] with the index actually used (planner DuckDB n/a —
> `Index Scan`/bitmap on `bench_ivf`) on a real run.

## Context

`docs/features/03-indice-ivfflat.md` + `04-indice-ivf.md` document the IVFFlat / IVF-family vector index as a
TheoDB capability. pgvector (shipped since M0, `Dockerfile`) provides `CREATE INDEX … USING ivfflat (col
{opclass}) WITH (lists = N)` with the query-time knob `SET ivfflat.probes = N`. The M2 recall@k harness
(`benchmarks/theodb_bench/`) already benchmarks HNSW + DiskANN with a shared spec framework
(`_hnsw_spec`/`_diskann_spec` + `run_benchmark`), but IVFFlat was never added — so features 03/04 are
"available in the extension, unvalidated by us" (honest gap from the features audit). This slice closes that:
add `_ivfflat_spec`, wire it into `--index`, and measure it on the same dataset as HNSW with the same
distance-thresholded recall (ANN-Benchmarks semantics). No new dependency (Rule 9 / parsimony rung 4).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `benchmarks/theodb_bench/__main__.py` | 143 | `c421550` (2026-06-27) | CLI: `--index {hnsw,diskann,both}` → `index_specs`; `_hnsw_spec`/`_diskann_spec` | existing hnsw/diskann specs + `--index` choices stay valid; ivfflat is additive |
| `benchmarks/tests/test_integration.py` | (exists) | `d00e330` (2026-06-28) | integration tests vs real container | existing tests stay green; an ivfflat test is appended |
| `docs/features/03-indice-ivfflat.md` | (exists) | — | IVFFlat spec | add an "implemented/validated" note |
| `docs/features/04-indice-ivf.md` | (exists) | — | IVF spec | add an "implemented/validated" note |
| `docs/benchmarks/m9-ivfflat.md` (NEW) | 0 | — | (to be created) measured IVFFlat-vs-HNSW recall report | — |
| `CHANGELOG.md` | (exists) | — | Public contract | `[Unreleased]` gets the F1 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `build_config` / `_hnsw_spec` / `_diskann_spec` in `benchmarks/theodb_bench/__main__.py`
  - **Callers:** `main()` (CLI); `benchmarks/tests/test_integration.py` (builds configs directly / via parser).
  - **External:** no — dev-only benchmark tooling. Adding `_ivfflat_spec` + an `ivfflat`/`all` choice is additive; the existing `hnsw`/`diskann`/`both` paths are unchanged.
- **Symbol:** `run_benchmark` in `harness.py` — consumes `index_specs` generically (iterates `config["index_specs"]`); a new spec needs no harness change.

Enumerated via `grep -rn '_hnsw_spec\|_diskann_spec\|index_specs\|--index' benchmarks/`.

### Domain glossary

- **IVFFlat** — pgvector's inverted-file-with-flat-quantization index: partitions vectors into `lists` clusters; queries scan `probes` nearest clusters. `WITH (lists = N)` at build, `SET ivfflat.probes = N` at query.
- **recall@k (distance-thresholded)** — ANN-Benchmarks semantics already implemented in `recall.py` (the harness metric); engine-agnostic across index types.
- **opclass** — `vector_l2_ops` / `vector_cosine_ops` (the `_OPCLASS` map), shared by all index types.
- **sweep** — the per-index list of (label, session-GUCs) the harness runs to trace the recall×QPS curve (e.g. `ivfflat.probes` values).

### Architecture boundaries affected

Per `rules/architecture.md`: the change is entirely within the **dev-only benchmark tooling** (`benchmarks/`),
which is a client of the DB via the `VectorDB` adapter. IVFFlat is a built-in pgvector access method already in
the shipped image — no product-layer code, no new dependency, no Dockerfile change.

## Prior Art & Related Work

- **Internal (the pattern to mirror):** `benchmarks/theodb_bench/__main__.py:50-61` (`_hnsw_spec`) — the exact spec shape (name/index_name/ddl/sweep) IVFFlat copies; `harness.py::run_benchmark` (consumes specs generically); `recall.py` (the shared recall metric).
- **Internal (M2 evidence):** `docs/benchmarks/m7-hybrid-recall.md` / the vectorscale recall reports — the report format.
- **External:** pgvector IVFFlat docs (`https://github.com/pgvector/pgvector#ivfflat`) — `lists` build param + `ivfflat.probes` query knob + the "create the index after the table has data" guidance.
- **Reference:** `.claude/knowledge-base/references/pgvector/README.md` (IVFFlat usage).

## Objective

- [ ] `_ivfflat_spec(table, opclass)` builds `USING ivfflat (embedding {opclass}) WITH (lists=…)` with a `probes` sweep, mirroring `_hnsw_spec`.
- [ ] `--index` accepts `ivfflat` (and `all` = hnsw+ivfflat[+diskann]); existing choices unchanged.
- [ ] An integration test benchmarks IVFFlat on a real container and asserts recall@10 ∈ [0,1] with the index used.
- [ ] A measured report `docs/benchmarks/m9-ivfflat.md` records IVFFlat recall×QPS vs HNSW (honest numbers).
- [ ] features 03 + 04 get an "implemented/validated" note pointing at the report.

## ADRs

### D1 — Validate the existing pgvector IVFFlat; do not add a dependency

**Decision:** Add IVFFlat to the recall harness as a first-class index (spec + `--index` choice + test +
report). Treat features 03 (IVFFlat) and 04 (IVF) as the **same pgvector IVFFlat family** — 04 (generic "IVF")
is satisfied by IVFFlat, the IVF index pgvector provides.

**Rationale:** IVFFlat ships in pgvector (already in the image) — Rule 9 / parsimony rung 4 (reuse the installed
dependency). The harness spec framework already supports a new index with zero harness changes. "IVF" and
"IVFFlat" are the same access method in pgvector; documenting them as one closes both specs honestly without
inventing a second index.

**Alternatives considered:** *A separate non-flat IVF index* — rejected: pgvector's IVF index IS IVFFlat; there
is no distinct "IVF" AM to implement (inventing one would be YAGNI + dishonest). *Skip IVFFlat (HNSW/DiskANN
suffice)* — rejected: the feature specs 03/04 exist and the audit flagged them unvalidated; closing them with
measured evidence is the mandate.

**Consequences:** features 03 + 04 are both closed by the one IVFFlat validation; the report notes 04≡03 (pgvector IVF family).

### D2 — `lists` + `probes` chosen for an honest recall×QPS sweep

**Decision:** Build with `lists ≈ rows/1000` (pgvector guidance for ≤ 1M rows) and sweep `ivfflat.probes` across
a small set (e.g. 1, 10, lists) to trace the recall×QPS curve, forcing the index on (`SET enable_seqscan=off`)
like `_hnsw_spec`.

**Rationale:** pgvector's documented `lists` heuristic; probes is the recall/speed knob. Forcing the index off
seqscan measures the index, not the planner's small-N seqscan choice (the existing harness methodology).

**Alternatives considered:** *A single probes value* — rejected: one point is not a curve (no recall/QPS
trade-off shown). *lists=100 fixed* — rejected: must scale with N for an honest build.

**Consequences:** the report shows a real IVFFlat recall×QPS curve comparable to HNSW's.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| IVFFlat recall depends on `lists`/`probes` tuning — a bad pick looks worse than HNSW unfairly | Medium | Use pgvector's `lists≈rows/1000` heuristic + a probes sweep; report the curve, not one point (honest) | Bench |
| IVFFlat must be built AFTER data load (empty-table build → poor clusters) | Medium | The harness loads vectors then builds the index (existing order); assert non-trivial recall | Bench |
| Small synthetic N may give low IVFFlat recall | Low | Report measured numbers honestly; the test asserts recall ∈ [0,1] + index used, not a fixed floor (IVFFlat trades recall for speed by design) | Bench |

## Unresolved Questions

- Q1 — Treat 04 (IVF) as a distinct index? Resolved at plan time: no — pgvector's IVF index IS IVFFlat; 04 is closed by the same validation (documented).
- Q2 — Assert a recall floor like HNSW's 0.90? Resolved: no — IVFFlat at low probes legitimately recalls lower; assert recall ∈ [0,1] + index-used + that higher probes ⇒ ≥ recall (monotone-ish), not a fixed 0.90 floor.

## Dependencies

F1 adds **no new dependency** (Unbreakable Rule 9). IVFFlat is a built-in pgvector access method already in the
shipped image; the harness (numpy/psycopg2, dev-only) is unchanged.

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `pgvector` IVFFlat | 0.8.3 (shipped) | the index under validation | PostgreSQL License | already shipped (M0/M2); no change |
| `numpy`/`psycopg2` (harness, dev-only) | as in requirements.txt | recall math + DB client | BSD/LGPL | already dev deps |

No CVE audit delta: zero new declared dependencies.

## Dependency Graph

```
Phase 1 (_ivfflat_spec + --index wiring + test) ──▶ Phase 2 (measured report + feature notes + CHANGELOG)
```

## Phase 1: IVFFlat harness spec + test

**Objective:** Add IVFFlat as a first-class benchmarked index with an integration test.

### T1.1 — `_ivfflat_spec` + `--index ivfflat`/`all` + integration test

#### Objective
Add the IVFFlat spec, wire it into the CLI, and benchmark it in an integration test asserting recall + index use.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds `_ivfflat_spec(table, opclass)` (`USING ivfflat … WITH (lists=…)` + a
   `ivfflat.probes` sweep) to `__main__.py`, extends `--index` choices with `ivfflat` and `all`, and appends an
   integration test that benchmarks IVFFlat against the container and asserts recall@10 ∈ [0,1] with the index
   used (monotone recall across probes).

2. **Why it is necessary now** — it closes features 03/04 with measured evidence; the harness already supports
   a new spec generically, so this is the minimal validation of an existing pgvector capability (parsimony).

#### Evidence
- Spec pattern: `benchmarks/theodb_bench/__main__.py:50-61` (`_hnsw_spec`), `:96-108` (`build_config`).
- Harness consumes specs generically: `benchmarks/theodb_bench/harness.py::run_benchmark`.
- IVFFlat surface: pgvector README (`USING ivfflat … WITH (lists=N)`, `SET ivfflat.probes`).
- Recall metric (shared): `benchmarks/theodb_bench/recall.py`.

#### Files to edit
```
benchmarks/theodb_bench/__main__.py — add _ivfflat_spec + extend --index choices (ivfflat, all)
benchmarks/tests/test_integration.py — RED ivfflat recall test appended
```

#### Deep file dependency analysis
- `__main__.py` (Baseline row, invariant: hnsw/diskann specs + choices unchanged): adds a function + extends the choice list + the `build_config` if-ladder; `run_benchmark` needs no change (generic over `index_specs`).
- `test_integration.py` (Baseline row, invariant: existing tests green): appends one ivfflat test using the existing `run_benchmark`/`db` fixture pattern.

#### Deep Dives
- **`_ivfflat_spec`:** `lists` computed from N at config time (≈ max(1, n//1000)); ddl `CREATE INDEX bench_ivf ON {table} USING ivfflat (embedding {opclass}) WITH (lists = {lists})`; sweep over `ivfflat.probes` ∈ {1, 10, lists} each with `SET enable_seqscan=off`.
- **build_config:** `--index ivfflat` → [ivfflat]; `all` → [hnsw, ivfflat] (+ diskann only if vectorscale present is out of scope here — `all` = hnsw+ivfflat to stay dependency-light; `both` stays hnsw+diskann for back-compat).
- **Edge cases:** lists must be ≥ 1; probes ≤ lists (clamp). IVFFlat needs data before build (harness loads then builds — existing order).

#### Pseudo-code / Signatures
```pseudocode
def _ivfflat_spec(table, opclass, n):
    lists = max(1, n // 1000)
    probes = sorted({1, 10, lists})
    return {"name":"ivfflat","index_name":"bench_ivf",
            "ddl": f"CREATE INDEX bench_ivf ON {table} USING ivfflat (embedding {opclass}) WITH (lists={lists})",
            "sweep": [{"label": f"probes={p}", "session": ["SET enable_seqscan=off", f"SET ivfflat.probes={min(p,lists)}"]} for p in probes]}
# --index ivfflat -> specs=[_ivfflat_spec]; all -> [_hnsw_spec, _ivfflat_spec]
```

#### Tasks
1. Add `_ivfflat_spec` to `__main__.py`.
2. Extend `--index` choices (`ivfflat`, `all`) + wire into `build_config`.
3. Append the ivfflat integration test.

#### TDD
```
RED:     test_ivfflat_recall_measured() [integration] — build_config(--index ivfflat, n>=2000), run_benchmark; assert an ivfflat result exists, recall@10 in [0,1], qps>0, and recall is non-decreasing as probes increases. MUST fail before _ivfflat_spec exists.
GREEN:   Implement _ivfflat_spec + wiring so it passes against the container.
REFACTOR: factor the shared "force index on" session prefix if it reduces dup; else "None expected".
VERIFY:  cd benchmarks && pytest -m integration tests/test_integration.py -k ivfflat -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — benchmark builds/queries an index sequentially; no shared mutable state, no locks/async.

#### Acceptance Criteria
- [ ] `--index ivfflat` runs an IVFFlat benchmark — `cd benchmarks && python3 -m theodb_bench --index ivfflat --n 2000 --dim 16 --n-queries 50 --k 10 --runs 1` (against a container) exits `0` and prints an `ivfflat` row.
- [ ] `test_ivfflat_recall_measured` passes — `cd benchmarks && pytest -m integration tests/test_integration.py -k ivfflat -q` exits `0`.
- [ ] Existing hnsw/diskann tests still green — `cd benchmarks && pytest -m integration -k 'hnsw or diskann' -q` exits `0` (no regression).
- [ ] Pass: lint — `cd benchmarks && ruff check theodb_bench tests` exits `0`; dead-code `vulture theodb_bench --min-confidence 80` clean.
- [ ] Pass: size — changed files `wc -l` < 500.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] IVFFlat benchmarked green; no regression — `cd benchmarks && pytest -m integration -k 'ivfflat or hnsw or diskann' -q` exits `0`.
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests` exits `0`.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'm9-ivfflat\|IVFFlat' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Phase 2: Measured report + feature notes

**Objective:** Record the measured IVFFlat-vs-HNSW numbers + mark features 03/04 validated.

### T2.1 — `docs/benchmarks/m9-ivfflat.md` + feature 03/04 notes + CHANGELOG

#### Objective
Run the IVFFlat-vs-HNSW benchmark, write the report, add the implemented/validated notes to specs 03/04.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — runs `--index all` (hnsw+ivfflat) on a real container, writes
   `docs/benchmarks/m9-ivfflat.md` with the measured recall×QPS for both, and adds an "implemented/validated"
   note to `docs/features/03-indice-ivfflat.md` + `04-indice-ivf.md`.

2. **Why it is necessary now** — the report is the measured evidence closing 03/04 (rule 5: measured, not
   asserted); the spec notes make the closure discoverable.

#### Evidence
- Report format: `docs/benchmarks/` prior reports.
- Measured output: T1.1's `run_benchmark` results.
- public-copy: `rules/public-copy.md` (measured numbers only).

#### Files to edit
```
docs/benchmarks/m9-ivfflat.md — (NEW) measured IVFFlat vs HNSW recall×QPS + reproduction
docs/features/03-indice-ivfflat.md — implemented/validated note
docs/features/04-indice-ivf.md — implemented/validated note (IVF ≡ pgvector IVFFlat)
CHANGELOG.md — [Unreleased] F1 entry
```

#### Deep file dependency analysis
- `docs/benchmarks/m9-ivfflat.md` (NEW): records T1.1 output.
- `03/04` specs (Baseline rows): additive note; the spec body (API-target) stays.

#### Deep Dives
- **Report honesty:** IVFFlat typically trades recall for build-speed/size vs HNSW; report the measured curve + state the trade-off plainly (no "IVFFlat beats HNSW" unless measured).

#### Tasks
1. Run `--index all`; capture numbers.
2. Write `docs/benchmarks/m9-ivfflat.md`.
3. Add the validated notes to specs 03/04; add the CHANGELOG entry.

#### TDD
```
RED:     report file absent before the run.
GREEN:   `test -f docs/benchmarks/m9-ivfflat.md` and it contains measured ivfflat recall numbers.
REFACTOR: none expected.
VERIFY:  test -f docs/benchmarks/m9-ivfflat.md && grep -ciE 'ivfflat|recall' docs/benchmarks/m9-ivfflat.md
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — markdown report + doc notes; no concurrent state.

#### Acceptance Criteria
- [ ] `docs/benchmarks/m9-ivfflat.md` exists with measured IVFFlat (and HNSW) recall×QPS + reproduction command — `grep -ciE 'ivfflat|recall@10|probes' docs/benchmarks/m9-ivfflat.md` returns `> 0`.
- [ ] `docs/features/03-indice-ivfflat.md` + `04-indice-ivf.md` carry a validated note — `grep -cil 'm9-ivfflat' docs/features/03-indice-ivfflat.md docs/features/04-indice-ivf.md` returns `2`.
- [ ] No unbenchmarked perf claim — `grep -ciE 'faster than|outperforms' docs/benchmarks/m9-ivfflat.md` returns `0`.
- [ ] Pass: size — changed files `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Report committed with measured numbers — `grep -ciE 'ivfflat|recall@10' docs/benchmarks/m9-ivfflat.md` returns `> 0`.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'm9-ivfflat\|IVFFlat' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | feature 03 IVFFlat validated + benchmarked | T1.1, T2.1 | `_ivfflat_spec` + recall test + measured report |
| 2 | feature 04 IVF validated (≡ pgvector IVFFlat) | T1.1, T2.1 | same validation; documented 04≡03 |
| 3 | recall measured (not asserted) | T1.1, T2.1 | recall test + report (rule 5) |
| 4 | no new dependency (Rule 9) | T1.1 | reuse shipped pgvector IVFFlat (per ADR D1) |
| 5 | no regression to hnsw/diskann | T1.1 | existing tests green |
| 6 | honest IVFFlat-vs-HNSW trade-off | T2.1 | report states measured curve + trade-off |

**Coverage: 6/6 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed — every phase DoD above exits `0`.
- [ ] Integration suite green — `cd benchmarks && pytest -m integration tests/test_integration.py -k 'ivfflat or hnsw or diskann' -q` exits `0` (no regression).
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests` exits `0`; `vulture theodb_bench --min-confidence 80` reports `0` dead symbols.
- [ ] File-size budget respected — changed files `wc -l` < `500` (per `rules/architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` — `grep -c 'm9-ivfflat\|IVFFlat' CHANGELOG.md` returns `> 0` (Unbreakable Rule 6).
- [ ] Backward compatibility preserved — `cd benchmarks && pytest -m integration -k 'hnsw or diskann' -q` exits `0` (hnsw/diskann/`both` paths unchanged).
- [ ] IVFFlat benchmarked with measured recall×QPS — `grep -ciE 'ivfflat|recall@10' docs/benchmarks/m9-ivfflat.md` returns `> 0`; specs 03/04 noted validated.
- [ ] Runtime-metric proof — the ivfflat `recall@10` value is printed by the integration run (not just compiling).
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| PostgreSQL (`psycopg`, container) | container not ready | run before ready | `VectorDB.connect/ping` raises a clear error |
| pgvector IVFFlat build | index built on empty table | (guarded) harness loads vectors before building | non-trivial recall; the harness order (load→build) prevents empty-cluster build |
| planner | IVFFlat not used (seqscan on small N) | force `enable_seqscan=off` in the sweep | the index is used (recall reflects IVFFlat, not a seqscan) |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate IVFFlat end-to-end against a real container.

### Execution
```
docker run -d --name f1-it -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db:dev   # wait for healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_integration.py -k 'ivfflat or hnsw or diskann' -q
python3 -m theodb_bench --index all --n 5000 --dim 16 --n-queries 100 --k 10 --runs 2   # measured report
ruff check theodb_bench tests && vulture theodb_bench --min-confidence 80
```

### Acceptance Criteria
- [ ] IVFFlat benchmark green; recall@10 measured + index used — `cd benchmarks && pytest -m integration tests/test_integration.py -k ivfflat -q` exits `0`.
- [ ] No regression to hnsw/diskann — `cd benchmarks && pytest -m integration -k 'hnsw or diskann' -q` exits `0`.
- [ ] Report committed with measured numbers; specs 03/04 noted validated — `grep -ciE 'ivfflat|recall@10' docs/benchmarks/m9-ivfflat.md` returns `> 0`.
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests` exits `0`.
