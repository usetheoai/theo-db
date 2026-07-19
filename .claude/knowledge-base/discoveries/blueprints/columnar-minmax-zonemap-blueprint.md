# Blueprint — `min(col)`/`max(col)` columnar aggregate + zone-map directory fast-path

Deep-research (Staff DB/storage engineer) blueprint. Primary source: the real `theodb_columnar` code (append-only TAM,
stripe-atomic MVCC, zone-map directory) — the correctness is code-specific, not literature. Independently verified by
the `council-index-storage` reviewer (visibility fold + value-domain traps). Cross-checked against PG 17 float/btree
NaN ordering and `min/max` aggregate semantics.

## Coverage Corner 1 — Integration Tests
Byte-identical A/B (`benchmarks/columnar_minmax_ab.py`, 1M rows, columnar vs identical heap) for `min`/`max` on every
in-scope native ordered type (int2/4/8, float4/8, bool, timestamp/date), scalar + GROUP BY + WHERE, plus: an all-NULL
column → NULL, an empty table → NULL, a same-xact `INSERT; SELECT max()` (pending fold), and a **float column with a
NaN row** (proves `max(float)` returns NaN = falls back to scan, not the directory 3.0). `#[pg_test]` mirrors.

## Coverage Corner 2 — Dependencies
No new dependency. DataFusion `min`/`max` aggregate (already pulled), the existing zone-map `ChunkDirEntry`
(`min_bits`/`max_bits`/`has_minmax`/`all_null`), `read_visible_stripes`, and the pending region — all already present.

## Coverage Corner 3 — Tools
pgrx 0.19 / DataFusion 54 / Arrow 58; droplet c-8 for the in-PG A/B.

## Coverage Corner 4 — Techniques (the load-bearing research)

### Two execution paths
- **Phase A — DataFusion scan (always correct).** Admit `min(col)`/`max(col)` → DataFusion `min()`/`max()` over the
  decoded VISIBLE columnar scan (identical path to `sum`/`avg`). Output type = input column type; the result Arrow
  array has the source column's Arrow type, so `arrow_value_to_datum(result_col, row, typoid)` (the existing
  build_arrow reverse) emits the native PG datum. Handles GROUP BY, WHERE, and — crucially — `max(float)` with NaN
  correctly (it decodes actual values). Byte-identical, guaranteed win vs the native heap plan.
- **Phase B — directory fast-path (the headline speedup).** Scalar `min(col)`/`max(col)`, no WHERE, no GROUP BY → fold
  the per-(chunk_group,col) `min_bits`/`max_bits` over the VISIBLE stripes' directory entries + scan-and-fold the
  pending region — WITHOUT decoding/decompressing any column chunk. O(nchunks + pending), not O(N).

### MVCC correctness of the fast-path (VERIFIED — the load-bearing proof)
`theodb_columnar` is append-only (`tuple_delete`/`tuple_update` are typed-error stubs) with STRIPE-ATOMIC visibility (a
stripe's rows are visible IFF its `columnar.stripe` catalog row is visible under the scan snapshot — `read_visible_stripes`
delegates to PG MVCC). Therefore **every row of a visible stripe is visible; there is no intra-stripe partial visibility**
(aborted sub-xact / mid-xact-flush-then-rollback / same-xact all resolve at the catalog-row grain). Folding min/max over
exactly the `read_visible_stripes` chunk-groups == the snapshot-visible min/max. Pending (same-xact, no directory entry)
MUST be scanned and folded (bounded by maintenance_work_mem).

### The 7 gating conditions for byte-identity (Phase B fires only when ALL hold; else → Phase A)
1. Scalar, `npred==0` (no WHERE), no GROUP BY, single min/max aggregate.
2. Bare column Var argument (no expression/cast — the directory is PRE-projection; reject `min(col+1)`).
3. `minmax_kind_of(typid) != None` — ordered native kind only (unordered text/numeric → Phase A).
4. Every VISIBLE chunk-group on the target column has `has_minmax==true`. Any `has_minmax==false` group → fall back
   (all-NULL and NaN-float are indistinguishable from the bit alone). *(Optional later: keep folding across groups
   flagged `all_null==true` via `FLAG_ALL_NULL` — provably contribute nothing.)*
5. **`max()` on a float kind (F4/F8) → fall back.** `compute_minmax` skips NaN, but PG `max(float)` returns NaN (NaN
   sorts greatest). `min()` on floats is safe (skipping NaN never changes the min; an all-NaN group is `has_minmax=false`
   → already falls back).
6. Pending region scanned + folded in the same domain. Empty visible + empty pending → emit SQL NULL.
7. Decode in the STORED domain and emit the NATIVE type (reuse `columnar_agg.rs:133-139` domain logic): ints compared
   as `i64` then narrowed to int2/4/8; F4 as `f64::from_bits(bits) as f32` → float4; F8 as `f64::from_bits` → float8;
   bool from `!=0`; temporal keeps its `typid`. NEVER a raw `u64` compare (negatives order wrong).

## ADRs
- **ADR-MM1:** ship BOTH phases. Phase A is the correctness floor (all shapes/types, byte-identical); Phase B is the
  directory-only speedup for the common scalar-no-filter case. Alternative rejected: Phase B only — would leave
  GROUP BY / WHERE / float-max min/max on the native plan.
- **ADR-MM2:** conservative kind-gate for `max(float)` (fall back to Phase A) instead of adding a `has_nan` bit to
  `ChunkDirEntry`. Alternative rejected: page-format `has_nan` bit — a STRIPE_VERSION bump + rewrite story for a case
  (float-max) the Phase-A scan already handles correctly. KISS + no format risk (Rule 10).
- **ADR-MM3:** min/max output type = input column type; reuse `arrow_value_to_datum` (build_arrow reverse) for Phase A
  and the `columnar_agg` const-domain decode for Phase B. Alternative rejected: a bespoke per-type emitter — redundant
  with the two proven decoders (Rule 9/DRY).

## Honest caveats
- Text/numeric `min/max` deferred (unordered `MinMaxKind::None` → Phase A can't decode numeric column input either;
  text min/max needs collation-aware handling). Integer/float/bool/temporal only.
- Phase B speedup depends on data being clean (no NaN in float-max target, no all-NULL groups); dirty data degrades to
  Phase A (still a win vs native). The A/B reports which path fired per shape (honest, no hidden fallback).

## Evidence citations
theodb_rs/src/am/columnar.rs:55-60,102-166,952-963,1256-1258 (append-only + stripe-atomic MVCC + pending + mid-xact
flush), columnar.rs:481-494 (`minmax_kind_of`), columnar.rs:708-771 (visible-stripe + pending fold set) ·
columnar_codec.rs:103-158 (ChunkDirEntry), :234 (`has_minmax && !all_null`), :293-340 (`compute_minmax` NaN-skip +
F4-widen + i64→u64 store), :126-128 (FLAG_ALL_NULL) · columnar_agg.rs:129-141 (const-domain decode to reuse), :157-163
(bare-Var gate precedent) · PG 17: float8 btree orders NaN greatest → `max` returns NaN; `min/max` over empty/all-NULL = NULL.
