# Verdict — byte-identical numeric-output integer aggregates (sum(int8), avg(int2/4/8))

**Date:** 2026-07-19
**Plan:** `.claude/knowledge-base/plans/numeric-output-aggregates-plan.md`
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/numeric-output-aggregates-blueprint.md`
**Harness:** `benchmarks/columnar_numeric_agg_ab.py` (1M-row `theodb_columnar` vs identical heap)
**Environment:** DigitalOcean droplet 159.89.85.126, PG 17.10 (pgrx-managed), release build, `max_parallel_workers_per_gather=0`, `shared_buffers=2GB`, `work_mem=256MB`.

## Goal

Extend the columnar aggregate CustomScan to admit `sum(int8)` and `avg(int2/4/8)` — the two aggregates the M114 slice
declined because their output is PG `numeric` — while remaining **byte-identical** to the native plan, including PG's
DATA-DEPENDENT `avg` scale and i128-exact `sum`.

## Result — GOAL MET

`NUMAGG_VERDICT all_identical_and_customscan=YES`. Every shape is a CustomScan (`theodb_columnar_agg`) AND byte-identical
to the heap (compared **as TEXT**, so any scale/rounding drift fails the assertion).

| Shape | CustomScan | Byte-identical (text) | columnar_ms | native_ms | speedup |
|---|---|---|---|---|---|
| `sum(s8)` (int8, within i64) | YES | YES (`500000500000000000`) | 107.9 | 770.1 | **7.14×** |
| `sum(big)` (int8, sum=**1e19 > i64 max 9.2e18**) | YES | YES (`10000000000000000000`) | 78.6 | 793.5 | **10.10×** |
| `avg(s2)` (int2, avg **scale 16**) | YES | YES (`49.5000000000000000`) | 81.1 | 797.2 | **9.83×** |
| `avg(s4)` (int4, avg **scale 12**) | YES | YES (`500000.500000000000`) | 83.2 | 795.3 | **9.56×** |
| `avg(s8)` (int8, avg **scale 8**) | YES | YES (`500000500000.00000000`) | 101.1 | 801.4 | **7.92×** |
| `GROUP BY g, sum(s8)` (4 groups) | YES | YES | — | — | — |
| `GROUP BY g, avg(s4)` (4 groups) | YES | YES | — | — | — |
| `avg(s4)` over empty set | — | `NULL` (zero-count guard) | — | — | — |

## Why the evidence is load-bearing (honesty — Rule 3 / Rule 5)

- **i128 exactness proven, not assumed.** `sum(big)` sums 1M rows of `1e13` to `1e19`, which **exceeds i64 max
  (9.2e18)**. DataFusion's native `sum(Int64)` uses `add_wrapping` and would silently wrap to a negative value; the
  columnar path casts to `Decimal128(38,0)` (i128 payload) and matches PG's i128 accumulator exactly. An identical
  `10000000000000000000` on both sides is the proof the Decimal128 path is required, not decorative.
- **Data-dependent avg scale reproduced byte-for-byte.** PG's `avg(int)` = `numeric_div(sum, count)` with
  `select_div_scale = max(16 − qweight·4, 0)`. The measured outputs show the scale shrinking as the sum grows:
  **16** sig-digits (`avg(s2)`) → **12** (`avg(s4)`) → **8** (`avg(s8)`). A fixed-scale reimplementation would NOT
  produce this. The implementation delegates division to pgrx `AnyNumeric / AnyNumeric` = PG's own `numeric_div`, so
  the scale selection is PG's, not ours.
- **Speedups are a side effect, not the claim.** The value of this slice is completeness + correctness (two more
  aggregate shapes now push down byte-identically); the 7–10× is the same columnar-scan advantage M114 already
  measured, reported for context, not as a new performance claim.

## Reproduce

```bash
# on the droplet, extension installed into the pgrx PG, instance on port 28817 (user theo, db e2ab)
cd /root/theo-db
PGPORT=28817 PGDB=e2ab PGUSER=theo N=1000000 python3 benchmarks/columnar_numeric_agg_ab.py
```

## Validation methodology (honesty — Rule 3)

The acceptance evidence is the **in-PG A/B integration benchmark** above: the release-built extension installed into a
real PG 17.10, `CREATE EXTENSION`, and byte-identical numeric output verified at 1M rows with the CustomScan engaged
(via `EXPLAIN`). This is the project's established validation path (M114/M115 used the same A/B harness) and is stronger
than a small-N unit test — real backend, real 1M-row data, real plan verification.

The mirrored `#[pg_test] test_numeric_output_aggregates_byte_identical` is committed as an executable RED-shape gate +
documentation, but **`cargo pgrx test` cannot be executed on this droplet**: the standalone Rust test-runner binary
fails to resolve PG server symbols (`PG_exception_stack`, `errstart`, `FlushErrorState`, …) — with `rust-lld` the link
fails outright, and forcing `--unresolved-symbols=ignore-all` links but the runner then aborts (exit 127) because those
symbols are NULL without a loaded postgres. This is a **pre-existing pgrx test-harness / environment limitation** (the
failure is at the crate test-binary link stage, before any test runs, so it affects every test identically) — NOT a
defect in this change: the extension `.so` links cleanly, `CREATE EXTENSION` succeeds, and 1M-row queries return
byte-identical numeric results.

## Scope / honest caveats

- Covers `sum(int8)` + `avg(int2/4/8)`. `sum/avg(numeric)` (numeric-COLUMN input) needs an Arrow `Decimal128` column
  decode in the columnar codec — deferred (bigger change; the blueprint records it as the next natural slice).
- `sum(int2/4)`→int8 and `avg(float8)` were already shipped in M114; this slice adds the numeric-output integer shapes.
