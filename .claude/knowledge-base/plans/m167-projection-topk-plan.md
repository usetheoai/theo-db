---
slug: m167-projection-topk
milestone_id: M167
target_project: theo-db
created_at: 2026-07-27
revision: 1.3 — Phase 1 re-scoped onto the EXISTING `benchmarks/m158_ec_harness.sql` after the SEPA initial brief and source verification showed the planned host (`columnar_type_ab.py`) declares this path out of scope, suppresses the Sort node it needs, and has no unique key; ADR-3/ADR-5 corrected accordingly. rev 1.2 rewrote the TDD sections to executable shape; rev 1.1 absorbed the edge-case review (EC-1..EC-6) and ordered the oracle before the flip
goal: Flip the `theodb.enable_columnar_late_mat` GUC default to ON — behind a decode-size guard that keeps the path's O(N) memory bounded — so ClickBench q23/q24 (`SELECT cols WHERE pred ORDER BY <non-text col> LIMIT k`) route to the M158 late-mat top-k by default, measured 2.4–12.8× faster and proven byte-identical by a LIMIT-preserving top-k oracle at 1M; q25/q26 documented as correct collation / multi-key honest-negatives.
---

# M167 — projection top-k (q23–q26)

## Goal

Flip `theodb.enable_columnar_late_mat` to default ON so the projection top-k `Limit(k) → Sort([non-text key]) → columnar-project`
shape (ClickBench q23/q24) routes to the M158 late-mat CustomScan by default, **bounded by a decode-size guard** (ADR-4) so the
path's O(N) decode cannot OOM a backend; success metric: `run_m128_clickbench.py --agg` reports q23/q24 with
`columnar_customscan==true`, both faster (measured 2.4–12.8× across 10k–1M), 43/43 storage A/B byte-identical (no regression),
**and the existing LIMIT-preserving top-k oracle (`m158_ec_harness.sql`, now automated + self-testing) green** (ADR-5);
q25/q26 stay declined (honest-negatives).

## Context

Discovery (`m167-projection-topk-blueprint.md`, council-index-storage + web SOTA) **falsified the milestone premise for q23/q24**:
the M158 `try_swap_topk` (`columnar_agg.rs:1813`) already routes single-key `ORDER BY <stored column> LIMIT k` byte-identically
(EventTime carried as resjunk). They don't show in the ClickBench numbers only because `enable_columnar_late_mat` defaults OFF
(`columnar_agg.rs:33`) and the harness never sets it. Measured on the live droplet (1M): q23 22527→6951 ms (3.24×), q24
2917→227 ms (12.8×); small-N: 10k 39.5→16.4 ms (2.4×), 100k 323→40 ms (8.0×) — late-mat wins at every measured N. Since a
columnar table has no btree on the sort column, native's only option is Sort-over-projected-rows, so late-mat is always ≥ native
for columnar top-k — the M158 default-OFF was conservative and is now superseded by evidence. q25 (`ORDER BY SearchPhrase`) and
q26 (`ORDER BY EventTime, SearchPhrase`) are collation / multi-key honest-negatives — the guards (`:1927–1939`, `:1853–1855`)
already decline them.

**What the edge-case review changed (rev 1.1).** The flip is not a one-line change in effect: it promotes a path that has only
ever run opt-in into the default for *every* `ORDER BY <col> LIMIT k` on a columnar table. The admission guards inside
`try_swap_topk` are thorough and were re-verified; the two gaps are elsewhere — **resource consumption** (EC-1, the O(N) decode
whose only stated mitigation was the default-OFF this milestone removes) and **what the oracles actually compare** (EC-2, the
43/43 ClickBench A/B strips the LIMIT, so it never exercises the top-k path at all). Both are absorbed below; the review is at
`.claude/knowledge-base/reviews/m167-projection-topk-edge-cases-2026-07-28.md`.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Why it exists |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~2810 | M166 | `try_swap_topk` (M158) + the `ENABLE_COLUMNAR_LATE_MAT` GUC default (`:33`); gains the decode-size guard (ADR-4) |
| `theodb_rs/src/am/df_executor.rs` | ~1050 | M166 | `run_columnar_topk` + the GUC-default doc comment (`:581`, which today cites default-OFF as the mitigation for the O(N) decode) |
| `benchmarks/m158_ec_harness.sql` | ~145 | M158 | THE top-k oracle (LIMIT-preserving, unique key `wid`, order-identical via `row_number()`); gains a positive control + Q9 multi-key decline |
| `benchmarks/run_m158_ec.py` | NEW | — | runner that gives the harness an exit code so it can actually gate (today it asks a human to eyeball "MUST be 0") |
| `benchmarks/test_run_m158_ec.py` | NEW | — | pure-logic tier for the runner's output parser |
| `benchmarks/run_m128_clickbench.py` | ~380 | M166 | ClickBench harness — read-only here; its A/B is LIMIT-stripped (`:283`) + order-normalized (`:243`), which is why it is the *storage* gate and not the top-k gate (ADR-5) |
| `docs/benchmarks/m158-late-mat-verdict.md` | ~60 | M158 | states `default OFF` as the shipped decision (`:50`) — must be superseded by T2.1 or `docs/` contradicts the new default |

### Current callers / dependents (real file:line)

- `try_swap_topk` gate: `if !ENABLE_COLUMNAR_LATE_MAT.get() { return None }` (`columnar_agg.rs:1818`) — the ONLY thing keeping
  q23/q24 from routing; no cost gate (forced post-planning swap).
- GUC default: `GucSetting::<bool>::new(false)` (`columnar_agg.rs:33`); registered via `define_bool_guc` (`:126–133`) which uses
  the GucSetting's own default as the boot value → flipping `:33` is the complete routing change.
- Single-key guard `(*sort).numCols != 1` (`:1853`); text-collation guard admits only OID 950(C)/951(POSIX) (`:1927–1939`),
  reading `sort.collations[0]` (the Sort's effective collation) rather than the column's `varcollid`.
- Qual loop (`:1958–1966`): an **empty** qual is admitted (the loop iterates zero times) — an unfiltered
  `SELECT … ORDER BY key LIMIT k` therefore decodes the whole relation. This is the EC-1 trigger.
- `run_columnar_topk` (`df_executor.rs:764`) — decode {projection ∪ key ∪ filter} for N (`:775`), `df.sort([key]).limit(0,k)`
  (bounded-heap TopK), materialize k survivors; output columns re-located **by name** in the post-sort batch (`:790–800`).
- Memory pool sizing (`df_executor.rs:583`): `max(work_mem, batch_bytes*2) + 64 MB` — deliberately sized to *fit* the batch, so
  DataFusion never rejects it; an over-large decode fails as a backend allocation failure, not a typed `Resources exhausted`.
- ClickBench A/B oracle: `ab_sql = re.sub(r"\s+LIMIT\s+\d+\s*;?\s*$", "", …)` (`run_m128_clickbench.py:283`) +
  `_canonical` sorts both sides (`:243–246`) — a *storage* oracle, blind to top-k ordering by construction.

### Domain glossary

- **projection top-k**: `SELECT cols WHERE pred ORDER BY <column> LIMIT k` — order by a *stored column* (not an aggregate; M158
  did the aggregate case).
- **late materialization**: decode only {sort key ∪ filter} columns for all N, TopK, then materialize the payload for the k
  survivors only (the M148 per-row cost paid for k, not N).
- **tie-row nondeterminism**: with a LIMIT over a key that has equal values, the *set of key values* returned is deterministic,
  but *which rows* among equal keys is unspecified — columnar and heap may pick different tie-rows. The A/B must compare the
  sort-key multiset OR use a **unique** sort key so no boundary tie can arise. `m158_ec_harness.sql` takes the second route
  (`wid` unique by construction), which is stronger: it keeps FULL-ROW comparison, so a key↔payload mismatch is visible (ADR-3).
- **collation honest-negative**: `ORDER BY <text>` — DataFusion memcmp ≠ PG collation order; only C/POSIX-OID collations route.
- **storage oracle vs top-k oracle**: a LIMIT-stripped, order-normalized comparison proves the *columnar storage* returns the
  same rows as the heap; it says nothing about *which k* and *in what order* the top-k node picked. They are different gates.

### Architecture boundaries affected

Read-path planner swap only (`planner_hook` → `try_swap_topk`) plus one plan-time size estimate. NO page-format / WAL / VACUUM /
crash / upgrade surface. Two correctness surfaces: the top-k-aware A/B (a wrong top-k is A/B-visible only when the oracle keeps
LIMIT+ORDER — that oracle already exists; this plan makes it able to gate) and the decode-size guard, governed by the existing
fail-closed guards + `.claude/rules/testing.md` § 5.1.

## Prior Art & Related Work

- `.claude/knowledge-base/discoveries/blueprints/m167-projection-topk-blueprint.md` (this cycle).
- `.claude/knowledge-base/reviews/m167-projection-topk-edge-cases-2026-07-28.md` (edge-case review absorbed in rev 1.1).
- `benchmarks/m158_ec_harness.sql` — the M158 correctness oracle this milestone REUSES rather than rebuilds (ADR-3).
- M158 (late-mat top-k of aggregate — the reused `try_swap_topk` + `run_columnar_topk`), M156 (text-where `LIKE`/`<>`), M149
  (columnar-project scan), M131 (the `resolve_special_varno` deparse recursion on the 105-col `hits`, #135 — the EC-3 precedent).
- SOTA: DataFusion `TopK` (`references/datafusion/.../topk/mod.rs` — bounded heap, payload interleave for kept rows only),
  DuckDB `PhysicalTopN` (selection vector — late mat), Abadi et al. ICDE 2007 (late materialization).

## Dependencies

**No dependency is added, removed, or version-changed by this milestone.** Every task is code and test changes over deps
already declared (parsimony rung 4 — reuse what is installed). Audited 2026-07-28:
`.claude/knowledge-base/audits/m167-projection-topk-deps-audit-2026-07-28.md`.

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `=0.19.0` (`theodb_rs/Cargo.toml:37`) | rust | The extension framework — `try_swap_topk` and the T2.2 guard are pgrx/`pg_sys` code |
| `datafusion` | `54` (`:49`) | rust | `run_columnar_topk`'s `filter → sort → limit` bounded-heap TopK — the engine this plan makes default |
| `arrow` | `58` (`:50`) | rust | The `RecordBatch` the decode produces and the TopK consumes |
| `psycopg2-binary` | `>=2.9` (`benchmarks/requirements.txt`) | python | The A/B harnesses' DB driver — `topk_ab_check` reuses it |
| `pytest` | `>=7` | python | The pure-logic tier of T1.1 |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | | | | |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

### Known advisory in the dependency graph — out of scope, declared

`GHSA-2f9f-gq7v-9h6m` (MEDIUM, *Memory Allocation with Excessive Size Value*) affects `thrift@0.17.0`, reached
transitively as `theodb_rs → datafusion 54 → parquet 58.3.0 → thrift`; fixed in `thrift 0.23.0`. It is **reachable in the
product** — `Cargo.toml:49` enables the `parquet` feature and `theodb_rs/src/parquet.rs` implements `theodb.read_parquet`,
whose input is a user-supplied file whose metadata is thrift-encoded. It is **not reachable on the M167 path**:
`run_columnar_topk` operates on an in-memory `RecordBatch` and `df_executor.rs` never touches parquet. **This milestone
therefore adds no exposure and is not gated by it**; the bump belongs to a separate `datafusion`/`parquet` upgrade slice
(the 54/58 set is deliberately pinned for proven pgrx-0.19 coexistence, `Cargo.toml:44`), tracked as repo-level debt rather
than smuggled into a default-flip milestone. Two unmaintained-crate warnings (`paste`, `serde_cbor`, both via `pgrx`) have
no patched version and are recorded in the audit for completeness.

## ADRs

### ADR-1 — flip `enable_columnar_late_mat` default to ON (no *cost* gate)
Change `columnar_agg.rs:33` `new(false)` → `new(true)`. **Rationale:** measured 2.4–12.8× faster byte-identical across 10k–1M;
a columnar table has no btree on the sort column, so native's only plan is Sort-over-projected-rows and late-mat is always ≥
native for columnar top-k — no small-N regression exists in the measured range, and below 10k the absolute time is sub-40 ms
(negligible). **Alternative rejected:** add a `plan_rows`-threshold **cost** gate — YAGNI: the win holds at every measured N and
late-mat dominates native structurally; a threshold would be a knob nobody's evidence asks for. **Alternative rejected:** keep
default OFF + add a `--late-mat` harness flag — that leaves the user-facing win opt-in behind a non-default flag, understating
the product (Rule 5); the DoD's "GUC honored" is satisfied by the swap still respecting the (now-ON) GUC and every guard.
**Scope note (rev 1.1):** this ADR is about *speed*. It does not speak to memory — that axis is ADR-4, which is not in tension
with it (a resource bound is not a cost model).

### ADR-2 — q25/q26 honest-negatives (portable build)
`ORDER BY <text>` (q25) and multi-key with a text tiebreaker (q26) route to a wrong top-k under any non-C/POSIX-OID collation
(byte-order ≠ collation-order; a deterministic collation constrains equality not order). The guards already decline them
(`:1927–1939`, `:1853`). **Alternative rejected:** admit text sort keys when the DB `datcollate` is C/POSIX/C.UTF-8 — glibc/
locale-name dependent, changes routing by cluster locale, and multi-key is new mechanism; deferred to a later ADR-gated slice
(blueprint §5). No wrong top-k shipped (mandate).

### ADR-3 — reuse `m158_ec_harness.sql`; ties are neutralized by a UNIQUE key, not by a multiset compare
**Superseded in rev 1.3 — the previous text was written without reading the existing harness, and was wrong on two counts.**

`benchmarks/m158_ec_harness.sql` already is the LIMIT-preserving top-k oracle for exactly this node (its own header:
*"LIMIT-preserving symmetric-EXCEPT — the CORRECT oracle"*). It toggles `theodb.enable_columnar_late_mat` off↔on over the same
columnar table `t_col` and takes the symmetric difference of the **full** result sets. Ties are not a problem there because the
sort key `wid` is **unique by construction** (`:13`, `:21` — 20 000 rows, `wid = g`), which is a strictly better tie fix than
comparing a key multiset: it keeps full-row comparison, so the EC-4 key↔payload defect class is covered. It also already carries
an **emission-order** oracle (`row_number() OVER ()` position-by-position on the raw output, `:49–55`), so order is proven too.

Corrections this ADR makes to rev 1.1/1.2:
- "the top-k A/B compares the sort-key multiset" — **wrong**; full rows are compared, tie-safety comes from the unique key.
- "the case group ALSO carries one case whose sort key is unique" — **already true** of Q1; nothing to add.
- ADR-5's implication that a LIMIT-preserving oracle had to be built — **false**; it exists and predates this milestone.

**Alternative rejected:** add the cases to `benchmarks/columnar_type_ab.py` (what rev 1.1/1.2 said). Three independent blockers,
each verified in source: that harness's own scope comment excludes this path (*"The late-materialization projection path (M158)
… belongs to M158's own A/B, not here"*, `:258–260`); its `session_setup` does `SET enable_sort = off` (`:44`, added for the agg
path per the M161 false-green lesson), and without a `Sort` node `try_swap_topk` cannot fire at all (`columnar_agg.rs:1852`), so
the decline cases would pass **vacuously**; and its fixture cycles `spec["edges"][i % len(edges)]` (`:100–103`), so **no column is
unique** and the EC-4 full-row case is not constructible there. **Alternative rejected:** build a third harness — parsimony rung 4
(reuse what is installed); two oracles for one node is how they drift apart.

### ADR-4 — bound the O(N) decode with a plan-time size guard (EC-1)
Decline the swap when the estimated decoded batch grossly exceeds the session's own sort budget:
`plan_rows × plan_width > work_mem × SAFETY_FACTOR` → `return None` (fall back to the native plan, correct for any input).
**Rationale:** `run_columnar_topk` decodes {projection ∪ key ∪ filter} for all rows *before* the bounded-heap TopK runs, so the
path is O(N) memory where native's top-N heapsort is O(k) — `df_executor.rs:576–581` states this and cites **"gated behind
`theodb.enable_columnar_late_mat`, default OFF"** as its mitigation. ADR-1 removes exactly that mitigation, so the trade-off must
be re-mitigated or it ships unbounded: an unfiltered `SELECT * FROM hits ORDER BY EventTime LIMIT 10` is admitted (empty qual,
`:1958`) and decodes the whole relation — tens of GB at the 100M scale this project already loads. The pool is sized to *fit* the
batch (`:583`), so the failure mode is a backend OOM (postmaster crash-recovery, all connections dropped), not a typed error.
**Alternative rejected:** a GUC for the threshold — YAGNI (parsimony rung 1/5); a constant plus an `admit_trace` decline reason
is observable enough, and a knob invites tuning nobody has evidence for. **Alternative rejected:** stream/chunk the decode so the
path becomes O(k) — that is real new mechanism (M158's design decodes one batch), out of scope for a default-flip milestone, and
is the honest long-term fix rather than this milestone's. **Alternative rejected:** accept the risk and only document it — the
regression is an availability incident on an ordinary query, not a corner case.
**Honest limitation (Rule 3):** `plan_rows` on the project node is the **post-qual** estimate, while the decode covers every row
surviving zone-map *chunk* skipping. The guard is therefore a **lower bound** on the real decode: it reliably catches the
unfiltered/weakly-filtered case (where `plan_rows ≈ N`, and which is the dangerous one), and can under-fire for a highly
selective predicate over poorly-clustered data. It is strictly better than no bound; tightening it with `reltuples` is an
Unresolved Question below, not a claim made here.

### ADR-5 — the 1M correctness gate is the top-k oracle, not the 43/43 storage A/B (EC-2)
`run_m128_clickbench.py` strips the trailing LIMIT before comparing (`:283`) and sorts both sides (`_canonical`, `:243–246`).
With the LIMIT gone there is no `T_Limit` parent, so `try_swap_topk` declines (`:1822–1825`) and the A/B compares
native-on-columnar vs native-on-heap. **`result_ab_identical == true` for q23/q24 is therefore true of a query that never
routes** — the 43/43 figure is a *storage* oracle and is structurally incapable of detecting a wrong top-k.

**Decision (corrected in rev 1.3):** keep the 43/43 run as the no-regression gate it actually is (and say so), and make
`benchmarks/m158_ec_harness.sql` the correctness gate. The gap is NOT that a LIMIT-preserving oracle is missing — that was rev
1.1's error, written before reading the file. It exists, it predates this milestone, and it already proves this node
byte-identical AND order-identical. The real gap is that **it cannot gate anything today**: it is a manual `psql` script with no
exit code (its last line asks a human to check "ALL … MUST be 0"), nothing in the repo invokes it (only `docs/` and the M158 plan
reference it), and it carries **no positive control** — so a broken oracle reporting all-zeros is indistinguishable from a
correct one (`rules/testing.md` § 5.1: an oracle that cannot fail is not an oracle). Phase 1 closes those three gaps and adds the
one query shape the harness never covered (multi-key decline).

**Alternative rejected:** keep citing 43/43 as the correctness evidence — unsupported by the cited measurement
(Rule 5 / `public-copy.md`). **Alternative rejected:** stop stripping the LIMIT in the ClickBench oracle — the strip exists for a
good reason (`:280–282`: tied aggregate counts make the LIMIT cut arbitrary-but-valid); removing it would false-positive across
the other 41 queries. **Consequence:** Phase 1 still precedes Phase 2, but for a narrower reason than rev 1.1 gave — the gate
must be *runnable and self-testing* before the default it protects is flipped.

## Dependency Graph

Phase 1 (top-k oracle) → Phase 2 (flip + decode guard, whose acceptance *uses* the Phase-1 oracle at 1M) → Phase 3 (verdict +
CHANGELOG, depends on both). **Changed in rev 1.1:** phases 1 and 2 are swapped relative to v1.0. The oracle is now built first
because it is the test that would catch a wrong top-k, and the flip is the change under test — RED before GREEN
(`rules/cycle-implement.md`). In v1.0 the two phases were declared parallel-capable; ADR-5 makes that false.

## Phase 1 — make the EXISTING top-k oracle able to gate

### T1.1 — automate `m158_ec_harness.sql`, give it a positive control, and cover multi-key decline
#### Why this step
The action: keep `benchmarks/m158_ec_harness.sql` as the oracle (it already proves this exact node byte-identical AND
order-identical over a unique key — ADR-3) and close the three gaps that stop it from gating the M167 flip:

1. **Automation** — add `benchmarks/run_m158_ec.py`, a thin runner that executes the `.sql` via psql, parses every
   `*_mism` / `*_order_mism` row, and exits non-zero when any is `> 0`. Today the script ends with a `\echo` asking a
   HUMAN to check "ALL … MUST be 0", nothing in the repo invokes it, and therefore it protects nothing.
2. **Positive control** — a deliberately divergent pair that the oracle MUST flag, so all-zeros means "verified", not
   "the oracle is broken" (`rules/testing.md` § 5.1). The unique `wid` key makes this deterministic: seed a twin table
   with one row's `wid` altered so it deterministically enters/leaves the top k.
3. **Multi-key decline (Q9)** — `ORDER BY wid, cid LIMIT 10` MUST show a native `Sort` (the `numCols != 1` guard,
   `columnar_agg.rs:1853`). This is the one shape in the plan's scope the harness never covered; q26 is its ClickBench
   analogue and ADR-2 declares it an honest-negative, so the guard needs a test that would notice if it stopped holding.

The reasoning: parsimony rung 4 — reuse what is installed. Q1 already covers full-row + emission-order over a unique key
(the EC-4 key↔payload class), Q7 covers the text-collation decline, Q2/Q3/Q4/Q5/Q6 cover predicates, direction, projected
subset and bpchar output. Building a parallel oracle would duplicate all of that and let the two drift apart.

#### Files to edit
- `benchmarks/m158_ec_harness.sql` (positive control + Q9 multi-key decline)
- `benchmarks/run_m158_ec.py` (NEW — runner with exit code; no new dependency: `psycopg2` is already declared)
- `benchmarks/test_run_m158_ec.py` (NEW — pure-logic tier for the output parser)

#### TDD
Parser tier runs locally (`python3 -m pytest benchmarks/test_run_m158_ec.py -k m158`); the harness itself runs on the droplet.

- RED: test_parser_fails_run_on_nonzero_mismatch —
  `assert parse_verdict("q1_ab_mism | 3") == "FAIL"` and `assert parse_verdict("q1_ab_mism | 0") == "PASS"`.
  Fails before `run_m158_ec.py` exists (ImportError is not acceptable as the RED — the module must exist with the
  function stubbed so the assertion itself is what fails).
- RED: test_parser_requires_positive_control_to_have_diverged —
  `assert overall(rows_with_control_diverged=0) == "ORACLE_BROKEN"` — an all-zeros run whose control did NOT diverge is
  a FAIL, not a pass.
- RED: test_q9_multikey_case_declared — `assert "q9_multikey" in harness_sql` and
  `assert "ORDER BY wid, cid" in harness_sql`.
- GREEN (droplet): `python3 benchmarks/run_m158_ec.py` exits `0`; every `*_mism` and `q1_order_mism` is `0`; the
  positive control reports a non-zero mismatch; Q9's `EXPLAIN` contains `Sort` and does NOT contain `Custom Scan`.
- GREEN (droplet, seeded failure): temporarily point the control at a NON-divergent pair and
  `assert runner_exit_code != 0` — proves the runner fails loudly rather than reporting a green from a dead oracle.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `python3 benchmarks/run_m158_ec.py` exits `0` on the droplet and non-zero when any mismatch or a dead control is present.
- The harness gains a positive control whose seeded divergence IS caught, and Q9 (multi-key) declining to native `Sort`.
- Parser tier green locally.
- The harness's own `\echo` closing line no longer asks a human to eyeball the result — the exit code is the verdict.

## Phase 2 — flip the GUC default, bounded

### T2.1 — flip `enable_columnar_late_mat` default ON
#### Why this step
The action: change `columnar_agg.rs:33` `GucSetting::<bool>::new(false)` → `new(true)` and update the three doc comments
(`:30`, `:1812`, `df_executor.rs:581`) from "default OFF" to "default ON" — `df_executor.rs:581` in particular must stop citing
default-OFF as the mitigation for the O(N) decode and cite the ADR-4 guard instead, or the comment becomes false. The reasoning:
the capability is proven (ADR-1); the only routing gate is this boot value; flipping it delivers the measured 2.4–12.8× win while
every fail-closed guard (numCols, text collation, LIMIT shape) still runs inside `try_swap_topk`.
#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` (`:33` default + `:30`/`:1812` comments)
- `theodb_rs/src/am/df_executor.rs` (`:581` comment — re-point the O(N) mitigation at ADR-4)
- `docs/benchmarks/m158-late-mat-verdict.md` (`:50` states "`default OFF` — the conservative honest default given the O(N)
  memory cost"; supersede it, or `docs/benchmarks/` contradicts the shipped default. The CHANGELOG's released M158 entry is
  NOT edited — Unbreakable Rule 6; it is superseded by the new `[Unreleased]` entry instead.)
#### TDD
Run on the droplet against the 1M `hits`/`hits_heap` pair, with the `.so` rebuilt from the edited source.

- RED: test_q23_q24_route_at_default — with NO session `SET`, for each of q23 and q24 capture
  `plan = EXPLAIN (VERBOSE, COSTS OFF) <query>`, then
  `assert "Custom Scan (theodb_columnar_agg)" in plan` and `assert "Sort" not in plan`.
  Fails before the flip (today the GUC boots `off`, so `try_swap_topk` returns at `columnar_agg.rs:1818`).
- RED: test_explain_verbose_returns_on_wide_hits (EC-3) — `EXPLAIN (VERBOSE, COSTS OFF)` of q23 and q24 on the real
  105-column `hits` `assert elapsed_s < 30` (i.e. it RETURNS). Guards the M131/#135 deparse recursion, which
  `statement_timeout` cannot interrupt because it happens during plan printing.
- GREEN: test_guc_off_restores_native_sort — after `SET theodb.enable_columnar_late_mat = off`,
  `assert "Sort" in plan_q24` and `assert "Custom Scan (theodb_columnar_agg)" not in plan_q24` (GUC honored both ways).
#### Concurrency tests
(none — single-threaded)
#### Acceptance criteria
- With no session `SET`, `EXPLAIN (VERBOSE, COSTS OFF)` of q23 **and** q24 against the real 105-column `hits` **returns**
  (does not hang) and contains `Custom Scan (theodb_columnar_agg)` with no `Sort`; `SET … = off` restores the native `Sort`
  plan (GUC honored). **VERBOSE is not optional (EC-3):** M131/#135 was a `resolve_special_varno` deparse recursion on this exact
  table that `statement_timeout` could not interrupt because it happened during plan *printing*; this node has the same deparse
  surface (fresh base Vars in `custom_scan_tlist`, `scanrelid = 0`) and, because the GUC defaulted OFF and the harness never set
  it, has plausibly never been `EXPLAIN VERBOSE`d on the wide table.
- Full `run_m128_clickbench.py --agg` (1M) reports 43/43 `result_ab_identical == true` — **the LIMIT-stripped storage oracle**
  (ADR-5), i.e. evidence of no storage/aggregate regression, NOT evidence that the top-k is right — with q23/q24 now
  `columnar_customscan == true`.
- **The top-k correctness gate (ADR-5):** the Phase-1 `topk_ab_check`, applied to q23/q24 over the 1M `hits`/`hits_heap` pair
  with LIMIT preserved, reports identical results. `run_m128_clickbench.py` imports `topk_ab_check` from `columnar_type_ab`
  rather than growing a second oracle.

### T2.2 — bound the decode with a plan-time size guard
#### Why this step
The action: in `try_swap_topk`, after the project node is identified and before the tlist loop, decline when the estimated
decode dwarfs the session sort budget:
```rust
// EC-1 / ADR-4 — the top-k path decodes {projection ∪ key ∪ filter} for all N BEFORE the bounded-heap TopK
// (df_executor.rs:775): O(N) memory where native top-N is O(k). Decline rather than risk a backend OOM.
let est_bytes = (*child).plan_rows * (*child).plan_width.max(1) as f64;
if est_bytes > (pg_sys::work_mem as f64) * 1024.0 * TOPK_DECODE_WORK_MEM_FACTOR {
    admit_trace("topk_decode_estimate_too_large");
    return None;
}
```
The reasoning: ADR-1 removes the default-OFF that `df_executor.rs:576–581` cites as the mitigation for this path's O(N) decode,
so the bound has to come from somewhere else. Declining falls back to the native plan, which is correct for any input — fail-closed,
consistent with every other guard in this function. `admit_trace` reuses the M152 decline-trace convention already in the file
(parsimony rung 4) rather than adding new observability machinery.
#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` (the guard + the `TOPK_DECODE_WORK_MEM_FACTOR` constant with its rationale comment)
#### TDD
Run on the droplet with the GUC at its new default (ON), so the guard is the only thing that can decline.

- RED: test_topk_declines_unfiltered_wide_projection — with a small `work_mem`, capture
  `plan = EXPLAIN (COSTS OFF) SELECT * FROM hits ORDER BY EventTime LIMIT 10` (no WHERE → empty qual → admitted today at
  `columnar_agg.rs:1958`), then `assert "Custom Scan" not in plan` and `assert "Sort" in plan`; with
  `THEODB_ADMIT_TRACE=1`, `assert "topk_decode_estimate_too_large" in trace`.
  Fails before the guard exists (the query routes and decodes the whole relation).
- GREEN: test_topk_still_routes_within_budget — at the same `work_mem`, for q23 and q24 (selective WHERE),
  `assert "Custom Scan (theodb_columnar_agg)" in plan` — the guard must not un-route the milestone's own target shape.
- GREEN: test_topk_guard_is_lower_bound_documented — `assert "post-qual" in guard_comment` — the code comment states the
  ADR-4 limitation rather than implying the bound is exact.
- REFACTOR: the factor is one named constant with a comment stating it is a heuristic safety factor, not a measured optimum.
#### Concurrency tests
(none — single-threaded)
#### Acceptance criteria
- An unfiltered wide top-k declines with the `topk_decode_estimate_too_large` trace; q23/q24 still route at the same `work_mem`.
- The code comment states the ADR-4 limitation verbatim: `plan_rows` is post-qual, so the bound is a lower bound, not exact.

## Phase 3 — benchmark verdict + honest-negative record

### T3.1 — fresh q23/q24 measurement + document q25/q26
#### Why this step
The action: record the before/after evidence and the honest-negatives in a benchmark artifact + CHANGELOG. The reasoning:
`rules/public-copy.md` and TheoDB rule 5 — a performance number is a claim, and a claim without a reproducible artifact does not
ship. Rev 1.1 adds one obligation: the artifact must distinguish which oracle proved what (ADR-5), or it repeats the very
conflation the edge-case review found.
#### Files to edit
- `docs/benchmarks/m167-projection-topk-verdict.md` (new), `CHANGELOG.md` (`[Unreleased]`)
#### TDD
Documentation task — its RED is the absence of the artifacts, checked mechanically rather than by eye.

- RED: test_m167_verdict_artifact_exists —
  `assert Path("docs/benchmarks/m167-projection-topk-verdict.md").exists()` and
  `assert "M167" in Path("CHANGELOG.md").read_text()`.
- GREEN: test_m167_verdict_attributes_each_claim_to_its_oracle (ADR-5) — in the verdict text,
  `assert "LIMIT-stripped" in text` (the 43/43 storage A/B) and `assert "top-k A/B" in text` (the 1M correctness gate),
  so the two oracles are never conflated.
#### Concurrency tests
(none — single-threaded)
#### Validation
- `docs/benchmarks/m167-projection-topk-verdict.md` records q23/q24 before/after (OFF vs ON, 10k/100k/1M), the ratio vs
  ClickHouse, and **states explicitly which oracle each claim rests on**: 43/43 = LIMIT-stripped storage A/B (no regression);
  top-k A/B at 1M = the correctness evidence for the routed path. q25/q26 recorded as collation / multi-key honest-negatives.
- The decode-size guard (ADR-4) is documented with its limitation, including the shape that now declines.
- `grep M167 CHANGELOG.md` non-empty.

## Coverage Matrix

| Goal claim / DoD item | Task(s) |
|---|---|
| q23/q24 route to late-mat by default (GUC flipped ON) | T2.1 |
| Measured faster byte-identical (2.4–12.8×) | T2.1, T3.1 |
| No storage/aggregate regression (43/43 LIMIT-stripped A/B) | T2.1 |
| Top-k correctness proven by a LIMIT-preserving, order-identical oracle | T1.1 (automated + self-tested), T2.1 (run after the flip) |
| GUC honored (off still declines) | T2.1 |
| `EXPLAIN VERBOSE` returns on the 105-col `hits` (M131 class) | T2.1 |
| Oracle can actually gate (exit code) + cannot silently die (positive control) | T1.1 |
| Multi-key decline covered (the `numCols != 1` guard, q26's analogue) | T1.1 |
| O(N) decode bounded — no unfiltered-wide OOM | T2.2 |
| q25/q26 collation/multi-key honest-negatives | T1.1 (decline cases), T3.1 (doc) |
| Benchmark evidence + CHANGELOG, per-oracle attribution | T3.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Flipping the default routes a top-k shape where late-mat loses to native | Medium | Measured 2.4–12.8× win at every N 10k–1M; late-mat ≥ native structurally (no btree on columnar sort col); full 43-query no-regression run in T2.1 | me |
| **The path is O(N) memory where native is O(k); default-ON exposes it to ordinary unfiltered queries (EC-1)** | **High** | ADR-4 decode-size guard (T2.2), fail-closed to the native plan; `admit_trace` makes declines observable | me |
| **The guard uses the post-qual `plan_rows` estimate — a lower bound; a highly selective predicate over poorly-clustered data can still decode more than estimated (EC-1)** | **Medium** | Stated honestly in ADR-4 + the code comment; the dangerous unfiltered case is where the estimate is *tight*; `reltuples` tightening tracked as an Unresolved Question | me |
| Tie-row nondeterminism → false A/B divergence | High | ADR-3: compare the sort-key multiset with LIMIT preserved, not full rows (except the unique-key case where ties cannot arise) | me |
| **Key↔payload misalignment invisible to a key-only oracle (EC-4)** | **High** | ADR-3 amendment: `topk_unique_key_full_row` compares whole rows on a tie-free key | me |
| A future edit admits a text sort key → silent wrong top-k | High | T1.1 `topk_text_order` decline case + the collation guard; the top-k A/B keeps LIMIT (not blind) | me |
| **`EXPLAIN VERBOSE` deparse recursion on the 105-col `hits` (M131/#135 class) — unkillable by `statement_timeout` (EC-3)** | **High** | T2.1 acceptance asserts `EXPLAIN (VERBOSE, COSTS OFF)` *returns* for q23 and q24 on the real wide table | me |
| Default-ON changes behavior for existing users relying on OFF | Low | Byte-identical results (only faster); GUC still available to force OFF | me |
| **Default-ON is validated only in the serial plan shape — the harness sets `max_parallel_workers_per_gather = 0` (`run_m128_clickbench.py:344`) while real users have parallelism on (EC-6)** | **Low** | Accepted, not tested: the node copies `parallel_safe` from the Sort and sets `parallel_aware = false` (`:1988–1989`), the identical pattern the shipped agg node has used since M110 (`:1730–1731`) — no novel mechanism | me |

## Unresolved Questions

- Whether to later admit text sort keys under a C/POSIX/C.UTF-8 `datcollate` (unlocks q25/q26 on byte-order deployments) —
  deferred to a separate ADR-gated slice (ADR-2 / blueprint § 5); resolved as "deferred" for M167.
- Whether the ADR-4 guard should estimate from `pg_class.reltuples` (pre-qual, matching what is actually decoded) instead of the
  project node's post-qual `plan_rows`. Deferred: it needs relation-cache access inside `planner_hook` (more machinery) and the
  lower-bound form already covers the unfiltered case that motivates the guard. Revisit if a decline/OOM report shows the
  estimate under-firing in practice.
- Whether the O(N) decode should eventually be chunked into an O(k) streaming top-k (removing the need for the guard entirely).
  That is new executor mechanism, out of scope for a default-flip milestone; recorded as the honest long-term fix (ADR-4).

## Failure scenarios

No external I/O is touched (in-process planner/executor logic over local columnar state), so there is no timeout / 5xx / connection
class here. The one resource-failure scenario, added in rev 1.1:

| Scenario | Behavior required |
|---|---|
| Estimated decode exceeds the work_mem-derived bound (wide and/or unfiltered top-k) | `try_swap_topk` returns `None` → native plan runs → correct result, O(k) memory; decline reason visible under `THEODB_ADMIT_TRACE=1` (T2.2) |
| Decode fits the bound but the batch still exhausts backend memory (estimate under-fired) | Backend allocation failure — the pre-existing M158 behavior, now narrowed but not eliminated; stated in ADR-4 rather than claimed fixed |

## Global Definition of Done

- **Oracle first:** `columnar_type_ab.py` gains `topk_ab_check` + `topk_int_order` / `topk_unique_key_full_row` (route) and
  `topk_text_order` / `topk_multikey` (decline); harness exit 0; top-k positive control reports `diverged > 0`; pure-logic tier
  green including `k < nrows`.
- q23/q24 route by default: `EXPLAIN (VERBOSE, COSTS OFF)` (no session SET) **returns** on the real 105-col `hits` and shows
  `Custom Scan (theodb_columnar_agg)` + no `Sort`; harness JSON `columnar_customscan == true` for both.
- **Correctness at scale:** the LIMIT-preserving top-k A/B over 1M `hits`/`hits_heap` is identical for q23/q24 (ADR-5).
- **No regression:** `run_m128_clickbench.py --agg` reports 43/43 `result_ab_identical == true`, understood and documented as the
  LIMIT-stripped *storage* oracle.
- **Bounded:** an unfiltered wide top-k declines with `topk_decode_estimate_too_large`; q23/q24 still route at the same `work_mem`.
- GUC honored: `SET theodb.enable_columnar_late_mat = off` restores the native `Sort` plan.
- Honest-negatives recorded: `docs/benchmarks/m167-projection-topk-verdict.md` states q25/q26 collation/multi-key AND attributes
  each claim to the oracle that proves it; `grep M167 CHANGELOG.md` non-empty.
- Gates: `run_structural.py` ≥ SHIPPABLE_WITH_CAVEATS; `/code-quality` ∉ {FAIL_HARD, INVALID}; `/review` (council-rust-pgrx +
  council-benchmark + council-index-storage) READY_TO_MERGE.
