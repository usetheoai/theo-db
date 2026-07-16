# Blueprint — Native graph engine (CSR + vectorized BFS + SQL/PGQ) fused with columnar + vector + AI-in-SQL (M107 Phase 0)

**Cycle:** discover · **Milestone:** M107 · **Date:** 2026-07-16 · **Verdict target:** SHIPPABLE_WITH_CAVEATS

The question this blueprint answers: **for an AI-native database where graph is a first-class, frequent, cross-system capability, what is the most EFFICIENT architecture — and does native-graph-over-columnar actually beat the recursive-CTE baseline (theo-rag) enough to justify building the engine?** Phase 0 is a measurement-first gate: this blueprint + a reproducible spike (M107 implement) produce a D3 GO / honest-partial / honest-negative verdict.

## Context & SOTA anchor

TheoDB is stock PostgreSQL 17 + a pgrx extension, already carrying THREE of the four ingredients the SOTA graph+vector engines need: a **columnar analytical engine** (DataFusion/Arrow, M99–M103), an **own vector AM with SIMD kernels** (`vec/ah.rs` FastScan), and **AI-in-SQL** (`ai.*`). The missing ingredient is **native graph traversal**. The SOTA convergence (DuckPGQ, Kùzu) is decisive: efficient graph+vector lives in ONE engine that pairs **columnar storage + CSR adjacency + vectorized traversal (MS-BFS) + WCOJ/factorization**, exposed via **SQL/PGQ (SQL:2023)**. The GraphRAG retrieval pattern (LazyGraphRAG/HippoRAG) is **vector-entry → bounded traversal → rerank/PPR**, which runs zero-copy only when vector + graph + columnar + LLM live together.

## Coverage Corner 1 — Integration Tests

The spike + eventual engine must be proven, not asserted:

- **Correctness oracle** — the CSR+BFS traversal MUST return the SAME reachable-set / scored-chunks as the recursive-CTE baseline on identical graphs (a differential test: native result == CTE result for the same seeds + hop budget). This is the load-bearing integration test — a faster-but-wrong traversal is worthless.
- **Multi-hop neighborhood expansion** — seed entities → 1/2/3-hop reachable set, scored by summed edge weight (the theo-rag `graph-retriever.ts` semantics), asserted equal across engines.
- **Bounded-budget honored** — hop budget ∈ [1,3] clamp respected; frontier BFS stops at the budget exactly like the CTE `WHERE hop < budget`.
- **Vector-entry integration** — the entry-point set comes from a vector similarity search on node embeddings (HippoRAG pattern); an integration test asserts the vector→graph handoff returns entities whose embeddings are the nearest to the query.
- **Scale sweep** — the same test at 10⁵ and 10⁶ edges to expose the growth curve (the D3 evidence), not a single point.

## Coverage Corner 2 — Dependencies

- **No new heavy dependency for the spike** (Rule 9 + parsimony): CSR + frontier BFS is own-code Rust over the existing edge representation; the vectorized kernels reuse the SIMD already in `vec/ah.rs`. The recursive-CTE baseline is plain PostgreSQL (already the substrate).
- **Rejected dependency — Apache AGE** (Apache-2.0, would pass D1): architecturally it compiles Cypher to recursive relational joins over `agtype` tables — the SAME per-hop-join tax as the CTE baseline, plus a separate query silo, plus "not available on managed services" (RDS/Aurora). Rejected on architecture, not license.
- **Rejected dependency — Kùzu / DuckPGQ as a bundled engine**: both are MIT and excellent, but both are SEPARATE single-node engines (DuckDB / Kùzu core) — bundling one forks the storage substrate away from TheoDB's columnar+vector. We adopt their *techniques* (CSR, MS-BFS, WCOJ, factorization — the design is public in the CIDR/VLDB papers) as own-code over TheoDB's engine (the GRFusion lesson: native traversal operators OVER the existing relational/columnar storage, not a new store).
- **Standard PG features** (inherited, no dependency): recursive CTE (baseline), arrays/`unnest`, `pg_advisory_xact_lock`, JSONB.

## Coverage Corner 3 — Tools

- **Benchmark harness** — a reproducible Rust spike (`benchmarks/` or a standalone crate) that: generates a deterministic synthetic graph (seeded RNG, power-law-ish degree to mimic an entity graph), builds CSR, runs frontier BFS, AND runs the recursive-CTE baseline against a real PostgreSQL over the same edge table, ≥3 runs mean±std, emits `docs/benchmarks/m107-graph-spike.{md,json}`. ZERO fabricated numbers (Rule 5).
- **Profiling** — wall-clock per phase (CSR build vs traversal) — critical because the DuckPGQ authors measured that **on-the-fly CSR construction can DOMINATE end-to-end runtime**; the spike MUST separate build-time from traverse-time so the persist-vs-rebuild ADR is evidence-based.
- **Statistical rigor** — mean ± std over ≥3 runs, at 10⁵ and 10⁶ edges; report the crossover (if any) where native pulls ahead of the CTE.
- **Existing SIMD** — the MS-BFS visited-set can reuse bitset/SIMD ideas already in the vector kernels (AVX512 tracks many searches per register — the DuckPGQ MS-BFS insight).

## Coverage Corner 4 — Techniques

- **CSR (Compressed Sparse Row) adjacency** — two-array layout: a vertex array indexing the start of each node's outgoing edges + an edge array of destination node ids. "All neighbors of node v" = a sequential scan `edge[vertex[v]..vertex[v+1]]`, cache-friendly, O(1) offset — vs the recursive-CTE's per-hop hash join. (DuckPGQ `create_csr_vertex`/`create_csr_edge`; USPTO 11,093,459 on main-memory CSR-in-RDBMS.)
- **MS-BFS (Multi-Source vectorized BFS)** — run many seed BFS simultaneously; a bitset per frontier tracks visited across sources, so one AVX512 register advances up to 512 searches; bulk sequential access through the CSR. Ideal for GraphRAG's "expand from N entry entities at once."
- **Worst-Case-Optimal Joins (WCOJ) + factorization** — for multi-hop *pattern* matching (paths with predicates), factorize intermediate results (represent the cartesian product compressed, materialize only what's needed) — the Kùzu/DuckPGQ technique that avoids the CTE's intermediate-result blow-up. (Scope for later phases; the Phase-0 spike targets neighborhood expansion, the dominant GraphRAG op.)
- **SQL/PGQ (SQL:2023)** — the standard property-graph sublanguage (Cypher-inspired `()-[]->()` patterns, `SHORTEST`/bounded paths, `ELEMENT_ID`), so graph queries compose with SQL + vector + `ai.*` in one statement — no separate Cypher silo. Surface decision (SQL/PGQ vs `graph_*` functions) is an ADR below.
- **Vector-entry → bounded-traversal → rerank/PPR (the GraphRAG retrieval flow)** — HippoRAG: encode query → cosine-nearest **entity nodes** (vector index on nodes) become the **personalization seeds** → **Personalized PageRank** biases mass to the relevant neighborhood → score passages. "In-database coupling" (unify vector + PPR in one engine) is the efficiency win vs the multi-system split (AWS Neptune + Neptune Analytics + Titan). Synonymy edges (cosine > 0.8) enrich the graph from the same embeddings.
- **On-the-fly vs persisted CSR** — the decisive perf ADR: DuckPGQ rebuilds CSR per query (can dominate runtime); a DB where the graph is stable across queries should PERSIST the CSR (an index-AM over the edge table, rebuilt on VACUUM/incremental) — the spike measures build-time to decide.

## ADRs

### ADR-1 — Native traversal operators over the existing columnar/vector storage (NOT a bundled graph engine, NOT recursive-CTE, NOT Apache AGE)
**Decision:** build own-code CSR adjacency + vectorized frontier/MS-BFS as first-class operators fused with TheoDB's columnar (M99–M103) + vector AM, adopting the DuckPGQ/Kùzu *techniques* (public papers) — not vendoring their engines.
**Alternatives:** (a) recursive-CTE — the baseline; per-hop hash join, intermediate blow-up (rejected: the inefficiency we're replacing). (b) Apache AGE — Cypher-on-relational-joins, same per-hop tax + separate silo + no managed-service support (rejected on architecture). (c) bundle Kùzu/DuckPGQ — forks the storage substrate away from the columnar+vector we already own (rejected: Rule 9 says reuse OUR engine; GRFusion shows native operators over existing storage win).
**Rationale:** GRFusion measured native-graph-views-over-relational beating native graph DBs by 3+ orders of magnitude — the gap is the EXECUTION model, not storage. TheoDB already owns the columnar+vector+SIMD substrate; the missing piece is the traversal operator.

### ADR-2 — Measurement-first gate before the engine (D3 / anti-sunk-cost)
**Decision:** M107 ships a blueprint + a reproducible spike + a GO/honest-partial/honest-negative verdict, NOT the engine. The engine phases are gated on GO.
**Alternatives:** build the engine directly (rejected: anti-sunk-cost — a whole graph engine is a multi-milestone pillar; prove the win first, exactly as the vector pillar's M75 spike gated M76–M82). honest-negative (CTE is enough at our scale) is a valid outcome that closes the pillar cheaply.

### ADR-3 — SQL/PGQ as the eventual surface, `graph_*`/`graph_expand` functions as the Phase-0/1 pragmatic surface
**Decision:** target SQL/PGQ (SQL:2023) as the long-term standards-based surface, but the spike + first engine phase expose a pragmatic `theodb.graph_expand(seeds, max_hops)` + `ai.extract_graph` so theo-rag can adopt immediately without a parser.
**Alternatives:** full SQL/PGQ parser first (rejected: huge, YAGNI for Phase 0 — the parser is a later phase); Cypher (rejected: SQL/PGQ is the STANDARD and composes with SQL).

## Prior Art & Related Work (references — no fabricated citations)

- DuckPGQ — [CIDR 2023 p66](https://www.cidrdb.org/cidr2023/papers/p66-wolde.pdf), [VLDB vol16 p4034](https://www.vldb.org/pvldb/vol16/p4034-wolde.pdf) — CSR + MS-BFS + SQL/PGQ over a columnar RDBMS; "outperforms all graph DBs tested".
- Kùzu — [CIDR 2023 p48](https://www.cidrdb.org/cidr2023/papers/p48-jin.pdf) — columnar + CSR + WCOJ + factorization + vector/FTS; "largest audience in AI / GraphRAG".
- WCOJ — [A Unified Architecture for Binary and WCOJ Processing, arXiv 2505.19918](https://arxiv.org/pdf/2505.19918); [Free Join, arXiv 2301.10841](https://arxiv.org/pdf/2301.10841).
- GRFusion — [Empowering In-Memory Relational Engines with Native Graph Processing, arXiv 1709.06715](https://arxiv.org/pdf/1709.06715) — native graph views over relational storage beat native graph DBs (the execution-model lesson).
- SQL/PGQ — [Towards Cross-Model Efficiency in SQL/PGQ, arXiv 2505.07595](https://arxiv.org/pdf/2505.07595); SQL:2023 standard.
- Apache AGE — [architecture / FAQ](https://age.apache.org/faq/); [Joins vs AGE traversals](https://medium.com/@sjksingh/postgresql-showdown-complex-joins-vs-native-graph-traversals-with-apache-age-78d65f2fbdaa).
- Microsoft GraphRAG / LazyGraphRAG — [LazyGraphRAG (700× cheaper global)](https://www.microsoft.com/en-us/research/blog/lazygraphrag-setting-a-new-standard-for-quality-and-cost/); [When to use Graphs in RAG, arXiv 2506.05690](https://arxiv.org/html/2506.05690v3).
- HippoRAG / HippoRAG 2 — [OSU-NLP-Group/HippoRAG](https://github.com/osu-nlp-group/hipporag); [in-database PPR coupling (Graphwise)](https://graphwise.ai/blog/from-retrieval-to-reasoning-enhancing-hipporag-with-graph-based-semantics/) — vector-entry → PPR seeds; synonymy edges.
- Internal: baseline to beat = `theo-rag/packages/core/src/domain/retrievers/graph-retriever.ts` (recursive-CTE); reuse targets = `theodb_rs/src/am/df_executor.rs` (columnar), `vec/ah.rs` (SIMD), `am/mod.rs` (vector AM).

## Drawbacks & Risks

- **Graph quality ≠ traversal speed** — GraphRAG gains hinge on extraction quality (entity/edge coverage + correctness). A fast engine over a noisy graph amplifies errors. The pillar must scope `ai.extract_graph` quality, not just the traversal (blueprint Corner 1 correctness oracle covers traversal; extraction quality is a separate eval).
- **On-the-fly CSR build can dominate** — if the CSR is rebuilt per query, build-time may swamp the traversal win (DuckPGQ measured this). Mitigation: the spike separates build vs traverse; the engine persists CSR as an index-AM.
- **Scope explosion** — a full graph query language is a multi-year trap. Mitigation: Phase 0 is bounded to the neighborhood-expansion primitive + the GraphRAG flow; SQL/PGQ conformance is deferred.
- **honest-negative is real** — at TheoDB's target corpus scale (RAG entity graphs are often ≤10⁶ edges, bounded ≤3 hops), the recursive CTE may be "good enough". The D3 gate must be honest: if native doesn't beat CTE meaningfully, the pillar closes and we ship `graph_expand` helpers over CTE instead.

## Unresolved Questions

- Persisted-CSR index-AM vs materialized-view vs on-the-fly — decided by the spike's build-vs-traverse split (Phase 1).
- PPR in-engine (columnar iterative) vs bounded-BFS scoring — which the GraphRAG flow actually needs (HippoRAG uses PPR; theo-rag uses summed-edge-weight BFS) — an eval question for Phase 1.
- SQL/PGQ parser effort vs `graph_*` functions — surface decision deferred to Phase 1 per ADR-3.
