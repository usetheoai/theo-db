# Blueprint: Hybrid Search (FTS + vector) + Reciprocal Rank Fusion

> **Version 1.0** — Synthesizes how permissive OSS PostgreSQL stacks combine full-text search (FTS)
> with vector similarity and fuse the two rankings via Reciprocal Rank Fusion (RRF), so TheoDB can
> ship **M7-S1 (hybrid search)** as a pure-OSS win on the M2 `pgvector` base — without the AGPL-barred
> `pg_search` (D1). Investigated: `pgvector` (canonical hybrid pointer), `supabase-postgres` (PG-native
> FTS + RUM), `paradedb` (working RRF field witness — AGPL, read for technique only), `pgvectorscale`
> (rescore lever), plus the RRF primary source (Cormack et al. 2009), the AlloyDB `ai.hybrid_search`
> SOTA anchor, and BEIR (recall methodology). Decides: FTS index (GIN vs RUM), the RRF fusion contract
> (`1/(k+rank)`, k=60, `FULL OUTER JOIN` + `COALESCE`), the API surface (manual SQL MVP → native fn
> wrapper), and the recall methodology to claim the win.

**Slug:** `m7-hybrid-search-rrf`
**Source plan:** `.claude/knowledge-base/discoveries/plans/m7-hybrid-search-rrf-plan.md`
**Owner:** paulohenriquevn
**Generated:** 2026-06-28 via `/discover-execute`
**Confidence verdict:** SHIPPABLE_WITH_CAVEATS (placeholder — updated by `/discover-confidence`)

## Context

ROADMAP `### M7` (IA avançada) DoD-2 requires "Hybrid search (texto + semântico) + reranking (RRF) com
recall medido (ex.: BEIR)". M7-S1 is the pure-OSS, immediate win on top of M2's `pgvector` base; BM25
permissivo is a separate, AGPL-shaped gap deferred to M7-S2. The target API is specified at
`docs/features/06-busca-hibrida.md` — both a native `ai.hybrid_search()` JSON-array surface and a manual
SQL/RRF path. The SOTA anchor is AlloyDB, which ships `ai.hybrid_search()` and "fuses results from each
search type into a single list using the Reciprocal Rank Fusion (RRF) algorithm" (AlloyDB docs,
`docs.cloud.google.com/alloydb/docs/ai/run-hybrid-vector-similarity-search`); TheoDB matches the
capability with permissive pieces and wins on model-agnosticism (CLAUDE.md TheoDB rule 1). M2 already
shipped the recall@k harness (`benchmarks/theodb_bench/recall.py`), the measurement-first base this
slice extends (CLAUDE.md TheoDB rule 5 — performance is a claim, not an opinion).

## Objective

Let the reader decide the M7-S1 implementation: FTS index choice, the RRF fusion contract, the minimal
API surface, and the recall methodology — entirely on permissive OSS.

---

## Coverage Corner 1 — Integration Tests

> How the OSS stack tests FTS+vector hybrid retrieval against a real Postgres (Q1). Answers Q1; carries E6.

### supabase-postgres — PG-native FTS, asserted against a live PG

FTS is tested as plain SQL against a real Postgres, not mocked. The suite builds `tsvector`s inline and
asserts matches via the `@@` operator:

```sql
-- .claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:32
select to_tsvector('english', 'green eggs and ham');
-- :40
where to_tsvector(title) @@ to_tsquery('Harry');
```

- **Pattern**: query operators are exercised end-to-end — boolean (`little & big` :76), OR (`little | big`
  :84), phrase/proximity (`big <-> dreams` :122, `year <2> school` :129), negation (`big & !little` :136),
  prefix (`Lit:*` :86) — `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql`.
- **Fixtures**: a real `books` table; the durable form uses a **generated stored `tsvector` column** plus a
  GIN index (`docs-full-text-search.sql:102-105`), the exact shape the M7 spec mandates
  (`docs/features/06-busca-hibrida.md` §2).
- **Coverage**: asserts *which rows match* a query; it does **not** assert RRF fusion or ranking order — that
  is the gap the paradedb witness fills below.

### paradedb — working RRF hybrid, asserted to exact fused scores (field witness; AGPL — technique only)

The `reciprocal_rank_fusion` test is the load-bearing field witness for the whole fusion contract. It seeds
a real PG, builds an HNSW vector index, runs both legs, fuses with `FULL OUTER JOIN`, and asserts the exact
RRF scores:

```rust
// .claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:99-118
WITH semantic AS (
    SELECT id, RANK () OVER (ORDER BY embedding <=> '[1,2,3]') AS rank
    FROM paradedb.bm25_search ORDER BY embedding <=> '[1,2,3]' LIMIT 20
), bm25 AS (
    SELECT id, RANK () OVER (ORDER BY pdb.score(id) DESC) as rank
    FROM paradedb.bm25_search WHERE bm25_search @@@ 'description:keyboard' LIMIT 20
)
SELECT COALESCE(semantic.id, bm25.id) AS id,
    (COALESCE(1.0 / (60 + semantic.rank), 0.0) + COALESCE(1.0 / (60 + bm25.rank), 0.0))::REAL AS score
FROM semantic FULL OUTER JOIN bm25 ON semantic.id = bm25.id
ORDER BY score DESC LIMIT 5;
```

- **Pattern**: ranks per leg via `RANK() OVER (...)`, fuses via `FULL OUTER JOIN`, scores via summed
  `1.0/(60+rank)` — `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:101-116`.
- **Coverage (E6 — empty-leg)**: the `COALESCE(semantic.id, bm25.id)` projection
  (`hybrid.rs:111`) **plus** `COALESCE(1.0/(60+rank), 0.0)` (`hybrid.rs:112-113`) is exactly the missing-leg
  handler — a doc matched by only one retriever still surfaces, its absent leg contributing `0.0`. The
  `FULL OUTER JOIN` (`hybrid.rs:116`) is what preserves rows present in either CTE.
- **Assertion**: the test pins fused scores to e.g. `(1, 0.03062178…, "Ergonomic metal keyboard")`
  (`hybrid.rs:124-128`) — i.e. the hybrid result *order and score* are regression-locked, not just
  membership. **AGPL boundary (E3/D3)**: we read this to learn the SQL shape; the math is the public Cormack
  technique, never the AGPL code.

---

## Coverage Corner 2 — Dependencies

> Runtime pieces for PG-native FTS+vector+RRF, and which are permissive vs AGPL-barred (Q2). Carries E3.

### Dependency table (piece → license → in-distribution?)

| Piece | Role | License | In TheoDB distribution? | Citation |
|---|---|---|---|---|
| PostgreSQL FTS (`to_tsvector`/`to_tsquery`/`@@`/`ts_rank`/`ts_rank_cd`) | FTS leg + ranking | PostgreSQL License (built-in, **zero dep**) | **YES** — core engine | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:32,40,105` |
| `pgvector` (`vector`, `<=>`, HNSW/IVFFlat) | vector leg | PostgreSQL License (permissive) | **YES** — already shipped in M2 | `.claude/knowledge-base/references/pgvector/README.md:628` (`ts_rank_cd` hybrid example over `pgvector`) |
| GIN index (`using gin (fts)`) | FTS index (default) | PostgreSQL License (built-in) | **YES** | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:105` |
| RUM index (`rum (a rum_tsvector_ops)`, `<=>` on tsvector) | FTS index (escape hatch) | PostgreSQL License (permissive, external ext.) | **OPTIONAL** — not core; note portability caveat below | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/z_15_rum.sql:27,31` |
| `pgvectorscale` `diskann.query_rescore` | vector-leg accuracy lever | PostgreSQL License (permissive) | OPTIONAL — complementary, not RRF | `.claude/knowledge-base/references/pgvectorscale/README.md:382,387` |
| `pg_search` / ParadeDB BM25 | BM25 lexical leg | **AGPL-3.0** | **NO — barred by D1** | `.claude/knowledge-base/references/paradedb/LICENSE:1` ("GNU AFFERO GENERAL PUBLIC LICENSE") |

- **E3 confirmed**: `paradedb/LICENSE:1` is AGPL-3.0 → barred from the distribution by PRD D1 / CLAUDE.md
  TheoDB rule 2. ParadeDB is a **field witness for the RRF math only**, never vendored.
- **RUM portability caveat**: the RUM test carries a header note that the extension "is excluded from
  oriole-17 because it uses an unsupported index type"
  (`.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/z_15_rum.sql:1-3`). RUM is therefore an
  optional escape hatch, not a baseline — reinforcing GIN-first (D1).
- **Net**: the M7-S1 baseline (PG FTS + `pgvector` + GIN + RRF in SQL) is **100% permissive and zero new
  hard dependency** — RRF is plain SQL, not an extension.

---

## Coverage Corner 3 — Tools

> Local-dev / smoke recipe to exercise a hybrid query end-to-end against `theo-db:dev` (Q3).

### Local dev story (grounded in the repo + the pgvector example)

The repo already has a minimal smoke harness that brings up the engine and proves `pgvector` is live:

```bash
# smoke.sh (repo root)
psql ... <<'SQL'
CREATE EXTENSION IF NOT EXISTS vector;
SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;
SQL
echo "SMOKE PASSED"
```

- **Build**: `Dockerfile` (repo root) builds the `theo-db` image; `smoke.sh` waits via `pg_isready` then
  runs `CREATE EXTENSION IF NOT EXISTS vector` — the hybrid smoke extends this same file.
- **Hybrid reproduction recipe (M7-S1 smoke)** — derived from the pgvector hybrid example
  (`.claude/knowledge-base/references/pgvector/README.md:628-629`) + the spec
  (`docs/features/06-busca-hibrida.md` §41):
  1. **Container up**: reuse `smoke.sh`'s `pg_isready` loop + `psql -v ON_ERROR_STOP=1`.
  2. **Extensions**: `CREATE EXTENSION IF NOT EXISTS vector;` (GIN is built-in; RUM only if testing the
     escape hatch).
  3. **Seed**: a `documents(doc_id, content, text_tsv tsvector GENERATED …, embedding vector(N))` table
     (spec §2) + GIN index on `text_tsv` (`docs-full-text-search.sql:105`) + HNSW on `embedding`.
  4. **Hybrid query**: the manual RRF CTE (spec §41 / paradedb `hybrid.rs:99-118` shape).
  5. **Assert**: top-k `doc_id` order matches an expected list (paradedb's `assert_eq!` on fused order,
     `hybrid.rs:120-148`, is the template — adapt to `psql` + a golden file).
- **CI shape**: not separately cited here — the existing `smoke.sh` is the reproduction unit; a CI workflow
  for the hybrid smoke is an M7-S1 implementation task (no fabricated workflow path).

---

## Coverage Corner 4 — Techniques

> Three technique subsections: (a) RRF fusion contract, (b) FTS leg index/ranking, (c) recall methodology.
> Frontier rigor R1 (SOTA anchor) + R2 (≥2 primary sources) + R3 (benchmark-or-`UNBENCHMARKED`).

### (a) RRF fusion contract — `1/(k+rank)`, k=60, FULL OUTER JOIN, RANK() window

**SOTA anchor (R1)**: AlloyDB's `ai.hybrid_search()` "fuses results from each search type into a single list
using the Reciprocal Rank Fusion (RRF) algorithm" and exposes a per-component `weight` parameter described as
the "contribution of this search entry to the overall Reciprocal Rank Fusion (RRF)" (AlloyDB docs,
`docs.cloud.google.com/alloydb/docs/ai/run-hybrid-vector-similarity-search`). **Gap TheoDB closes**: match
the capability with permissive PG primitives (FTS + `pgvector` + RRF-in-SQL), model-agnostic, no managed-AI
lock-in (CLAUDE.md TheoDB rule 1).

**Primary source (R2)**: Cormack, Clarke & Büttcher, SIGIR'09, "Reciprocal Rank Fusion outperforms Condorcet
and individual Rank Learning Methods" (`dl.acm.org/doi/pdf/10.1145/1571941.1572114`, DOI
`10.1145/1571941.1572114`). Formula: `s_RRF(d) = Σ_{r∈R} 1/(k + r(d))` — sum over rankers of the reciprocal
of the document's rank. **k=60 (E1)**: purely *empirical* — k=60 "is found to be the value with the best
average result in the benchmarks evaluated in the original work"; it is a smoothing constant that dampens the
outsized influence of a single top-ranked outlier, and RRF is **score-independent** (only ranks enter the
fusion, not raw scores).

**Field realizations (cross-validated, R2 second witness)**:

| Source | RRF formula | k | join shape | weight | Citation |
|---|---|---|---|---|---|
| Cormack 2009 (authority) | `Σ 1/(k+r(d))` | 60 (empirical) | n/a (set fusion) | unweighted | `dl.acm.org/doi/pdf/10.1145/1571941.1572114` |
| paradedb `reciprocal_rank_fusion` | `1.0/(60+rank)` summed | 60 | `FULL OUTER JOIN` + `COALESCE` | unweighted | `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:112-116` |
| paradedb `hybrid_deprecated` (weighted variant) | `1.0/(60+rank)*w` | 60 | `FULL OUTER JOIN` | `*0.1` / `*0.9` | `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:65-68` |
| TheoDB spec (manual SQL) | `COALESCE(1.0/(60+rank),0)` summed | 60 (hardcoded) | `FULL OUTER JOIN` + `COALESCE` | unweighted | `docs/features/06-busca-hibrida.md` §37, §41 |
| AlloyDB `ai.hybrid_search` | RRF, weighted | k smoothing | native fn | per-component `weight` | `docs.cloud.google.com/alloydb/docs/ai/run-hybrid-vector-similarity-search` |

**Notable differences**: paradedb's *deprecated* variant multiplies each leg by a weight
(`hybrid.rs:65-66`), matching AlloyDB's `weight` knob; the *current* `reciprocal_rank_fusion` test is
unweighted (`hybrid.rs:112-113`). The TheoDB spec hardcodes 60 (§37). **Decision E1**: default k=60 (cite
Cormack), expose as an optional parameter so per-corpus tuning is possible without a re-deploy — see D2.

**MVP surface (E5)**: the native `ai.hybrid_search()` JSON-array API (spec §9-§40) is elaborate
(per-component JSONB: `data_type`, `weight`, `ranking_function`, `distance_operator`, `query_text_input`…).
The manual SQL/RRF CTE (spec §41) reproduces the *same* fused result with zero new surface — it is literally
the paradedb shape (`hybrid.rs:99-118`) over `pgvector`. **Parsimony ladder (rule)**: ship the manual SQL
path first (rung 5-6: minimal that works); add `ai.hybrid_search()` later as a thin wrapper over the same
CTE — D2.

### (b) FTS leg — `ts_rank`/`ts_rank_cd` + GIN vs RUM `<=>` (Q5, E2)

| Option | Index DDL | Ranking fn | Trade-offs | Citation |
|---|---|---|---|---|
| **GIN + `ts_rank`/`ts_rank_cd`** (default) | `create index … using gin (fts)` | `ts_rank_cd(textsearch, query)` | Built-in (zero dep); GIN stores no positional/rank payload, so ranking is a post-filter compute, not index-served; portable everywhere | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:105`; `.claude/knowledge-base/references/pgvector/README.md:628-629` |
| **RUM + `<=>` distance** (escape hatch) | `create index … using rum (a rum_tsvector_ops)` | `a <=> to_tsquery(...)` (rank served by index, ordered ASC) | Stores rank info → can order by relevance from the index; external extension; **not portable** (excluded from oriole-17) | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/z_15_rum.sql:27,31,37`; portability note `z_15_rum.sql:1-3` |

- **E2 (language config)**: the durable FTS column fixes the language at DDL time —
  `to_tsvector('english', description || ' ' || title)` in a `GENERATED ALWAYS … STORED` column
  (`docs-full-text-search.sql:102-103`), matching the spec (`docs/features/06-busca-hibrida.md` §2). The
  `english` config is baked into the stored `tsvector`; changing language requires a column re-definition.
  **Recommendation**: default `'english'`; document `'simple'` (unstemmed) and other configs as the escape
  hatch, and set the language explicitly in the generated-column DDL (never rely on the cluster
  `default_text_search_config`). `to_tsquery` (advanced), `plainto_tsquery` (plain), and the spec's preferred
  `g_to_tsquery` (AlloyDB-style) are the parser options (spec §27-§29; AlloyDB notes `g_to_tsquery` as the
  default high-relevance parser).
- **Recommendation (D1)**: **GIN + `ts_rank_cd` as the default** — it is the zero-dependency, portable,
  100%-permissive baseline and is exactly what the pgvector hybrid example uses
  (`pgvector/README.md:628-629`). RUM is the documented escape hatch for relevance-ordered FTS at scale,
  gated on a measured need (its portability caveat is real).

### (c) Recall methodology — hybrid vs pure-vector vs pure-keyword (Q6, E4)

**SOTA-anchored metric (R3)**: BEIR (Thakur et al. 2021, `arxiv.org/abs/2104.08663`) is the field-standard
heterogeneous zero-shot IR benchmark — 18 datasets / 9 tasks; **primary metric nDCG@10**, with **Recall@100**
reported alongside, computed via the official TREC eval tool. BEIR's headline finding is directly relevant:
BM25 (lexical) is a strong generalization baseline often beaten only by re-ranking/late-interaction at high
cost — i.e. **a hybrid (lexical + dense) leg is exactly the regime where fusion pays off**, motivating the
RRF win claim.

**Recall numbers**: `UNBENCHMARKED` (R3). No hybrid-vs-vector-vs-keyword recall has been measured on TheoDB
yet. The M2 harness (`benchmarks/theodb_bench/recall.py:61` `recall_at_k`) computes **distance-thresholded,
pure-vector** recall with exact brute-force ground truth (ANN-Benchmarks semantics, `recall.py:3-4`); it has
**no FTS/keyword leg and no graded-relevance (qrels) support** — confirmed by inspecting the module
(`benchmarks/theodb_bench/`: `recall.py`, `harness.py`, `metrics.py`, `dataset.py`, `db.py` — none reference
FTS/`ts_rank`/hybrid).

- **E4 (harness gap) — honest**: the M2 recall@k harness does **not** extend to a keyword/hybrid leg as-is.
  Claiming the M7 "recall win" requires **extending the harness** to: (1) ingest a BEIR-style dataset with
  graded qrels, (2) run three retrievers (pure-vector, pure-FTS, RRF-hybrid), (3) score nDCG@10 + Recall@100
  per the BEIR methodology. This is an M7-S1 implementation task, **not** a property the harness has today.
  No recall number may be stated until this lands (CLAUDE.md TheoDB rule 5; `public-copy.md` §4-§5).
- **`pgvectorscale` rescore lever**: `diskann.query_rescore` (default 50) tunes the *vector leg's*
  accuracy/speed trade-off at query time (`.claude/knowledge-base/references/pgvectorscale/README.md:382,387`).
  It is **complementary to, not a substitute for, RRF** — it raises the recall of the vector candidate set
  *before* fusion; RRF then fuses the two legs. Optional, gated on a measured need.

---

## Cross-cutting Comparison

| Dimension | pgvector | supabase-postgres | paradedb (AGPL — witness only) | pgvectorscale |
|---|---|---|---|---|
| Integration-test style | README example (hybrid pointer), `README.md:628-632` | live-PG SQL asserts on `@@` match, `docs-full-text-search.sql:40` | live-PG `assert_eq!` on **fused RRF order+score**, `hybrid.rs:120-148` | README usage examples, `README.md:166` |
| FTS leg | `plainto_tsquery` + `ts_rank_cd` (`README.md:628-629`) | `to_tsvector`/`to_tsquery` + GIN (`docs-full-text-search.sql:105`); RUM `<=>` (`z_15_rum.sql:27`) | `@@@` BM25 (AGPL — barred, `LICENSE:1`) | n/a (vector only) |
| RRF realization | links to external `rrf.py` (not cloned), `README.md:632` | n/a | `1.0/(60+rank)` + `FULL OUTER JOIN`, `hybrid.rs:112-116` | n/a |
| In-distribution? | YES (M2) | YES (built-in FTS + GIN) | **NO** (AGPL) | OPTIONAL (rescore lever) |
| License | PostgreSQL | PostgreSQL (core) | AGPL-3.0 | PostgreSQL |

## ADRs

### D1 — FTS index: GIN + `ts_rank_cd` as default, RUM as escape hatch

**Decision:** Default the FTS leg to a built-in **GIN index over a `GENERATED … STORED` `tsvector` column**,
ranked with `ts_rank_cd`. Offer **RUM** (`rum_tsvector_ops` + `<=>`) only as a documented, opt-in escape
hatch for relevance-ordered FTS at scale.

**Rationale:** GIN is zero-dependency, 100% permissive (PostgreSQL License), portable to every PG build, and
is exactly the index the canonical pgvector hybrid example assumes
(`.claude/knowledge-base/references/pgvector/README.md:628-629`;
`.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:105`). It keeps
M7-S1 free of new hard dependencies — RRF itself is plain SQL.

**Alternatives considered:** *RUM as default* — rejected: it is an external extension and is "excluded from
oriole-17 because it uses an unsupported index type"
(`.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/z_15_rum.sql:1-3`) → portability risk
unjustified without a measured ranking-latency need. *BM25 via `pg_search`* — rejected: AGPL-3.0, barred by
PRD D1 (`.claude/knowledge-base/references/paradedb/LICENSE:1`); deferred to M7-S2.

**Consequences:** GIN does not store rank payload, so `ts_rank_cd` is a post-match compute, not index-served;
acceptable for the FTS *leg* of a top-k hybrid (each leg is already `LIMIT`-bounded before fusion). RUM stays
available behind a flag for the day a measurement proves GIN ranking is the bottleneck.

### D2 — RRF contract: k=60 default (exposed as param); manual SQL MVP, native fn as thin wrapper

**Decision:** Implement RRF as the manual SQL CTE (`RANK() OVER` per leg → `FULL OUTER JOIN` → summed
`COALESCE(1.0/(k+rank),0)`) with **k=60 as a tunable default**. Ship this manual path first; add the native
`ai.hybrid_search()` JSON-array surface later as a **thin wrapper over the same CTE**.

**Rationale:** k=60 is the empirically-justified Cormack 2009 default
(`dl.acm.org/doi/pdf/10.1145/1571941.1572114`), cross-validated by two field witnesses
(`.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:112-116`; TheoDB spec
`docs/features/06-busca-hibrida.md` §37). The manual SQL is the minimal thing that works (parsimony ladder
rung 5-6) and is byte-for-byte the proven paradedb shape over `pgvector`. Exposing k as a parameter honors
the primary source's caveat that the optimum is application-specific.

**Alternatives considered:** *Hardcode k=60, no param* — rejected: forecloses per-corpus tuning the paper
explicitly says matters. *Native `ai.hybrid_search()` first* — rejected (E5): the JSON-array surface (spec
§9-§40) is elaborate and would be over-engineering before the fusion math is proven; KISS says manual SQL
first. *Weighted RRF from day one* — deferred: the deprecated paradedb variant
(`hybrid.rs:65-68`) and AlloyDB's `weight` show the path, but the unweighted contract is the MVP.

**Consequences:** Empty-leg queries are handled by the `FULL OUTER JOIN` + `COALESCE` already in the contract
(E6, `hybrid.rs:111-116`). The native fn becomes a presentation wrapper, not a second engine — one source of
truth for the fusion math.

### D3 — Borrow the RRF technique, never the AGPL code (D1/PRD compliance)

**Decision:** ParadeDB (`paradedb/`) is read **only** to understand the RRF algorithm and SQL shape. No AGPL
code, schema, or test is copied into TheoDB. The RRF formula is the public Cormack et al. 2009 technique,
cited from the primary paper.

**Rationale:** PRD D1 / CLAUDE.md TheoDB rule 2 bar AGPL from the distribution; the LICENSE is unambiguous
(`.claude/knowledge-base/references/paradedb/LICENSE:1` — "GNU AFFERO GENERAL PUBLIC LICENSE"). Algorithms
are not the licensed artifact — the formula `Σ 1/(k+r(d))` is independently citable from
`dl.acm.org/doi/pdf/10.1145/1571941.1572114`.

**Alternatives considered:** *Vendor `pg_search`* — rejected (AGPL). *Reimplement BM25 ourselves for M7-S1* —
rejected: out of scope (PG-native FTS is the M7-S1 baseline; permissive BM25 is M7-S2).

**Consequences:** The blueprint cites paradedb as a *field witness* of the SQL shape, never as a source to
copy; the authority for the math is the paper. Clean-room boundary documented for the M7-S1 implementer.

## Recommendations for the project

| # | Recommendation | Linked to | Priority |
|---|---|---|---|
| 1 | Ship M7-S1 as the **manual RRF CTE** over `pgvector` + GIN-indexed `tsvector` (spec §41 shape) — no new hard dependency | Q1, Q4, D1, D2 | HIGH |
| 2 | Default **k=60, exposed as a query parameter**; handle empty legs with `FULL OUTER JOIN` + `COALESCE(…,0)` | Q4, E1, E6, D2 | HIGH |
| 3 | Default FTS to **GIN + `ts_rank_cd`** on a `GENERATED STORED tsvector('english',…)` column; document `'simple'`/other configs + RUM as opt-in escape hatches | Q5, E2, D1 | HIGH |
| 4 | **Extend the M2 recall harness** to a BEIR-style hybrid eval (qrels ingest + 3 retrievers + nDCG@10/Recall@100) **before** any recall claim — currently a gap | Q6, E4, CLAUDE.md rule 5, public-copy.md §4-5 | HIGH |
| 5 | Add the hybrid query to `smoke.sh` (extend the existing `pg_isready`+`psql` harness) with a golden top-k assertion | Q3 | MEDIUM |
| 6 | Add `ai.hybrid_search()` JSON-array fn later as a **thin wrapper** over the manual CTE (one fusion source of truth) | Q4, E5, D2 | MEDIUM |
| 7 | Treat `pgvectorscale` `diskann.query_rescore` as an **optional pre-fusion vector-leg accuracy lever**, gated on a measured need | Q6 | LOW |
| 8 | Keep the **clean-room boundary**: read paradedb for technique, cite Cormack 2009 as authority; never vendor AGPL | Q2, E3, D3 | HIGH |

## Blocked questions (if any)

| Question | Reason | Suggested human follow-up |
|---|---|---|
| (none — all 6 questions answered) | — | — |

> Partial note (not blocking): `pgvector`'s RRF example file `pgvector/examples/hybrid_search/rrf.py` is
> **not cloned locally** — the README only *links* to the external `pgvector-python` repo
> (`.claude/knowledge-base/references/pgvector/README.md:632`). The RRF technique is fully covered by the
> paradedb field witness (`hybrid.rs:99-118`) + the Cormack 2009 primary source + the TheoDB spec, so Q4 is
> answered, not blocked. Re-cloning `pgvector-python` for the canonical `rrf.py` is an optional M7-S2 nicety.

## Halt-loop progress (audit trail)

- Iterations used: 1 / (single-pass — all sources reachable on first read)
- Questions answered: 6 / 6 (Q1–Q6)
- Questions blocked: 0
- Citations verified: 13 local references read at cited lines (`pgvector/README.md:623-660`;
  `paradedb/tests/tests/hybrid.rs:26-148`; `paradedb/LICENSE:1`;
  `supabase-postgres/.../docs-full-text-search.sql:32,40,76,84,86,102-105,122,129,136`;
  `supabase-postgres/.../z_15_rum.sql:1-3,27,31,37`; `pgvectorscale/README.md:307,382,387`; repo `smoke.sh`,
  `benchmarks/theodb_bench/recall.py:1-82`, `benchmarks/theodb_bench/` module list) + 3 allowlisted web
  sources (Cormack 2009 `dl.acm.org/doi/pdf/10.1145/1571941.1572114`; AlloyDB hybrid-search doc
  `docs.cloud.google.com/alloydb/docs/ai/run-hybrid-vector-similarity-search`; BEIR `arxiv.org/abs/2104.08663`).
- `UNBENCHMARKED` markers left: 1 — Corner 4(c): no hybrid-vs-vector-vs-keyword recall measured on TheoDB
  yet (harness gap E4 → M7-S1 task). GIN-vs-RUM build/query latency also unmeasured (deferred, gated on need).
- Promise emitted at iteration: 1 — `<promise>BLUEPRINT_COMPLETE</promise>` (all 4 corners populated, all
  citations resolve on disk, 3 ADRs with alternatives, every web perf claim either methodology-backed or
  `UNBENCHMARKED`).

## Related

- Discovery plan: `.claude/knowledge-base/discoveries/plans/m7-hybrid-search-rrf-plan.md`
- Target API spec: `docs/features/06-busca-hibrida.md`
- North-star ADR: `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`
- Frontier rigor profile: `.claude/rules/discover-phd-rigor.md`
- Confidence report: `.claude/knowledge-base/reviews/m7-hybrid-search-rrf-confidence-2026-06-28.md` (generated by `/discover-confidence`)
- Project rules: `.claude/rules/public-copy.md` (§4-§5 — no unbenchmarked perf claims), `.claude/rules/testing.md`, `.claude/rules/architecture.md`
