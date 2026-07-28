---
slug: m167-projection-topk
milestone_id: M167
target_project: theo-db
created_at: 2026-07-27
revision: 2.1 — ADR-2's mechanism corrected mid-Phase-1: PG18 has no `lc_collate` GUC (falsified by the harness), so byte-order is read from `pg_database.datcollate` via the syscache. 2.0 — re-scoped against the ROADMAP DoD after measuring reality on the droplet (q23/q24 already route; q25 declines on collation OID; q26 on numCols). Supersedes rev 1.2, whose Phase 1 targeted the wrong harness (SEPA C1–C4) and whose ADR-2 deferred q25/q26 that the DoD requires.
goal: Make ClickBench q23–q26 (projection top-k) route to the columnar late-materialization path with byte-identical results — by flipping the boot default (q23/q24, already routable), replacing the text-collation OID allowlist with a byte-order predicate proven from PG's own `pg_database.datcollate` (q25), and generalizing the single-key guard to multi-key (q26) — with the O(N) decode bounded and a LIMIT-preserving, order-checked oracle proving each.
---

# M167 — projection top-k (q23–q26)

## Goal

Deliver the ROADMAP M167 DoD: q23, q24, q25 and q26 show the columnar late-mat Custom Scan in `EXPLAIN` with
byte-identical results vs the heap twin (`diverged = 0`), the `LIKE` filter still pushed down (composed with M156),
a text `ORDER BY` routing **only** where byte-order is provable, the late-mat GUC honored, and the M163/M164
harness exercising the projection top-k case. Success is measured on the same box, before vs after.

## Context

Discovery (`m167-projection-topk-blueprint.md`) plus a **measurement taken before writing any code**
(`docs/benchmarks/m167-baseline-and-routing-facts-2026-07-28.md`) falsify the ROADMAP entry's framing that the
late-mat projection path must be *implemented*. The M158 mechanism (`try_swap_topk`, `columnar_agg.rs:1813`)
already routes it. Measured with the GUC forced ON, no code change:

| Query | today | why |
|---|---|---|
| q23 | **routes** | single non-text key; `LIKE` filter pushes down |
| q24 | **routes** | single non-text key |
| q25 | declines | text key: guard admits only collation OID 950/951, column carries 100 (`default`) — in a `C\|C` database |
| q26 | declines | `numCols != 1` (`:1853`) — two keys |

So the milestone is three distinct pieces, not one: **flip the boot default** (q23/q24), **prove byte-order instead
of allowlisting OIDs** (q25), **generalize to multi-key** (q26). Baseline hot times, same box, GUC off:
q23 21.5151 s · q24 5.9088 s · q25 6.0517 s · q26 5.9517 s; suite 42/43 ok, `diverged = 0`.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Why it exists |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~2810 | M166 | `try_swap_topk` + the GUC default (`:33`), the single-key guard (`:1853`), the collation guard (`:1927–1939`), `encode_topk_private` |
| `theodb_rs/src/am/df_executor.rs` | ~1050 | M166 | `run_columnar_topk` (`:764`) — the decode + DataFusion `filter → sort → limit`; the stale "default OFF" comment (`:581`) |
| `benchmarks/m158_ec_harness.sql` | ~200 | M158 | **the correct top-k oracle** — LIMIT-preserving symmetric-EXCEPT over `t_col` (20 000 rows, `wid` UNIQUE) |
| `benchmarks/columnar_type_ab.py` | ~320 | M166 | type-coverage A/B — gains ONLY the collation cases (its scope note at `:258–260` correctly excludes the projection path) |
| `docs/benchmarks/m167-projection-topk-verdict.md` | new | — | the measured verdict |

### Current callers / dependents (real file:line)

- Boot gate: `GucSetting::<bool>::new(false)` (`columnar_agg.rs:33`) → `if !ENABLE_COLUMNAR_LATE_MAT.get() { return None }` (`:1818`).
- Single-key guard: `if (*sort).numCols != 1 { return None }` (`:1853`).
- Collation guard: `sort_coll != 950 && sort_coll != 951` on `(*(*sort).collations.add(0))` (`:1927–1939`).
- Key resolution: `sortColIdx[0]`, `sortOperators[0]`, `nullsFirst[0]` (`:1911–1950`) — all index 0, the shape multi-key must generalize.
- Wire: `encode_topk_private(table_oid, k, sort_attno, ascending, nulls_first, &proj_meta, &zpreds)` (`:2000`).
- Executor: `df.sort(vec![col(key).sort(asc, nulls_first)])?.limit(0, Some(k))` (`df_executor.rs:786`) — already a `Vec`.
- Decode: `decode_to_batch(...)` for ALL rows BEFORE the TopK (`df_executor.rs:775`); pool sized to fit (`:583`).
- `SearchSysCache1` / `SysCacheGetAttr` / `SysCacheGetAttrNotNull` / `ReleaseSysCache` / `text_to_cstring` are bound;
  `Anum_pg_database_datcollate = 13` (`pg18.rs:1707`). `pg_newlocale_from_collation` / `collate_is_c` are **not** bound,
  and PG18 has **no** `lc_collate` GUC (measured).

### Domain glossary

- **projection top-k** — `SELECT cols WHERE pred ORDER BY <column(s)> LIMIT k` (order by stored columns, not an aggregate).
- **late materialization** — decode {keys ∪ filter} for all N, TopK, materialize the payload for the k survivors only.
- **byte-order collation** — one whose sort equals `memcmp`. C (950), POSIX (951) always; `default` (100) iff the
  database's `datcollate` is C/POSIX. Determinism is NOT sufficient: it constrains equality, not order.
- **storage oracle vs top-k oracle** — a LIMIT-stripped, order-normalized compare proves the columnar storage
  returns the same rows; it says nothing about *which k* and *in what order*. Different gates.

### Architecture boundaries affected

Read-path planner swap (`planner_hook` → `try_swap_topk`) + one plan-time GUC read + the DataFusion sort expression
list. NO page-format / WAL / VACUUM / crash / upgrade surface — the invariants at risk are **snapshot correctness**
(the swap must return exactly the rows the native plan would) and **result-order correctness**, both proven by the
Phase-1 oracle. Per `theodb-evolution § invariant catalog`, no persistent-format invariant is touched, so no
upgrade/rollback story is required beyond the GUC.

## Prior Art & Related Work

- `.claude/knowledge-base/discoveries/blueprints/m167-projection-topk-blueprint.md`
- `.claude/knowledge-base/reviews/m167-projection-topk-edge-cases-2026-07-28.md` (EC-1..EC-6)
- `docs/benchmarks/m167-baseline-and-routing-facts-2026-07-28.md` (the measurement this plan is built on)
- M158 (`try_swap_topk` + `run_columnar_topk` + `benchmarks/m158_ec_harness.sql`), M156 (`LIKE` pushdown), M149
  (columnar-project scan), M131 (the `resolve_special_varno` deparse hazard on the 105-column `hits`).
- SOTA: DataFusion `TopK` (bounded heap), DuckDB `PhysicalTopN`, Abadi et al. ICDE 2007 (late materialization).

## Dependencies

**No dependency is added, removed, or version-changed.** Audited 2026-07-28
(`.claude/knowledge-base/audits/m167-projection-topk-deps-audit-2026-07-28.md`).

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `=0.19.0` (`Cargo.toml:37`) | rust | the extension framework; supplies `GetConfigOption` |
| `datafusion` | `54` (`:49`) | rust | `filter → sort → limit` bounded-heap TopK |
| `arrow` | `58` (`:50`) | rust | the `RecordBatch` the decode produces |
| `psycopg2-binary` | `>=2.9` | python | the A/B harness driver |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | | | | |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

### Known advisory — out of scope, declared

`GHSA-2f9f-gq7v-9h6m` (MEDIUM) in `thrift@0.17.0`, transitively `datafusion 54 → parquet 58.3.0 → thrift`, fixed in
0.23.0. Reachable in the product via `theodb.read_parquet` (thrift-encoded Parquet metadata, user-supplied file) but
**not on the M167 path** (`run_columnar_topk` operates on an in-memory `RecordBatch`; `df_executor.rs` never touches
parquet). Belongs to a `datafusion`/`parquet` bump slice.

## ADRs

### ADR-1 — flip `enable_columnar_late_mat` to default ON (no cost gate)
`columnar_agg.rs:33` `new(false)` → `new(true)`. **Rationale:** measured — q23/q24 already route when the GUC is on,
and a columnar table has no btree on the sort column, so native's only plan is Sort-over-projected-rows; late-mat is
structurally ≥ native for columnar top-k. **Alternative rejected:** a `plan_rows` cost gate — YAGNI on the perf axis;
no measured N where late-mat loses. **Alternative rejected:** keep OFF + a harness flag — leaves the user-facing win
behind a non-default flag, understating the product (TheoDB rule 5). Memory is a different axis → ADR-4.

### ADR-2 — prove byte-order from `pg_database.datcollate`; do not allowlist collation OIDs
Replace `sort_coll != 950 && sort_coll != 951` with a predicate: `950`/`951` → byte-order; `100` (`default`) →
byte-order **iff** the database's `datcollate` is `C` or `POSIX`, read from `pg_database` via the syscache
(`SearchSysCache1(DATABASEOID, MyDatabaseId)` + `SysCacheGetAttr(Anum_pg_database_datcollate = 13)` +
`text_to_cstring`, all bound in `pg18.rs`) and cached per backend; anything else (named ICU/libc collation) →
decline; unreadable catalog → decline (fail-closed). **Rationale:** the OID allowlist declines a *provably safe*
case — measured: a `C|C` database whose column carries OID 100. The ROADMAP DoD asks precisely for "text routes only
with a deterministic [byte-order] collation". **Alternative rejected:** `pg_newlocale_from_collation()` +
`pg_locale_struct.collate_is_c` — semantically the ideal predicate (PG's own), but pgrx 0.19 generates **no binding**
for either (verified in `pg18.rs`); using it means a hand-written `extern "C"` plus a struct-layout assumption across
the C boundary — new unsafe surface for no additional correctness. **Alternative FALSIFIED during Phase 1:** reading a
`lc_collate` GUC — **PG 18 has no such GUC** (`pg_settings` exposes only `lc_messages`/`monetary`/`numeric`/`time`;
it was removed when per-database collation became authoritative). The harness block errored with `unrecognized
configuration parameter "lc_collate"` before any Rust was written — the catalog read replaced it. **Alternative rejected:** keep declining q25 (rev
1.2's ADR-2) — fails the ROADMAP DoD, and "we cannot tell" is false: PG tells us.
**Honest limitation:** `datcollate` reports the *database* collation. A column or `ORDER BY … COLLATE "xx"` carrying
a named non-C collation still declines (correctly — the predicate only ever *adds* provable cases).

### ADR-3 — generalize the single-key guard to multi-key
Replace the `numCols != 1` early return with a loop over `0..numCols`, resolving `(attno, type, asc, nulls_first,
collation)` per key, applying the existing per-key guards (supported type; byte-order for a text key; non-bpchar;
btree ordering operator) to EVERY key, and encoding N keys in `custom_private`. The executor already takes a `Vec`
(`df_executor.rs:786`). **Rationale:** q26 is in the DoD, and the guards are per-key by nature — the `!= 1` check is
a scope limit, not a safety property. **Alternative rejected:** admit only 2 keys — an arbitrary cap with no
principle behind it. **Alternative rejected:** defer q26 (rev 1.2) — fails the DoD. **Fail-closed:** any key failing
any guard declines the whole swap; a key count above a small bound (`TOPK_MAX_SORT_KEYS = 8`) declines rather than
growing the int wire format unboundedly.

### ADR-4 — bound the O(N) decode with a plan-time size guard
`relpages × BLCKSZ > work_mem × TOPK_DECODE_WORK_MEM_FACTOR` → decline. **Rationale:** `run_columnar_topk`
decodes {projection ∪ keys ∪ filter} for all rows *before* the bounded-heap TopK (`df_executor.rs:775`), so the path
is O(N) memory where native's top-N heapsort is O(k). `df_executor.rs:576–581` cites "gated behind …, default OFF"
as the mitigation — ADR-1 removes exactly that. An unfiltered `SELECT * FROM hits ORDER BY EventTime LIMIT 10` is
admitted today (empty qual, `:1958`) and would decode the whole relation; the pool is sized to *fit* (`:583`), so the
failure is a backend OOM, not a typed error. **Alternative rejected:** a GUC for the threshold — YAGNI (parsimony
rung 1/5); a constant plus an `admit_trace` decline reason is observable enough. **Alternative rejected:** stream the
decode to make the path O(k) — real new executor mechanism, out of scope for this milestone; recorded as the honest
long-term fix. **Alternative rejected:** document the risk without a bound — it is an availability incident on an
ordinary query. **Mechanism FALSIFIED and corrected during Phase 2:** the first implementation used the planner's
`plan_rows × plan_width`. It never fired, and the measurement says why — the columnar TableAM reports no tuple count,
so `reltuples` stays **0** and the planner estimates **`rows=1 width=1604`** for a 1M-row relation (`EXPLAIN` of the
unfiltered wide top-k). A bound built on that is inert by construction. `pg_class.relpages` IS maintained (measured:
**27863** pages ≈ 228 MB for the same relation) and is the honest size signal; the guard now reads it via the syscache
(`Anum_pg_class_relpages = 10`), the same pattern as ADR-2's `datcollate` read.
**Honest limitation:** `relpages` is a STATISTIC (ANALYZE/VACUUM), so it lags a relation that just grew; and the
on-disk bytes are COMPRESSED, so the decoded Arrow batch is LARGER than the estimate. The second dominates, which is
the safe direction for a ceiling — the guard under-estimates the decode and therefore only ever declines relations
that are at least that big. It does NOT account for a selective predicate reducing what is decoded, so a highly
selective query over a large relation can be declined even though its decode would have been small.

### ADR-5 — the top-k oracle is `m158_ec_harness.sql`, not `columnar_type_ab.py`
Extend the existing M158 harness (`t_col`, 20 000 rows, `wid` **UNIQUE** so the top-k boundary has no ties) with the
q23–q26 shapes. **Rationale:** it is purpose-built for this path — LIMIT-preserving symmetric-EXCEPT over full rows,
comparing GUC-off vs GUC-on. Three measured facts make `columnar_type_ab.py` the wrong home: its own scope note
excludes this path (`:258–260`), its `SET enable_sort = off` (`:44`) suppresses the `Sort` node `try_swap_topk`
requires (so decline cases would pass vacuously), and its fixture cycles ≤ N distinct values per column (`:100–104`)
so **no column is unique** — the tie-free full-row comparison EC-4 needs cannot be built there. **Alternative
rejected:** build a third harness — parsimony rung 4, the asset exists. **Alternative rejected:** rev 1.2's plan of
adding cases to `columnar_type_ab.py` — falsified by the three facts above.
**Scope carve-out:** `columnar_type_ab.py` DOES gain the *collation* cases (ADR-2), which are type-class, its actual
remit; and its `:258–260` scope note is updated in the same commit.

### ADR-6 — the 1M correctness gate is the top-k oracle, not the 43/43 storage A/B
`run_m128_clickbench.py:283` strips the trailing LIMIT and `_canonical` (`:243`) sorts both sides, so with no `Limit`
node `try_swap_topk` declines (`:1822`) and the A/B compares native-on-columnar vs native-on-heap. `result_ab_identical`
for q23–q26 is therefore true of a query that never routes. **Decision:** keep the 43/43 as the no-regression
*storage* gate it is (and label it so), and prove top-k correctness with the Phase-1 oracle. **Alternative rejected:**
cite 43/43 as correctness evidence — unsupported by the cited measurement (TheoDB rule 5). **Alternative rejected:**
stop stripping the LIMIT — the strip exists because tied aggregate counts make the cut arbitrary-but-valid across the
other 41 queries.

## Dependency Graph

Phase 1 (oracle) → Phase 2 (flip + decode guard) → Phase 3 (collation) → Phase 4 (multi-key) → Phase 5 (verdict).
Phase 1 first is load-bearing: the oracle that would catch a wrong top-k must exist before the changes that expose or
extend the path. Phases 3 and 4 are independent of each other but both build on Phase 2's default being ON.

## Phase 1 — the top-k oracle (build the gate first)

### T1.1 — extend `m158_ec_harness.sql` with the q23–q26 shapes
#### Why this step
The action: add four query blocks mirroring ClickBench q23–q26 against `t_col` — wide `SELECT *` + filter + single
non-text key; narrow projection + single non-text key; narrow + **text** key; narrow + **multi-key** — each run under
`enable_columnar_late_mat = off` then `= on`, compared by symmetric `EXCEPT` over FULL rows (tie-free because `wid`
is unique), plus an `EXPLAIN` assertion that the ON arm actually routed. Add a **negative control**: one query whose
ON arm is seeded to differ must report a non-zero symmetric difference, proving the oracle can fail.
The reasoning: rev 1.2 put these in the wrong harness (ADR-5). This one already compares GUC-off vs GUC-on with
LIMIT preserved — exactly the contract, and it needs no fixture surgery.
#### Files to edit
- `benchmarks/m158_ec_harness.sql`
#### TDD
Run: `psql -f benchmarks/m158_ec_harness.sql` on the droplet cluster.
- RED: test_m167_shapes_present_in_harness — `assert grep -c 'M167' benchmarks/m158_ec_harness.sql >= 4` fails before the blocks exist.
- RED: test_m167_negative_control_diverges — the seeded-divergence block `assert symmetric_diff_count > 0`;
  fails before the control exists (an oracle that cannot fail is not an oracle, `rules/testing.md` § 5.1).
- GREEN: test_m167_all_shapes_zero_diff — for each of the four blocks, `assert symmetric_diff_count == 0`
  AND `assert 'Custom Scan' in explain_on_arm` for the shapes expected to route at that point in the sequence
  (at Phase 1 the text and multi-key arms still legitimately decline — the harness records which arm routed, so the
  same file becomes the regression gate for Phases 3 and 4 without editing).
#### Concurrency tests
(none — single-threaded)
The harness runs serially in one psql session.
#### Acceptance criteria
- `psql -f benchmarks/m158_ec_harness.sql` exits 0 with every symmetric-difference count reported and the four M167
  blocks present.
- The negative control reports `> 0`; every non-seeded block reports `0`.
- Each block prints its `EXPLAIN` so "routed" is evidence, not assumption.

## Phase 2 — flip the default, bounded

### T2.1 — flip `enable_columnar_late_mat` default ON
#### Why this step
The action: `columnar_agg.rs:33` `new(false)` → `new(true)`; update the three stale "default OFF" comments (`:30`,
`:1812`, `df_executor.rs:581` — the last must cite ADR-4 as the O(N) mitigation instead of the default) AND
`docs/benchmarks/m158-late-mat-verdict.md:50`, which otherwise contradicts the shipped default.
The reasoning: measured — this boot value is the only thing keeping q23/q24 off the path.
#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`, `theodb_rs/src/am/df_executor.rs`, `docs/benchmarks/m158-late-mat-verdict.md`
#### TDD
- RED: test_q23_q24_route_at_default — with the rebuilt `.so` and NO session `SET`, for q23 and q24
  `assert 'Custom Scan (theodb_columnar_agg)' in explain` and `assert 'Sort' not in explain`.
- RED: test_explain_verbose_returns_on_wide_hits — `EXPLAIN (VERBOSE, COSTS OFF)` of q23 and q24 on the 105-column
  `hits` `assert elapsed_s < 30` (EC-3 / M131 deparse recursion, uninterruptible by `statement_timeout`).
- GREEN: test_guc_off_restores_native_sort — after `SET theodb.enable_columnar_late_mat = off`,
  `assert 'Sort' in explain_q24`.
#### Concurrency tests
(none — single-threaded)
The harness sets `max_parallel_workers_per_gather = 0`, and the node copies `parallel_safe` from the Sort exactly as
the shipped agg node has since M110.
#### Acceptance criteria
- q23 and q24 route with no session `SET`; `EXPLAIN (VERBOSE)` returns on the wide table.
- `SET … = off` restores the native `Sort` plan (GUC honored both ways).
- Full `run_m128_clickbench.py --agg` at 1M reports 43/43 `result_ab_identical == true` — the **storage** oracle
  (ADR-6), i.e. no regression, NOT top-k correctness.
- The Phase-1 harness re-run reports `symmetric_diff_count == 0` for every non-seeded block.

### T2.2 — bound the decode with a plan-time size guard
#### Why this step
The action: inside `try_swap_topk`, after the table OID is resolved, decline when the relation's size exceeds
`work_mem × TOPK_DECODE_WORK_MEM_FACTOR`, emitting `admit_trace("topk_decode_estimate_too_large")`.
The reasoning: ADR-1 removes the mitigation `df_executor.rs:576–581` relies on; the bound has to come from somewhere.
Declining falls back to the native plan, correct for any input — fail-closed like every other guard here.
#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`
#### TDD
- RED: test_topk_declines_unfiltered_wide_projection — with a small `work_mem`,
  `assert 'Custom Scan' not in explain('SELECT * FROM hits ORDER BY EventTime LIMIT 10')` and `assert 'Sort' in explain`.
  Fails before the guard (the query routes and decodes the whole relation).
- GREEN: test_topk_still_routes_within_budget — at a `work_mem` whose budget exceeds the relation size (measured:
  64MB → 512MB budget vs 228MB relation), `assert 'Custom Scan (theodb_columnar_agg)' in explain(q23)` and the same
  for q24. NOTE the guard is per-RELATION, not per-query: at `work_mem = 4MB` every top-k on `hits` declines,
  including q23/q24 — so the budget must be stated for this assertion to be satisfiable at all.
- REFACTOR: the factor is one named constant whose comment states it is a heuristic safety factor, that the
  estimate reads `pg_class.relpages` and falls back to the true physical size when that statistic is absent, and
  that under-estimation is the DANGEROUS direction for an OOM bound (false admits), not the safe one.
#### Concurrency tests
(none — single-threaded)
#### Acceptance criteria
- The unfiltered wide top-k declines with the `topk_decode_estimate_too_large` trace; q23/q24 still route.
- Peak RSS during q23 with late-mat ON is measured (`/usr/bin/time -v` max RSS in kB) and written to the verdict.

## Phase 3 — byte-order collation predicate (q25)

### T3.1 — replace the collation OID allowlist with a proven byte-order predicate
#### Why this step
The action: extract `sort_collation_is_byte_order(coll: u32) -> bool` — `950`/`951` true; `100` true iff
the database `datcollate` (syscache, cached per backend) is `C`/`POSIX`; else false; unreadable → false. Use it at `:1935`.
The reasoning: measured — a `C|C` cluster declines q25 only because the column carries OID 100. PG reports the
database collation; we stop guessing (ADR-2).
#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`, `benchmarks/columnar_type_ab.py` (collation cases + the `:258–260` scope note)
#### TDD
- RED: test_q25_routes_under_c_locale — on the `datcollate = C` cluster, with no session `SET`,
  `assert 'Custom Scan (theodb_columnar_agg)' in explain(q25)` and `assert 'Sort' not in explain(q25)`.
  Fails today (measured: `Sort` present, guard rejects OID 100).
- RED: test_named_non_c_collation_still_declines — `ORDER BY SearchPhrase COLLATE "en_US.utf8"`
  `assert 'Custom Scan' not in explain` (the predicate must only ADD provable cases).
- GREEN: test_q25_topk_identical — the Phase-1 harness text block `assert symmetric_diff_count == 0`.
#### Concurrency tests
(none — single-threaded)
#### Acceptance criteria
- On the `datcollate = C` cluster `assert 'Custom Scan' in explain(q25)`, its harness block reports `symmetric_diff_count == 0`, and `ORDER BY … COLLATE "en_US.utf8"` reports `assert 'Custom Scan' not in explain`.
- `columnar_type_ab.py` gains the collation cases and its scope note no longer excludes this path.

## Phase 4 — multi-key top-k (q26)

### T4.1 — generalize the single-key guard and wire to N keys
#### Why this step
The action: loop `0..numCols` resolving each key's `(attno, type, asc, nulls_first, collation)`, applying every
existing per-key guard to each; encode N keys in `custom_private` (bounded by `TOPK_MAX_SORT_KEYS = 8`); build
`df.sort(vec![...])` with one expression per key.
The reasoning: q26 is in the DoD; the guards are per-key by nature and the executor already takes a `Vec`.
#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs`, `theodb_rs/src/am/df_executor.rs`
#### TDD
- RED: test_q26_routes_multikey — `assert 'Custom Scan (theodb_columnar_agg)' in explain(q26)` and
  `assert 'Sort' not in explain(q26)`. Fails today (`numCols != 1`).
- RED: test_multikey_declines_when_any_key_fails_a_guard — `ORDER BY EventTime, <bpchar col>`
  `assert 'Custom Scan' not in explain` (fail-closed per key).
- RED: test_multikey_over_bound_declines — a sort with more than `TOPK_MAX_SORT_KEYS` keys `assert 'Custom Scan' not in explain`.
- GREEN: test_q26_topk_identical — the Phase-1 harness multi-key block `assert symmetric_diff_count == 0`,
  including a case where the FIRST key ties so the second key decides the order.
#### Concurrency tests
(none — single-threaded)
#### Acceptance criteria
- `assert 'Custom Scan' in explain(q26)` and the multi-key harness block reports `symmetric_diff_count == 0`, including the first-key-ties case.
- A key failing any guard reports `assert 'Custom Scan' not in explain`, and a sort with more than `TOPK_MAX_SORT_KEYS` keys reports the same.

## Phase 5 — measured verdict

### T5.1 — before/after benchmark + verdict artifact
#### Why this step
The action: re-run `run_m128_clickbench.py --agg --n 1000000 --sample systematic` on the same box, compare against
`docs/benchmarks/m167-baseline-and-routing-facts-2026-07-28.md`, and write the verdict.
The reasoning: TheoDB rule 5 — a performance number is a claim; a claim without a reproducible same-box artifact does
not ship.
#### Files to edit
- `docs/benchmarks/m167-projection-topk-verdict.md` (new), `CHANGELOG.md`
#### TDD
- RED: test_m167_verdict_artifact_exists — `assert Path('docs/benchmarks/m167-projection-topk-verdict.md').exists()`
  and `assert 'M167' in Path('CHANGELOG.md').read_text()`.
- GREEN: test_m167_verdict_attributes_each_claim_to_its_oracle — `assert 'LIMIT-stripped' in text` (the 43/43 storage
  A/B) and `assert 'm158_ec_harness' in text` (the top-k correctness gate), so ADR-6's distinction survives into the
  published artifact.
#### Concurrency tests
(none — single-threaded)
#### Validation
- Before/after hot times for q23–q26 on the same box; 43/43 storage A/B; every harness block 0; peak RSS for q23.
- Honest statement of what remains unmeasured (see § Unresolved Questions).

## Coverage Matrix

| Goal claim / ROADMAP DoD item | Task(s) |
|---|---|
| q23, q24 show the late-mat Custom Scan, `diverged = 0` | T2.1 (route), T1.1 (oracle proves identity) |
| q25 shows it — text key with provable byte-order | T3.1, T1.1 |
| q26 shows it — multi-key | T4.1, T1.1 |
| `LIKE` filter still routed (composed with M156) | T2.1 (q23 carries the filter; EXPLAIN asserts one node) |
| Text `ORDER BY` routes ONLY with byte-order collation | T3.1 (predicate + named-collation decline test) |
| LIMIT-k heapsort is O(k), not an O(N) tuple batch | T2.2 (bound the decode), T5.1 (measure peak RSS) |
| late-mat GUC honored | T2.1 (`off` restores native Sort) |
| M163/M164 harness exercises the projection top-k case | T1.1 (m158 harness), T3.1 (type-class collation cases) |
| No regression across the suite | T2.1 (43/43 storage A/B) |
| CHANGELOG `[Unreleased]` | T5.1 |
| Measured before/after evidence | T5.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| O(N) decode becomes the default and OOMs a backend on an unfiltered wide top-k (EC-1) | **High** | ADR-4 guard (T2.2), fail-closed to native; `admit_trace` makes declines observable | me |
| The guard is a lower bound (post-qual `plan_rows`, average `plan_width`, Arrow buffers unaccounted) | Medium | stated in ADR-4 and in the code comment; the dangerous unfiltered case is where the estimate is tight; `reltuples` tightening is an Unresolved Question | me |
| Routing becomes cluster-locale-dependent — the same query routes in a C cluster and declines in an en_US one (ADR-2) | Medium | that IS the correct semantic (byte-order is a property of the collation); the predicate only ever adds provable cases, never removes a decline; documented in the verdict | me |
| Multi-key widens the planner surface and the int wire format | Medium | per-key fail-closed guards + `TOPK_MAX_SORT_KEYS` bound; first-key-ties test proves the second key actually decides | me |
| Key↔payload misalignment invisible to a key-only oracle (EC-4) | High | ADR-5: full-row symmetric-EXCEPT over a tie-free `wid` | me |
| A future edit admits a non-byte-order text key → silent wrong top-k | High | the named-collation decline test (T3.1) + the harness text block | me |
| Default-ON changes behavior for users relying on OFF | Low | byte-identical results, only faster; GUC still forces OFF | me |
| Validated only in the serial plan shape (harness sets `max_parallel_workers_per_gather = 0`) (EC-6) | Low | accepted, not tested; same `parallel_safe` pattern the agg node has shipped since M110 | me |

## Unresolved Questions

- Whether the ADR-4 guard should estimate from `pg_class.reltuples` (pre-qual, matching what is actually decoded)
  instead of the post-qual `plan_rows`. Deferred: needs relation-cache access inside `planner_hook`, and the
  lower-bound form already covers the motivating case. Revisit if a decline/OOM report shows it under-firing.
- Whether the O(N) decode should become a streaming O(k) top-k, removing the need for the guard. New executor
  mechanism; recorded as the honest long-term fix (ADR-4).
- Behavior of the top-k path beyond 1M rows is unmeasured in this milestone (M162 measured the *scan* at 100M and
  found a `byte array offset overflow` at Arrow varlena > 2 GB — that class may also reach a wide top-k decode).

## Failure scenarios

No external I/O (in-process planner/executor over local columnar state). The resource-failure scenarios:

| Scenario | Required behavior |
|---|---|
| Estimated decode exceeds the work_mem-derived bound | `try_swap_topk` returns `None` → native plan → correct result, O(k) memory; reason visible under `THEODB_ADMIT_TRACE=1` |
| Decode fits the bound but still exhausts backend memory (estimate under-fired) | Backend allocation failure — the pre-existing M158 behavior, narrowed but not eliminated; stated in ADR-4, not claimed fixed |
| `pg_database.datcollate` unreadable via syscache | predicate returns false → text key declines (fail-closed) |

## Global Definition of Done

- **Oracle first:** `benchmarks/m158_ec_harness.sql` carries the four M167 shapes + a negative control; every
  non-seeded block reports symmetric difference `0`; the control reports `> 0`; each block prints its `EXPLAIN`.
- **q23–q26 all route by default:** `EXPLAIN (VERBOSE, COSTS OFF)` with no session `SET` shows
  `Custom Scan (theodb_columnar_agg)` and no `Sort` for all four, and RETURNS on the 105-column `hits`.
- **Correctness:** every M167 harness block `0`, including the first-key-ties multi-key case.
- **Guards hold:** a named non-C collation declines; a guard-failing key declines; above `TOPK_MAX_SORT_KEYS` declines;
  the unfiltered wide top-k declines with `topk_decode_estimate_too_large`.
- **No regression:** `run_m128_clickbench.py --agg` at 1M reports 43/43 `result_ab_identical == true`, documented as
  the LIMIT-stripped storage oracle.
- **GUC honored:** `SET theodb.enable_columnar_late_mat = off` restores the native `Sort` plan.
- **Measured evidence:** `docs/benchmarks/m167-projection-topk-verdict.md` carries before/after hot times for q23–q26
  on the same box, peak RSS for q23, per-claim oracle attribution, and the honest unmeasured edges;
  `grep M167 CHANGELOG.md` non-empty.
- **Gates:** `run_structural.py` ≥ SHIPPABLE_WITH_CAVEATS; `/code-quality` ∉ {FAIL_HARD, INVALID};
  `/review` READY_TO_MERGE.
