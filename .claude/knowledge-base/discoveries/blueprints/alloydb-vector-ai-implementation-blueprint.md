# Blueprint: AlloyDB Vector/AI Engine — How It's Implemented

> **Discovery verdict:** `SHIPPABLE` (98.7/100, `/discover-confidence` M2 — 0 hard caps, 4/4 corners, 20 citações verificadas, 2026-06-26)
> **Slug:** `alloydb-vector-ai-implementation` · **Created:** 2026-06-26 · **Owner:** paulohenriquevn (CTO)
> **Plan:** `.claude/knowledge-base/discoveries/plans/alloydb-vector-ai-implementation-plan.md`
> **Method note (honesty):** produced by deep-reading the OSS analogs (`pgvector`, `pgvectorscale`) end-to-end
> + reconstructing the closed-source AlloyDB engine from **published** primary sources on the
> `discover-web-allowlist.txt` (ScaNN paper, AlloyDB docs, Google Research blog). AlloyDB is
> **closed-source** — every AlloyDB internal here is published or explicitly flagged `inferred (not confirmed)`.
> Performance claims carry methodology+source or the literal marker `UNBENCHMARKED` (rule `discover-phd-rigor.md` R3).

## Context

TheoDB é SOTA-anchored no AlloyDB (CLAUDE.md regra 1) e seu pilar killer é o vetorial/IA (`ROADMAP.md` M2/M7; PRD P2). Esta discovery foi disparada porque: (a) o M2 DoD exige índice ANN avançado **com recall medido e benchmark reproduzível**; (b) o M7 DoD (recém-expandido) exige filtered search, hybrid+reranking e funções `ai.*`, espelhados em `docs/features/` (hoje só especificação); (c) PRD **D3** só autoriza fork de `pgvector`/`pgvectorscale` mediante **benchmark de gatilho reproduzível** — e precisávamos da evidência. Plano: `.claude/knowledge-base/discoveries/plans/alloydb-vector-ai-implementation-plan.md`.

## Objective

Permitir decidir **qual stack de índice ANN + camada de IA o TheoDB adota para igualar o motor vetorial/IA do AlloyDB usando só peças OSS permissivas**, e **se/quando** o fork D3 se justifica. Critérios de sucesso: 4 coverage corners populados com citações reais; tabela comparativa SOTA (ScaNN × StreamingDiskANN/SBQ × HNSW/IVFFlat); ≥1 decisão por questão; todo claim de performance com metodologia+fonte ou `UNBENCHMARKED`; verdict `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS.

## Executive summary

The AlloyDB vector engine is **`alloydb_scann`**: a learned **k-means tree partition → anisotropic/PQ quantization → full-precision rescore** pipeline (the open **ScaNN** algorithm, Guo et al. ICML 2020), with PCA dimensionality reduction and a cost-based **adaptive filtered search** (pre/inline/post) wired into the planner. The AI layer is two closed extensions — **`google_ml_integration`** (in-SQL embeddings + `ai.*` generate/rank/filter) and **`alloydb_ai_nl`** (NL→SQL).

**No single permissive OSS piece reproduces "ScaNN-quality ANN as a Postgres index" today.** The two closest analogs take *different* algorithmic routes: `pgvector` (HNSW proximity graph + IVFFlat with **uncompressed** lists, **no quantization**) and `pgvectorscale` (**DiskANN/Vamana graph + Statistical Binary Quantization**, disk-resident, two-pass approximate-then-rescore). The hybrid-search layer (FTS + vector + RRF), by contrast, is **plain OSS SQL with no closed magic** — the cheapest win for TheoDB.

**Decision drivers it produces (detailed in § ADRs):** adopt `pgvectorscale` StreamingDiskANN as the M2 ANN index; the D3 fork trigger is **not yet justified** (no reproducible recall benchmark exists — neither in the analogs nor reachable for AlloyDB); the single highest-value gap to close locally is a **reproducible recall@k harness** (`docs/benchmarks/`), which is a prerequisite for *both* the M2 DoD and the D3 fork evidence.

---

## Coverage Corner 1 — Integration Tests

How the OSS analogs test the ANN boundary against a real Postgres (informs the M2/M7 test plan + `rules/testing.md` pyramid).

**Key honest finding:** **neither analog ships a numeric recall@k harness.** `grep -rln "recall"` over `pgvectorscale` hits only GUC description strings — the published "99% recall" numbers come from an external blog, not from an in-repo test. The committed tests validate **correctness** (exact row counts, distance invariants), **not recall**. → the M2 DoD "recall medido" is a **gap TheoDB must build itself**.

| Layer | What it asserts | Citation |
|---|---|---|
| `pgvectorscale` Rust `#[pg_test]` (PGRX, real PG) | index create/delete/vacuum/update over cosine/L2/IP + null handling | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/tests.rs` |
| `pgvectorscale` filtered tests | **row-count correctness** (`assert_eq!(2, … "Should find 2 documents with label 1")`), null/empty labels, update-labels-after-index | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/labels/filtering_tests.rs` |
| `pgvectorscale` Python integration | install ext, build diskann index, NN query validates **distance invariants** (cosine ∈ [0,2], returns K rows) — **not** vs brute-force ground truth ("relaxed ordering") | `.claude/knowledge-base/references/pgvectorscale/tests/test_basic_operations.py` |
| concurrency | multi-process concurrent inserts | `.claude/knowledge-base/references/pgvectorscale/tests/test_concurrent_inserts.py` |
| runner / oracle | `pg_isready` gate + pytest markers (`integration`/`concurrency`) | `.claude/knowledge-base/references/pgvectorscale/scripts/run-python-tests.sh`, `.claude/knowledge-base/references/pgvectorscale/TESTING.md` |
| `pgvector` recall infra | **opt-in compile macros only** (`HNSW_BENCH`/`IVFFLAT_BENCH`); recall "monitored by comparing approx vs exact" | `.claude/knowledge-base/references/pgvector/src/hnsw.h`, `.claude/knowledge-base/references/pgvector/src/ivfflat.h`, `.claude/knowledge-base/references/pgvector/README.md` |

**Recall measurement:** `UNBENCHMARKED` (no reproducible recall harness in either analog). TheoDB owns this for M2.

---

## Coverage Corner 2 — Dependencies

Runtime/build dependency chain — informs the D3 fork policy CI-of-rebase surface.

| Dependency | Version / constraint | Citation | Note |
|---|---|---|---|
| `pgrx` (Postgres-Rust framework) | **`=0.16.1`** (exact pin) | `.claude/knowledge-base/references/pgvectorscale/Cargo.toml` | The critical coupling; every PG-major or pgrx bump is the rebase break-point |
| PostgreSQL majors (pgvectorscale) | **14, 15, 16, 17, 18** (`default = ["pg18","build_parallel"]`) | `.claude/knowledge-base/references/pgvectorscale/Cargo.toml` | Confirms D5 (PG17→18) is viable for the ANN piece |
| Rust MSRV | **not declared** (no `rust-version`) → `BLOCKED` (field absent) | `.claude/knowledge-base/references/pgvectorscale/Cargo.toml` | Honest gap; CI must pin a toolchain itself |
| serialization / SIMD / misc | `rkyv 0.7.43`, `simdeez 1.0.8`, `rand 0.8`, `lru 0.14`, `criterion 0.5.1` (dev) | `.claude/knowledge-base/references/pgvectorscale/Cargo.toml` | rkyv = zero-copy page (means) serialization |
| `pgvector` build | PG 13+; PostgreSQL License; `META.json` + `Makefile` | `.claude/knowledge-base/references/pgvector/META.json`, `.claude/knowledge-base/references/pgvector/Makefile` | Baseline, C extension |
| license posture | pgvector = PostgreSQL License; pgvectorscale = PostgreSQL License | `references-catalog.md` | Both permissive → **D1-clean** for the distribution |

**Implication (D3):** the rebase CI must cover the matrix **PG 14–18 × pgrx =0.16.1**; the exact-pin on pgrx is the most brittle edge.

---

## Coverage Corner 3 — Tools

Build / dev / benchmark story — informs the reproducible-benchmark requirement (R3, `public-copy.md`, PRD D3).

- **Build = `cargo pgrx`** (pgvectorscale): install cargo-pgrx matched to the Cargo pgrx version → `cargo pgrx init --pgNN` → `cargo pgrx install --release` → `CREATE EXTENSION vectorscale CASCADE`. Citations: `.claude/knowledge-base/references/pgvectorscale/DEVELOPMENT.md`, `.claude/knowledge-base/references/pgvectorscale/Makefile`.
- **Tests:** `cargo pgrx test pgNN` (Rust `#[pg_test]`) + `make test-python`/`test-concurrency`/`test-integration`. Citation: `.claude/knowledge-base/references/pgvectorscale/TESTING.md`.
- **Benchmark = the gap.** The only `[[bench]]` targets are **criterion micro-benchmarks of distance + list-search-result** (`benches/distance.rs`, `benches/lsr.rs`) — they measure function latency, **not recall@k of the index**. There is **no reproducible recall harness**. Citation: `.claude/knowledge-base/references/pgvectorscale/Cargo.toml`.

**Conclusion:** the build pipeline is mature (`cargo pgrx`); the **reproducible recall benchmark must be built by TheoDB from scratch** — it is the prerequisite artifact for the M2 DoD and the D3 fork-trigger evidence. `UNBENCHMARKED` (no recall harness exists to inherit).

---

## Coverage Corner 4 — Techniques

### T1 — ScaNN (AlloyDB `alloydb_scann`) — 3-phase tree-quantization

A query runs **partition → quantize → rescore** (closed extension; reconstructed from published sources):

1. **Partition (learned k-means tree).** Vectors grouped into leaves; query prunes to nearby leaves. Tuning: `num_leaves`, `max_num_levels` (1=two-level default, 2=three-level), `num_leaves_to_search` (QPS↔recall dial). PCA applied first (`scann.enable_pca`; docs: "90% of information retained with 20% of dimensions").
2. **Quantize.** Three quantizers: **SQ8** (default scalar, "<1-2% recall loss"), **AH** (asymmetric hashing / PQ family, Preview, "up to 4x compression vs SQ8"), **FLAT** (no compression, "99%+ recall").
3. **Rescore.** Shortlist of PCA'd candidates re-ranked by **original full-precision vectors** (`scann.pre_reordering_num_neighbors`).

**What "anisotropic" means (the core idea):** ScaNN's quantization loss is **score-aware** — it "more greatly penalizes the parallel component of a datapoint's residual relative to its orthogonal component." For MIPS, parallel error distorts the inner product far more than orthogonal error; accepting orthogonal error preserves the top-K ranking. Anchored to Guo et al. ICML 2020 (`arxiv.org/abs/1908.10396`) + Google Research blog (`research.google/blog/announcing-scann-efficient-vector-similarity-search`).

**Benchmarks:** glove-100-angular — ScaNN serves "~2× QPS for a given accuracy as the next-fastest library" (ann-benchmarks suite; hardware unspecified → partially reproducible). AlloyDB-vs-HNSW vendor claims ("4× faster latency, 8× faster build, 3-4× smaller memory") = **`UNBENCHMARKED`** (no dataset/recall/method in the allowlisted source — do **not** repeat as fact, `public-copy.md`). The deep *"ScaNN for AlloyDB whitepaper"* is on `services.google.com` → **off-allowlist → BLOCKED** as a citable source.

### T2 — pgvectorscale StreamingDiskANN + SBQ (the closest OSS analog)

**Not ScaNN.** It is **DiskANN/Vamana graph + Statistical Binary Quantization** (README declares the lineage). Two-pass approximate-then-rescore — *functionally* equivalent to ScaNN's reorder, *algorithmically* different.

- **SBQ (`sbq/quantize.rs`):** bits packed in `u64`. **1 bit/dim** = `v > mean[i]` (threshold is the **trained per-dimension mean**, not zero); **2+ bits/dim** = **z-score** quantization `(v-mean)/std_dev` mapped to unary-coded bands. Means/std trained online via **Welford**. Default: 2 bits <900 dims, 1 bit otherwise. Distance over codes = Hamming/XOR. Citations: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs`, `.../sbq/mod.rs`, `.../meta_page.rs`.
- **Vamana graph (`graph/mod.rs`):** greedy-search build with `search_list_size`; **α-pruning** (keep edge unless a chosen neighbor is closer by factor α, α grows 1.0→`max_alpha` by ×1.2). Code TODO admits missing DiskANN `max_occlusion_size`. Citation: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/graph/mod.rs`.
- **Streaming disk traversal (`scan.rs`):** neighbors read page-by-page from disk (`GraphNeighborStore::Disk`), traversal distances use the **compressed SBQ** vector, finalists **resort** with the exact full vector. Citation: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/scan.rs`.
- **Tuning defaults:** `num_neighbors=50`, `search_list_size=100`, `max_alpha=1.2`, `query_search_list_size=100`, `query_rescore=50`. Citations: `.../access_method/options.rs`, `.../access_method/guc.rs`, README §StreamingDiskANN.
- **Perf:** README claims "28× lower p95 latency, 16× higher throughput vs Pinecone s1 @99% recall, 50M Cohere-768" → **`UNBENCHMARKED`** (third-party claim, no in-repo harness, dataset/hardware not replicable from refs).

### T3 — pgvector HNSW + IVFFlat (baseline)

- **HNSW** = Malkov & Yashunin proximity graph (paper cited in-repo, `arxiv.org/abs/1603.09320`). Build = Algorithm 1 (random exponential level `−log(rand)·ml`, greedy descent ef=1, per-layer ef=`ef_construction`, neighbor heuristic Algorithm 4 = keep candidate only if closer to q than to any selected neighbor, bidirectional links + shrink). Search = Algorithm 2 (two pairing-heaps, stop when nearest-candidate > furthest-result). Built in `maintenance_work_mem`, spills to disk with a user NOTICE. Params `m=16`, `ef_construction=64`, `ef_search=40`. Citations: `.claude/knowledge-base/references/pgvector/src/hnswutils.c`, `.../src/hnswbuild.c`, `.../src/hnswscan.c`, `.../src/hnsw.h`.
- **IVFFlat** = k-means inverted lists, **members stored uncompressed** (`IvfflatListData.center`, raw `IndexTuple`, **exact** distance at scan). Build: reservoir-sample `lists*50` rows → **k-means++ seeding + Elkan's algorithm** (triangle inequality, max 500 iters) → assign tuples. Params `lists=100`, `probes=1`. Citations: `.claude/knowledge-base/references/pgvector/src/ivfflat.c`, `.../src/ivfbuild.c`, `.../src/ivfkmeans.c`, `.../src/ivfscan.c`, `.../src/ivfflat.h`.
- **The structural gap that ScaNN/SBQ fill:** pgvector IVFFlat has **no quantization** — its list members are raw vectors (`ivfflat.h` `IvfflatListData`). That uncompressed-scan cost is exactly what ScaNN's anisotropic PQ and pgvectorscale's SBQ remove. This is the evidence feeding the M2 ANN choice + the D3 fork decision.
- **Perf:** HNSW-vs-IVFFlat comparisons are README author qualitative claims only → **`UNBENCHMARKED`**.

### T4 — Filtered vector search

- **AlloyDB:** **cost-based adaptive** pre/inline/post selection in the planner (needs a secondary index on the metadata column). Inline = a **Custom Scan ("vector scan")** consuming a **Bitmap Index Scan** (`EXPLAIN` shows "Bitmap assisted vector Scan"). Strategy flips dynamically with selectivity. Exact cost thresholds **not published → inferred (not confirmed)**. Sources: `cloud.google.com/alloydb/docs/ai/filtered-vector-search-overview`, `.../adaptive-filtering`.
- **pgvectorscale:** **in-filter via label-aware Filtered-DiskANN** (Microsoft Filtered DiskANN, cited `dl.acm.org/doi/10.1145/3543507.3583552`). Labels = `SMALLINT[]`, overlap operator `&&`; build prunes label-aware (`contains_intersection`), per-label start nodes, dual insert (filtered + default), in-traversal filter in scan. Arbitrary `WHERE` = post-filter. Limitation: no parallel build with labels. Citations: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/labels/`, README §Filtered Vector Search.
- **Gap:** no permissive OSS reproduces AlloyDB's **adaptive 3-way cost-based** planner choice. pgvector iterative scan is the nearest primitive `[NEEDS-VERIFY pgvector ≥0.8.0]`. Closing it = planner/custom-scan work (essential complexity).

### T5 — AI engine (`ai.*`) and hybrid search

| AlloyDB AI capability | Permissive OSS equivalent | Gap |
|---|---|---|
| In-SQL **multimodal** embeddings (`google_ml.embedding`) | pgai `[NEEDS-VERIFY license]` / app-side | Native multimodal + model registry unmatched |
| `ai.generate` / `ai.if` (semantic filter) | pgai / none native | **No permissive in-planner LLM filter operator** |
| `ai.rank` (semantic reranking) | none in-DB (external cross-encoder) | **No permissive in-DB reranker** |
| **Hybrid search (FTS + vector + RRF)** | **pure OSS SQL** (`tsvector`+`ts_rank_cd` + pgvector + RRF in CTE) | **No gap — easiest win.** pgvector ships only the vector half + the composition idiom; RRF/cross-encoder are *external example scripts*, not SQL functions (`.claude/knowledge-base/references/pgvector/README.md`) |
| **BM25 lexical** | — | **Hard gap.** Postgres `ts_rank_cd` is cover-density TF·IDF, **not BM25**; the obvious piece (ParadeDB `pg_search`) is **AGPL → barred by D1** |
| `alloydb_ai_nl` (NL→SQL) | none in-DB permissive | Real gap (app/middleware in OSS) |

All `ai.*` are Preview/Pre-GA; no latency/quality numbers published → **`UNBENCHMARKED`**.

---

## Cross-cutting Comparison

| Axis | AlloyDB `alloydb_scann` (closed) | pgvectorscale (PostgreSQL Lic.) | pgvector (PostgreSQL Lic.) |
|---|---|---|---|
| Structure | k-means **tree** partition | **DiskANN/Vamana graph** | HNSW graph / IVFFlat lists |
| Compression | **anisotropic PQ** (AH) / SQ8 / FLAT | **Statistical Binary Quantization** | **none** (IVFFlat raw); binary only via expr-index idiom |
| Disk-resident | yes (in-engine) | **yes (streaming)** | HNSW in-`maintenance_work_mem` (spills) |
| Rescore | full-precision reorder | full-vector resort | exact within probed lists (IVFFlat) |
| Filtered | **adaptive pre/inline/post (planner)** | in-filter label-aware (Filtered DiskANN) | iterative scan `[NEEDS-VERIFY]` |
| Recall benchmark | vendor `UNBENCHMARKED` | third-party `UNBENCHMARKED` | author-claim `UNBENCHMARKED` |

---

## Recommendations

Por research question, a decisão concreta proposta (detalhada nos ADRs abaixo):

1. **Índice ANN de M2 →** adotar `pgvectorscale` StreamingDiskANN+SBQ sobre `pgvector` (ADR D1). É o análogo OSS permissivo mais próximo do ScaNN (disk-resident, quantize+rescore).
2. **Gatilho de fork D3 →** **não forkar agora** (ADR D2). Sem benchmark reproduzível, forkar viola D3 e o anti-sunk-cost.
3. **Primeiro item de M2 →** construir um **harness de recall@k reproduzível** em `docs/benchmarks/` (ADR D3) — pré-requisito do M2 DoD e da evidência de fork. Nenhum análogo o fornece.
4. **Sequência de M7 →** hybrid search (FTS+vector+RRF) primeiro (puro OSS, sem gap); `ai.rank`/`ai.generate`/`ai.if`, BM25 e NL→SQL são gaps permissivos reais (ADR D4). BM25 via `pg_search` é AGPL → barrado (D1).
5. **Filtered search →** in-filter por labels (Filtered-DiskANN do pgvectorscale) cobre o caso de labels; o planner adaptativo pré/inline/pós do AlloyDB é gap → `[NEEDS-VERIFY]` pgvector iterative scan + trabalho de planner (essencial).

## ADRs (synthesized decisions)

### D1 — M2 ANN index = pgvectorscale StreamingDiskANN (compose, don't reinvent)

**Decision:** adopt `pgvectorscale` StreamingDiskANN+SBQ as the M2 "ANN beyond HNSW", on top of `pgvector`.
**Rationale:** it is the closest permissive analog to ScaNN's disk-resident quantize+rescore design; PostgreSQL-License (D1-clean); supports PG14–18 (D5-compatible); `cargo pgrx` build is mature. Reinventing ScaNN-as-an-access-method is essential-complexity-high and not justified before evidence (`parsimony-ladder.md`, Rule 9; `CLAUDE.md` "Esforço ≠ Complexidade").
**Alternatives:** wrap the Apache-2.0 ScaNN C++ library as a PG access method (high effort, deferred until benchmark evidence demands it); pgvector HNSW only (insufficient — no quantization, the IVFFlat raw-list gap).
**Consequence:** TheoDB inherits a *different* algorithm than AlloyDB (Vamana+SBQ vs tree+anisotropic-PQ) — honest divergence to document (`public-copy.md`).

### D2 — D3 fork trigger is NOT yet justified

**Decision:** do **not** fork `pgvector`/`pgvectorscale` now. Keep upstream-as-is (D3 upstream-first).
**Rationale:** PRD D3 authorizes a fork only on **reproducible-benchmark evidence**. No such benchmark exists — neither analog ships a recall harness, and AlloyDB's numbers are off-allowlist/`UNBENCHMARKED`. Forking without the gating benchmark violates D3 and the anti-sunk-cost rule.
**Consequence:** the fork decision is **blocked on building the recall harness** (D3 below). This is the honest state, not a deferral of convenience.

### D3 — Build a reproducible recall@k harness (the prerequisite artifact)

**Decision:** TheoDB builds its own recall@k + latency harness in `docs/benchmarks/` (ground-truth brute-force vs ANN), as the first M2 work item.
**Rationale:** it is the missing piece for **both** the M2 DoD ("recall medido + benchmark reproduzível") **and** the D3 fork-trigger evidence. The analogs only have correctness tests + criterion micro-benchmarks. Required by `public-copy.md` (no perf claim without it) and PRD D3.
**Consequence:** unblocks D2 (fork decision) and every future M2/M7 performance claim.

### D4 — M7 sequencing: hybrid-search first (easy win), reranker/NL→SQL flagged as real gaps

**Decision:** in M7, ship **hybrid search (FTS+vector+RRF)** first — it is pure OSS SQL with no closed dependency. Treat `ai.rank` reranking, `ai.generate`/`ai.if`, BM25, and NL→SQL as **real permissive gaps** requiring external model-serving or middleware (already reflected in the expanded M7 DoD + `docs/features/`).
**Rationale:** hybrid is the lowest-effort, highest-certainty M7 deliverable; the rest carry license/architecture gaps (BM25 = AGPL `pg_search` barred by D1).
**Consequence:** M7 plan should slot hybrid as the first slice; reranker/NL→SQL get their own discovery before implementation.

---

## Blocked questions / honesty register

| Flag | Item | Reason |
|---|---|---|
| `BLOCKED` | ScaNN-for-AlloyDB whitepaper | hosted on `services.google.com` — off-allowlist (R5) |
| `BLOCKED` | AlloyDB recall@k vs HNSW (reproducible) | no allowlisted source publishes dataset+method |
| `UNBENCHMARKED` | all AlloyDB-vs-HNSW perf ratios; SBQ "28×/16×"; HNSW-vs-IVFFlat | no reproducible harness in refs / no method in vendor sources |
| `inferred (not confirmed)` | AlloyDB filtered-search cost thresholds; "proxy models" internals | qualitative-only in docs |
| `[NEEDS-VERIFY]` | pgvector ≥0.8.0 iterative-scan as adaptive-filter primitive; pgai license | not fetched this pass — verify before relying |
| `BLOCKED` | Rust MSRV of pgvectorscale | `rust-version` field absent in `Cargo.toml` |

---

## References

**OSS analogs (in `.claude/knowledge-base/references/`):** `pgvectorscale/` (`Cargo.toml`, `DEVELOPMENT.md`, `Makefile`, `TESTING.md`, `README.md`, `pgvectorscale/src/access_method/{sbq/,graph/,labels/,scan.rs,storage.rs,options.rs,guc.rs,meta_page.rs}`, `tests/`, `scripts/`); `pgvector/` (`README.md`, `META.json`, `Makefile`, `src/{hnswbuild.c,hnsw.c,hnswutils.c,hnswscan.c,hnsw.h,ivfflat.c,ivfbuild.c,ivfkmeans.c,ivfscan.c,ivfflat.h}`).

**Primary literature + SOTA docs (allowlist):**
- Guo et al., *Accelerating Large-Scale Inference with Anisotropic Vector Quantization*, ICML 2020 — `arxiv.org/abs/1908.10396`
- Malkov & Yashunin, *HNSW* — `arxiv.org/abs/1603.09320` (cited in-repo by pgvector)
- Microsoft *Filtered DiskANN* — `dl.acm.org/doi/10.1145/3543507.3583552` (cited in-repo by pgvectorscale)
- Google Research, *Announcing ScaNN* — `research.google/blog/announcing-scann-efficient-vector-similarity-search`
- AlloyDB ScaNN index — `cloud.google.com/alloydb/docs/ai/create-scann-index`, `.../blog/.../understanding-the-scann-index-in-alloydb`
- AlloyDB filtered search — `cloud.google.com/alloydb/docs/ai/filtered-vector-search-overview`, `.../adaptive-filtering`
- AlloyDB AI / `ai.*` / hybrid — `cloud.google.com/alloydb/docs/ai`, `.../ai/rank-rerank-search-results-rag`, `.../ai/run-hybrid-vector-similarity-search`
- ScaNN library (Apache 2.0) — `github.com/google-research/google-research/tree/master/scann`

**Project rules consumed:** `discover-phd-rigor.md` (R1–R6), `public-copy.md` (no perf claim sans benchmark), `parsimony-ladder.md` (Rule 9), PRD **D1/D3/D5**, `architecture.md` (extension behind interface — DIP).
