---
slug: m30-columnar-bm25-scope
milestone_id: M30
created_at: 2026-07-03
goal: Decide (ADR) to KEEP columnar + BM25 as permissive analytics pillars, validated by a columnar-at-scale benchmark proving the win the m6 marked UNBENCHMARKED.
---

# Plan: M30 — v1-legacy scope decision (columnar + BM25): KEEP, validated by a scale benchmark

> **Version 1.0** — M30 is the last open `[ ]` in the active ROADMAP: an ADR deciding the fate of the two
> v1-legacy pillars (columnar M6 `pg_mooncake`, BM25 M7 `pg_textsearch`), both throwaway/un-shipped. CTO steer:
> **KEEP both** — columnar is a general analytics/HTAP capability (AlloyDB parity; observability is one
> workload among many), BM25 is a measured lexical win (nDCG 0.95 vs 0.51). The honesty gate (goal:
> "DADOS E VALIDAÇÕES EM BENCHMARK"): the keep-columnar decision is defensible ONLY with a benchmark proving
> columnar wins at analytical scale — the exact gap `docs/benchmarks/m6-columnar-vs-row.md` left UNBENCHMARKED.

## Goal

> Decide via ADR to **KEEP** columnar (`pg_mooncake`) and BM25 (`pg_textsearch`) as permissive analytics
> pillars (Rule-9 exceptions, gated for adoption), validated by a reproducible columnar-vs-row **scale**
> benchmark, measured by `docs/benchmarks/m30-columnar-scale.json` showing the DuckDB columnstore beats the
> row-store `Seq Scan` at large analytical scale AND `docs/adr/0013-v1-legacy-columnar-bm25-scope.md` +
> the ROADMAP note present, with `python3 .claude/scripts/check_xrefs.py` clean.

## Context

`ROADMAP.md § M30` requires an ADR (keep/deprecate/rewrite) for the two v1-legacy pillars, with evidence.
`§ Fora de escopo do v2` says columnar reopening requires an ADR — this is it. The blueprint
`m30-columnar-bm25-scope-blueprint.md` established: both are throwaway/un-shipped; the shipped hybrid FTS leg
is native `ts_rank_cd` (own composition, untouched); columnar is permissive `pg_mooncake` (MIT; Citus/Hydra
columnar are AGPL — barred by D1); BM25 (`pg_textsearch`) is measured better than native ts_rank (m7). The CTO
decision is KEEP both. No product code change — this is a decision (ADR) + a validating benchmark + a ROADMAP
note. Feasibility of shipping columnar is a documented gated-adoption path (fix the PG17 from-source build OR
bump PG17→PG18 where pg_mooncake ships prebuilt) — NOT executed in M30.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `docs/adr/0013-v1-legacy-columnar-bm25-scope.md` (NEW) | 0 | — | (the MADR 3.0 decision) | — |
| `benchmarks/run_m30_columnar_scale.py` (NEW) | 0 | — | (scale-sweep driver over the mooncake substrate) | — |
| `benchmarks/tests/test_run_m30_columnar_scale.py` (NEW) | 0 | — | (structure test of the scale harness) | — |
| `docs/benchmarks/m30-columnar-scale.md` / `.json` (NEW) | 0 | — | (the scale-win artifact) | — |
| `benchmarks/theodb_bench/columnar.py` | 96 | `5887132` (2026-07-02) | `run_columnar_vs_row(db,n,table)` — seed + mirror + agg + timings | READ-ONLY (import/reuse; no change) |
| `ROADMAP.md` | — | — | active roadmap; M30 `[ ]` | flip M30 `[ ]`→`[x]` at release; add the KEEP note under `§ Fora de escopo do v2` / M30 |
| `CHANGELOG.md` | — | — | public contract | `[Unreleased]` entry (Rule 6) |

### Current callers / dependents

- **`theodb_bench.columnar.run_columnar_vs_row`** (`columnar.py:44`) — the M6 harness; M30 REUSES it by import
  (Rule 9) in the scale driver. Callers today: `benchmarks/tests/test_columnar.py`. M30 does NOT modify it.
- **`VectorDB.{ensure_mooncake_extension,create_columnstore_mirror,explain_plan,timed_query}`** (`db.py:244-263`)
  — the columnar substrate API; reused unchanged.
- **ADR 0002** (columnar out-of-scope; reopening = this ADR), **ADR 0003** (BM25 permissive). No production code
  touched; no external API.

### Domain glossary

- **columnstore mirror** — `CALL mooncake.create_table(mirror, base)` — a DuckDB+Iceberg columnar copy of a
  row table that auto-syncs; analytical queries on it plan as `DuckDBScan` (vectorized) vs `Seq Scan` on the row.
- **crossover scale** — the row count above which the columnstore aggregation beats the row-store `Seq Scan`
  (below it, the row-store wins — m6 measured 100k where row wins; the crossover is the UNBENCHMARKED point).
- **HTAP** — hybrid transactional/analytical processing: analytics over live transactional data without ETL.
- **permissive exception (Rule 9)** — a third-party dep kept despite the own-code mandate because no permissive
  own-code piece resolves it and it delivers measured value (documented in the ROADMAP).

### Architecture boundaries affected

None in the product. M30 is docs (ADR + ROADMAP note) + a benchmark under `benchmarks/` over a THROWAWAY
substrate (`mooncakelabs/pg_mooncake`, not the shipped image). No `theodb_rs`/`sql`/shipped-`Dockerfile` change.

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/m30-columnar-bm25-scope-blueprint.md`.
- **Internal benchmarks (the decision evidence):** `docs/benchmarks/m6-columnar-vs-row.md` (columnar no-win at
  100k; large-scale UNBENCHMARKED — the gap M30 fills), `docs/benchmarks/m7-bm25-vs-tsrank.md` (BM25 nDCG 0.95
  vs ts_rank 0.51 — the measured win).
- **ADRs:** `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (columnar out-of-scope; reopening = ADR),
  `docs/adr/0003-permissive-bm25-pg-textsearch.md` (BM25 permissive vetting).
- **Reuse:** `benchmarks/theodb_bench/columnar.py` (the M6 harness), `benchmarks/theodb_bench/db.py` (substrate API).

## Dependencies

**(none — no new package.)** Reuses `mooncakelabs/pg_mooncake:latest` (MIT — the canonical substrate, already
the M6 measurement image) + `theodb_bench` + `psycopg2` (present). No CVE surface change.

## Objective

- [ ] SG1 — ADR `0013-v1-legacy-columnar-bm25-scope.md` (MADR 3.0): decision = KEEP both, with per-pillar
  trade-offs + evidence + at least one rejected alternative (deprecate) + consequences + the feasibility/adoption path.
- [ ] SG2 — Scale benchmark: columnar-vs-row over a scale sweep proves the columnar crossover (columnstore beats
  row `Seq Scan` at large scale), `match=True` (correctness), plan = `DuckDBScan` — `docs/benchmarks/m30-columnar-scale.{md,json}`.
- [ ] SG3 — ROADMAP note: columnar + BM25 recorded as **permissive exceptions** to the own-code mandate (Rule 9),
  gated for adoption; M30 marked `[x]`-ready (flip at release).
- [ ] SG4 — CHANGELOG `[Unreleased]` + the "Relação com o v1" note updated with the decision.
- [ ] SG5 — No product code change; no unsubstantiated performance claim (the scale win is measured + linked).

## ADRs

### D1 — KEEP columnar (pg_mooncake) as a permissive analytics/HTAP pillar
- **Decision:** columnar stays (not deprecated); recorded in ADR 0013 as a Rule-9 permissive exception, gated
  for a future adoption milestone.
- **Rationale:** analytics/HTAP is a North-Star (AlloyDB) capability; general analytical workloads (dashboards,
  aggregations over live data, observability rollups) are columnar's home; `pg_mooncake` is MIT (Citus/Hydra
  columnar are AGPL — barred by D1/ADR 0002). The m6 "no win at 100k" is the wrong scale; M30 measures the win.
- **Alternatives considered:** deprecate+remove (rejected — throws away a real AlloyDB-parity pillar the CTO
  wants; the no-win was only at 100k); rewrite-own columnar (rejected — YAGNI + enormous; DuckDB is battle-tested,
  Rule 9).
- **Consequences:** the scale benchmark is load-bearing (the keep is honest only if columnar wins at scale);
  shipping columnar is a separate gated milestone (PG17-build-fix or PG18).

### D2 — KEEP BM25 (pg_textsearch) as a permissive lexical pillar
- **Decision:** BM25 stays (not deprecated); Rule-9 permissive exception, gated for adoption.
- **Rationale:** m7 measured BM25 nDCG@10 0.9546 vs native ts_rank 0.5143 — a large lexical-quality win;
  permissive (ADR 0003). Deprecating it would discard a measured win (Rule 3 dishonesty).
- **Alternatives considered:** deprecate (rejected — measured win); ship it now (rejected — a bigger adoption
  milestone, out of M30's decision scope).
- **Consequences:** the shipped hybrid FTS leg stays native `ts_rank_cd`; BM25 adoption is a future milestone.

### D3 — Validate the KEEP with a scale benchmark on the canonical substrate (no product change)
- **Decision:** measure columnar-vs-row at increasing scale on `mooncakelabs/pg_mooncake:latest` (the M6
  substrate), reusing `theodb_bench.columnar` — do NOT touch the shipped image.
- **Rationale:** the goal demands benchmark data; the honest gap is m6's UNBENCHMARKED large scale. The substrate
  is the canonical MIT distribution (measurement-first, ADR 0002) — proving capability, not shipping it.
- **Alternatives considered:** ship columnar on the theo-db image + benchmark there (rejected — that's the gated
  adoption milestone, needs the PG17 build fix; out of M30 scope).
- **Consequences:** the artifact proves the columnar win reproducibly; shipping remains a documented follow-up.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Keeping two third-party pillars dilutes the own-code mandate | Medium | Documented as explicit Rule-9 permissive exceptions in the ROADMAP + ADR, with the measured justification | CTO |
| Columnar not shipped → "kept" but unusable in the product today | Medium | The ADR records the gated-adoption path (PG17 build fix OR PG18) as a named follow-up milestone; M30 is a decision + evidence, not shipping | maintainers |
| Scale benchmark might NOT show a columnar win at the tested scale | Medium | Sweep increasing scale until the crossover is found (or honestly report the crossover is beyond the tested scale — measurement-first, no cherry-pick) | maintainers |
| mooncake substrate is PG18, shipped image is PG17 → a version caveat in the evidence | Low | Documented explicitly (the substrate proves the CAPABILITY; shipping is the gated step) | maintainers |

## Unresolved Questions

- Q1 — At what scale does columnar cross over row-store for the group-by aggregate? (M30 measures: sweep
  100k → 1M → 5M → 10M until columnstore `DuckDBScan` beats row `Seq Scan`; report the crossover honestly.)
- Q2 — Ship on PG17 (fix the from-source build) vs bump to PG18? (Out of M30 scope — the ADR records both as the
  adoption path; the shipping decision is a future milestone.)

## Dependency Graph

```
Phase 1 (scale benchmark — the data) ──▶ Phase 2 (ADR 0013 + ROADMAP note, cite the data)
                                                │
                                                ▼
                                     Final Phase (integration validation)
```

Phase 1 produces the evidence; Phase 2 writes the decision grounded in it. Sequential.

---

## Phase 1: Columnar-at-scale benchmark (the data)

### T1.1 — `run_m30_columnar_scale.py` scale sweep + artifact

#### Objective
Prove the columnar crossover: sweep row counts on the mooncake substrate, report where the DuckDB columnstore
beats the row-store `Seq Scan`, with correctness (`match=True`) and the plan shapes.

#### Why this step (action + reasoning)
1. **What:** a driver that, for each n in a sweep, calls `theodb_bench.columnar.run_columnar_vs_row(db, n)` and
   records `row_ms`, `columnar_ms`, speedup, plan (`DuckDBScan` vs `Seq Scan`), and `match`; writes the artifact.
2. **Why now:** the keep-columnar decision (D1) is honest ONLY with this evidence — the m6 UNBENCHMARKED gap.

#### Evidence
- `benchmarks/theodb_bench/columnar.py:44` (`run_columnar_vs_row`), `docs/benchmarks/m6-columnar-vs-row.md`
  (100k: row 10.9ms vs columnar 44.3ms — the crossover is above 100k).

#### Files to edit
```
benchmarks/run_m30_columnar_scale.py (NEW) — sweep + artifact writer
benchmarks/tests/test_run_m30_columnar_scale.py (NEW) — RED: structure test (tiny scale, gated on mooncake)
```

#### Deep file dependency analysis
- `run_m30_columnar_scale.py` imports `theodb_bench.columnar.run_columnar_vs_row` + `theodb_bench.db.VectorDB`.
  Downstream: `main()` + the structure test. No production code.

#### Deep Dives
- **Sweep:** n ∈ {100_000, 1_000_000, 5_000_000} (configurable `--scales`); per n, run the harness, compute
  `speedup = row_ms / columnar_ms`, capture `plan` (assert columnar plan contains `DuckDB`), `match`.
- **Verdict:** report the crossover (first n where `speedup > 1`); if none in range, say so honestly (no cherry-pick).
- **Substrate caveat:** record image = `mooncakelabs/pg_mooncake` (PG18); note shipping on PG17 is the gated step.
- **Edge cases:** mooncake unavailable → the harness/driver fails loud (not a silent skip in the real run);
  mirror sync barrier (already in `_wait_mirror_synced`); cross-engine numeric tolerance (`match` eps=1e-3).

#### Pseudo-code / Signatures
```python
def run(port, scales=(100_000, 1_000_000, 5_000_000)):
    db = VectorDB(dsn(port)).connect()
    points = []
    for n in scales:
        r = run_columnar_vs_row(db, n)           # reuse M6 harness
        points.append({"n": n, "row_ms": r["row"]["ms"], "columnar_ms": r["columnar"]["ms"],
                       "speedup": round(r["row"]["ms"]/r["columnar"]["ms"], 2),
                       "columnar_plan_duckdb": "DuckDB" in r["columnar"]["plan"], "match": r["match"]})
    crossover = next((p["n"] for p in points if p["speedup"] > 1.0), None)
    return {"points": points, "crossover_n": crossover, "substrate": "mooncakelabs/pg_mooncake (PG18)"}
```

#### Tasks
1. Create `run_m30_columnar_scale.py` (`run()` + `main()` with `--port --scales --write-doc`).
2. RED: structure test (tiny scale, gated on mooncake availability).
3. `--write-doc` → `docs/benchmarks/m30-columnar-scale.{md,json}`.

#### TDD
```
RED: test_run_m30_scale_emits_points_and_verdict() [integration, gated on mooncake] — tiny scales; asserts each
     point has n/row_ms/columnar_ms/speedup/match + a crossover_n key (int or None) + substrate recorded.
GREEN: implement run()/main() reusing run_columnar_vs_row.
REFACTOR: none expected.
VERIFY: PORT=<mooncake> python3 -m pytest benchmarks/tests/test_run_m30_columnar_scale.py -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_run_m30_columnar_scale.py -q` structure test green against a mooncake container.
- [ ] The real run writes `docs/benchmarks/m30-columnar-scale.json` with ≥3 scale points, each with `match=True` and the columnar `DuckDB` plan, plus `crossover_n`.
- [ ] The `.md` reports the crossover honestly (or states the crossover is beyond the tested range — no cherry-pick) and records the PG18-substrate caveat.
- [ ] Pass: size — `run_m30_columnar_scale.py` ≤ 200 lines. Pass: lint — `pyflakes` clean.

#### DoD
- [ ] Benchmark artifact present + honest verdict; CHANGELOG `[Unreleased]` updated; structure test green.

---

## Phase 2: ADR 0013 + ROADMAP note

### T2.1 — Write `docs/adr/0013-v1-legacy-columnar-bm25-scope.md` (MADR 3.0) + ROADMAP note

#### Objective
Record the KEEP decision (per pillar) grounded in m6 + m7 + the new scale benchmark, with the feasibility path.

#### Why this step (action + reasoning)
1. **What:** the MADR 3.0 ADR (context → decision KEEP both → per-pillar rationale + evidence + rejected
   alternative → consequences → feasibility/adoption path) + a ROADMAP note under `§ Fora de escopo do v2` /
   M30 declaring columnar + BM25 as permissive Rule-9 exceptions gated for adoption.
2. **Why now:** the ADR is the M30 deliverable; it must cite the Phase-1 scale evidence (else the keep is a claim).

#### Evidence
- Phase-1 artifact `docs/benchmarks/m30-columnar-scale.md` (columnar win at scale); m6 (100k no-win); m7 (BM25 win).

#### Files to edit
```
docs/adr/0013-v1-legacy-columnar-bm25-scope.md (NEW) — the MADR 3.0 decision
ROADMAP.md — M30 note: columnar + BM25 KEPT as permissive Rule-9 exceptions (gated adoption); cite ADR 0013 + the scale benchmark
CHANGELOG.md — [Unreleased] entry (the decision)
```

#### Deep file dependency analysis
- The ADR references the benchmarks + ADR 0002/0003. The ROADMAP note references ADR 0013. `check_xrefs.py`
  validates the cross-references resolve. No code.

#### Deep Dives
- MADR 3.0 sections: Title/Status/Date/Deciders, Context & Problem, Decision Drivers, Considered Options
  (keep / deprecate-and-remove / rewrite-own), Decision Outcome (KEEP both, per-pillar), Consequences
  (+ the gated-adoption feasibility path: PG17 build fix OR PG18), Pros/Cons per option, evidence links.
- ROADMAP note: one paragraph under `§ Fora de escopo do v2` (columnar) making explicit it is a **permissive
  exception justified by measured value** (Rule 9), gated for adoption; not shipped in M30.

#### Tasks
1. Write ADR 0013 (MADR 3.0) citing the scale benchmark + m6 + m7.
2. Add the ROADMAP permissive-exception note; update the M30 DoD checkboxes.
3. CHANGELOG `[Unreleased]` + the "Relação com o v1" note.

#### TDD
```
RED: test_adr_0013_present_and_grounded() — asserts docs/adr/0013-*.md exists, contains "KEEP"/"manter" for both
     pillars, links docs/benchmarks/m30-columnar-scale.md + m7-bm25-vs-tsrank.md, and lists a rejected alternative.
GREEN: write the ADR + ROADMAP note.
REFACTOR: none.
VERIFY: python3 -m pytest benchmarks/tests/test_run_m30_columnar_scale.py -k adr -q ; python3 .claude/scripts/check_xrefs.py
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docs/adr/0013-v1-legacy-columnar-bm25-scope.md` exists (MADR 3.0), decision = KEEP both, cites the scale benchmark + m6 + m7, lists the rejected deprecate alternative + the gated-adoption path.
- [ ] `ROADMAP.md` has the permissive-exception note (Rule 9) for columnar + BM25, citing ADR 0013.
- [ ] `python3 .claude/scripts/check_xrefs.py` clean (no dangling reference).
- [ ] CHANGELOG + "Relação com o v1" updated.

#### DoD
- [ ] ADR + ROADMAP note present + xrefs clean; CHANGELOG updated.

---

## Coverage Matrix

| # | Gap / Requirement (ROADMAP M30 DoD) | Task(s) | Resolution |
|---|---|---|---|
| 1 | ADR (keep/deprecate/rewrite) MADR 3.0 with trade-offs + evidence | T2.1 | ADR 0013, decision KEEP both, per-pillar + alternatives + evidence |
| 2 | If keep: explicit permissive-exception note (Rule 9) in ROADMAP | T2.1 | ROADMAP note under § Fora de escopo do v2 |
| 3 | Benchmark data validating the decision (goal) | T1.1 | columnar-at-scale sweep proving the win (m6 UNBENCHMARKED gap) |
| 4 | CHANGELOG + "Relação com o v1" updated | T2.1 | [Unreleased] + the v1-relation note |

**Coverage: 4/4 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] Scale benchmark artifact present (`docs/benchmarks/m30-columnar-scale.{md,json}`) with an honest crossover verdict
- [ ] ADR 0013 present (MADR 3.0), KEEP both, grounded in the benchmarks; `check_xrefs.py` clean
- [ ] ROADMAP permissive-exception note present; M30 DoD checkboxes reflect the decision
- [ ] Tests green — `python3 -m pytest benchmarks/tests/test_run_m30_columnar_scale.py -q`
- [ ] Zero lint — `pyflakes benchmarks/run_m30_columnar_scale.py`
- [ ] File-size budget respected (≤ 500 lines each)
- [ ] CHANGELOG.md `[Unreleased]` updated
- [ ] No product code change (theodb_rs/sql/shipped-Dockerfile untouched); no unsubstantiated perf claim
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merged

## Failure scenarios (when I/O external)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `mooncakelabs/pg_mooncake` container (psycopg2) | mooncake not available / not ready | point at a non-mooncake or stopped container | the driver fails LOUD (`ensure_mooncake_extension` raises); the structure test SKIPS with a clear reason; the REAL run never silently produces an empty artifact |
| columnstore mirror sync (async) | mirror never converges to base count | `_wait_mirror_synced` barrier (existing) | raises a clear timeout error, never a confusing aggregate mismatch |
| cross-engine numeric (PG vs DuckDB avg) | last-decimal summation differs | `_results_match` eps=1e-3 (existing) | `match` tolerates eps; a real divergence fails the correctness assert |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** prove the decision is evidence-backed and nothing in the product broke (nothing was removed).

### Execution
```
# 1. Scale benchmark (the data) against a mooncake container:
PORT=<mooncake> python3 -m pytest benchmarks/tests/test_run_m30_columnar_scale.py -q
python3 benchmarks/run_m30_columnar_scale.py --port <mooncake> --write-doc
test -f docs/benchmarks/m30-columnar-scale.json
# 2. ADR + xrefs:
test -f docs/adr/0013-v1-legacy-columnar-bm25-scope.md && python3 .claude/scripts/check_xrefs.py
# 3. No product regression (nothing removed): the shipped image + smoke unaffected
PGHOST=localhost PGPORT=<theo-db> bash scripts/smoke.sh   # SMOKE PASSED (product untouched)
pyflakes benchmarks/run_m30_columnar_scale.py
```

### Acceptance Criteria
- [ ] Scale benchmark artifact present; crossover reported honestly; `match=True` + `DuckDB` plan at each point
- [ ] ADR 0013 present + grounded; `check_xrefs.py` clean
- [ ] ROADMAP note present; CHANGELOG updated
- [ ] `smoke.sh` SMOKE PASSED (the shipped product is untouched — nothing was removed)
- [ ] Zero lint; no unsubstantiated performance claim (the scale win is measured + linked)

### If Validation Fails
1. Separate plan-caused vs pre-existing.
2. Fix plan-caused (e.g., crossover not found → extend the sweep + report honestly).
3. Re-run the chain.
