# Verdict — columnar `min(col)`/`max(col)` aggregate + zone-map directory fast-path

**Date:** 2026-07-19
**Plan:** `.claude/knowledge-base/plans/columnar-minmax-zonemap-plan.md`
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/columnar-minmax-zonemap-blueprint.md` (MVCC correctness verified by `council-index-storage`)
**Harness:** `benchmarks/columnar_minmax_ab.py` (1M-row `theodb_columnar` vs identical heap)
**Environment:** DigitalOcean droplet 138.197.74.150 (c-8), PG 17.10 (pgrx-managed), release build, `THEODB_SCAN_PROFILE=1`, `max_parallel_workers_per_gather=0`, `shared_buffers=2GB`, `work_mem=256MB`.

## Goal

Admit `min(col)`/`max(col)` on ordered native types byte-identical to PostgreSQL, and add a directory-only fast-path
that answers scalar `min/max` (no WHERE) by folding the zone-map `min_bits`/`max_bits` already written per
(chunk_group, col) — never decoding a column chunk.

## Result — GOAL MET

`MINMAX_VERDICT all_identical=YES`. Every shape is a CustomScan AND byte-identical to the heap (compared **as TEXT**),
with the fast-path firing exactly where the 7 gating conditions allow.

### Scalar min/max — byte-identical + path + latency

| Column | min path | max path | byte-identical | columnar_ms (min+max) | native_ms | speedup |
|---|---|---|---|---|---|---|
| int2 | **fastpath** | **fastpath** | YES (`-500` / `499`) | 0.7 | 1043.9 | **1395×** |
| int4 | **fastpath** | **fastpath** | YES (`-499999` / `500000`) | 0.8 | 1044.2 | **1332×** |
| int8 | **fastpath** | **fastpath** | YES (`-499999000000` / `500000000000`) | 0.7 | 1036.3 | **1411×** |
| real (float4) | **fastpath** | scan (NaN gate) | YES (`-749998.5` / `750000`) | 65.9 | 1045.7 | 15.9× |
| float8 | **fastpath** | scan (NaN gate) | YES (`-1249997.5` / `1250000`) | 76.9 | 1050.1 | 13.7× |
| timestamptz | **fastpath** | **fastpath** | YES (`2020-01-01 00:00:01+00` / `2020-01-12 13:46:40+00`) | 0.8 | 1053.1 | **1357×** |
| date | **fastpath** | **fastpath** | YES (`2020-01-01` / `2029-12-28`) | 0.8 | 1052.2 | **1350×** |

The integer/temporal scalar min/max answer from the directory alone in **< 1 ms** (reading the small per-chunk-group
directory, no chunk decode/decompress) — **~1300–1400× faster** than the native heap scan. `min(float)` also fast-paths
(NaN never changes the min); `max(float)` correctly falls back to the decoded scan (still byte-identical), so the
float latency row is dominated by the max scan.

### Correctness / edge cases (all pass)

| Case | Result |
|---|---|
| GROUP BY min/max (int4, 4 groups) | byte-identical (Phase A) |
| WHERE max(int4) | `path=scan`, byte-identical (npred>0 → Phase A) |
| Empty set → NULL | `c=None h=None` PASS |
| All-NULL column → NULL | `c=None h=None` PASS |
| **Float NaN**: `max` → NaN (fallback), `min` → smallest non-NaN | `max_c=NaN max_h=NaN max_path=scan`, `min_c=1 min_h=1` PASS |
| **Same-xact pending**: `INSERT 999999999; SELECT max()` (uncommitted) | `max=999999999` — pending folded PASS |

## Why the evidence is load-bearing (honesty — Rule 3 / Rule 5)

- **MVCC-correct fold, proven end-to-end.** The same-xact pending case proves the fast-path folds uncommitted
  backend-local rows (no directory entry); the visible-stripe-only fold + append-only + stripe-atomic visibility (per
  `council-index-storage`) means the directory min/max == the snapshot-visible min/max.
- **The NaN gate is not decorative.** `max(float)` with a NaN row returns `NaN` (via the Phase-A scan) — matching PG's
  btree ordering (NaN greatest). A naive directory fold would have returned `1000` (compute_minmax skips NaN); the gate
  forces the correct fallback. `min(float)` safely fast-paths (skipping NaN never changes the min).
- **Speed is a directory-fold win, honestly bounded.** The ~1300× is reading a small directory instead of decoding 1M
  rows — the whole point of a zone-map. Gated shapes (float-max, WHERE, GROUP BY) get the ordinary columnar-scan speed
  and stay byte-identical. The A/B reports the path per shape, so no fallback is hidden.

## Reproduce

```bash
# postgres started with THEODB_SCAN_PROFILE=1 (backend emits the path NOTICE), extension installed, port 28817
cd /root/theo-db
PGPORT=28817 PGDB=e2ab PGUSER=theo N=1000000 python3 benchmarks/columnar_minmax_ab.py
```

## Validation methodology note

Acceptance evidence is the in-PG A/B integration benchmark above (1M rows, real backend, EXPLAIN-verified CustomScan,
per-shape path NOTICE). The mirrored `#[pg_test]`s (`test_columnar_minmax_phase_a_byte_identical`,
`test_columnar_minmax_fast_path`) are committed as executable RED-shape gates; `cargo pgrx test` remains inexecutable on
the build droplet (pre-existing pgrx harness/linker limitation, unrelated to this change).

## Scope / honest caveats

- Ordered native types only: int2/4/8, float4/8, timestamp/date. **Bool has no PG min/max aggregate** (only
  `bool_and`/`bool_or`), so it is out of scope. Text/numeric min/max deferred (unordered / needs collation or Decimal128
  column decode).
- `max(float)` never uses the directory fast-path (NaN correctness) — it uses the decoded scan. This is by design (KISS,
  no page-format `has_nan` bit — ADR-MM2).
