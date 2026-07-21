---
slug: columnar-agg-planner-hang-fix
milestone_id: M131
created_at: 2026-07-21
goal: Fix the self-referential custom_scan_tlist so EXPLAIN deparse of an ORDER-BY-aggregate over the swapped columnar-agg CustomScan terminates, and measure the columnar-accelerated ClickBench byte-identical vs heap
---

# Plan — M131 Fix #135 (columnar-agg CustomScan EXPLAIN deparse infinite recursion)

## Goal

Replace the self-referential `custom_scan_tlist` in `try_swap_agg` with a deparse-safe one (real base-rel Vars for
group keys, `Aggref`s with base-rel arg Vars for aggregates) so `resolve_special_varno` terminates, making
`EXPLAIN` of the two ClickBench queries that hang (Q16, Q33) complete in **< 1 s**, with the columnar-accelerated
ClickBench run measured byte-identical vs heap.

**Single metric:** `EXPLAIN` of all 43 ClickBench queries with `theodb.enable_columnar_agg = on` completes with
**0 hangs** (currently 2), each in < 1 s, recorded in `docs/benchmarks/m131-columnar-agg-accelerated.md`.

## Context

Implements the fix for issue **#135**. The discovery blueprint
(`knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md`) FALSIFIED the issue's hypothesis via
a live gdb backtrace: the hang is **not** a planner-cost pathology on wide/mixed-type tables, it is an **infinite
recursion in `ruleutils.c::resolve_special_varno` during EXPLAIN deparse of a Sort key** that references the
aggregate output of the swapped CustomScan. It is EXPLAIN-only (Q16 executes in 0.537 s with correct results) and is
triggered by `ORDER BY <aggregate>` above the CustomScan, not by table width. Unblocking it enables the
columnar-accelerated ClickBench run (the M128 harness currently runs with the aggregate pushdown OFF).

## Baseline Context

Repo state: git sha `fb339bf`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | 1179 | M115 Agg-swap: `try_swap_agg` builds the replacement CustomScan; L658 sets `custom_scan_tlist = plain_var_tlist(tlist)` (the defect) | Build a deparse-safe `custom_scan_tlist` from the admission metadata + the original Aggrefs. Add a unit test. |
| `benchmarks/run_m128_clickbench.py` | 260 | ClickBench harness; forces `SET theodb.enable_columnar_agg = off` (the #135 workaround) | Add a flag to run with the aggregate pushdown ON (the accelerated A/B). |
| `docs/benchmarks/m131-columnar-agg-accelerated.md` | — | (NEW) | Evidence: 43-query EXPLAIN hang-count, accelerated vs storage-path timings, byte-identical A/B. |

### Current callers / dependents (verified `file:line`)

- `theodb_rs/src/am/columnar_agg.rs:567` — `try_swap_agg(plan, rtable)`, called from `swap_walk` (`:665`), called from `planner_hook` (`:481`) post-`standard_planner`.
- `theodb_rs/src/am/columnar_agg.rs:514` — `plain_var_tlist(tlist)` — used for BOTH `plan.targetlist` (L641, correct) and `custom_scan_tlist` (L658, the defect).
- `theodb_rs/src/am/columnar_agg.rs:586` — `find_scan_relid` yields the base-rel `scanrelid` (still valid in `rtable` after the child plan node is dropped) — the varno source for the deparse-safe Vars.
- `theodb_rs/src/am/columnar_agg.rs:240` — `admit()` produces `Admitted { aggs (kind, attno), group_cols (attno, typoid), layout }` — the metadata to rebuild the output expressions.
- `benchmarks/run_m128_clickbench.py` — the 43-query byte-identical A/B oracle (M128).

### Domain glossary

- **`custom_scan_tlist`** — for a `scanrelid = 0` CustomScan, the targetlist that DESCRIBES the node's output columns. Consumed by `set_customscan_references` and by ruleutils (EXPLAIN deparse). Not executed.
- **`resolve_special_varno`** — the ruleutils routine that resolves a special-varno Var (`INDEX_VAR`/`OUTER_VAR`/`INNER_VAR`) down through child targetlists to a real expression, so EXPLAIN can print it. Terminates on a non-special expression.
- **M115 Agg-swap** — the architecture: the CustomScan is inserted AFTER `set_plan_refs`, so setrefs never re-processes it; EXPLAIN deparse is the only consumer of `custom_scan_tlist`.
- **Storage path vs accelerated path** — ClickBench with `enable_columnar_agg = off` measures columnar STORAGE only (heap-equivalent latency); ON engages the vectorized aggregate pushdown.

### Architecture boundaries affected

Per `rules/architecture.md`: a localized change inside the existing columnar CustomScan planner-integration module
(`theodb_rs/src/am/columnar_agg.rs`). No new module, no API change, no change to the executed `plan.targetlist`
(the M115 invariant "no Aggref in the executed tlist" is preserved). Benchmark tooling change is additive (a flag).

## Prior Art & Related Work

- Blueprint (live gdb, 2026-07-21): `knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md` — the backtrace, the falsification of the #135 hypothesis, and the fix design.
- PostgreSQL `ruleutils.c::resolve_special_varno` + `setrefs.c::set_customscan_references` — the contract that `custom_scan_tlist` entries describe output columns as REAL expressions; Citus / TimescaleDB grouped CustomScans follow it.
- Internal: `docs/benchmarks/columnar-groupby-verdict.md` (the M115 composability verdict + the INDEX_VAR caveat), `docs/benchmarks/m128-clickbench-columnar.md` (the storage-path run this milestone upgrades).

## ADRs

### ADR M131-1 — fix the `custom_scan_tlist` content; do NOT add the planner-latency guard #135 suggested

**Decision:** build a deparse-safe `custom_scan_tlist` (group keys → `Var(base_rel_varno, attno)`; aggregates →
`Aggref` whose arg Var is a base-rel Var). Keep `plan.targetlist = plain_var_tlist` unchanged. Do NOT add a
width/type "planner-latency guard".

**Rationale (cites the blueprint + Rule 3):** the gdb backtrace proves there is no planner-cost pathology — the cost
is copied from the standard_planner Agg (`columnar_agg.rs:646-647`) and planning is cheap (a non-ORDER-BY GROUP BY
plans in 27 ms). Guarding a hang that does not exist would be cargo-cult defense; the honest defense-in-depth is the
regression test on the ACTUAL trigger plus the byte-identical accelerated A/B.

**Alternatives rejected:**
- **Width/type planner guard (the #135 suggestion)** — REJECTED: guards a non-existent pathology; the hang is in EXPLAIN deparse.
- **`custom_scan_tlist = tlist` (the post-setrefs Agg targetlist)** — REJECTED: its group-key Vars are `OUTER_VAR` referencing the dropped child subtree; deparse would follow `OUTER_VAR` into a null `lefttree`.
- **Decline the swap when an ORDER BY references the aggregate** — REJECTED: that disables acceleration on exactly the ClickBench queries that need it.

### ADR M131-2 — the regression test asserts the real trigger (EXPLAIN + ORDER BY aggregate), not table width

**Decision:** the regression test executes `EXPLAIN` of a GROUP BY with `ORDER BY <aggregate>` over a columnar table
with the pushdown ON, and asserts it completes (and that the plan engages the CustomScan) — the exact #135 trigger.
It does NOT assert anything about column count or column types.

**Rationale (cites the blueprint):** the wide/TEXT correlation in #135 was coincidental. A width-based test would
pass while the real defect regressed.

**Alternatives rejected:** a 100+-column TEXT-heavy fixture table (the #135 suggestion) — REJECTED: tests the wrong
invariant and is slow to build.

## Dependencies

`## Dependencies`: **none new**. The fix uses `pg_sys` (pgrx 0.19.0, already declared in `theodb_rs/Cargo.toml`) —
`makeVar`, `makeTargetEntry`, `copyObject` — and the Python harness change reuses `psycopg` (already in
`benchmarks/requirements.txt`). No crate/pip added (parsimony rung 4 — reuse what is installed).

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Deparse-safe `custom_scan_tlist` so `resolve_special_varno` terminates | T1.1 |
| EXPLAIN of the ORDER-BY-aggregate shapes (Q16/Q33) completes < 1 s; 0/43 hangs | T2.1 |
| Regression test on the real trigger | T1.2 |
| Columnar-accelerated ClickBench MEASURED byte-identical vs heap | T3.1 |

## Phase 1 — the fix

### T1.1 — build a deparse-safe `custom_scan_tlist`

#### Why this step
The defect is `custom_scan_tlist[i].expr = Var(INDEX_VAR, i)`, which makes `resolve_special_varno` recurse into the
same entry forever. Reasoning: replace each entry with a NON-special expression built from the admission metadata —
group keys become `Var(scanrelid, group_attno, typoid)` (a real varno; the RTE survives dropping the child plan
node), aggregates keep their `Aggref` with any arg Var rebuilt as a base-rel Var (so deparse never follows
`OUTER_VAR` into the dropped subtree). `plan.targetlist` is untouched (M115 invariant preserved).

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` (replace the `custom_scan_tlist` construction at L658 with a new
  `deparse_safe_tlist(tlist, &adm, scanrelid)` helper).

#### TDD
- RED: `test_deparse_safe_tlist_has_no_special_varno` — build a tlist + an `Admitted`, call the helper, assert NO entry's expr is a `Var` with `varno ∈ {INDEX_VAR, OUTER_VAR, INNER_VAR}` (the termination condition of `resolve_special_varno`).
- GREEN: implement `deparse_safe_tlist`.
- REFACTOR: keep `plain_var_tlist` unchanged for `plan.targetlist`.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `test_deparse_safe_tlist_has_no_special_varno` asserts `count == 0` for entries whose expr is a `Var` with `varno ∈ {INDEX_VAR, OUTER_VAR, INNER_VAR}`, and `cargo test` exits 0.
- `grep -c "plan_out.targetlist = plain_var_tlist(tlist)" theodb_rs/src/am/columnar_agg.rs` returns `1` (the executed output is provably unchanged).

#### DoD
- `cargo build` exits 0; `cargo test -p theodb_rs columnar_agg` exits 0.

### T1.2 — regression test on the real trigger

#### Why this step
ADR M131-2: the test must assert the ACTUAL trigger (EXPLAIN + ORDER BY aggregate over the swapped CustomScan), so a
regression cannot slip through a width-based test. Reasoning: an in-PG A/B — with the pushdown ON, `EXPLAIN` the
Q16-shaped query and assert it returns a plan mentioning `theodb_columnar_agg` (i.e. it engaged AND deparsed).

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` (test module).

#### TDD
- RED: a test that EXPLAINs a `GROUP BY k ORDER BY count(*) DESC LIMIT 10` over a columnar table with the pushdown on, asserting the plan text is produced and contains the CustomScan (before the fix this hangs / cannot deparse).
- GREEN: the T1.1 fix makes it pass.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- The test EXPLAINs the Q16-shaped query and asserts the returned plan text contains the literal `theodb_columnar_agg`; `cargo test` exits 0 on the fixed build (and the same test times out / errors on the pre-fix build, recorded once).

#### DoD
- Test present and green on the fixed build.

## Phase 2 — measured evidence

### T2.1 — 43-query EXPLAIN hang sweep (0 hangs)

#### Why this step
The single metric. Reasoning: re-run the EXPLAIN sweep over all 43 ClickBench queries with the pushdown ON under a
per-query timeout, and record the hang count (currently 2: Q16, Q33) and per-query planning time.

#### Files to edit
- `docs/benchmarks/m131-columnar-agg-accelerated.md` (NEW); `docs/benchmarks/m131-explain-sweep.json` (NEW).

#### TDD
- RED: the sweep on the pre-fix build records `hung = 2`.
- GREEN: on the fixed build it records `hung = 0`, each EXPLAIN < 1 s.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- The sweep script prints `hung=0` over the 43 queries and records `max_explain_ms < 1000` in `docs/benchmarks/m131-explain-sweep.json`; the Q16 and Q33 entries each show `customscan=1`.

#### DoD
- The sweep JSON + the markdown record the before/after counts with the measured times.

### T3.1 — columnar-accelerated ClickBench, byte-identical vs heap

#### Why this step
The value that unblocks a defensible columnar rank: prove the ACCELERATED path (pushdown ON) is both correct
(byte-identical to heap) and faster than the storage path on the same box. Reasoning: add an `--agg-on` flag to the
M128 harness and run the full 43-query suite with the byte-identical A/B oracle preserved.

#### Files to edit
- `benchmarks/run_m128_clickbench.py`; `docs/benchmarks/m131-columnar-agg-accelerated.md`.

#### TDD
- RED: a harness unit test asserting the agg GUC statement reflects the flag (`on` when set, `off` by default).
- GREEN: implement the flag.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- The accelerated run diverges from heap on some query → the byte-identical A/B reports it loudly per query (never silently passed); the honest outcome is recorded rather than the query being dropped.
- The DB connection drops mid-sweep → the harness clean-exits `UNBENCHMARKED` with the reason (no fabricated timings).

#### Acceptance criteria
- `run_m128_clickbench.py --agg-on` prints `byte-identical 43/43` and writes per-query `accelerated_ms` + `storage_path_ms` into the M131 JSON; the evidence markdown contains the literal `NOT canonical hardware` (grep-asserted).

#### DoD
- Evidence markdown + JSON committed; CHANGELOG updated.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Rebuilding `theodb_rs` on the shared droplet is slow and could fail | MEDIUM | Build once, verify with the EXPLAIN sweep; if the build fails, record honestly and do not claim the fix | engine |
| The rebuilt `Aggref` arg Var could mis-deparse for a multi-arg/edge aggregate shape | MEDIUM | The admitted set is narrow (count(*), sum/avg/min/max of ONE bare column — `columnar_agg.rs:318-369`); the unit test asserts no special varno remains, and the 43-query sweep exercises the real shapes | engine |
| The accelerated path might be SLOWER than the storage path on some queries | LOW | Report measured per-query numbers honestly (a slower query is a finding, not something to hide); no claim without the artifact | benchmarks |
| Fixing deparse could mask a latent setrefs issue if the swap ever moves pre-setrefs | LOW | Document the post-setrefs assumption in the code comment; the M115 invariant is unchanged by this fix | engine |

## Unresolved Questions

- Does the rebuilt `Aggref` deparse identically to the native plan's text for every admitted aggregate kind? The 43-query sweep answers it empirically; any mismatch is cosmetic (EXPLAIN text) and recorded, not hidden.
- Will the accelerated ClickBench beat the storage path on ALL aggregation queries, or only some? Measured, not assumed.

## Failure scenarios

- **Deparse still recurses after the fix** (some entry remains a special-varno Var): the EXPLAIN sweep still records a hang for that query; the unit test's no-special-varno assertion localizes it. Reproduced by the sweep under a per-query timeout.
- **`cargo pgrx install` fails on the droplet**: no fixed `.so` → the sweep is not run and the artifact is `UNBENCHMARKED` with the build error (no fabricated "0 hangs").
- **A ClickBench query diverges byte-wise with the pushdown ON**: the A/B oracle reports the query id + the divergence; the honest outcome is recorded (this is exactly the correctness oracle the pillar retains).

## Global Definition of Done

- [ ] `cargo test -p theodb_rs columnar_agg` exits 0 and `test_deparse_safe_tlist_has_no_special_varno` asserts that **0 of N** `custom_scan_tlist` entries is a `Var` with `varno ∈ {INDEX_VAR, OUTER_VAR, INNER_VAR}`.
- [ ] The 43-query EXPLAIN sweep script prints `hung=0` (measured `hung=2` before the fix) with **max per-query EXPLAIN time < 1000 ms**, recorded in `docs/benchmarks/m131-explain-sweep.json`.
- [ ] `EXPLAIN` of Q16 and Q33 each returns a plan whose text contains the literal `theodb_columnar_agg` (asserted by the sweep script; proves the CustomScan engaged AND deparsed).
- [ ] The regression test EXPLAINs a `GROUP BY k ORDER BY count(*) DESC LIMIT 10` over a columnar table with the pushdown ON and asserts the returned plan text is non-empty and contains `theodb_columnar_agg`; `cargo test` exits 0.
- [ ] `python3 benchmarks/run_m128_clickbench.py --agg-on` reports **43/43 byte-identical** vs heap and writes per-query accelerated-vs-storage-path milliseconds into `docs/benchmarks/m131-columnar-agg-accelerated.md` + its JSON.
- [ ] `git status --porcelain` is empty after committing `docs/benchmarks/m131-columnar-agg-accelerated.md`, its JSON, and a `CHANGELOG.md` `[Unreleased] § Fixed` entry; `gh issue view 135` shows the issue closed with a comment stating the measured root cause (EXPLAIN deparse recursion, NOT a planner hang).
- [ ] The evidence markdown contains the literal string `NOT canonical hardware` and contains **no** unqualified `faster than` claim (grep-asserted).

## Final Phase — Integration Validation

- `cargo build` + unit tests green.
- Rebuild on the droplet; run the 43-query EXPLAIN sweep (expect 0 hangs) and the accelerated ClickBench A/B.
- council-benchmark + council-rust-pgrx review: real measurement vs supposition; unsafe/FFI safety of the tlist construction.
