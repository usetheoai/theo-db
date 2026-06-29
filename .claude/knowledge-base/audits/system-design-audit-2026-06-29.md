# System Design Audit Report — TheoDB

> Staff-level system design analysis applying distributed-system principles at the
> codebase level. Target: an open-source, PostgreSQL-compatible database (pgrx Rust
> extension + SQL umbrella extension) positioned against AlloyDB.

- **Date:** 2026-06-29
- **Target:** `/home/paulo/Projetos/usetheo/theo-data/theo-db`
- **Mode:** full (all 5 dimensions) · severity threshold: medium
- **Figures:** `figures/dimension_scores.svg`, `figures/severity_distribution.svg`, `figures/data_flow_map.svg`
- **ADR drafts:** `adr-drafts/adr-A-synchronous-per-row-model-http.md`, `adr-drafts/adr-B-no-embedding-chat-cache.md`

## Executive Summary

TheoDB is a small, young (~8.8K LOC), **measurement-first** codebase with unusually
strong decision hygiene: 6 LOCKED ADRs, a clean M17 three-boundary Rust split
(pgrx glue ← embed domain ← api surface, DIP-correct), SSRF-guarded typed-error
outbound I/O, `REVOKE … FROM PUBLIC` on every privileged surface, pgvector HNSW +
pgvectorscale StreamingDiskANN as the declared ANN path, and **no premature
distribution** (the only distributed topology is a textbook opt-in Patroni HA stack).
Boundaries, trade-offs and deletion hygiene are all in good shape for the codebase's
age and size.

The single most important thing to fix is the **embedding N+1**: `theodb.embed` is a
per-row synchronous UDF with **no batch entry point**, while the chat path already ships
`ai.generate_batch` (N prompts → 1 round-trip). So the most common bulk operation
(embed a corpus) is the worst-scaling one — `SELECT theodb.embed(content) FROM big_table`
fans out to one blocking HTTP round-trip per row, each holding a PostgreSQL backend for
up to 30s. The fix is low-effort and high-leverage: add `theodb.embed_batch(text[])`
mirroring `ai.generate_batch` (the embeddings endpoint already accepts an input array).

**Overall Score: 3.2 / 5.0**

| Dimension | Score | Assessment |
|---|---|---|
| Boundaries & DDD | 4/5 | DIP-clean Rust 3-boundary split + package-by-capability SQL surface; 1 medium cross-extension runtime seam with no enforced `pg_depend` edge. |
| Data Flow & State | 3/5 | 6 flows traced; SSRF-guarded, typed, REVOKE-bounded; but per-row fan-out has no backpressure, no retry/backoff, and bulk import is unbounded. |
| Scaling Readiness | 2/5 | One critical gap (embed N+1 / no batch path) + one high (blocking I/O holds backend); strong vector-index story and no premature distribution. |
| Deletion Safety | 3/5 | Clean GUC config + `ALTER EXTENSION UPDATE` versioning, no zombie deps; but a silent-drop coupling and a missing plpython3u-embed retirement migration. |
| Trade-offs & Pragmatism | 4/5 | 6 LOCKED ADRs, measurement-first, explicit YAGNI; 2 high-impact decisions (sync-per-row, no cache) documented only in code comments. |

### Top 3 Risks

1. **Embedding N+1 / no batch path** (`sql/theodb--1.0.sql:90`, owner: `sql_surface` + `theodb_rs`) — bulk embed = N synchronous HTTP round-trips; asymmetric with the chat path that ships `ai.generate_batch`.
2. **`theodb_rs` ↔ `theodb` transition coupling** (`sql/40-theodb-hybrid.sql:62`) — `ai.hybrid_search_rrf` calls `theodb.embed` at runtime with no `pg_depend` edge; `DROP EXTENSION theodb_rs` silently breaks the vector leg.
3. **Upgrade handover gap** (`sql/theodb--1.0--1.1.sql:1`) — the 1.0→1.1 upgrade is a no-op; an existing v1.0 install still owns the deprecated plpython3u `theodb.embed` with no retirement migration (duplicate-definition clash risk when adding `theodb_rs`).

## Scope & Methodology

- **Target:** `/home/paulo/Projetos/usetheo/theo-data/theo-db`
- **Mode:** full (boundaries · data-flow · scaling · deletion · trade-offs)
- **Languages detected:** Rust (pgrx 0.16.1), SQL (plpython3u + plpgsql), Python (benchmarks + model servers), Shell, Dockerfile
- **Frameworks detected:** pgrx, minreq (native-tls), pgvector, pgvectorscale (StreamingDiskANN), psycopg2 / numpy / pytest / h5py / fastembed, Patroni / etcd / pgBackRest
- **Modules inventoried:** 8
- **Files inventoried:** 73 (1 excluded: cloned third-party study repos under `.claude/knowledge-base/references`)
- **Quality gates passed:** phases 2 (1.00), 3 (0.93), 4 (0.95), 5 (0.95)
- **Findings:** 15 consolidated system-design findings (1 critical, 1 high, 9 medium, 4 info) + 6 data flows + 11 trade-off decisions
- **Note on coverage:** the `coverage-stats` view reports 0% "deep-read" because per-file inspection rows were not individually marked; coverage was instead achieved at the module level (all 8 modules + all 6 entry points analyzed across 4 phase meetings). This is an honest tooling limitation, not a gap in analysis.

## Module Inventory

| Module | Domain | Boundary Type | LOC | Files | Public API | Entry Point |
|---|---|---|---|---|---|---|
| theodb_rs | embedding-generation | bounded_context | 322 | 7 | 2 | yes |
| sql_surface | ai-vector-sql-surface | bounded_context | 2056 | 9 | 24 | yes |
| benchmarks | benchmarking-measurement | supporting | 4324 | 30 | 20 | yes (dev/CI only) |
| tools | model-serving | supporting | 278 | 2 | 2 | yes |
| ha | high-availability | infrastructure | 387 | 6 | 0 | no |
| packaging | packaging-build | infrastructure | 199 | 6 | 0 | no |
| scripts | ops-tooling | infrastructure | 356 | 4 | 0 | no |
| build_image | image-build | infrastructure | 659 | 4 | 0 | no |

> LOC is **physical line count** (Python `wc`-style), not a tokenized/semantic SLOC — no `scc`/`cloc`/`tokei` is installed (see *What Was NOT Analyzed*).

## Findings by Severity

### Critical (1)

| # | Dimension | Title | File:Line | Category |
|---|---|---|---|---|
| 1 | Scaling | Embedding a whole table = N synchronous HTTP round-trips; `theodb.embed` has NO batch path (`ai.generate_batch` exists, embed does not) | `sql/theodb--1.0.sql:90` | n_plus_one_query |

### High (1)

| # | Dimension | Title | File:Line | Category |
|---|---|---|---|---|
| 2 | Scaling | Synchronous outbound HTTP holds the PG backend for the full round-trip (≤30s) — vertical ceiling is backend/connection exhaustion under fan-out | `theodb_rs/src/embed.rs:55` | blocking_io_in_hot_path |

### Medium (9)

| # | Dimension | Title | File:Line | Category |
|---|---|---|---|---|
| 3 | Boundaries | `ai.hybrid_search_rrf → theodb.embed` is a runtime cross-extension call with no enforced `pg_depend` edge (silent DROP breakage) | `sql/40-theodb-hybrid.sql:62` | cross_boundary_import |
| 4 | Data Flow | Per-row synchronous external-HTTP fan-out with no concurrency limit / backpressure | `sql/50-theodb-ai.sql:88` | missing_backpressure |
| 5 | Data Flow | External-HTTP calls have a timeout but no retry/backoff for transient failures | `theodb_rs/src/embed.rs:55` | missing_retry_policy |
| 6 | Data Flow | `theodb.import_pinecone` materializes the entire export as one in-memory jsonb in a single transaction (no chunking) | `sql/80-theodb-migrate.sql:32` | unbounded_collection |
| 7 | Scaling | `theodb.import_pinecone` resident-in-memory + single-transaction ingest (scaling lens of the same root cause) | `sql/80-theodb-migrate.sql:32` | memory_inefficiency |
| 8 | Deletion | Excisability: `DROP EXTENSION theodb_rs` silently breaks `ai.hybrid_search_rrf` vector leg | `sql/40-theodb-hybrid.sql:62` | tangled_module |
| 9 | Deletion | No removal migration for the deprecated plpython3u `theodb.embed` — 1.0→1.1 upgrade is a no-op seed | `sql/theodb--1.0--1.1.sql:1` | missing_deprecation_marker |
| 10 | Trade-offs | Per-row synchronous blocking model HTTP call (embed + ai.*) has no ADR — documented only in SQL COMMENTs | `sql/theodb--1.0.sql:89` | undocumented_decision |
| 11 | Trade-offs | No embedding/chat cache and no record of the deferral — decision by omission | `sql/30-theodb-embed.sql:1` | undocumented_decision |

### Low (0)

_None._

### Info / Positive Baselines (4)

| # | Dimension | Title | File:Line |
|---|---|---|---|
| 12 | Boundaries | POSITIVE: `theodb_rs` 3-boundary split has clean inward layering; `sql_surface` is cleanly package-by-capability (not a god module) | `theodb_rs/src/embed.rs:11` |
| 13 | Data Flow | POSITIVE: resilience hygiene sound — circuit-breaker/rate-limiter deliberately N/A for per-call UDFs; SSRF-guarded, typed-error, REVOKE-bounded I/O | `theodb_rs/src/embed.rs:43` |
| 14 | Scaling | POSITIVE: sound vector-scaling story (HNSW + StreamingDiskANN, LIMIT-bounded fns) and NO premature distribution | `sql/40-theodb-hybrid.sql:27` |
| 15 | Deletion | POSITIVE: clean extension-versioning + GUC config + REVOKE posture; no app-style feature flags and no zombie dependencies | `theodb.control:1` |

## Positive Findings

A report that only lists problems is incomplete. TheoDB does the following genuinely well:

- **Clean module boundaries.** `theodb_rs` has correct inward dependency direction
  (`lib.rs` → `embed` → `pg`; the pg-glue layer never imports the domain). The 2056-LOC
  SQL surface is **not** a god module — it is split package-by-capability into cohesive
  ≤308-LOC files over 3 schemas, with `ai._chat` as the single HTTP source of truth.
- **Resilience hygiene where it matters.** Every outbound call is SSRF-hardened
  (http(s)-only, no redirects), 30s-timeout, fail-fast with typed SQLSTATEs (22023 input /
  38000 external), `api_key` never logged, and `REVOKE … FROM PUBLIC` on every HTTP-making
  function. Circuit-breaker/rate-limiter are *correctly* judged N/A for per-call UDFs
  (Staff pragmatism — not cargo-culted).
- **Sound scaling foundation.** pgvector HNSW + pgvectorscale StreamingDiskANN is the
  declared ANN path; all list-returning functions are `LIMIT`-bounded; no premature
  sharding/distributed cache.
- **Exemplary decision hygiene.** 6 LOCKED ADRs with enumerated alternatives + rejection
  rationale; measurement-first (ADR 0002/0004), anti-sunk-cost, and explicit YAGNI
  (Cargo-workspace deferral, BM25 adoption deferral, ScaNN-fork deferral). License posture
  is enforced mechanically (`packaging/license-sweep.sh`).
- **Healthy test posture.** A Python recall@k benchmark harness + 17 pytest files act as a
  cross-language oracle for both the SQL surface and the Rust rewrite (parity-by-test gate).

## Findings by Dimension

### Boundaries & DDD — Score 4/5

8 modules, two of them genuine bounded contexts (`theodb_rs` = embedding-generation,
`sql_surface` = ai-vector-sql-surface). Layering is DIP-clean inside `theodb_rs` and the
SQL surface is cohesive package-by-capability. The single deduction is the **cross-extension
runtime seam**: `ai.hybrid_search_rrf` calls `theodb.embed(query_text)` (finding #3) by
name at execution time, so PostgreSQL records no `pg_depend` edge — and the only declared
catalog edge runs the *opposite* way (`theodb_rs` requires `theodb`). The two contexts form
a conceptual cycle the catalog cannot see or protect. Remediation: prefer an explicit
`query_vector` at the boundary, OR declare a real `requires` edge, OR add a fail-fast guard
(`to_regprocedure('theodb.embed(text)') IS NULL → RAISE 0A000`) + a regression test.

### Data Flow & State Management — Score 3/5

Six flows traced (see `figures/data_flow_map.svg`): two `http_sync` external (embed,
`ai._chat`), three `db_query`, plus the per-row fan-out shape. The data plane is safe by
construction (SSRF guard, typed errors, REVOKE, bound `%I` identifiers, read-only NL-SQL
sandbox with L2/L3/L4 guards). The deductions are resilience gaps under fan-out: **no
backpressure / concurrency cap** (finding #4), **no retry/backoff** so a single transient
5xx aborts a whole statement and discards already-paid calls (finding #5), and **unbounded
single-transaction import** (finding #6). All three are operationally bounded by
`REVOKE-from-PUBLIC` and are medium, not high.

### Scaling Readiness — Score 2/5

The one critical and one high finding both live here. The **embed N+1** (finding #1) is the
headline: per-row `theodb.embed` with no batch path, asymmetric with the chat path's
`ai.generate_batch`. The **blocking-I/O ceiling** (finding #2) means `max_connections` —
not CPU/RAM — is the first vertical wall under fan-out. Both are partly inherent to the
AlloyDB-compatible in-DB model-call pattern, but the *absent batch entry point* is a real,
low-effort-to-close gap, and the batch path is also the highest-leverage mitigation for the
blocking-I/O ceiling. Strong positives keep this from being a 1: HNSW/diskANN ANN path,
LIMIT-bounded functions, and no premature distribution.

### Deletion Safety — Score 3/5

Correct-for-a-DB-extension posture: GUC-based config + `ALTER EXTENSION UPDATE` versioning
(not app-style feature flags — their absence is *right*, not a gap), no zombie dependencies
(all `requires` are live), REVOKE + `superuser=true` on privileged surfaces. Two medium
debts: the **silent-drop coupling** (finding #8, the deletion-lens view of #3) and the
**missing plpython3u-embed retirement migration** (finding #9) — the 1.0→1.1 upgrade is a
literal `DO $$ NULL $$` no-op, and the committed generated `sql/theodb--1.0.sql` is stale
(still defines the plpython3u embed at line 16). Both are narrow blast radius (≤1 module).

### Trade-offs & Pragmatism — Score 4/5

The strongest dimension. 11 trade-offs catalogued; **9 are documented in LOCKED ADRs**
(no-engine-fork, unification-as-moat, own-code-Rust/Go pivot, permissive-license ceiling,
single-node strong consistency, typed fail-fast errors, ScaNN-substitute, BM25 deferral,
module-first-before-workspace). The 2 deductions are high-impact decisions recorded only in
code comments: **synchronous per-row model HTTP** (finding #10) and **no embedding/chat
cache** (finding #11). Both are likely correct (AlloyDB-compat + VOLATILE correctness;
YAGNI pre-GA) but should be durable ADRs — drafts provided.

## Scoring Card

```
Boundaries     [████████░░] 4/5   DIP-clean; 1 cross-extension seam
Data Flow      [██████░░░░] 3/5   safe I/O; fan-out backpressure/retry gaps
Scaling        [████░░░░░░] 2/5   embed N+1 (critical) + blocking I/O (high)
Deletion       [██████░░░░] 3/5   clean versioning; silent-drop + upgrade-handover debt
Trade-offs     [████████░░] 4/5   6 LOCKED ADRs; 2 decisions only in comments
─────────────────────────────────
Overall        [██████░░░░] 3.2/5  equal-weighted mean
```

Scoring legend (0–5):
- **5 = Excellent** — a Staff engineer would approve with no comments.
- **4 = Good** — minor issues, all documented.
- **3 = Acceptable** — known gaps with a mitigation plan.
- **2 = Partial** — significant gaps affecting maintainability/scale.
- **1 = Rudimentary** — fundamental patterns missing.
- **0 = Missing** — dimension not addressed at all.

## Top Refactor Priorities

Ranked by (severity × blast radius × ease of fix):

| # | Title | Dimension | Severity | Blast Radius | Effort | ROI |
|---|---|---|---|---|---|---|
| 1 | Add `theodb.embed_batch(text[])` mirroring `ai.generate_batch` (endpoint already accepts array input) | Scaling | Critical | High (every bulk embed) | Low | **Very High** |
| 2 | Fail-fast guard + explicit/declared dependency for the `theodb.embed` seam (+ regression test) | Boundaries / Deletion | Medium | Medium (vector leg + DROP safety) | Low | High |
| 3 | Retirement migration for the deprecated plpython3u `theodb.embed` (or record the gap in ADR/CHANGELOG if v1.0 never shipped) + regen `theodb--1.0.sql` | Deletion | Medium | Low (existing v1.0 installs) | Low | High |
| 4 | Bounded retry-with-jittered-backoff for the recoverable HTTP class only (timeout/502/503/429) | Data Flow | Medium | Medium (all external calls) | Medium | Medium |
| 5 | Chunked / paginated `import_pinecone` (caller-side chunk or `chunk_size` arg with per-batch COMMIT) | Scaling / Data Flow | Medium | Medium (large migrations) | Medium | Medium |

Also: promote the two ADR drafts (below) into `docs/adr/` to close the trade-offs gaps.

## ADR Suggestions

Two undocumented high-impact decisions were flagged (`suggests_adr=1` in the trade-off
ledger). Full MADR 3.0 drafts are written under `adr-drafts/`:

### ADR-A: Synchronous per-row model HTTP calls (embed + ai.*), batch/async deferred
- **Status:** proposed → `adr-drafts/adr-A-synchronous-per-row-model-http.md`
- **Context:** `theodb.embed` / `ai.*` issue one blocking HTTP round-trip per row (VOLATILE, no plan folding); documented only in code comments.
- **Decision:** keep synchronous per-row for AlloyDB-compat + correctness; add `theodb.embed_batch` as the leverage mitigation; defer async/queue until a measured backend-exhaustion bottleneck.
- **Consequences:** Good — API parity, planner correctness, no premature infra. Bad — backend-per-row occupation; `max_connections` is the first wall; no retry today.

### ADR-B: No embedding/chat cache in v1 (YAGNI deferral)
- **Status:** proposed → `adr-drafts/adr-B-no-embedding-chat-cache.md`
- **Context:** every call re-hits the endpoint, even for identical `(content, model)`; the deferral is recorded nowhere (decision by omission).
- **Decision:** no cache in v1 (stateless per-call, consistent with VOLATILE); the `(content, model)→vector` cache is the sanctioned future optimization, gated on a measured re-embedding cost; chat memoization rejected on semantics.
- **Consequences:** Good — no invalidation/staleness surface, simplest. Bad — repeated deterministic embeds pay full cost until a cache lands.

## What Was NOT Analyzed (honest limits)

- **No semantic LOC tool.** `scc` / `cloc` / `tokei` are not installed; **all LOC figures
  are physical line counts** (Python line counting), which over-counts blanks/comments
  vs SLOC. Use them as size signals, not precise SLOC.
- **No static coupling/dependency-graph tool.** `tach` (Python) and `cargo-modules` /
  `cargo-depgraph` (Rust) are not installed; boundary and coupling analysis is
  **read-based** (manual import tracing of `theodb_rs/src/*.rs` and `sql/*.sql`), not
  tool-computed Ca/Ce/instability metrics.
- **Per-file coverage not individually marked** — analysis was conducted at module +
  entry-point granularity (all 8 modules, all 6 data-flow entry points). `coverage-stats`
  consequently shows 0% deep-read despite full module coverage.
- **Docs/markdown not inventoried as modules**; `.claude/knowledge-base/references`
  (cloned third-party study repos) explicitly excluded.
- **No runtime/benchmark execution** in this audit — scaling findings are derived from code
  shape (VOLATILE per-row, single-transaction loops, synchronous `req.send`), not from a
  load test. The repo's own recall@k benchmarks remain the empirical authority (ADR 0002),
  and current ANN numbers carry a synthetic-gaussian caveat (ADR 0004).
- **`benchmarks` (4.3K LOC, dev/CI-only) and infra modules** (`ha`, `packaging`, `scripts`,
  `build_image`) were inventoried but not deeply audited for system-design findings — they
  are not shipped in the product image.

## Threshold Sourcing Legend

| Metric | Value | Source | Applied to |
|---|---|---|---|
| N+1 queries | always a bug (threshold 1) | consensus (Martin Fowler) | embed N+1 (critical) |
| Circuit breaker for external calls | required | consensus (Nygard, *Release It!*) | judged N/A for per-call UDFs (positive #13) |
| Connection pooling (multi-tenant) | mandatory | consensus | blocking-I/O ceiling context (high) |
| Queue for spike-prone endpoints | required | consensus (AWS Well-Architected) | import + fan-out (medium) |
| Cross-boundary import ratio | > 30% = leaky | heuristic | embed seam (medium) |
| Max synchronous chain depth | 3 hops | heuristic | fan-out / nl_query → chat |
| Module LOC upper bound | 2000 | heuristic (folklore) | `sql_surface` (2056, not flagged — cohesive) |
| Public API size per module | 20 | heuristic | `sql_surface` 24 (not flagged — package-by-capability) |
| Files per module | 30 | heuristic | `benchmarks` 30 (dev/CI only) |
| Feature-flag max age | 30 days | default (LaunchDarkly) | N/A — no app-style flags (positive #15) |

| Source | Meaning |
|---|---|
| consensus | Industry-wide agreement, multiple authoritative sources |
| default | Tool/vendor default, single authoritative source |
| heuristic | Experience-based, no strong authority |
| agent | Assigned by the auditing specialist for a documented trade-off |

## Appendix

### Finding counts per table
- `modules`: 8 · `files_inventoried`: 73 (1 excluded)
- `boundary_findings`: 2 (1 medium + 1 info) · `data_flows`: 6
- `state_findings`: 4 (3 medium + 1 info) · `scaling_findings`: 5 (1 critical, 1 high, 1 medium, 2 info)
- `deletion_findings`: 3 (2 medium + 1 info) · `tradeoff_decisions`: 11 (9 documented, 2 suggest ADRs)
- `system_design_findings` (consolidated): 15 → 1 critical, 1 high, 9 medium, 4 info
- `quality_gates`: 4 (phases 2–5, all passed)

### Quality gate history
| Phase | Score | Status | Evaluator |
|---|---|---|---|
| 2 — Boundaries | 1.00 | passed | boundary-cartographer |
| 3 — Data Flow & State | 0.93 | passed | data-flow-tracer |
| 4 — Scaling | 0.95 | passed | scaling-analyst |
| 5 — Deletion & Trade-offs | 0.95 | passed | deletion-safety-and-tradeoff-auditor |

### Tool run log
No automated tool runs recorded (`tool_runs` = 0): `scc`/`cloc`/`tokei`, `tach`, and
`cargo-modules`/`cargo-depgraph` are not installed in this environment. Analysis was
read-based + module-level (see *What Was NOT Analyzed*).
