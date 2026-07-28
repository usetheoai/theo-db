# M167 — projection top-k (q23–q26): measured verdict

**Date:** 2026-07-28 (rev 2 — supersedes the first draft; see § 7)
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NOT the canonical ClickBench c6a.4xlarge.
**Build:** `cargo pgrx install --release`, PG 18.4 (pgrx), `datcollate = C`, `work_mem = 64MB`, `shared_buffers = 4GB`.
**Data:** ClickBench `hits` / `hits_heap`, 1,000,000 rows each — verified by `count(*)`.
**Raw artifacts:** `docs/benchmarks/m167-artifacts/` (`after-1m.json`, `paired-ab-ctas.log`,
`before-1m-SUPERSEDED.json`).

## 1. Result — paired same-binary A/B

The headline measurement toggles `theodb.enable_columnar_late_mat` **inside one session on one binary**, 5 alternating
off/on pairs per query, each arm materialized with `CREATE TEMP TABLE … AS` so the k surviving rows are actually
formed. Raw log: `m167-artifacts/paired-ab-ctas.log`; harness: `benchmarks/m167_paired_ab.sql`.

| Query | shape | off (median) | on (median) | ratio | pairs on&lt;off |
|---|---|---|---|---|---|
| q23 | `SELECT *` + `LIKE` + `ORDER BY EventTime LIMIT 10` | 21.3814 s | **4.2897 s** | **4.98×** | 5/5 |
| q24 | narrow + `ORDER BY EventTime LIMIT 10` | 6.1929 s | **0.1383 s** | **44.78×** | 5/5 |
| q25 | narrow + `ORDER BY SearchPhrase LIMIT 10` (text key) | 5.8742 s | **0.1046 s** | **56.18×** | 5/5 |
| q26 | narrow + `ORDER BY EventTime, SearchPhrase LIMIT 10` (multi-key) | 5.8529 s | **0.1405 s** | **41.67×** | 5/5 |

**20 of 20 pairs favour the new path.** Per-arm spread is tight (q23 off 21.33–22.31, on 4.15–4.79; the narrow three
have `on` between 0.094 and 0.177 s against `off` between 5.60 and 6.71 s).

Why paired-in-one-binary and not before-run vs after-run: the first draft of this verdict compared two separate suite
runs and inflated three of the four ratios ~2× (§ 7). A GUC toggle inside one session removes build, cluster, session
and thermal drift by construction; the toggle is the only asymmetry left.

## 2. Routing — the metric that actually discriminates

From `m167-artifacts/after-1m.json`, per query:

| Query | `columnar_agg_routed` | `result_ab_identical` |
|---|---|---|
| q23 | **true** | true |
| q24 | **true** | true |
| q25 | **true** | true |
| q26 | **true** | true |

Suite total: `columnar_agg_routed` **36/43**.

**`columnar_customscan` is deliberately NOT cited here.** That field is `"theodb_columnar_agg" in plan or "Custom
Scan" in plan` (`run_m128_clickbench.py:271`) and its own docstring calls it "broad and ~always True … CANNOT tell an
agg pushdown from a declined agg over a projection scan". Proof it is vacuous for this claim: `m166-clickbench-agg.json`
(late-mat **off**) also reports `columnar_customscan = true` for q23–q26 while `columnar_agg_routed = false`. The first
draft of this verdict cited the broad field; that was a false-green.

## 3. Correctness — which oracle proves what

The two gates are not interchangeable, and the distinction is the reason M167 has both:

| Claim | Proven by | Why the other cannot prove it |
|---|---|---|
| No storage/aggregate regression across the suite | the 43-query A/B — 42/43 `result_ab_identical`, `diverged = 0` | it **strips the trailing LIMIT** (`run_m128_clickbench.py:283`) and order-normalizes both sides (`_canonical`, `:243`); with no `Limit` node the top-k swap declines (`columnar_agg.rs` parent check), so it never exercises the path |
| The top-k returns the right k rows **in the right order** | `benchmarks/m158_ec_harness.sql` — LIMIT-preserving symmetric-`EXCEPT` over full rows on a tie-free key, plus an emission-order oracle via `row_number()` | the suite A/B is blind to ordering by construction |

**Harness result:** every `*_ab_mism` / `*_order_mism` block **0**; the seeded negative control reports **2**, so the
zeros carry information. Blocks and their measured verdicts:

| Block | Shape | Expected | Measured |
|---|---|---|---|
| M167-A | `SELECT *` + `LIKE` + non-text key | route | route, `ab_mism = 0` |
| M167-B | narrow projection, emission-order oracle | route | route, `order_mism = 0` |
| M167-C | text key, DATABASE DEFAULT collation | route (cluster is `datcollate = C`) | route, `ab_mism = 0` |
| M167-C2 | text key, named `en_US.utf8` | decline | decline |
| M167-D | multi-key, first key ties | route | route, `order_mism = 0` |
| **M167-D2** | **multi-key with a TEXT second key** (q26's real shape) | route | route, `order_mism = 0` |
| **M167-D3** | second key `COLLATE "en_US.utf8"` | decline | decline |
| **M167-D3b** | `bpchar` as a non-first sort key | decline | decline |
| **M167-D4** | 9 sort keys (over `TOPK_MAX_SORT_KEYS = 8`) | decline | decline |
| M167-E | seeded divergence (negative control) | `> 0` | **2** |

D2/D3 matter most: they are the only place the two new mechanisms intersect. A per-key loop that validated key 0's
collation and forgot key 1's would pass every other test and fail exactly there.

`benchmarks/columnar_type_ab.py` (the M163/M164 type-coverage gate, required by the ROADMAP DoD) carries four
projection-top-k routing cases: **35/35 as-expected, positive control `diverged = 2`**.

## 4. What changed

**q23/q24 — the boot default.** Both were already routable; `enable_columnar_late_mat` booted `off` and the harness
never set it. Flipping the default is the whole change for these two.

**q25 — byte-order proven, not allowlisted.** M158 admitted a text sort key only under collation OID 950/951. On a
`datcollate = C` cluster the column carries OID **100** (`default`), so a provably safe case was declined. The
predicate now resolves `default` against `pg_database.datcollate` **and requires `datlocprovider = 'c'`** — see § 5.

**q26 — multi-key.** `numCols != 1` was a scope limit, not a safety property. The wire format carries N keys, every
key is checked independently, one failing key declines the whole swap, ceiling `TOPK_MAX_SORT_KEYS = 8`.

**The decode bound (ADR-4).** With the default ON, an unfiltered wide top-k would decode the whole relation before the
bounded-heap TopK. The guard declines when the relation's size exceeds `work_mem × 8`.

## 5. Two correctness holes found in review and closed

Both were found by independent reviewers, re-verified here, and fixed before release.

**ICU provider (`datlocprovider`).** `CREATE DATABASE d LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LOCALE 'C'` stores
`datcollate = 'C'` while the DEFAULT collation orders by ICU (`pg_locale.c` dispatches on `datlocprovider`;
`dbcommands.c` writes the two fields independently). Reading `datcollate` alone admitted a text sort key whose
DataFusion byte order disagrees with PG — **the exact wrong-rows class the M158 guard existed to prevent**, made
reachable without a session `SET` by the default flip. Verified by creating that database: the text sort key now
declines.

**The guard was inert without ANALYZE.** `pg_class.relpages` is written only by ANALYZE/VACUUM, and a columnar table
never triggers either on its own — there is no `pgstat` counting anywhere in `theodb_rs/src/`, so autoanalyze never
fires, and `relation_vacuum` (`columnar.rs:1851`) is an error stub. Measured on a fresh 200k-row columnar table:
`relpages = 0` and **the guard did not fire even at `work_mem = 64kB`**. The first draft's guard demonstration only
worked because ANALYZE had been run by hand during investigation, unrecorded. Fixed by falling back to the relation's
true current size; verified: a `relpages = 0` table now declines at 64kB and routes at 1GB.

## 6. What is NOT proven

- **PostgreSQL's stock `work_mem` is 4 MB**, giving a budget of 32 MB — below the 1M-row `hits`. On a stock cluster
  these queries **decline**. The measurements here were taken at `work_mem = 64MB`. The headline "routes by default"
  is true of the default *GUC*, not of a default *cluster*.
- **Peak memory of the top-k path is UNMEASURED.** `VmRSS` is dominated by the 4 GB of `shared_buffers` mapped into
  every backend (both arms returned an identical 4,705,368 kB); per-PID sampling did not capture the transient
  backend. What is proven is the *bound* (`est_bytes` vs `budget`), not the actual peak. A reviewer noted the batch
  size is already computed in-process (`df_executor.rs:585` `batch.get_array_memory_size()`) and one trace line would
  expose it — not done here.
- **Beyond 1M rows.** M162 measured the columnar *scan* at 100M and hit `byte array offset overflow` (Arrow varlena
  i32 offsets > 2 GB); a wide top-k decode can reach the same class.
- **Parallel plans.** The harness sets `max_parallel_workers_per_gather = 0`.
- **The guard bills the whole relation**, ignoring projection width and filter selectivity, so a narrow projection
  over a large relation can be declined even though its decode would be small. And because on-disk bytes are
  compressed, the decoded Arrow batch is *larger* than the estimate — for an OOM bound that under-estimation is the
  **dangerous** direction (false admits). It is a ceiling on catastrophe, not a tight bound.
- **q28 exceeds the 60 s query ceiling on this cluster** and is ERRORED in both arms, so it is the one query with no
  A/B verification. It completed in 33.64 s in `m166-clickbench-agg.json`, so calling it "pre-existing" (as the first
  draft did) is not supported; the cause on this cluster is unexplained.

## 7. What the first draft of this verdict got wrong

Recorded because the error is more instructive than the result.

The first draft compared a "before" suite run against an "after" suite run and published **6.51× / 42.51× / 62.07× /
41.88×**. The repo's own `docs/benchmarks/m166-clickbench-agg.json` — same box, same parameters, one day earlier,
late-mat off — records q24 **3.0751 s**, q25 **2.7126 s**, q26 **2.9036 s** against that draft's baseline of 5.9088 /
6.0517 / 5.9517. Geomeans were equivalent (0.40934 vs 0.36917), so it was not a slower box; only the three narrow
Sort-bound shapes doubled. The baselines were not comparable and the ratios were not defensible.

Two further method errors, both self-caught while fixing the first:

1. **A `count(*)` wrapper erased the effect.** The first paired attempt wrapped each query in
   `SELECT count(*) FROM (…)`, which lets PostgreSQL skip materializing the projected columns — exactly the cost late
   materialization saves. Measured under that wrapper: q23 **1.03×**, q24 **0.99×**. The mechanism was being optimized
   away by the instrument. CTAS fixed it.
2. **Running `columnar_type_ab.py` against the same database DROPped and recreated `hits`** with its 2000-row
   synthetic schema, destroying the ClickBench data. Two measurements taken afterwards were reading the type fixture
   and were void. Data reloaded and everything re-measured.

The direction was never in doubt; the magnitude was, and the instrument is what decided it.

## 8. Reproduction

```bash
ssh root@165.227.121.20            # confirm the host with `doctl compute droplet list`, never from memory
P=/root/.pgrx/18.4/pgrx-install/bin

# headline: paired same-binary A/B
su - pgtest -c "$P/psql -h localhost -p 28900 -d postgres -U postgres -q -f /root/theo-db/benchmarks/m167_paired_ab.sql"

# correctness oracle (must be all 0, control > 0)
su - pgtest -c "$P/psql -h localhost -p 28900 -d postgres -U postgres -f /root/theo-db/benchmarks/m158_ec_harness.sql"

# type-coverage routing gate (35/35)
cd /root/theo-db && PGHOST=localhost PGPORT=28900 PGDATABASE=postgres PGUSER=postgres \
  python3 benchmarks/columnar_type_ab.py     # NOTE: recreates `hits` — never against a ClickBench database

# suite (storage oracle + columnar_agg_routed)
cd /root/theo-db && PGHOST=localhost PGPORT=28900 PGDATABASE=postgres PGUSER=postgres \
  python3 benchmarks/run_m128_clickbench.py --agg --n 1000000 --sample systematic --out after.json
```

Companion artifacts: `m167-baseline-and-routing-facts-2026-07-28.md` (pre-code measurement; note its `lc_collate` GUC
recommendation was falsified during implementation — PG 18 has no such GUC), and the review report at
`.claude/knowledge-base/reviews/m167-projection-topk-review-2026-07-28.md`.
