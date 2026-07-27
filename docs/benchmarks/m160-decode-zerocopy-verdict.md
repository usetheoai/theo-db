# M160 — zero-copy fixed-width decode → Arrow: measured verdict

**Date:** 2026-07-27
**Hardware:** DigitalOcean droplet (theo-m160), 8 vCPU / 16 GB, PostgreSQL 18.4 + `theodb_rs` @ develop, `max_parallel_workers_per_gather=0`, `shared_preload_libraries='theodb_rs'`.
**Data:** ClickBench `hits`, 1,000,000-row systematic 1-in-99 subsample (same regime as M159), loaded into `theodb_columnar` (`hits`) + a heap twin (`hits_heap`) by a controlled load (no `run_m128` — a concurrent-process race on the shared table corrupted an earlier attempt; the honest note is below).

## What M160 changes

The pushdown decode path (`decode_to_batch` → `decode_columns_v2` → `build_arrow_from_decoded`) now decodes NON-NULL
fixed-width columns (int2/4/8, float4/8, timestamp/tz, date) as one contiguous little-endian buffer (`DecodedColumn::FixedRaw`)
and builds the Arrow `PrimitiveArray` via ONE typed `Vec<T>` per column (`fixed_raw_array`), instead of the legacy per-cell
`Vec<Option<Vec<u8>>>` (`decode_column` `.to_vec()` per cell) + a re-read in `build_arrow`. This eliminates the per-cell
allocation storm the post-M159 deep-dive flamegraph measured as the covered-class bottleneck (the M148-twin cost).
Nullable / varlena / text / bool columns and same-xact pending rows keep the legacy cell path (fail-safe). Gated by
`theodb.enable_columnar_fast_decode` (default ON) — the toggle exists so the win can be measured same-binary.

## Correctness — A/B byte-identical (the gate)

Symmetric-EXCEPT of the columnar result (fast_decode ON) vs the heap twin, per covered query class (deterministic aggregates, no LIMIT):

| Query class | diverged |
|---|---|
| `SELECT RegionID, sum(ResolutionWidth), count(*) GROUP BY RegionID` | **0** |
| `SELECT count(DISTINCT UserID)` | **0** |
| `SELECT SearchEngineID, sum(IsRefresh), count(*) GROUP BY SearchEngineID` | **0** |
| `SELECT min(EventDate), max(EventDate)` | **0** |

Byte-identical to the legacy path — as expected by construction (`fixed_raw_array` does the exact same `from_le_bytes`
per value as `cells_to_array`; no epoch/other transform, verified in `build_arrow`'s arms) and asserted by the unit test
`m160_fixed_raw_tests` (int2/4/8, float8, date, timestamp). Endian-safe (`from_le_bytes` reads little-endian explicitly).

## Speedup — same-binary A/B (fast_decode OFF vs ON, agg pushdown ON in both, EXPLAIN ANALYZE median-of-3)

The ONLY difference between OFF and ON is the decode path (both go through the aggregate CustomScan). Same binary, same box, same loaded data → the delta is purely the decode change.

| Covered query | OFF (cell path) | ON (M160 FixedRaw) | Speedup |
|---|---|---|---|
| `RegionID, sum(ResolutionWidth), count(*) GROUP BY RegionID` | 303 ms | 53 ms | **5.7×** |
| `SearchEngineID, sum(IsRefresh), count(*) GROUP BY SearchEngineID` | 289 ms | 39 ms | **7.4×** |
| `sum(ResolutionWidth), count(*)` | 173 ms | 21 ms | **8.3×** |
| `count(DISTINCT UserID)` | 191 ms | 107 ms | **1.8×** |

**The M160 zero-copy decode gives ~2–8× on covered fixed-width aggregations** — a large, clean win, consistent with the
deep-dive flamegraph's finding that the per-cell decode bridge was ~80% of the covered-scan cost. The smaller 1.8× on
`count(DISTINCT)` is expected: the distinct-hashing dominates there, so the decode is a smaller fraction.

Context (M159 baseline, same 1M regime): the covered class was 7.54× ClickHouse. Removing the decode bridge cuts the
covered-aggregation time 2–8×, moving the covered class materially toward the 2–3× target on these query shapes.

## Honest notes

- **Flamegraph:** the deep-dive's pushdown flamegraph (318 samples, directional) localized the decode bridge as the
  bottleneck; this milestone's decisive proof is the controlled same-binary OFF/ON A/B above (which measures the exact
  change's impact — a stronger, less-noisy signal than a re-profiled flamegraph). The predicted cause and the measured
  effect agree.
- **Measurement discipline:** an earlier attempt ran `run_m128` concurrently with a leftover `run_m128`, which drop/recreated
  the shared `hits` table mid-run and produced impossible outliers (sub-ms "results", one `relation "hits" does not exist`
  error). Those numbers were discarded as a process-management artifact, NOT reported — the results above come from a
  single controlled load with no concurrent process. (Rule 5 / owner mandate: never mask or report contaminated numbers.)
- **Scope:** fixed-width non-null columns only; the covered ClickBench aggregation class is dominated by these. Nullable
  fast path is a documented fast-follow (needs null-bitmap + typed values). At 100M larger-than-RAM the decode/IO balance
  shifts (M162, `[NEEDS-100M]`).

## Reproduce

Controlled load (`benchmarks/clickbench/theodb/create.sql` + `\copy` the systematic 1M sample + `INSERT INTO hits SELECT`),
then per query: `SET theodb.enable_columnar_agg=on; SET theodb.enable_columnar_fast_decode=off|on; EXPLAIN (ANALYZE) <q>`;
A/B via symmetric-EXCEPT columnar vs heap. Unit test: `cargo test m160_fixed_raw`.
