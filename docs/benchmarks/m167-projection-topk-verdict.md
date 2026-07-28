# M167 — projection top-k (q23–q26): measured verdict

**Date:** 2026-07-28
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NOT the canonical ClickBench c6a.4xlarge.
Before and after were measured on **this same box, same cluster, same 1M sample**, sequentially.
**Build:** `cargo pgrx install --release`, PG 18.4 (pgrx), `datcollate = C`, `work_mem = 64MB`, `shared_buffers = 4GB`.
**Data:** ClickBench `hits` / `hits_heap`, 1,000,000 rows each — verified by `count(*)`.

## 1. Result

| Query | shape | before (hot) | after (hot) | ratio | Custom Scan | A/B |
|---|---|---|---|---|---|---|
| q23 | `SELECT *` + `LIKE` + `ORDER BY EventTime LIMIT 10` | 21.5151 s | **3.3026 s** | **6.51×** | yes | identical |
| q24 | narrow + `ORDER BY EventTime LIMIT 10` | 5.9088 s | **0.1390 s** | **42.51×** | yes | identical |
| q25 | narrow + `ORDER BY SearchPhrase LIMIT 10` (text key) | 6.0517 s | **0.0975 s** | **62.07×** | yes | identical |
| q26 | narrow + `ORDER BY EventTime, SearchPhrase LIMIT 10` (multi-key) | 5.9517 s | **0.1421 s** | **41.88×** | yes | identical |

Suite: 42/43 ok in both runs (q28 `statement timeout`, pre-existing and unrelated), `columnar_customscan` 37 in both,
`result A/B: byte-identical (columnar == heap), diverged = 0`. Overall `hot_geomean` **0.36917 s → 0.25108 s**.

## 2. Which oracle proves what — the two are NOT interchangeable

This distinction is the reason the milestone has two gates (ADR-6), and the verdict states it explicitly so no
downstream reader conflates them:

| Claim | Proven by | Why the other one cannot prove it |
|---|---|---|
| No storage/aggregate regression across the suite | the **43-query A/B** in `run_m128_clickbench.py` — `diverged = 0` | it **strips the trailing LIMIT** (`:283`) and order-normalizes both sides (`_canonical`, `:243`); with no `Limit` node the top-k swap declines, so this oracle never exercises the path at all |
| The top-k returns the right k rows in the right order | `benchmarks/m158_ec_harness.sql` — LIMIT-preserving symmetric-`EXCEPT` over full rows on a tie-free key, plus an emission-order oracle via `row_number()` | the suite A/B is blind to ordering by construction |

**Harness result (M167 blocks added by this milestone):** 12 comparison blocks, **all 0**; the seeded
**negative control reports 2** — the oracle demonstrably can fail, so the zeros carry information.
`M167-A/B/C/D` route; `M167-C2` (a named `en_US.utf8` collation) declines, as it must.

## 3. What changed, and what each piece is bounded by

**q23/q24 — the boot default.** They were already routable; `enable_columnar_late_mat` booted `off` and the harness
never set it. Flipping the default is the whole change for these two.

**q25 — byte-order is now proven, not allowlisted.** M158 admitted a text sort key only under collation OID 950 (`C`)
or 951 (`POSIX`). Measured: on a `datcollate = C` cluster the column carries OID **100** (`default`), so the guard
declined a case that is provably safe. The predicate now resolves `default` against `pg_database.datcollate` (syscache,
cached per backend). It only ever **adds** provable cases: `ORDER BY … COLLATE "en_US.utf8"` still declines.

**q26 — multi-key.** `numCols != 1` was a scope limit, not a safety property; the guards (type, collation, bpchar,
btree ordering operator) are per-key by nature. The wire format now carries N keys, each key is checked
independently, and **one failing key declines the whole swap**. Ceiling: `TOPK_MAX_SORT_KEYS = 8`.

**The decode bound (ADR-4).** With the default ON, an unfiltered wide top-k would decode the whole relation before
the bounded-heap TopK. Measured, with the trace on:

```
topk_decode_estimate_too_large est_bytes=228253696 budget=33554432
```

`SELECT * FROM hits ORDER BY EventTime LIMIT 10` declines at `work_mem = 4MB` and routes again at `64MB` — the gate
is the size, not a blanket refusal. `count(*)` still routes at `work_mem = 4MB`, confirming the aggregate path is
untouched.

## 4. The four apparent regressions are noise — measured, not assumed

The after-run showed four queries >10% slower. None is a top-k shape and no causal mechanism connects them to this
change, so the delta was tested against run-to-run variance (5 executions each, same session, same box):

| Query | before | after | delta | measured amplitude over 5 runs |
|---|---|---|---|---|
| q4 | 67.8 ms | 90.5 ms | +22.7 ms | **41.6 ms** |
| q29 | 77.7 ms | 86.2 ms | +8.5 ms | **29.9 ms** |
| q37 | 38.6 ms | 51.6 ms | +13.0 ms | **16.9 ms** |
| q38 | 39.4 ms | 43.6 ms | +4.2 ms | **26.7 ms** |

In every case the amplitude exceeds the delta. Honest caveat on this sub-measurement: its absolute values sit above
both the before and after runs (the box had been building all evening), so it establishes the **variance**, not a
third data point for the ratio.

## 5. What is NOT proven here

- **Peak memory of the top-k path is UNMEASURED.** Three attempts failed to isolate it: `VmRSS` is dominated by the
  4 GB of `shared_buffers` mapped into every backend (both arms returned an identical 4,705,368 kB), `Private_Dirty`
  sampled across backends returned the same value for both arms (3,619 MB), and per-PID sampling did not capture the
  transient backend. What IS proven is the **bound** — the guard's estimate and budget above — not the actual peak.
  Publishing a number I could not defend would be worse than this gap.
- **Beyond 1M rows.** M162 measured the columnar *scan* at 100M and hit `byte array offset overflow` (Arrow varlena
  i32 offsets > 2 GB). A wide top-k decode can reach the same class. Unmeasured here.
- **Parallel plans.** The harness sets `max_parallel_workers_per_gather = 0`; the serial shape is what was validated.
- **Selective predicates vs the guard.** The bound reads `relpages` and does not discount a filter, so a highly
  selective query over a large relation can be declined even though its decode would have been small. Conversely the
  on-disk bytes are compressed, so the true decode is larger than the estimate — the guard is a lower bound in that
  direction.

## 6. Reproduction

```bash
# environment
ssh root@165.227.121.20   # confirm with `doctl compute droplet list`, never from memory
P=/root/.pgrx/18.4/pgrx-install/bin
su - pgtest -c "$P/psql -h localhost -p 28900 -d postgres -U postgres"

# correctness (the top-k oracle)
su - pgtest -c "$P/psql -h localhost -p 28900 -d postgres -U postgres -f /root/theo-db/benchmarks/m158_ec_harness.sql"
#   every *_ab_mism / *_order_mism must be 0 AND m167e_control_diff must be > 0

# suite before/after (storage oracle + timings)
cd /root/theo-db && PGHOST=localhost PGPORT=28900 PGDATABASE=postgres PGUSER=postgres \
  python3 benchmarks/run_m128_clickbench.py --agg --n 1000000 --sample systematic --out after.json

# the decode bound, with the reason printed
su - pgtest -c "THEODB_ADMIT_TRACE=1 $P/pg_ctl -D /home/pgtest/m167data -l /tmp/s.log -w restart"
#   then: SET work_mem='4MB'; EXPLAIN (COSTS OFF) SELECT * FROM hits ORDER BY EventTime LIMIT 10;
```

Baseline artifact: `docs/benchmarks/m167-baseline-and-routing-facts-2026-07-28.md`.
