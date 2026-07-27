# M161 — Expression-routing coverage verdict (ClickBench 1M)

**Date:** 2026-07-27
**Build under test:** `theodb_rs` develop @ M161 (post-v0.151.0), PG18, DataFusion 54 / Arrow 58.
**Host:** DigitalOcean droplet (theo-m160, 167.99.123.44), 1M-row ClickBench `hits` (columnar TableAM).
**Method:** same-binary A/B via `SET theodb.enable_columnar_agg = on|off`; oracle = symmetric-EXCEPT over the **full** grouped
result set (`diverged = 0` ⇒ byte-identical). Routing proven by `EXPLAIN (COSTS OFF)` showing `Custom Scan
(theodb_columnar_agg)` in the ON arm — never inferred from a trivial `diverged = 0` (the M158 false-green lesson: a
declined ON arm equals the OFF arm trivially).

**Two distinct planner-GUC states, kept separate (do not conflate):**

1. **Coverage measurement + routing proof** run the **real ClickBench queries as-written** (with `ORDER BY <count> LIMIT`)
   under **DEFAULT planner GUCs** (only `enable_columnar_agg = on`). At ~700K–1M near-unique keys with `ORDER BY` on the
   COUNT (not the key), PG's cost model picks HashAggregate on its own → the swap fires. This is the production number
   (`m161-artifacts/coverage-default-gucs.txt`, `routing-proof-default-gucs.txt`): q40, q35, q18 each show `Custom Scan`
   under DEFAULT GUCs, `enable_sort` untouched.
2. **The A/B byte-identity oracle** strips `ORDER BY/LIMIT` to compare the FULL grouped multiset via symmetric-EXCEPT.
   Stripping the LIMIT removes the reason PG chose HashAggregate → the plan reverts to SORTED GroupAggregate, which a
   text/numeric key declines by design (M153 AGG_SORTED). So the A/B sets `SET enable_sort = off` ONLY to restore the
   **same HashAggregate path the real query already picks** — a mechanism to exercise the routed path for the value
   oracle, NOT a routing crutch. The routed plan proven byte-identical is the same HASHED plan production runs.

## Goal

Flip the SAFE expression classes of the 11 M159 non-pushdown ClickBench queries to the vectorized columnar CustomScan —
integer `IN`-list WHERE + the safe expression GROUP BY classes — each cleared through its per-class correctness gauntlet.
Honest realistic target: **~+3–5 queries** (NOT +11 — the blockers compound, per M152).

## What flipped (measured, byte-identical)

| Query | Class | Before (M159) | After (M161) | A/B | Notes |
|---|---|---|---|---|---|
| q40 | integer `IN`-list WHERE (`TraficSourceID IN (-1,6)`) | native GroupAggregate | **Custom Scan** | `diverged=0` (785 rows) | all remaining quals already pushable |
| q35 | integer `col ± k` GROUP BY (`ClientIP, ClientIP-1..-3`) | native GroupAggregate | **Custom Scan** | `diverged=0` (687 281 rows) | widened Int64 compute + base-type range-check |
| q18 | `extract(minute FROM EventTime)` GROUP BY (+ UserID, SearchPhrase) | native GroupAggregate | **Custom Scan** (HASHED) | `diverged=0` (996 433 rows) | epoch-invariant minute; numeric out via AnyNumeric |

## What did NOT flip (honest negatives)

- **q34 (`GROUP BY 1, URL`) — constant group key.** The PostgreSQL planner **eliminates constant group keys** before
  the final plan (grouping by a literal is redundant), so the plan's `Agg` has only `URL` as a group key while the admit
  counted two (`1` + `URL`) → grouping-key-count mismatch at swap time → declines. Verified: even under forced HASHED the
  plan is `HashAggregate  Group Key: url`. A `Const` group-key variant was implemented, measured to **never** fire for any
  input for this reason, and **removed** (dead for all inputs — no unvalidated path shipped). q34 remains native.
- **Text MIN/MAX** (q22/q23) — the M158 collation trap (deterministic ≠ byte-order); out of scope.
- **`regexp_replace` group key** (q29) — honest-negative (RE2 ≠ POSIX); declined.
- **`HAVING`** (q29) — structural, needs a separate slice.
- **`extract(day|month|quarter|year|second)`** — `day`/coarser shift under the PG↔Arrow 10957-day epoch offset;
  `second` is fractional (numeric with µs) vs DataFusion's integer date_part. Restricted to the epoch-invariant,
  integer-valued units {minute, hour}; the rest decline to native (verified: `extract(day)`/`extract(month)` → native).

## Correctness gauntlet per class

| Class | Gauntlet | Guard |
|---|---|---|
| IN-list | 3-valued logic | `IN (NULL, …)` declines (verified: native plan); non-Const / non-integer element declines |
| int `col ± k` | integer overflow (PG raises 22003, Arrow wraps) | compute WIDENED to Int64 (exact grouping) + RANGE-CHECK to the base type at materialize (reproduces 22003) |
| `extract` | epoch offset + fractional seconds | whitelist {minute, hour} only (epoch-invariant + integer-valued); numeric output via `AnyNumeric::from(i64)` |

## Coverage delta (measured)

Measured over the full 43-query ClickBench set (`EXPLAIN (COSTS OFF)` per query, counting `Custom Scan
(theodb_columnar_*)`; `scratchpad/coverage_count.sh`):

- **M161 pushdown = 35/43.** M159 baseline was 32/43 (11 non-pushdown per the deep-dive) → **+3 queries flipped**:
  q40 (IN-list), q35 (`ClientIP ± k`), q18 (`extract(minute)`). Each measured **native pre-M161** (GroupAggregate) and
  **pushdown post-M161** this session — the flip is measured on both states, not assumed.
- The remaining 8 natives are the documented honest-negatives: text `MIN/MAX` (q22/q23, M158 collation), `regexp` +
  `HAVING` (q29), the constant-group-key q34 (PG const-elimination), and 3 other queries whose blocker is outside the
  two SAFE slices (a scalar aggregate + limit-shape cases). No SAFE class was left on the table.

## Reproduction

```sql
-- routing proof (ON arm must be Custom Scan)
SET theodb.enable_columnar_agg = on; SET enable_sort = off;
EXPLAIN (COSTS OFF) SELECT ClientIP, ClientIP-1, ClientIP-2, ClientIP-3, COUNT(*) FROM hits
  GROUP BY ClientIP, ClientIP-1, ClientIP-2, ClientIP-3;
-- A/B byte-identity: materialize ON and OFF, symmetric EXCEPT must be 0 (see scratchpad m161_expr_ab2.sql).
```

## Review findings addressed (council review, pre-release)

Three council reviewers (index-storage, rust-pgrx, benchmark) read the real code + artifact. Findings and resolutions:

- **BLOCKER (index-storage) — `col ± k` used the COLUMN type as output, not the operator result type.** PG integer
  `+`/`-` are cross-type and widen (`int2±int4→int4`, `int4±int8→int8`; an undecorated literal is int4). So
  `int2col + 5` at 32767 → PG gives int4 32772, but the code would `i16::try_from(32772)` → error where PG succeeds
  (and a wrong result-type OID otherwise). **Fixed:** `out_typoid = (*op).opresulttype`; an int8 result is declined
  (fail-closed — the widened i64 compute can itself overflow for an int8 result, not PG-`22003`-equivalent). q35
  (`int4-int4→int4`, opresulttype=int4) is unaffected and still routes. Negative-tested: `UserID+5`, `CounterID+3e9`
  (int8 results) decline; `AdvEngineID+5` routes with result type `integer`.
- **HIGH (rust-pgrx) — temporal/date columns leaked through the "integer-class" gate.** Both new gates used
  `minmax_kind_of`, which folds `timestamp→I8`/`date→I4`, so `WHERE ts IN (…)` and `GROUP BY date+1` were admitted then
  errored/diverged (the IN-list filter emits a bare `lit(i64)` against a Timestamp Arrow column). **Fixed:** both gates
  now test the true integer OIDs `{20,21,23}` — temporal declines to the native plan (the M151 class the A/B never
  exercises). Negative-tested: `EventTime IN (…)`, `EventDate IN (…)`, `EventDate+1` all decline.
- **HIGH (benchmark) — coverage number's planner-GUC state was undisclosed.** Resolved by the Method § above +
  archived artifacts: the `35/43` and the q40/q35/q18 routing proofs are DEFAULT-GUC (real queries, no `enable_sort=off`);
  `enable_sort=off` is only the A/B's mechanism to exercise the routed path on the LIMIT-stripped full-set oracle.
- **LOW/INFO** — extract deparse Var type/attno divergence (confirmed safe, name-only); `deconstruct_array` detoast
  (safe for parser/const-folded literals); stale `func 3 Const` comments removed. No action beyond doc cleanup.

## Verdict

The two SAFE slices land **q40, q35, q18** (+3, DEFAULT-GUC), each **A/B byte-identical**, inside the honest ~+3–5
target. `col ± k` routes only for int2/int4 results (int8 declines, fail-closed); temporal columns decline. The
constant-group-key class is an honest negative (PG constant-elimination); text MIN/MAX, regexp, HAVING remain out of
scope as documented. No performance claim beyond the routed-query byte-identity + the coverage flip is made here.
