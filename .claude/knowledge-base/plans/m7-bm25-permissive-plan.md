---
slug: m7-bm25-permissive
created_at: 2026-06-28
goal: Identify + functionally prove + measure a permissive BM25 lexical option (pg_textsearch) vs ts_rank_cd
---

# Plan: Permissive BM25 — identify, prove, measure (M7-S2)

> **Version 1.0** — Close M7-S2's ROADMAP DoD ("alternativa permissiva a BM25 full-text identificada") with
> evidence, not just a claim: (1) record the identification of **pg_textsearch** (PostgreSQL License, true
> Okapi BM25) as an ADR; (2) make the permissive-vs-AGPL verdict **reproducible** in `license-sweep.sh`
> (pg_textsearch permissive ✅, VectorChord-bm25 AGPL barred ❌); (3) **functionally prove + measure** BM25 in
> a throwaway image (build pg_textsearch v1.3.1 on `theo-db:dev`) by extending the M7-S1 recall harness with
> a BM25 retriever and measuring recall vs the already-shipped `ts_rank_cd` — the measurement-first gate
> (ADR 0002 / D3) that will decide any future distribution-integration. The distribution image is NOT
> changed: per measurement-first, adopting pg_textsearch into the shipped image is a later decision gated on
> this measurement. BM25F (multi-field) is explicitly out of scope (YAGNI — see ADR D4).

## Goal

> Enable the TheoDB team to decide BM25 adoption on evidence by identifying the permissive piece (pg_textsearch),
> proving it permissive reproducibly, and measuring its recall vs `ts_rank_cd`, measured by the BM25 recall
> benchmark (`run_three_retrievers` extended with a `bm25` retriever) reporting nDCG@10 + Recall@100 for the
> BM25 leg against a real pg_textsearch build AND `license-sweep.sh` asserting pg_textsearch=permissive / VectorChord-bm25=AGPL-barred.

## Context

ROADMAP `### M7` DoD-3 + top-risk #1: "Sem peça permissiva madura para BM25 full-text (paradedb `pg_search` é
AGPL)" → "**alternativa permissiva** identificada". The DISCOVER cycle (blueprint
`.claude/knowledge-base/discoveries/blueprints/m7-bm25-permissive-blueprint.md`, SHIPPABLE_WITH_CAVEATS 89)
identified **timescale/pg_textsearch** — PostgreSQL License (verbatim from canonical repo), GA v1.3.1, true
Okapi BM25 (k1=1.2/b=0.75), Block-Max WAND — and confirmed **VectorChord-bm25 = dual AGPLv3/Elastic (barred,
D1)**, native `ts_rank_cd` = NOT BM25 (cover-density). A live PoC (build + query) already verified BM25 runs on
`theo-db:dev` (k1=1.20/b=0.75/avg_length=3.80, correctly ranked). Per the blueprint's measurement-first
recommendation (ADR 0002 / PRD D3), distribution-integration is gated on a reproducible recall benchmark vs
`ts_rank_cd`; this slice produces that benchmark + the identification record + the reproducible license proof.
It does NOT ship pg_textsearch in the distribution image (that is a future, evidence-gated decision).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `docs/adr/0003-permissive-bm25-pg-textsearch.md` (NEW) | 0 | — | (to be created) the identification record (DoD) | — |
| `packaging/license-sweep.sh` | 44 | `24a9b02` (2026-06-28) | M1 reproducible AGPL sweep (apt + Rust crates) | existing apt + pgvectorscale checks stay; BM25 candidate checks are additive |
| `docs/packaging/license-audit.md` | 59 | `24a9b02` (2026-06-28) | M1 committed license evidence | append a BM25-candidates section; existing content unchanged |
| `packaging/Dockerfile.bm25` (NEW) | 0 | — | (to be created) throwaway image: `theo-db:dev` + pg_textsearch v1.3.1 (NOT the shipped image) | — |
| `benchmarks/theodb_bench/db.py` | 190 | `1c2e095` (2026-06-28) | `VectorDB` adapter (vector + FTS + hybrid helpers) | existing methods backward-compatible; `bm25_query` is additive |
| `benchmarks/theodb_bench/hybrid.py` | 58 | `1c2e095` (2026-06-28) | `rrf_fuse` + `run_three_retrievers` driver | extend driver to optionally score a `bm25` retriever; existing signature kept (default off) |
| `benchmarks/tests/test_bm25.py` (NEW) | 0 | — | (to be created) BM25 retriever integration test (gated on pg_textsearch present) | — |
| `docs/benchmarks/m7-bm25-vs-tsrank.md` (NEW) | 0 | — | (to be created) the measured BM25-vs-ts_rank_cd report (the gate evidence) | — |
| `.github/workflows/ci.yml` | 345 | `e2abef2` (2026-06-28) | CI | existing jobs stay; add `bm25-measure` job (builds throwaway image + measures) |
| `CHANGELOG.md` | (exists) | — | Public contract | `[Unreleased]` gets the M7-S2 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `run_three_retrievers` in `benchmarks/theodb_bench/hybrid.py`
  - **Callers (production):** none (dev tooling); **Callers (tests):** `benchmarks/tests/test_integration.py::test_three_retrievers_report_metrics`.
  - **External:** no. The S2 change adds an optional `bm25` retriever; the existing 3-retriever call path stays valid (additive param, default preserves behavior).
- **Symbol:** `VectorDB` in `benchmarks/theodb_bench/db.py`
  - **Callers:** `harness.py`, `__main__.py`, `tests/test_db.py`, `tests/test_integration.py`.
  - **External:** no. New `bm25_query` method is additive (no existing-method signature change).
- **Symbol:** `license-sweep.sh` (M1) — invoked manually + (future) CI; additive candidate checks do not change its exit-contract (still non-zero on any real AGPL in the *distribution*).

Enumerated via `grep -rln 'run_three_retrievers\|VectorDB\|license-sweep' --include='*.py' --include='*.sh' --include='*.yml' benchmarks/ packaging/ .github/`.

### Domain glossary

- **BM25 (Okapi)** — probabilistic lexical ranking: `score = Σ IDF(qi)·(f·(k1+1))/(f + k1·(1−b+b·|D|/avgdl))`; saturates term frequency (`k1`) + normalizes by document length (`b`). Defaults k1=1.2, b=0.75 (Robertson & Zaragoza 2009).
- **pg_textsearch** — Timescale's PostgreSQL-License C extension: `CREATE INDEX … USING bm25(content)` + `content <@> 'query'` (negative BM25 score; lower = better); requires `shared_preload_libraries=pg_textsearch`.
- **ts_rank_cd** — PostgreSQL's built-in cover-density ranking (NOT BM25); the lexical leg shipped in M7-S1.
- **BM25F** — fielded BM25 (per-field weights, pre-saturation combination). Out of scope (ADR D4).
- **measurement-first** — TheoDB rule (ADR 0002 / PRD D3): adopt/fork only on reproducible benchmark evidence; no performance claim without a `docs/benchmarks/` artifact.
- **throwaway image** — a build image used for measurement/CI, never shipped in the distribution (cf. M1 `Dockerfile.regress`).

### Architecture boundaries affected

Per `rules/architecture.md`: pg_textsearch would be an **infrastructure** extension inside the DB image — but this slice deliberately keeps it in a **throwaway** image (`packaging/Dockerfile.bm25`), NOT the shipped `Dockerfile`, so the distribution's dependency surface is unchanged until the measurement justifies adoption (measurement-first + YAGNI). The benchmark harness is **dev-only tooling** (client of the DB via `psycopg`); the new `bm25_query` rides the existing `VectorDB` adapter boundary (DIP). No product-layer code changes.

## Prior Art & Related Work

- **Internal blueprint (design source):** `.claude/knowledge-base/discoveries/blueprints/m7-bm25-permissive-blueprint.md` — identifies pg_textsearch, the license matrix, the BM25 algorithm, adopt-vs-own-vs-keep decision.
- **Internal (M7-S1):** `benchmarks/theodb_bench/hybrid.py::run_three_retrievers` + `benchmarks/theodb_bench/beir.py` — the BEIR-style recall harness this slice extends with a BM25 retriever; `sql/40-theodb-hybrid.sql` (the ts_rank_cd leg BM25 is measured against).
- **Internal (M1):** `packaging/license-sweep.sh` + `docs/packaging/license-audit.md` — the reproducible license-sweep pattern this slice extends to BM25 candidates; `packaging/Dockerfile.regress` — the throwaway-image pattern `Dockerfile.bm25` mirrors.
- **Reference (AGPL witness, study-only):** `.claude/knowledge-base/references/paradedb/LICENSE` (AGPL-3.0).
- **External:** `github.com/timescale/pg_textsearch` (PostgreSQL License, v1.3.1 — the identified piece); Robertson & Zaragoza 2009 "The Probabilistic Relevance Framework: BM25 and Beyond" (`doi.org/10.1561/1500000019`); Thakur et al. 2021 BEIR (`arxiv.org/abs/2104.08663`).

## Objective

- [ ] ADR `0003-permissive-bm25-pg-textsearch.md` records the identification (pg_textsearch, PostgreSQL License) + the measurement-first adoption gate + BM25F-deferred decision — the literal ROADMAP DoD.
- [ ] `license-sweep.sh` reproducibly asserts pg_textsearch=PostgreSQL License (permissive) AND VectorChord-bm25=AGPL/Elastic (barred) — fetched from canonical repos, pass/fail.
- [ ] A throwaway `packaging/Dockerfile.bm25` builds pg_textsearch v1.3.1 on `theo-db:dev` (NOT the shipped image).
- [ ] `VectorDB.bm25_query` + `run_three_retrievers` extended with a `bm25` retriever; integration test gated on pg_textsearch present (skips cleanly otherwise).
- [ ] The BM25 recall benchmark runs against the real pg_textsearch build and reports nDCG@10 + Recall@100 for `bm25` vs `fts`(ts_rank_cd) vs `vector` vs `hybrid` — measured numbers in `docs/benchmarks/m7-bm25-vs-tsrank.md`.
- [ ] CI `bm25-measure` job builds the throwaway image + runs the BM25 measurement (deterministic offline; no external API).

## ADRs

### D1 — Identify pg_textsearch; gate distribution-adoption on this measurement (measurement-first)

**Decision:** Identify **pg_textsearch** (PostgreSQL License, v1.3.1, true Okapi BM25) as THE permissive BM25
alternative (closes the DoD). Do NOT add it to the shipped `Dockerfile` in this slice; prove it functional +
measure its recall vs `ts_rank_cd` in a throwaway image. A future slice adopts it into the distribution ONLY
if this measurement shows a recall gain that justifies the build-dependency cost.

**Rationale:** ADR 0002 / PRD D3 (measurement-first): adopt on reproducible benchmark evidence, not on a
spec promise. Keeps the shipped image's dependency surface unchanged until justified (YAGNI). The blueprint's
explicit recommendation.

**Alternatives considered:** *Ship pg_textsearch in the distribution now* — rejected: premature adoption
before measuring the gain over the already-shipped `ts_rank_cd` (anti measurement-first). *Own BM25 in SQL
over `ts_stat`* — rejected: Rule-9 reinvention of an existing permissive extension (blueprint D1). *Adopt
VectorChord-bm25* — rejected: dual AGPLv3/Elastic, barred by D1.

**Consequences:** S2 delivers the identification + the gate measurement; the distribution stays unchanged.
The benchmark result feeds the future adoption decision.

### D2 — Reproducible license verdict in license-sweep (D1 fail-closed)

**Decision:** Extend `packaging/license-sweep.sh` with a BM25-candidates check that fetches each candidate's
LICENSE from its canonical repo and asserts pg_textsearch=PostgreSQL-License (permissive) and
VectorChord-bm25=AGPL (barred); a candidate whose license cannot be fetched is reported `UNVERIFIED` (non-fatal
note, never assumed permissive).

**Rationale:** D1 (no AGPL in the distribution) is a release gate; a license verdict must be reproducible from
source, not asserted from memory (Rule 3). Mirrors the M1 sweep pattern.

**Alternatives considered:** *Document the licenses in prose only* — rejected: not reproducible, drifts.
*Block CI on the candidate sweep* — rejected for now: pg_textsearch is not yet in the distribution, so its
license is informational (the sweep records it); the distribution AGPL gate (M1) is unchanged.

**Consequences:** the license identification is re-runnable evidence; the audit doc cites it.

### D3 — Throwaway image + harness BM25 retriever (functional proof + measurement)

**Decision:** `packaging/Dockerfile.bm25` (FROM `theo-db:dev`, build pg_textsearch v1.3.1 via PGXS) is the
measurement substrate. `VectorDB.bm25_query` issues `ORDER BY content <@> $query LIMIT n` over a BM25 index;
`run_three_retrievers` gains an optional `bm25` retriever. The eval measures BM25 vs the other legs on the
BEIR-style fixture.

**Rationale:** the functional proof ("no mock, 100% functional") + the measurement-first gate are the same
artifact (build + run + measure). PGXS build ≈ pgvector (de-risked live). pg_textsearch needs
`shared_preload_libraries=pg_textsearch` — set on the throwaway container.

**Alternatives considered:** *Mock BM25 scores* — rejected (not functional evidence; Rule 3 + the project's
"real, no mock" bar). *Measure in the shipped image* — rejected (would require shipping pg_textsearch,
violating D1's measurement-first staging).

**Consequences:** the BM25 retriever test is gated on pg_textsearch being loaded (skips cleanly elsewhere);
the throwaway image is the CI substrate for the measurement.

### D4 — BM25F (fielded BM25) is out of scope

**Decision:** BM25F (multi-field weighted BM25) is explicitly NOT in scope for M7-S2. Recorded as a future
discovery seed gated on (a) a concrete multi-field document use case AND (b) a measured gain.

**Rationale:** Parsimony ladder rung 1 (does this need to exist now? → no): our search schema is single-field
(`content`); pg_textsearch's index is single-column plain BM25 (BM25F isn't free); and a naive per-field
weighted sum is the exact anti-pattern BM25F was designed to correct (Robertson, Zaragoza & Taylor 2004) —
adopting it without a real multi-field need + measurement is premature. We have not even measured plain BM25
vs `ts_rank_cd` yet (this slice's gate).

**Alternatives considered:** *Implement BM25F now* — rejected (YAGNI + measurement-first + no single-field-to-
multi-field driver). *Approximate via weighted per-field BM25 sum* — rejected (known anti-pattern; theoretically
weaker than true BM25F).

**Consequences:** the report + ADR note BM25F as a deferred seed; no BM25F code ships.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| The synthetic BEIR-style fixture may not show a BM25 win over `ts_rank_cd` (lexical-embedder fixture has little headroom, cf. M7-S1) | Medium | Goal is **measured numbers reported**, not a hardcoded BM25-superiority; report states results honestly + flags that a real-corpus measurement is the decisive follow-up (rule 5 / Rule 3) | Bench |
| pg_textsearch needs `shared_preload_libraries` — a real operational constraint if ever adopted into the distribution | Medium | Documented in the ADR + report; the throwaway image sets it; adoption decision accounts for it | DB |
| Building pg_textsearch adds a build dependency (postgresql-server-dev) IF adopted into the shipped image | Low | Kept in a throwaway image this slice (D1/D3); shipped image unchanged until measurement justifies | DB |
| Candidate license could change upstream (pg_textsearch relicense) | Low | `license-sweep.sh` re-fetches from the canonical repo each run → drift is caught reproducibly | Security |
| pg_textsearch GA tag v1.3.1 pinned; upstream moves | Low | Pin by tag in `Dockerfile.bm25`; bump deliberately | DB |

## Unresolved Questions

- Q1 — Does BM25 beat `ts_rank_cd` on the synthetic fixture, or only on a real corpus? Resolved at plan time: the slice **measures** it on the synthetic fixture (deterministic, CI) and documents that a real-corpus eval is the decisive follow-up; no superiority is hardcoded.
- Q2 — Should the BM25 leg also feed a `hybrid_bm25` (RRF over vector+BM25)? Resolved: measure the standalone `bm25` leg in S2; a `hybrid_bm25` fusion comparison is an optional report extension, not required for the DoD.
- Q3 — Pin pg_textsearch by tag or digest? Resolved: by GA tag `v1.3.1` in `Dockerfile.bm25` (consistent with the M3 source-pin convention), bumped deliberately.

## Dependencies

M7-S2 adds **no new runtime dependency to the shipped distribution** (the shipped `Dockerfile` is unchanged —
D1/D3). pg_textsearch is built only in the throwaway `Dockerfile.bm25` (measurement substrate).

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `pg_textsearch` (throwaway image only) | v1.3.1 | BM25 leg under measurement | PostgreSQL License | identified permissive (D2); NOT in shipped image |
| `postgresql-server-dev-17` + `build-essential` (throwaway build) | bundled (Debian) | build pg_textsearch (PGXS) | PostgreSQL / GPL (build-time only, not shipped) | build-time only |
| `psycopg2-binary`, `numpy` (harness, dev-only) | as in `benchmarks/requirements.txt` | DB client + metrics | LGPL / BSD | already dev deps |

No CVE audit delta on the shipped distribution: zero new declared runtime dependencies in the shipped image.

## Dependency Graph

```
Phase 1 (ADR + license-sweep) ──▶ Phase 3 (report + CI + CHANGELOG)
                                        ▲
Phase 2 (Dockerfile.bm25 + bm25_query + harness + measure) ─┘
   (Phase 1 and Phase 2 are independent; Phase 3 depends on BOTH)
```

## Phase 1: Identification (ADR) + reproducible license proof

**Objective:** Record the permissive-BM25 identification (DoD) + make the license verdict reproducible.

### T1.1 — ADR 0003 + extend license-sweep with BM25 candidates

#### Objective
Write the identification ADR and extend `license-sweep.sh` + `docs/packaging/license-audit.md` with the BM25-candidate license verdicts.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `docs/adr/0003-permissive-bm25-pg-textsearch.md` (identifies pg_textsearch
   + measurement-first gate + BM25F deferral) and adds a BM25-candidates block to `packaging/license-sweep.sh`
   that fetches pg_textsearch + VectorChord-bm25 LICENSEs from their canonical repos and asserts
   permissive/barred; appends the evidence to `docs/packaging/license-audit.md`.

2. **Why it is necessary now** — the ADR is the literal ROADMAP DoD ("alternativa permissiva identificada");
   the reproducible sweep makes that identification evidence (Rule 3 — not a memory claim) and the D1 gate
   re-runnable (mirrors M1).

#### Evidence
- DoD source: ROADMAP `### M7` DoD-3 (line 383) + risk #1 (line 408).
- Identification source: `.claude/knowledge-base/discoveries/blueprints/m7-bm25-permissive-blueprint.md` (Coverage Corner 2).
- Sweep pattern: `packaging/license-sweep.sh:1-44`; AGPL witness `.claude/knowledge-base/references/paradedb/LICENSE:1`.
- pg_textsearch license: `github.com/timescale/pg_textsearch` LICENSE (PostgreSQL License — verified live).

#### Files to edit
```
docs/adr/0003-permissive-bm25-pg-textsearch.md — (NEW) identification + measurement-first gate + BM25F deferral
packaging/license-sweep.sh — add a BM25-candidates check (fetch + assert pg_textsearch permissive, VectorChord-bm25 AGPL)
docs/packaging/license-audit.md — append BM25-candidates section with the verdicts
```

#### Deep file dependency analysis
- `0003-...md` (NEW): follows the existing ADR shape (`docs/adr/0001`, `0002`). No code depends on it.
- `license-sweep.sh` (Baseline row, invariant: existing apt + pgvectorscale checks + exit-contract preserved): adds an additive function; the distribution AGPL gate is unchanged (candidates are informational).
- `docs/packaging/license-audit.md` (Baseline row): additive section.

#### Deep Dives
- **Sweep check:** `curl`/docker fetch each candidate's raw LICENSE; grep `Affero|AGPL` → VectorChord-bm25 must match (barred), pg_textsearch must NOT match + must contain "PostgreSQL License" (permissive). Unfetchable → `UNVERIFIED` note (non-fatal).
- **Invariant:** the sweep still exits non-zero on any real AGPL in the *distribution* (M1 contract); the BM25-candidate block is informational (pg_textsearch isn't shipped), so it prints verdicts but only fails if a *barred* candidate were found in the shipped image (it isn't).

#### Tasks
1. Write ADR 0003.
2. Add the BM25-candidates function to `license-sweep.sh`.
3. Append the verdicts section to `docs/packaging/license-audit.md`.

#### TDD
```
RED:     run `bash packaging/license-sweep.sh` BEFORE the BM25 block exists → no BM25 verdict lines printed.
GREEN:   after the block, the sweep prints "pg_textsearch: PostgreSQL License (permissive)" and "VectorChord-bm25: AGPL/Elastic (barred)" and still exits 0 (no AGPL in the distribution).
REFACTOR: factor the per-candidate fetch into a shell function; else "None expected".
VERIFY:  bash packaging/license-sweep.sh; echo "exit=$?"  (expect the two verdict lines + exit 0)
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — a shell sweep + a markdown ADR; no shared mutable state, no locks/async.

#### Acceptance Criteria
- [ ] `docs/adr/0003-permissive-bm25-pg-textsearch.md` exists, states pg_textsearch (PostgreSQL License) as the identified permissive BM25 + measurement-first gate + BM25F deferral — `test -f docs/adr/0003-permissive-bm25-pg-textsearch.md` exits `0`.
- [ ] `bash packaging/license-sweep.sh` prints a pg_textsearch=permissive verdict AND a VectorChord-bm25=AGPL-barred verdict and exits `0` — `bash packaging/license-sweep.sh | grep -c -iE 'pg_textsearch.*permissive|vectorchord.*agpl'` returns `2`.
- [ ] `docs/packaging/license-audit.md` contains a BM25-candidates section — `grep -c -i 'pg_textsearch' docs/packaging/license-audit.md` returns `> 0`.
- [ ] Pass: lint — `bash -n packaging/license-sweep.sh` exits `0`.
- [ ] Pass: size — `wc -l` on each changed file returns `< 500`.

#### DoD
- [ ] All tasks completed and validated
- [ ] `bash packaging/license-sweep.sh` exits `0` with the two BM25 verdict lines
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

## Phase 2: Functional BM25 + measurement (throwaway image)

**Objective:** Build pg_textsearch in a throwaway image and measure BM25 recall vs ts_rank_cd — the measurement-first gate + functional proof.

### T2.1 — `packaging/Dockerfile.bm25` + `VectorDB.bm25_query` + `run_three_retrievers` BM25 leg + test

#### Objective
Create the throwaway BM25 image, add a BM25 query method + an optional BM25 retriever to the harness, and an integration test gated on pg_textsearch present.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `packaging/Dockerfile.bm25` (FROM `theo-db:dev`, build pg_textsearch
   v1.3.1 via PGXS); adds `VectorDB.bm25_query(table, query_text, n)` (`ORDER BY content <@> $1 LIMIT n` over
   a BM25 index) + `create_bm25_index`; extends `run_three_retrievers` with an optional `bm25` retriever
   (off by default — backward-compatible); adds `benchmarks/tests/test_bm25.py` gated on pg_textsearch loaded.

2. **Why it is necessary now** — this IS the measurement-first gate (build + run + measure) + the "100%
   functional, no mock" proof. The build was de-risked live (pg_textsearch v1.3.1 builds + ranks correctly).

#### Evidence
- Live PoC: pg_textsearch v1.3.1 built on `theo-db:dev`, BM25 query returned correctly-ranked rows (k1=1.20/b=0.75/avg_length=3.80) — see implementation log.
- Throwaway-image pattern: `packaging/Dockerfile.regress` (M1).
- Harness to extend: `benchmarks/theodb_bench/hybrid.py:28-58` (`run_three_retrievers`), `benchmarks/theodb_bench/db.py:137-190` (FTS/hybrid helpers), `benchmarks/theodb_bench/beir.py` (fixture).
- BM25 surface: `github.com/timescale/pg_textsearch` README (`USING bm25(content)`, `content <@> 'query'`, `shared_preload_libraries`).

#### Files to edit
```
packaging/Dockerfile.bm25 — (NEW) FROM theo-db:dev + build pg_textsearch v1.3.1 (PGXS); throwaway, not shipped
benchmarks/theodb_bench/db.py — add create_bm25_index() + bm25_query() (additive)
benchmarks/theodb_bench/hybrid.py — run_three_retrievers gains an optional bm25 retriever (default off)
benchmarks/tests/test_bm25.py — (NEW) BM25 retriever integration test, skip cleanly if pg_textsearch absent
```

#### Deep file dependency analysis
- `Dockerfile.bm25` (NEW): mirrors `Dockerfile.regress` (root-user build deps + git clone + make install); pins `v1.3.1`. Not referenced by the shipped image.
- `db.py` (Baseline row, invariant: existing methods backward-compatible): adds two methods; existing vector/FTS/hybrid methods untouched.
- `hybrid.py` (Baseline row, invariant: existing 3-retriever call path preserved): the `bm25` retriever is added via an optional flag/param defaulting to the current behavior.
- `test_bm25.py` (NEW): `integration` marker; detects pg_textsearch via `pg_extension`/`shared_preload_libraries` and `pytest.skip` if absent.

#### Deep Dives
- **BM25 query:** `SELECT doc_id FROM tbl ORDER BY content <@> %s LIMIT n` (lower `<@>` = better, per pg_textsearch); requires the BM25 index + `shared_preload_libraries=pg_textsearch`.
- **Retriever:** `run_three_retrievers(..., include_bm25=False)`; when True, add a `bm25` entry scored by the same nDCG@10/Recall@100 metrics.
- **Skip semantics:** the test queries `SELECT count(*) FROM pg_extension WHERE extname='pg_textsearch'`; 0 → `pytest.skip("pg_textsearch not loaded — run against packaging/Dockerfile.bm25 image")` (no silent green).
- **Edge:** the documents table for BM25 reuses the `create_documents_table` content column; the BM25 index is created on `content` (text), separate from the GIN/tsvector.

#### Pseudo-code / Signatures
```pseudocode
# db.py
def create_bm25_index(self, table, text_col='content', text_config='english'):
    EXECUTE f"CREATE INDEX {table}_bm25 ON {table} USING bm25({text_col}) WITH (text_config='{text_config}')"
def bm25_query(self, table, query_text, n, text_col='content'):
    EXECUTE f"SELECT doc_id FROM {table} ORDER BY {text_col} <@> %s LIMIT {n}" , (query_text,) -> [doc_id...]
# hybrid.py
run_three_retrievers(db, dataset, embed_fn, dim, ..., include_bm25=False):
    retrievers = {vector, fts, hybrid}
    if include_bm25: db.create_bm25_index(table); retrievers['bm25'] = lambda q,_: db.bm25_query(table, q, top)
    # score each with ndcg@10 + recall@100
```

#### Tasks
1. Write `packaging/Dockerfile.bm25` (pin v1.3.1).
2. Add `create_bm25_index` + `bm25_query` to `db.py`.
3. Extend `run_three_retrievers` with the optional `bm25` retriever.
4. Write `test_bm25.py` (gated on pg_textsearch present).

#### TDD
```
RED:     test_bm25_retriever_reports_metrics() [integration] — against the bm25 image: include_bm25=True; assert results['bm25'] has finite nDCG@10 + Recall@100 in [0,1]. MUST fail before db.bm25_query exists.
RED:     test_bm25_skips_without_extension() — when pg_textsearch absent, the test skips with a clear reason (no silent pass).
GREEN:   Implement Dockerfile.bm25 + db methods + retriever so it passes against the bm25 image.
REFACTOR: fold the BM25 index DDL into db.py; else "None expected".
VERIFY:  docker build -f packaging/Dockerfile.bm25 -t theo-db-bm25 . && (run container w/ shared_preload_libraries) && cd benchmarks && pytest -m integration tests/test_bm25.py -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — read-only BM25 queries within single statements; no shared mutable state, no locks/async.

#### Acceptance Criteria
- [ ] `docker build -f packaging/Dockerfile.bm25 -t theo-db-bm25 .` exits `0` (pg_textsearch v1.3.1 builds).
- [ ] Against the bm25 image, `pytest -m integration tests/test_bm25.py -k retriever` exits `0` — `bm25` retriever reports finite nDCG@10 + Recall@100.
- [ ] `pytest -m integration tests/test_bm25.py -k skips` exits `0` against an image WITHOUT pg_textsearch (clean skip, no silent pass).
- [ ] Pass: lint — `cd benchmarks && ruff check theodb_bench tests/test_bm25.py` exits `0`.
- [ ] Pass: size — every changed/new file `wc -l` returns `< 500`.

#### DoD
- [ ] All tasks completed and validated
- [ ] BM25 image builds; retriever test green against it; skip test green without it
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests` exits `0`.
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

## Phase 3: Measured report + CI + CHANGELOG

**Objective:** Record the measured BM25-vs-ts_rank_cd numbers, gate the measurement in CI, document the decision.

### T3.1 — Measured report + `bm25-measure` CI job

#### Objective
Run the BM25 measurement against the throwaway image, write the measured report, and add a CI job.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — runs `run_three_retrievers(include_bm25=True)` against the bm25 image, writes
   `docs/benchmarks/m7-bm25-vs-tsrank.md` with the measured nDCG@10/Recall@100 per retriever (vector/fts/bm25/
   hybrid) + the honest reading, and adds a `bm25-measure` CI job (build throwaway image + run the measurement).

2. **Why it is necessary now** — this is the measurement-first gate's recorded evidence (the adoption decision
   input) + the observable runtime proof; CI keeps it reproducible. Per `public-copy.md`, the report states
   only measured numbers.

#### Evidence
- Report convention: `docs/benchmarks/` (M2/M7-S1).
- CI job pattern: `.github/workflows/ci.yml` `hybrid-search`/`pg-regression` jobs.
- public-copy rule: `rules/public-copy.md` §4-§5.

#### Files to edit
```
docs/benchmarks/m7-bm25-vs-tsrank.md — (NEW) measured nDCG@10/Recall@100 per retriever + honest reading + BM25F-deferred note
.github/workflows/ci.yml — add bm25-measure job (build Dockerfile.bm25 + run the BM25 measurement) with timeout-minutes
CHANGELOG.md — [Unreleased] M7-S2 entry
```

#### Deep file dependency analysis
- `docs/benchmarks/m7-bm25-vs-tsrank.md` (NEW): records T2.1's measured output.
- `.github/workflows/ci.yml` (invariant: existing jobs stay): additive `bm25-measure` job; reuses buildx cache; `timeout-minutes` per convention; starts the container with `shared_preload_libraries=pg_textsearch`.

#### Deep Dives
- **Report honesty:** if BM25 does not beat ts_rank_cd on the synthetic fixture, the report says so plainly (Rule 3); the decision is "measured numbers reported", not a hardcoded BM25 win. A real-corpus eval is named as the decisive follow-up.
- **CI:** the `bm25-measure` job builds `Dockerfile.bm25`, runs the container with `-c shared_preload_libraries=pg_textsearch`, runs `pytest -m integration tests/test_bm25.py` (deterministic synthetic fixture; no external API).

#### Tasks
1. Run the measurement; write `docs/benchmarks/m7-bm25-vs-tsrank.md` with the numbers + reproduction.
2. Add the `bm25-measure` CI job with `timeout-minutes`.
3. Add the CHANGELOG entry.

#### TDD
```
RED:     CI yaml invalid / job missing before edit (python3 -c yaml.safe_load asserts presence).
GREEN:   `python3 -c "import yaml; assert 'bm25-measure' in yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']"` exits 0; report file exists with measured numbers.
REFACTOR: none expected.
VERIFY:  python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && test -f docs/benchmarks/m7-bm25-vs-tsrank.md
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — markdown report + YAML edit; no concurrent state.

#### Acceptance Criteria
- [ ] `docs/benchmarks/m7-bm25-vs-tsrank.md` exists with measured nDCG@10 + Recall@100 for vector/fts/bm25/hybrid + reproduction command — `grep -c -iE 'bm25|ndcg|recall' docs/benchmarks/m7-bm25-vs-tsrank.md` returns `> 0`.
- [ ] CI `bm25-measure` job parses + present — `python3 -c "import yaml; assert 'bm25-measure' in yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']"` exits `0`; job has `timeout-minutes`.
- [ ] No unbenchmarked perf claim — `grep -ciE 'faster than|outperforms|[0-9]+x ' docs/benchmarks/m7-bm25-vs-tsrank.md` returns `0` (only measured numbers, Rule 5).
- [ ] Pass: size — changed files `wc -l` within budget.

#### DoD
- [ ] All tasks completed and validated
- [ ] Report committed with measured numbers; CI job parses — `test -f docs/benchmarks/m7-bm25-vs-tsrank.md && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` exits `0`. + runs locally-validated steps
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

## Coverage Matrix

| # | Gap / Requirement (ROADMAP M7 DoD-3 + blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Permissive BM25 alternative IDENTIFIED (the DoD) | T1.1 | ADR 0003 names pg_textsearch (PostgreSQL License) |
| 2 | License verdict reproducible (D1 fail-closed) | T1.1 | license-sweep asserts pg_textsearch permissive + VectorChord-bm25 AGPL-barred |
| 3 | VectorChord-bm25 confirmed AGPL-barred | T1.1 | sweep + audit doc |
| 4 | BM25 functionally proven on TheoDB engine (no mock) | T2.1 | Dockerfile.bm25 builds v1.3.1; bm25_query returns ranked results |
| 5 | Measurement-first gate: BM25 recall measured vs ts_rank_cd | T2.1, T3.1 | bm25 retriever + measured report |
| 6 | Honest reading (no unbenchmarked superiority claim) | T3.1 | report states measured numbers only (Rule 5) |
| 7 | Distribution unchanged until measurement justifies (YAGNI) | T2.1 | pg_textsearch only in throwaway `Dockerfile.bm25` (D1/D3); shipped image untouched |
| 8 | BM25F deferred with rationale | T1.1, T3.1 | ADR 0003 §D4 (written in T1.1) + report note (T3.1) |
| 9 | Reproducible in CI | T3.1 | bm25-measure job |

**Coverage: 9/9 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] `bash packaging/license-sweep.sh` exits `0` with the BM25 verdict lines
- [ ] `docker build -f packaging/Dockerfile.bm25 -t theo-db-bm25 .` exits `0`; BM25 retriever test green against it; skip test green without it
- [ ] Measured report committed (`docs/benchmarks/m7-bm25-vs-tsrank.md`) — numbers only, no unbenchmarked claim
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests` exits `0`. — `cd benchmarks && ruff check theodb_bench tests`
- [ ] File-size budget respected (per `rules/architecture.md`)
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6)
- [ ] Backward compatibility preserved — shipped `Dockerfile` unchanged; `VectorDB`/`run_three_retrievers` existing paths intact
- [ ] Runtime-metric proof — the `bm25` retriever is observed reporting finite nDCG@10/Recall@100 against the real pg_textsearch build (not just compiling)
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| pg_textsearch extension (in-DB, requires shared_preload_libraries) | extension not loaded | run the BM25 test against an image without pg_textsearch / without the preload | `test_bm25_skips_without_extension` skips with a clear reason (no silent green) |
| PostgreSQL (`psycopg`, throwaway container) | container not ready | run before healthy | `VectorDB.connect/ping` raises a clear error; CI waits on healthcheck |
| pg_textsearch BM25 index build | index build on empty/edge content | seed a doc with empty content | index build does not crash; BM25 query returns no row for a non-matching query (no error) |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate the BM25 identification, license proof, and measurement end-to-end.

### Execution
```
bash packaging/license-sweep.sh                                  # BM25 verdict lines + exit 0
docker build -f packaging/Dockerfile.bm25 -t theo-db-bm25 .      # pg_textsearch v1.3.1 build
docker run -d --name bm25-it -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db-bm25 -c shared_preload_libraries=pg_textsearch
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_bm25.py -q                    # bm25 retriever measured
ruff check theodb_bench tests/test_bm25.py
# no regression on the shipped image:
docker build -t theo-db:dev . && PGPORT=<port2> bash smoke.sh
```

### Acceptance Criteria
- [ ] license-sweep prints the BM25 verdicts + exits 0
- [ ] BM25 image builds; `bm25` retriever reports finite nDCG@10/Recall@100 against the real build
- [ ] skip test green when pg_textsearch absent — `pytest -m integration tests/test_bm25.py -k skips` exits `0` (clean skip, no silent pass).
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests` exits `0`.
- [ ] Shipped image smoke still green — `docker build -t theo-db:dev . && PGPORT=<p> bash smoke.sh` exits `0` (no regression; distribution unchanged).
- [ ] Report committed with measured numbers; CI job parses — `test -f docs/benchmarks/m7-bm25-vs-tsrank.md && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` exits `0`.
