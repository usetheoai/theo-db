# M49 Blueprint — Cosine + Inner-Product Opclasses for `theodb_hnsw` / `theodb_ivfflat`

Milestone: M49 · slug: `cosine-ip-opclasses` · Verdict: SHIPPABLE (deep-research via council-index-storage, pgvector-anchored)

**One-line gap:** the opclass DDL, `Metric` enum (L2/Ip/Cosine, `ann/mod.rs:30-63`), the persisted `metric_tag`, and the VACUUM read-back (`build.rs:206,255`) ALL exist — but the *initial* build hardcodes `Metric::L2` (`build.rs:65,87`), so a cosine index today builds+scores as L2. M49 is 90% plumbing, 10% two kernels.

## § Q1 — Opclass registration

pgvector (`sql/vector.sql:283-332`): one opclass per (type, AM, metric); **strategy is always `OPERATOR 1 … FOR ORDER BY float_ops`** — the metric is encoded in WHICH operator (`<->`/`<#>`/`<=>`) + support FUNCTION 1, not the strategy number. `<#>` = `vector_negative_inner_product` (smaller=closer). Cosine uniquely carries `FUNCTION 2 vector_norm` (its presence = "is cosine").

Ours (`am/mod.rs:233-244`): only DEFAULT L2 (`<->`), `amsupport=0`.

**Recommendation:** add 4 non-default opclasses via `extension_sql!` — `theodb_{hnsw,ivfflat}_{cosine,ip}_ops`, `OPERATOR 1 <=>|<#> FOR ORDER BY float_ops`, `FUNCTION 1 theodb_metric_{cosine,ip}(internal)`. Keep L2 DEFAULT. Set `amsupport = 1` (`mod.rs:75`).

## § Q2 — Metric resolution at build (the crux) — ADR-1

pgvector NEVER reads the opclass name. It resolves distance via **support procedures** looked up by `(attno=1, procnum)`: `index_getprocinfo(index, 1, HNSW_DISTANCE_PROC=1)` + `HnswOptionalProcInfo(index, HNSW_NORM_PROC=2)` (`hnswutils.c:140-158`). `normprocinfo != NULL` IS the "is cosine?" test.

**pgrx 0.16 EXPOSES `index_getprocid` + `index_getprocinfo`** (verified `pg17.rs:35331-35332`) — the "get_opfamily_name unavailable" TODO (`mod.rs:230`, `build.rs:63`) is a red herring.

**ADR-1 (RECOMMENDED):** amproc support function returning the tag. Add `#[pg_extern] theodb_metric_{l2,ip,cosine}` → tag (0/1/2, matching `Metric::tag()`). At `ambuild`/`ambuild_hnsw`, replace hardcoded `Metric::L2` with `resolve_metric(indexrel)` = `index_getprocid(indexrel,1,1)`; if InvalidOid → L2 (DEFAULT fallback, mirrors `HnswOptionalProcInfo` NULL); else `FunctionCall0Coll` → `from_tag`. Thread `metric` into `IvfflatIndex::build`/`HnswIndex::build_cancellable` + persist `metric.tag()`. Everything downstream already honors the tag. **No format bump** (metric_tag already persisted; we stop writing constant 0).
- Rejected: reloption `WITH(metric=)` (contradiction with opclass — new mine); ordering-operator OID introspection (brittle, pgvector doesn't do it).

## § Q3 — Fused SIMD kernels + normalize-vs-compute — ADR-2

pgvector cosine at search does NOT use `cosine_distance` — it **normalizes stored vectors at build/insert** (`hnswutils.c:406-428`, `HnswNormValue`→`l2_normalize`, zero-norm tuple REJECTED by `HnswCheckNorm:170-176`) + normalizes the query at scan (`hnswscan.c:108-110`), so the (negative) inner-product kernel serves both `<#>` and `<=>`. IVF cosine uses spherical k-means on unit vectors.

Ours: fused zero-alloc exists only for L2 (`vec.rs:192-206 l2_dist_from_bytes`). The mine: `score()` (`hnsw_page.rs:426-437`) + IVF scan (`scan.rs:197-204`) take the fused path only when `is_l2`; the non-L2 branch decodes a fresh `Vec<f32>` PER visited node (`hnsw_page.rs:431-435` → allocating `metric.dist`, `ann/mod.rs:71`).

**ADR-2 (RECOMMENDED): Design B — compute over RAW stored bytes; do NOT normalize at store.** Diverges from pgvector's normalize-at-build to preserve page-format MEANING (crash-safety, VACUUM rebuild `build.rs:275`, rerank untouched — no REINDEX-forcing semantic change). Add `ip_dist_from_bytes` (= `-Σ q·r`, shared by IP + cosine numerator) + `cosine_dist_from_bytes` (one-pass sim/norma/normb, clamp), both AVX2+FMA + scalar fallback, `assert_eq!(raw.len(), query.len()*4)` up front (no OOB in unsafe). Generalize `score()`/scan dispatch from `is_l2` bool → 3-way `match metric`; delete the allocating `metric.dist` from the hot loop (keep it for small-K centroid scoring + rerank). Zero-vector→NaN handled by existing "NaN LAST" `Cand` ordering (`ann/mod.rs:117`).
- **Escalation (documented, not silent):** if the M49 benchmark shows cosine latency materially behind pgvector due to per-node norm recompute, revisit Design A (normalize) as a follow-up with an explicit format bump. pgvector proves A works.

**ADR-3:** `<#>` ORDER BY key = negative IP; cosine key = `1-cos` (value equals the `<=>` operator, avoids recheck surprises). `Metric::Ip::dist` already returns `-inner_product` (`ann/mod.rs:76`).

## § Edge cases / mines (MUST cover in the plan)

1. Zero vector under cosine → NaN (`vec.rs:66`): Design B relies on "NaN LAST" ordering (`ann/mod.rs:117`) — TEST it (do not leave both A+B half-done).
2. `<#>` sign: opclass binds `<#>` (negative IP); a raw-IP kernel forgetting the sign flip inverts ranking silently — parity oracle must catch it.
3. IP not a metric (no triangle ineq): HNSW-over-IP works empirically (pgvector ships it, `sql/vector.sql:318`); build+scan MUST use the SAME metric (ADR-1 guarantees it).
4. Metric consistency build↔scan↔vacuum: after ADR-1, crash-safety test (build cosine → simulated restart → scan identical) is the M49 acceptance gate.
5. L2 fallback: `resolve_metric` MUST return L2 when `index_getprocid(indexrel,1,1)==InvalidOid` (L2 opclass has no support proc).
6. IVF cosine centroids: raw-vector cosine k-means (Design B) diverges from pgvector's spherical k-means (`ivfkmeans.c:33`) — MUST-VERIFY recall parity, not silent.
7. `amsupport=1` touches `amvalidate` (`mod.rs`): must tolerate L2 opclass with 0 support procs while cosine/ip have 1.

## § References (≥2 independent)

1. pgvector source: `.claude/knowledge-base/references/pgvector/` — opclass `sql/vector.sql:283-332`; resolution `src/hnswutils.c:140-158,406-428,525-528`, `src/ivfutils.c:66-72`, `src/ivfkmeans.c:33,287`, `src/ivfbuild.c:67-71,174-179`; kernels `src/vector.c:36-39,554-689,761-813`; query norm `src/hnswscan.c:108-110`; proc constants `src/hnsw.h:33-34`, `src/ivfflat.h:36-38`.
2. pgrx 0.16 bindings: `pgrx-pg-sys-0.16.1/src/include/pg17.rs:35331-35332` (`index_getprocid`/`index_getprocinfo` exposed).
3. Our code: `am/mod.rs:75,77,233-244`; `am/build.rs:65,69,87,206,255,275`; `ann/mod.rs:30-63,71-88,117`; `am/hnsw_page.rs:426-437,516`; `am/scan.rs:120,154,162,171,197-204`; `vec.rs:35,43,55-68,192-206`.
4. Malkov & Yashunin HNSW (arXiv:1603.09320, cited `ann/mod.rs:1-8`) — basis for HNSW-over-IP empirical validity.
