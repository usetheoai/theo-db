# Deep-Research Dossier — Single-Planner In-Postgres Columnar + Vectorized Execution + AI (AlloyDB-class HTAP)

**Date:** 2026-07-14 · **Purpose:** collect the 2026 SOTA sources (papers + OSS, permissive AND non-permissive for
inspiration) BEFORE the `/discover-plan`, per the owner's directive. **Method:** 3 parallel deep-research councils
(execution / AI-over-columnar / storage), R0 web-grounded (GitHub Licenses API `spdx_id`, arXiv, venue PDFs, vendor
docs). **Discipline:** M73/M97 — every claim cited; licenses verified; unresolved items flagged, never fabricated.

## The bet (owner directive, time is NOT a constraint)

Match AlloyDB's model — **one engine, one planner, columnar in-core, auto-maintained** — so `ai.generate()` /
`embedding()` / semantic operators + vector search + columnar aggregation compose in a **single query plan**. This
re-opens the M97 DEFER (ADR-0041) with a **genuinely new route ADR-0041 did NOT examine**: a **DataFusion-vectorized
`CustomScan` single-planner** path (ADR-0041 only looked at pg_duckdb / pg_mooncake / Hydra / Citus, all judged
two-engine or AGPL/BSL).

## Headline findings (source-verified)

1. **The permissive vectorized-executor route EXISTS and is single-planner.** **Apache DataFusion** (Apache-2.0, Rust,
   Arrow-native) driven by a **Postgres `CustomScan` node** batching heap tuples into Arrow `RecordBatch` — this is
   **one planner** (PG plans; the CustomScan executes vectorized), not the two-planner ceiling of pg_duckdb
   (ADR-0023's `ERROR: DuckDB execution is not supported inside functions`). **ParadeDB `pg_search` is a LIVE
   reference implementation** of exactly this (`postgres/customscan/` + `scan/execution_plan.rs`, 38 DataFusion
   sites) — but it is **AGPL-3.0 → STUDY-ONLY** (design gold, code cannot ship). **TheoDB already owns the seam:**
   `theodb_rs/src/am/customscan.rs` (the M94/M95 vecfilter `CustomScan` + `set_rel_pathlist_hook`).
2. **ADR-0041 factual correction: Hydra columnar is Apache-2.0** (`gh api repos/hydradatabase/columnar` →
   `Apache-2.0`; the local clone's top-level LICENSE is Apache 2.0 — the AGPL was a stale inner `columnar/LICENSE`).
   So a **permissive Postgres-native columnar Table Access Method exists** (Citus-columnar lineage: stripe 150k →
   chunk 10k → per-column, zstd/lz4/pglz + min/max skip). M97 wrongly barred it.
3. **AI operators must be PLAN NODES, not `pg_extern` functions.** Our AI surface (`ai.generate`, `nl_to_sql`,
   `hybrid_search_rrf`, `embed`, `rerank`) matches AlloyDB at the FUNCTION level but every op is a black box the
   planner cannot cost / reorder / batch / fuse with a scan. The 2024-2026 research wave (LOTUS Apache-2.0,
   Palimpzest MIT, DocETL MIT) formalizes **semantic operators as an optimizable relational algebra**
   (`sem_filter`/`sem_join`/`sem_topk`/`sem_agg`). AlloyDB/BigQuery ship them as SQL-embeddable ops
   (`AI.GENERATE`, `AI.IF`, `AI.GENERATE_BOOL`). The gap is **planner-integration** — and it reuses the SAME
   `customscan.rs` seam.
4. **Storage: heap-authoritative Arrow columnar CACHE (AlloyDB model) is the MVCC-correct permissive sweet spot** —
   NOT a native columnar TAM as source-of-truth. The heap stays authoritative (MVCC correct by construction);
   columnar is a derived, in-memory, workload-populated Arrow structure DataFusion reads zero-copy. A **native
   append-only columnar TAM (Hydra-parity, Apache-2.0)** is the lower-risk FIRST milestone for analytical tables
   that don't need in-place updates. **Full updatable MVCC-columnar-as-truth is UNSHIPPED permissively** (zedstore
   never merged; Hydra/Citus are append-optimized) — do NOT scope it early.
5. **Vector + columnar unified: Lance (Apache-2.0)** stores vector indexes (IVF/HNSW) natively in a columnar format
   alongside scalar columns — the closest permissive prior art for "vector search + analytics share one substrate".
   STUDY the format (it's a file format, not a PG TAM → lakehouse-style integration).

## The honest ceiling (M73/M97 discipline)

- **Realistic ceiling = DuckDB/Photon-class 15–30× on columnar-RESIDENT data** (M97 measured DuckDB 15–23×; Photon
  SIGMOD 2022 documents the vectorization gain). **Match AlloyDB's capability, NEVER claim superiority** over its
  in-core in-memory auto-tuned engine — same paradigm ceiling as the vector pillar (M73).
- **Batch advantage requires columnar-resident data** — M61 measured vectorized-over-row-heap LOSES (0.63–0.89×).
- **Own-code glue, not adoption.** DataFusion/Arrow/Hydra are GO dependencies; the CustomScan↔Arrow↔DataFusion glue
  (batching scan, pushdown planner hooks, qual/agg→DataFusion `Expr` translation) is what ParadeDB spent years on and
  is AGPL — TheoDB builds it own-code. **Multi-milestone engine effort — "Esforço ≠ Complexidade": high effort
  welcome because the North Star (igualar/superar AlloyDB) justifies it; the COMO stays parsimonious.**
- **Batched inference conflict:** composing AI with a vectorized scan for real throughput REQUIRES batched model
  inference over a columnar stripe — our per-row synchronous HTTP model (ADR-0007) forfeits it. **A new ADR must
  revisit ADR-0007** or the columnar speedup is thrown away one round-trip at a time.
- **Market signal:** ParadeDB ARCHIVED the permissive `pg_analytics` (2025-03) and moved analytics behind AGPL
  `pg_search` — the sustainable version is hard enough its inventor put it behind a copyleft moat. Honest risk.

## Source table (collected 2026-07-14 · licenses `gh api` `spdx_id`-verified)

### Permissive — CLONED into `.claude/knowledge-base/references/` (D1-clean: reference + potential reuse)

| Source | License | Role | Path |
|---|---|---|---|
| **apache/datafusion** | Apache-2.0 | THE vectorized executor (Arrow-native, Rust) | `references/datafusion/` |
| **apache/arrow-rs** | Apache-2.0 | Arrow `RecordBatch` — the batch unit the CustomScan fills | `references/arrow-rs/` |
| **hydradatabase (columnar)** | Apache-2.0 | the permissive Postgres columnar TAM (stripe/chunk/skip) | `references/hydra/columnar/` |
| **lancedb/lance** | Apache-2.0 | columnar format w/ native vector indexes (vector+columnar unified) | `references/lance/` |
| **lotus-data/lotus** | Apache-2.0 | semantic-operator algebra (sem_filter/join/topk/agg) | `references/lotus/` |
| **mitdbg/palimpzest** | MIT | cost-based optimizer for AI/LLM operators | `references/palimpzest/` |
| **postgresml/postgresml** | MIT | batched in-Postgres model inference (Rust/pgrx) | `references/postgresml/` |
| **citusdata/cstore_fdw** | Apache-2.0 | columnar ancestor (FDW-era design; stripe/chunk/ORC) | `references/cstore_fdw/` |
| **ucbepic/docetl** | MIT | agentic operator-rewrite rules | *(catalog; clone on demand)* |
| **duckdb/duckdb** | MIT | embeddable vectorized engine (two-planner — the shipped route) | `references/duckdb/` |

### Study-only — DESIGN inspiration, NEVER copy code (D1 bars distribution)

| Source | License | Why study | Path |
|---|---|---|---|
| **paradedb/paradedb (`pg_search`)** | AGPL-3.0 | THE reference impl of CustomScan+DataFusion single-planner | `references/paradedb/` |
| **citusdata/citus (columnar)** | AGPL-3.0 | the canonical columnar-TAM design (Hydra is its Apache-2.0 twin) | `references/citus/` |
| **Mooncake-Labs/pg_mooncake + moonlink** | MIT shell / **BSL 1.1** engine | Iceberg/Parquet columnstore + auto-sync (moonlink barred) | `references/pg_mooncake/` |
| **AlloyDB** (columnar engine + AI) | Proprietary (Google) | the architecture bar (heap-authoritative Arrow cache, auto-populate, planner-choose) | docs only |

### Key papers (venue/DOI; ≥2 primary sources per major claim)

- Morsel-Driven Parallelism — Leis et al., **SIGMOD 2014**, DOI `10.1145/2588555.2610507` (batch-at-a-time theory).
- Photon — Behm et al., **SIGMOD 2022**, DOI `10.1145/3514221.3526054` (15–30× vectorization gain, interpreted-vs-compiled).
- Velox — Pedreira et al., **PVLDB 15** (VLDB 2022), `p3372-pedreira` (unified vectorized execution library).
- Apache DataFusion — Lamb et al., **SIGMOD 2024 Companion**, DOI `10.1145/3626246.3653368` (embeddable extension points).
- AlloyDB — **SIGMOD 2024**, DOI `10.1145/3626246.3653369` (heap-authoritative in-memory columnar engine).
- LOTUS semantic operators — Patel/Guestrin et al., **arXiv 2407.11418** (Apache-2.0 repo `lotus-data/lotus`).
- Palimpzest — MIT DBG, **CIDR 2025 / arXiv 2405.14696** (AI-op cost optimizer, MIT repo).
- DocETL — UC Berkeley, **arXiv 2410.12189** (agentic rewrite, MIT repo).
- Umbra/CedarDB — Neumann/Kemper, **CIDR 2020** (compiling-vs-vectorizing; TheoDB lands vectorized).

### Honestly-flagged unresolved (Rule 3 — not fabricated)

- "ELEET" table-LLM paper: no trustworthy arXiv ID resolved → excluded pending a real lookup.
- Google docs + ACM/VLDB pages are JS-rendered; URLs confirmed to resolve (200) but body not scraped — AlloyDB
  architecture corroborated across the docs URL + the SIGMOD 2024 DOI, not a fetched quote.
- WebSearch/WebFetch were unavailable in the council envs; `curl` against GitHub Licenses API + venue PDFs was the
  substitute (stronger for license verification). A `/discover-execute` with live WebFetch should re-confirm the
  arXiv IDs before they enter a plan.

## Honest scope ladder (feeds the roadmap milestones)

1. **M-α (low risk):** append-only native columnar TAM (Hydra-parity, Apache-2.0) for analytical tables — proves
   the permissive columnar-storage envelope; MVCC = insert-visibility only.
2. **M-β:** DataFusion `CustomScan` executor over the columnar/Arrow batch — the single-planner vectorized scan
   (measure vs pg_duckdb, vs row-heap; honest 15–30× ceiling on columnar-resident data).
3. **M-γ:** heap-authoritative Arrow columnar CACHE (AlloyDB model) — MVCC-correct HTAP over live heap; manual
   "columnarize these columns" pragma first (auto-populate/evict tuner is the ambitious tail).
4. **M-δ:** AI operators as plan nodes (`AI.IF`/`sem_filter` as a pushable predicate over the vectorized scan) —
   requires the batched-inference ADR (revisit ADR-0007). Reuses the `customscan.rs` seam.
5. **M-ε:** vector + columnar shared substrate (Lance-inspired) — filtered vector + analytics in one plan.
6. **Cross-cutting:** a decision ADR that (i) **supersedes/amends ADR-0041** (Hydra is Apache-2.0; the
   DataFusion-CustomScan route was unexamined), (ii) locks the honest ceiling (capability-match, never superiority),
   (iii) the security review of any NL→SQL/AI-operator surface (council-security).

## Next step (owner sequence)

Sources are COLLECTED. Proceed to **`/discover-plan`** on this topic (single-planner columnar+AI) reading these
references, then `/discover-edge-cases` → `/discover-plan-confidence` → `/discover-execute` → a multi-milestone
roadmap via `/roadmap-feature` (one milestone per scope-ladder rung). Every performance claim gates on a
`docs/benchmarks/` artifact (Rule 5); the honest ceiling (DuckDB/Photon-class, not AlloyDB-superior) is locked from
the start.
