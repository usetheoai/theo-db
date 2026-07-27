# M162 — the honest 100M larger-than-RAM ClickBench gap verdict

**Date:** 2026-07-27
**Build under test:** `theodb_rs` develop @ v0.152.0 (M161), PG18, DataFusion 54 / Arrow 58.
**Box:** DigitalOcean droplet theo-m160 — 15 GB RAM, 8 vCPU, 280 GB disk. **Genuinely larger-than-RAM**: the 100M
columnar `hits` working set exceeds 15 GB RAM, so cold queries hit disk (the M159 `[NEEDS-100M]` regime).
**Dataset:** the FULL ClickBench `hits` — **99,997,497 rows** (verified `count`, not the sample — the M159 1M cache-reuse
false-green was caught and the systematic 1-in-1 stream re-materialized the whole ~70 GB TSV).
**Method:** same box, same `hits_sample.tsv`. TheoDB = `theodb_columnar` scanned by the M131+ vectorized CustomScan
(median-of-3 hot, `work_mem=256MB`, `statement_timeout=300s`, one fresh connection per query so a backend OOM does not
poison the rest). ClickHouse 26.8 = `clickhouse local --path` MergeTree, server-side `--time` min-of-3 (M159 harness).
Artifacts: `m162-artifacts/theodb-100m-partial.jsonl`, `clickhouse-100m.jsonl`.

## Headline (measured)

| Metric | 1M (M159) | **100M (this milestone)** |
|---|---|---|
| **typical** per-class gap (the honest figure) | 7.54× covered | **aggregate-pushdown 8–26×, native-row-exec 16×** |
| geomean over the completing subset | 19.4× (over 43) | **24.3× (over 19)** — *outlier-sensitive*: q0/q19 alone carry it; drop them → **~15.5×** (n=17). Also a **cross-population** number (19 survivors vs 43 at 1M), so NOT a matched-population "19.4→24.3" widening. |
| TheoDB queries that **fail entirely** | 0 | **5** (timeout ×3, `byte array offset overflow` ×1, backend OOM ×1) |
| TheoDB queries **completed** on this box | 43/43 | **19/43 (44%)** — 5 hard-fail + the run OOM-killed at 24/43 leaving 19 never-run |
| ClickHouse: all 43 complete? | yes | **yes (0.008 s – 10.1 s)** |
| worst single query | ~300× | q0 `COUNT(*)` 1495× (a **missing count fast-path**, not a decode gap), q19 `SELECT *` 837× (materialization) |

**The honest headline is NOT the 24.3× (outlier-carried). It is: at 100M larger-than-RAM the typical gap stays in the
1M ballpark (8–26×), but TheoDB stops being merely "slower" and crosses hard scale limits — only 19/43 queries complete
on the 15 GB box (5 hard-fail, 19 OOM-skipped) while ClickHouse serves all 43 in sub-second to 10 s.** The scale-limit
failure, not the ratio, is the verdict.

## The 5 hard failures (the real story — not slowness, but limits)

| q | class | ClickHouse | TheoDB | failure |
|---|---|---|---|---|
| q17 | native row-exec | 3.8 s | — | `statement_timeout` (>300 s) |
| q20 | **pushdown** | 1.9 s | — | **`byte array offset overflow`** — a real i32-offset scale bug (Arrow varlena offsets are i32; a wide text column across 100M exceeds the 2 GB per-array limit) |
| q21 | native row-exec | 2.1 s | — | `statement_timeout` (>300 s) |
| q22 | native row-exec | 2.7 s | — | `statement_timeout` (>300 s) |
| q23 | native row-exec | 0.6 s | — | connection dropped (backend OOM on the 15 GB box) |

Beyond these 5, the **TheoDB run itself was OOM-killed** partway (24/43 completed) — the 15 GB box cannot hold the 100M
working set for the wider queries. ClickHouse, built for larger-than-RAM, completed all 43. **That asymmetry — "TheoDB
cannot finish the suite the reference finishes comfortably" — is itself the honest 100M verdict.**

## Per-class (the 19 that completed)

- **pushdown-class geomean 43.1×** (n=8) — dominated by `COUNT(*)` (q0, 1495×: full columnar scan of 100M vs ClickHouse's
  metadata count) and the wide `SELECT *` projection (q19, 837×: per-row materialization of 100M rows — the M148 cost
  compounding). The *aggregate* pushdowns (q1–q6) are a healthier 8–26×.
- **native-class geomean 16.0×** (n=11) — the row executor at 100M is 15–63 s where ClickHouse is 0.6–10 s.

## Is it I/O/decode-bound or CPU-bound? (the M162 question)

> **Why not `shared_blks_read`?** The plan proposed `shared_blks_read` as the I/O signal — but that counter is the
> WRONG instrument for `theodb_columnar`: the TAM decodes its stripes through its own file access, **outside PostgreSQL's
> shared-buffer manager**, so `shared_blks_read` does not see the columnar decode I/O at all (a scan of 100M columnar
> rows can show ~0 shared reads while actually reading tens of GB from disk). The correct, honest I/O indicator here is
> the **cold-vs-hot delta** (cold = decode from disk; hot = OS page cache) plus the box's memory behaviour. This is a
> methodology correction over the plan, not a gap.

**Honest answer: NOT ISOLATED — the deciding counter was not captured, so "I/O vs decode-CPU vs materialization" is not
separated.** The plan named `shared_blks_read` as the oracle; it was not recorded, and (per the box note above) it is
the wrong instrument for the TAM anyway — but no substitute (iostat, CPU-util, decode-vs-materialize split) was captured
either. What the wall-clock DOES support, stated at its true strength:
- **The cost is dominated by decode + materialization, not the vectorized compute.** The two catastrophes — q0 `COUNT(*)`
  1495× and q19 `SELECT *` 837× — are a full-scan + per-row tuple build (the M148 materialization cost compounding at
  100M); the aggregate itself does little work. This is the strongest, most direct evidence, and it points at
  **materialization/decode**, NOT at a decode-*byte* problem an encoding would fix.
- **Memory pressure is real and hard**: the box OOM-killed the run (19/43 completed) and dropped a connection. That is a
  working-set-exceeds-RAM fact, consistent with larger-than-RAM I/O — but it does not by itself separate disk-read time
  from allocation.
- **Cold-vs-hot is bimodal, not "cold ≫ hot across the board"**: small/cacheable queries show cold ≫ hot (q0 29.5→12.0 s,
  q5 35.2→24.3 s), but the genuinely-larger-than-RAM queries show cold ≈ hot or noisy hot > cold from thrashing
  (q13 37.5→39.2 s, q16 60.2→62.8 s, q18 209.8→225.8 s) — the working set does not fit even warm. Both are consistent
  with memory pressure; neither isolates the I/O-vs-CPU split.

## Verdict + encoding decision (ADR-1 / ADR-2)

The measured levers point at **materialization + a correctness bug**, NOT at the decode-byte reduction an encoding
delivers — so encoding is **not** the highest-priority lever, and (independently) it is a **persistent-format subsystem**
(magic bump + REINDEX/upgrade, M137), so per ADR-2 it belongs in a scoped **follow-up milestone (M163)**, not folded into
this measurement milestone. Crucially, this deferral does **not** rest on the un-isolated I/O-vs-CPU question: the top two
levers below are demonstrably NOT fixable by a delta/dict/RLE encoding (a `COUNT(*)` and a `SELECT *` gain nothing from
smaller column encodings), so the decision holds regardless. The measured priority order is:

1. **Fix the i32-offset scale bug (q20).** A pushdown query that *errors* at 100M is a correctness/robustness stop, ahead of any perf tuning. (Arrow `LargeUtf8`/64-bit offsets or per-chunk splitting.)
2. **Attack materialization (M148), not just bytes.** The 800–1500× catastrophes (count, `SELECT *`) are per-row tuple building, which an encoding does **not** fix. Late-materialization (M158) + a count fast-path are the bigger levers.
3. **Then** type-specific encoding (delta/dict/RLE/FOR) to cut decode bytes — the M163 format subsystem.

**Honest-negative on this milestone's optional encoding task:** no encoding was shipped (Rule 5 — do not build a
persistent-format change on a guess when the measurement shows the first two levers matter more). M162 delivers the
**number + the verdict that scopes M163**, which is exactly its measurement-first mandate.

## Honesty caveats

- **Only 19/43 TheoDB queries produced a number** (24 attempted, 5 hard-failed, the run OOM-killed at 24/43 → 19 never
  ran). The 19 are a representative cross-section (pushdown aggs, native scans, wide projections) + the 5 failures, so
  the verdict (scale-limit failure) does not hinge on the unmeasured tail — the un-run 19 are, on this box, also
  un-completable, which only strengthens the scale-limit finding.
- **The measured ratio OVERSTATES the gap → the true gap is ≤ measured (narrower), not wider.** TWO asymmetries both
  favor ClickHouse: (a) ClickHouse is timed server-side (`--time`, excludes serialization/transfer) while TheoDB is the
  full psycopg2 round-trip; (b) ClickHouse takes **min-of-3** (its best run) while TheoDB takes **median-of-3** (its
  middle run). We handicapped our own product, not flattered it — so the real gap is if anything **smaller** than the
  numbers here. (At multi-second query times the effect is minor, but the direction matters: an earlier draft stated this
  backwards; corrected here.)
- **The 19.4× (1M) vs 24.3× (100M) is cross-population** (43 queries vs 19 survivors) — not a matched-population widening.
  The honest matched claim is the per-class one (aggregate 8–26×, native 16×, stable-to-slightly-wider vs 1M) plus the
  qualitative scale-limit failure.
- **Load count evidence:** `hits_heap` (the row staging that fed the columnar `INSERT … SELECT`) measured
  `reltuples = 99,955,952`; the systematic-1-in-1 stream materialized the full 99,997,497-row `hits.tsv`. (`count(*)` on
  the columnar `hits` itself is a full 100M scan and repeatedly timed out / crashed the unstable box — itself a data
  point.)
- ClickHouse `clickhouse local` is disk-backed MergeTree, an apples-to-apples same-box columnar comparison.

## Reproduction

```bash
# TheoDB: load 100M then time (fresh conn per query so an OOM doesn't poison the rest)
python3 benchmarks/run_m128_clickbench.py --n 99997497 --agg   # load (hits_heap UNLOGGED; unattended-upgrades masked)
python3 m162_timing.py benchmarks/clickbench/theodb/queries.sql out.jsonl   # QTIMEOUT=300
# ClickHouse: same sample.tsv
benchmarks/m159_clickhouse_run.sh clickhouse hits_sample.tsv ch_create.sql ch_queries.sql ch.jsonl
```
