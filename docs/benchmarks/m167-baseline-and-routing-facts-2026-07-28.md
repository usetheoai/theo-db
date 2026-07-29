# M167 — measured baseline and routing facts (before any code change)

**Date:** 2026-07-28
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NOT the canonical ClickBench c6a.4xlarge
**Build:** `theodb_rs` 1.2.0, `cargo pgrx install --release` from `develop@4fe1240` (75 MB `.so`)
**Cluster:** PG 18.4 (pgrx), port 28900, datadir `/home/pgtest/m167data`, `--locale=C`
**Data:** ClickBench `hits` 1,000,000 rows (systematic 1-in-99 sample) + `hits_heap` twin — **both verified by
`count(*)`**, not by the harness `DONE` line (the M162 false-100M lesson).

Every number below was produced by a command in § Reproduction. Nothing here is estimated.

## 1. Baseline timings — GUC at its current default (`enable_columnar_late_mat = off`)

Source: `/root/m167-baseline-before.json` (`run_m128_clickbench.py --agg --n 1000000 --sample systematic`).

| Query | SQL | hot (s) | A/B vs heap |
|---|---|---|---|
| q23 | `SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10` | **21.5151** | identical |
| q24 | `SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10` | **5.9088** | identical |
| q25 | `SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10` | **6.0517** | identical |
| q26 | `SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10` | **5.9517** | identical |

Run totals: 42/43 ok · 1 ERRORED (q28, `statement timeout` at 60 s — pre-existing, out of M167 scope) ·
`columnar_customscan` 37/43 · hot geomean 0.36917 s · result A/B byte-identical across the suite, `diverged = 0`.

**Caveat on the A/B, carried from the edge-case review (EC-2):** this oracle strips the trailing `LIMIT`
(`run_m128_clickbench.py:283`) and order-normalizes both sides (`_canonical`, `:243`). It is a *storage* oracle.
It says nothing about which k rows a top-k returns, or in what order. It must never be cited as top-k correctness
evidence.

## 2. Routing reality with the GUC forced ON — no code change

`SET theodb.enable_columnar_agg = on; SET theodb.enable_columnar_late_mat = on; EXPLAIN (COSTS OFF) <q>`

| Query | Custom Scan | Sort | Verdict | Why |
|---|---|---|---|---|
| q23 | 1 | 0 | **ROUTES** | single non-text key (`EventTime`); `LIKE` filter pushes down (M156) |
| q24 | 1 | 0 | **ROUTES** | single non-text key (`EventTime`) |
| q25 | 1 | 2 | **DECLINES** | text sort key; collation guard (`columnar_agg.rs:1927–1939`) |
| q26 | 1 | 2 | **DECLINES** | multi-key — `numCols != 1` guard (`columnar_agg.rs:1853`) |

**This falsifies the "M167 must implement late-mat projection top-k" framing of the ROADMAP entry.** The M158
mechanism already routes q23 and q24 byte-identically; they do not appear in the ClickBench numbers only because
`enable_columnar_late_mat` boots `off` (`columnar_agg.rs:33`) and the harness never sets it
(verified: `run_m128_clickbench.py` only ever `SET`s `enable_columnar_agg`).

## 3. Why q25 declines — measured, not inferred

| Fact | Value | Source |
|---|---|---|
| Database collation | `C` / `C` | `SELECT datcollate, datctype FROM pg_database WHERE datname = current_database()` |
| `hits.SearchPhrase` `attcollation` | **100** | `pg_attribute` |
| OID 100 is | `default`, provider `d` | `pg_collation` |
| OID 950 / 951 | `C` / `POSIX`, provider `c` | `pg_collation` |
| Guard admits | **only 950 or 951** | `columnar_agg.rs:1935–1938` |

So in a cluster whose collation **is** byte-order (`lc_collate = C`), the guard still declines, because the column
carries the `default` collation OID (100) rather than the named `C` OID (950). The guard is not wrong — it is
*conservative in a way that costs a real, safe routing opportunity*, and it is exactly the case the ROADMAP DoD
bullet 2 describes ("`ORDER BY` de texto só roteia com colação determinística").

### The safe fix exists and is native (no new unsafe FFI)

- PG 18 does carry the right predicate — `pg_locale_struct.collate_is_c`, reachable via
  `pg_newlocale_from_collation()` (`utils/pg_locale.h:118–123`, `:144`). **But pgrx 0.19 does not generate bindings
  for it** (verified: neither `pg_newlocale_from_collation` nor `collate_is_c` appears in the generated
  `pg18.rs`). Using it would require a hand-written `extern "C"` plus an assumption about the struct's layout — a
  new unsafe surface across the C boundary.
- **`GetConfigOption` IS bound** (`pg18.rs:44730`). Reading the `lc_collate` GUC gives PG's own report of the
  database collation with no struct-layout coupling. Byte-order is then provable at plan time:
  `coll ∈ {950, 951}` → true; `coll == 100` → true iff `lc_collate ∈ {C, POSIX}`; anything else → false
  (fail-closed on a null/unknown GUC).

## 4. Why q26 declines

`(*sort).numCols != 1` (`columnar_agg.rs:1853`). `ORDER BY EventTime, SearchPhrase` is two keys. Supporting it is
new mechanism (encode N keys in `custom_private`, apply the per-key type/collation guards in a loop, build a
DataFusion multi-expr sort) — not a guard relaxation. DataFusion's `.sort()` already takes a `Vec` of sort
expressions, so the executor side is a natural generalization; the planner side is the actual work.

## 5. What this means for the milestone scope

The ROADMAP M167 DoD requires all four of q23–q26 to show the Custom Scan with `diverged = 0`. Against the measured
reality that decomposes into three distinct pieces of work, not one:

| Piece | Query | Nature |
|---|---|---|
| Flip the boot default (+ bound the O(N) decode) | q23, q24 | already routable — the default is the only gate |
| Byte-order-provable collation predicate | q25 | replace an OID allowlist with PG's own report |
| Multi-key top-k | q26 | new planner mechanism |

## Reproduction

```bash
ssh root@165.227.121.20
P=/root/.pgrx/18.4/pgrx-install/bin
su - pgtest -c "$P/psql -h localhost -p 28900 -d postgres -U postgres"
  SELECT datcollate, datctype FROM pg_database WHERE datname = current_database();
  SELECT attcollation FROM pg_attribute WHERE attrelid='hits'::regclass AND attname='searchphrase';
  SELECT oid, collname, collprovider FROM pg_collation WHERE collname IN ('C','POSIX','default');
  SET theodb.enable_columnar_agg = on; SET theodb.enable_columnar_late_mat = on;
  EXPLAIN (COSTS OFF) SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;

# baseline suite (≈29 min incl. the systematic 1-in-99 stream)
cd /root/theo-db && PGHOST=localhost PGPORT=28900 PGDATABASE=postgres PGUSER=postgres \
  python3 benchmarks/run_m128_clickbench.py --agg --n 1000000 --sample systematic --out /root/m167-baseline-before.json
```
