---
slug: custom-scan-node-integration
milestone_id: M93
created_at: 2026-07-13
goal: Assemble the proven M92 spike primitives into a 2-child Custom Scan node that filters arbitrary-WHERE vector search MVCC-correctly, proven by a filtered result byte-identical to exact seqscan on a non-label column (incl. lossy + pending) and an inline-vs-post-filter benchmark on SIFT.
---

# M93 — Custom Scan node integration (MultiExec bitmap + MVCC recheck; closes M92)

## Context

The M92 spike proved the three primitives in isolation (all committed, 260 pg_tests GREEN, gated OFF behind
`theodb.enable_vecfilter`): v0 the hand-rolled Custom Scan node lifecycle (`7224ae0`); v1a the TID-membership inline
skip in the IVF-AQ Stage-1 (`c20db0f`); v1b `materialize_bitmap()` — a native `TIDBitmap` → exact-TID + lossy-block
sets (`a882027`). M93 assembles them into a 2-child Custom Scan node so arbitrary-`WHERE` filtered vector search
works end-to-end and MVCC-correctly. The design is grounded in a code-read of the Postgres executor source
(`nodeBitmapHeapscan.c`, `nodeCustom.c`) — the integration is a near-verbatim port of that node's MultiExec + recheck.

## Goal

Assemble the proven M92 spike primitives into a 2-child Custom Scan node that filters arbitrary-`WHERE` vector search
MVCC-correctly, proven by a filtered result byte-identical to exact seqscan on a non-label column (incl. lossy +
pending) and an inline-vs-post-filter benchmark on SIFT.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Role |
|---|---:|---|---|
| `theodb_rs/src/am/customscan.rs` | 425 | `a882027` (v1b materialize) | the Custom Scan Provider — hook, node callbacks (v0), membership side channel (v1a), `materialize_bitmap` (v1b) |
| `theodb_rs/src/am/scan.rs` | ~960 | `c20db0f` (v1a membership skip) | the AM scan — the v7 Stage-1 membership skip (`scan_ivf_aq_split_v7`) |

### Current callers / dependents (from code read)

- `customscan.rs` — `pathlist_hook` (`:69`) adds a pass-through `CustomPath`; `plan_custom_path` (`:117`) builds the
  `CustomScan`; `begin_custom_scan` (`:155`)/`exec_custom_scan` (`:257`, calls `pg_sys::ExecProcNode` — PROVEN callable
  in-build by the passing v0 test)/`end_custom_scan` (`:267`) are the pass-through lifecycle; `set_membership`/
  `membership`/`has_membership` (`:37-49`) the side channel; `materialize_bitmap` (`:57`) the TIDBitmap iterator.
- `scan.rs` — `amrescan` (`:108`) reads `has_membership()` into `state.filtering` + `xs_recheck`;
  `scan_ivf_aq_split_v7` (`:560`) Stage-1 skips `!membership.contains(&tid)` (`:642` area).
- `tid::encode` (`theodb_rs/src/am/tid.rs:7`) — `(block<<16)|offset`, the membership TID encoding.

### Domain glossary

- **membership** — the admission set the AM Stage-1 filters by: exact encoded TIDs + lossy block numbers.
- **lossy page** — a bitmap page whose per-offset detail was dropped under memory pressure (`ntuples < 0`); only the
  block is known → every candidate on it is admitted then rechecked.
- **recheck** — `ExecQual` of the scalar `WHERE` on the heap tuple, removing over-admitted (lossy/pending) rows.
- **bitmapqual** — `BitmapHeapPath.bitmapqual`, the bitmap-generating sub-path the planner already built.

### Architecture boundaries affected

`customscan.rs` is the interface layer (planner hook + executor node callbacks). The change stays within it + a small
extension to the AM membership skip in `scan.rs`. No page-format change → no REINDEX. GUC default-OFF → production
byte-identical when disabled.

## Prior Art & Related Work

- Internal: the M92 blueprint `knowledge-base/discoveries/blueprints/arbitrary-where-custom-scan-blueprint.md` + the
  spike commits (`7224ae0`/`c20db0f`/`a882027`).
- External (study, Rule 9): the Postgres executor source `nodeBitmapHeapscan.c` (the MultiExec block + `BitmapHeapRecheck`
  + the `ExecScan` shape) and `nodeCustom.c` (the custom-struct-embed sanction + `css.ss.ps.qual = ExecInitQual(...)`
  done by core). AlloyDB inline filtering (the published "vector scan + bitmap index scan" design).
- Discovery finding (M93 executor-lifecycle research, council-rust-pgrx): the recheck qual is ALREADY compiled by core
  into `css.ss.ps.qual` (`nodeCustom.c:101`); `pg_sys::ExecProcNode` free-fn is a build-time-generated inline wrapper
  (proven callable by the passing v0 test) — but `ExecQual`/`ResetExprContext` are inline, so the qual is evaluated via
  the `ExprState.evalfunc` fn-pointer + `MemoryContextReset` (struct fields, always present) to be robust.

## ADRs

### ADR M93-1 — 2-child Custom Scan node (vector-ordered + bitmapqual), reuse the planner's bitmap

**Decision:** the hook finds the planner-built `BitmapHeapPath.bitmapqual` + the vector-ordered `IndexPath` in
`rel->pathlist` and builds a `CustomPath` with `custom_paths = [vector_ordered, bitmapqual]`; Postgres plans both
into `custom_plans`. The node MultiExecs the bitmap child → TIDBitmap → membership → drives the vector child.

**Rejected alternatives:** (a) *hand-build the bitmap sub-plan from the restriction clauses* — REJECTED (Rule 9: the
planner already did the clause↔index matching + BitmapAnd/Or composition; re-doing it is fragile reinvention).
(b) *direct `index_beginscan` on the scalar index (Option B)* — REJECTED: only handles a single index (no BitmapAnd),
and re-implements what the bitmap sub-plan does. (c) *AM-only via a new ScanKey* — REJECTED: a TIDBitmap is not a SQL
operator and cannot ride a ScanKey (the reason the side channel exists).

### ADR M93-2 — carry the lossy block set in the membership; recheck is the authority

**Decision:** extend the membership side channel from `HashSet<i64>` (exact only) to `{exact: HashSet<i64>, lossy:
HashSet<u32>}`. The AM Stage-1 admits a candidate if `exact.contains(tid) OR lossy.contains(block(tid))`; the node's
`ExecQual` recheck removes the lossy over-admits. Membership is an ADMISSION filter, never the final authority.

**Rejected alternatives:** (a) *exact-only membership (drop lossy pages)* — REJECTED: it UNDER-ADMITS (silently misses
valid rows on lossy pages) — a correctness bug the discovery surfaced. (b) *error out if the bitmap is ever lossy* —
REJECTED: not robust at scale (a loose filter legitimately lossifies); admit-then-recheck is the native semantics
(`nodeBitmapHeapscan.c:317`).

### ADR M93-3 — recheck via `css.ss.ps.qual` + `ExprState.evalfunc` (not inline `ExecQual`)

**Decision:** the recheck evaluates `css.ss.ps.qual` (the scalar `WHERE`, already compiled by core at
`nodeCustom.c:101`) via the `ExprState.evalfunc` fn-pointer + `MemoryContextReset(ecxt_per_tuple_memory)` on the
per-tuple context, with `ecxt_scantuple = slot` — the `nodeBitmapHeapscan.c:571-573` pattern.

**Rejected alternatives:** (a) *call `pg_sys::ExecQual`/`ExecQualAndReset`* — REJECTED: they are `static inline` in PG,
not guaranteed exported by pgrx's binding generation (the discovery's caution); the `evalfunc` fn-pointer is a struct
field, always present. (b) *hand-build a recheck qual from the bitmap conditions* — REJECTED: core already compiled the
exact scalar predicate into `ss.ps.qual`.

## Dependency Graph

```
Phase 1 (membership carries lossy) ──> Phase 2 (2-child node: hook + MultiExec + recheck) ──> Phase 3 (correctness gate: seqscan-identical + lossy + pending) ──> Phase 4 (benchmark: inline vs post on SIFT)
```

## Phase 1 — membership carries the lossy block set (correctness prerequisite)

### Task T1.1 — extend the side channel to `{exact, lossy}` and admit lossy blocks in Stage-1

#### Why this step

**Action:** change `set_membership`/`membership` to carry `{exact: HashSet<i64>, lossy: HashSet<u32>}`; update the
`scan_ivf_aq_split_v7` Stage-1 skip to admit a TID when `exact.contains(tid) || lossy.contains((tid>>16) as u32)`.

**Reasoning:** the discovery proved that dropping lossy pages under-admits (misses valid rows). The membership must
admit lossy-block candidates so the node's recheck can then filter them (ADR M93-2). This is the correctness
foundation the node integration builds on — without it a lossy bitmap silently loses rows.

#### Files to edit

- `theodb_rs/src/am/customscan.rs` — the `SCAN_MEMBERSHIP` type + `set_membership`/`membership`/`has_membership`.
- `theodb_rs/src/am/scan.rs` — the Stage-1 membership admit in `scan_ivf_aq_split_v7`.

#### Deep file dependency analysis

`set_membership` is called by the v1a tests and (Phase 2) by `begin_custom_scan`. `membership()` is read by
`scan_ivf_aq_split_v7`. The block extraction reuses the `tid::encode` layout (`(block<<16)|offset`), so
`block = (tid >> 16) as u32`.

#### TDD

```
test_membership_admits_lossy_block (pg_test):
  GIVEN a membership with exact={} and lossy={block B} and a row whose ctid is on block B
  WHEN a plain vector scan runs under that membership
  THEN that row IS returned (admitted via the lossy block), whereas a row on a non-member block is NOT.
```

#### Concurrency tests

(none — single-threaded: the membership is a backend-local `thread_local`; a Postgres backend serves one query at a
time. No shared mutable state across threads.)

#### Acceptance criteria

- `test_membership_admits_lossy_block` asserts a row on a lossy-member block is returned AND a row on a non-member
  block is not (oracle: the returned id set).
- The v1a exact-membership tests still pass (exact path unchanged).

#### DoD

- `cargo pgrx test` green for the new + existing membership tests (droplet).
- `git diff --stat` shows no `page.rs` change (no format change).

## Phase 2 — the 2-child Custom Scan node (hook + MultiExec + recheck)

### Task T2.1 — hook builds the 2-child path; node MultiExecs the bitmap, materializes, sets membership, rechecks

#### Why this step

**Action:** (a) `pathlist_hook` finds a `BitmapHeapPath` (→ `bitmapqual`) + the vector-ordered `IndexPath` (pathkeys
!= NIL) and builds `custom_paths=[vector, bitmapqual]`; (b) `create_custom_scan_state` allocates a `VecFilterState`
struct embedding `CustomScanState` first; (c) `begin_custom_scan` inits both children, `MultiExecProcNode`s the bitmap
→ TIDBitmap (tag-checked, not `IsA`), `materialize_bitmap` → `set_membership({exact,lossy})`; (d) `exec_custom_scan`
pulls the ordered tuple from the vector child and rechecks `ss.ps.qual` via `evalfunc`; (e) `end_custom_scan` clears
membership + ends both children. Clear membership defensively at `begin_custom_scan` entry too (error-longjmp leak).

**Reasoning:** this is the assembly the discovery grounded as a near-verbatim port of `nodeBitmapHeapscan.c`
(MultiExec `:106-115`, recheck `:571-573`, 2-child init `:736`, end `:652`) + `nodeCustom.c` (custom-struct embed
`:34-42`, `ss.ps.qual` compiled by core `:101`). Every proven primitive (materialize, set_membership) is reused.

#### Files to edit

- `theodb_rs/src/am/customscan.rs` — the hook (2-child path), `VecFilterState`, the 4 callbacks.

#### Deep file dependency analysis

`begin_custom_scan` reads `cscan.custom_plans` (the planned [vector, bitmap]). `exec_custom_scan` uses the
`PlanState.ExecProcNode` fn-pointer of the vector child + `css.ss.ps.qual`/`ps_ExprContext`. `end_custom_scan` calls
`set_membership(None)` (the anti-leak contract) + `ExecEndNode` on both. Does NOT `tbm_free` (the TIDBitmap lives in
the bitmap child's context, freed by `ExecEndNode`).

#### TDD

```
test_customscan_arbitrary_where_equals_seqscan (pg_test):
  GIVEN a table with a NON-label scalar column `cat` (btree indexed) + a vector index, hook ON
  WHEN `SELECT id FROM t WHERE cat = K ORDER BY e <-> q LIMIT n`
  THEN EXPLAIN shows the Custom Scan node with 2 children AND the id set equals the exact seqscan-filtered top-n.
test_customscan_pending_rechecked (pg_test):
  GIVEN a post-build INSERT with a NON-matching `cat` and a near vector
  THEN it does NOT appear in a filtered result (the recheck removes the label-less pending false positive).
```

#### Concurrency tests

(none — single-threaded backend; membership is thread-local.)

#### Acceptance criteria

- `test_customscan_arbitrary_where_equals_seqscan`: the id set == exact seqscan set (oracle: `assert_eq` on sorted ids)
  AND EXPLAIN contains `Custom Scan (theodb_vecfilter)`.
- `test_customscan_pending_rechecked`: the non-matching pending row is absent (oracle: `!contains`).

#### DoD

- `cargo pgrx test` green for the 2 new tests (droplet).
- No `page.rs` change.

## Phase 3 — correctness gate (lossy + full-suite no-regression)

### Task T3.1 — force a lossy bitmap and prove the result stays exact

#### Why this step

**Action:** a pg_test that forces the bitmap lossy (`SET work_mem` low + a filter matching many rows on many pages)
and asserts the filtered vector result still equals the exact seqscan set (the recheck removes the lossy over-admits).

**Reasoning:** the lossy path is the #1 correctness risk (ADR M93-2). A green happy-path is not enough — the lossy
over-admit + recheck must be proven, or a large loose filter silently returns wrong rows.

#### Files to edit

- `theodb_rs/src/am/customscan.rs` — a new pg_test (tests module).

#### TDD

```
test_customscan_lossy_bitmap_still_exact (pg_test):
  GIVEN `SET work_mem='64kB'` + a filter matching thousands of rows across many heap pages (forces tbm lossy)
  WHEN the filtered vector query runs under the Custom Scan
  THEN the id set equals the exact seqscan-filtered set (recheck removed every lossy over-admit).
```

#### Concurrency tests

(none — single-threaded.)

#### Acceptance criteria

- `test_customscan_lossy_bitmap_still_exact`: id set == exact seqscan set even under forced-lossy `work_mem` (oracle:
  `assert_eq` sorted).

#### DoD

- Full suite `cargo pgrx test` GREEN (≥ 263 tests, 0 failed) — the GUC-off path byte-identical (no-regression).

## Failure scenarios

- **MultiExec returns a non-TIDBitmap** (corrupt plan): `begin_custom_scan` tag-checks `type_ == T_TIDBitmap` and
  `pg_sys::error!`s (fail-loud), never a bad cast. Test: covered by the invariant; a fabricated child would error.
- **Null `ExecProcNode`/`evalfunc` fn-pointer** (corrupt plan/expr): matched with `Some(f) => .. , None =>
  pg_sys::error!` — never `.unwrap()` panicking across the C boundary.
- **Membership leak on mid-scan `error!` longjmp**: `begin_custom_scan` clears the thread-local on ENTRY (defensive)
  so a prior aborted scan cannot bleed into this one; `end_custom_scan` clears on normal exit. Test:
  `m92_v1a_membership_cleared_is_inert` (existing) + the arbitrary-where tests run back-to-back in one backend.
- **Bitmap child owns the TIDBitmap** — the node does NOT `tbm_free` (double-free hazard); `ExecEndNode(bitmap_child)`
  frees it. Covered by the End path (no free call) + the no-regression suite (no crash across many tests).

(No external I/O — HTTP/DB-driver/queue/RPC/object-store — is touched; all reads are internal buffer-manager /
executor calls, error-propagated via `pg_sys::error!`.)

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Membership carries lossy blocks (no under-admission) | T1.1 | `{exact,lossy}` side channel + Stage-1 lossy-block admit |
| 2 | Hook builds the 2-child path (vector + bitmapqual) | T2.1 | find `BitmapHeapPath.bitmapqual` + vector-ordered path |
| 3 | Node MultiExecs bitmap → materialize → set_membership | T2.1 | `MultiExecProcNode` + `materialize_bitmap` (v1b) + `set_membership` (v1a) |
| 4 | MVCC recheck removes lossy/pending over-admits | T2.1, T3.1 | `ExecQual` of `ss.ps.qual` via `evalfunc` on the heap tuple |
| 5 | End clears membership (anti-leak) | T2.1 | `set_membership(None)` in End + defensive clear in Begin |
| 6 | Arbitrary-WHERE result == exact seqscan (non-label col) | T2.1 | `test_customscan_arbitrary_where_equals_seqscan` |
| 7 | Lossy bitmap stays exact under recheck | T3.1 | `test_customscan_lossy_bitmap_still_exact` |
| 8 | Pending row rechecked (no false positive) | T2.1 | `test_customscan_pending_rechecked` |
| 9 | Zero regression (GUC-off byte-identical) | T3.1 | full suite ≥ 263 GREEN |
| 10 | Benchmark: inline-by-bitmap recall > post-filter (SIFT, arbitrary col) — M92 DoD | T4.1 | droplet benchmark |
| 11 | sign-off council-rust-pgrx + council-index-storage | T4.1 | review |

**Coverage: 11/11 gaps covered (100%)**

## Drawbacks & Risks

| # | Risk | Severity | Mitigation | Owner |
|---|---|---|---|---|
| 1 | Lossy-bitmap UNDER-admission (dropping lossy blocks silently loses valid rows) | HIGH | ADR M93-2: membership carries lossy blocks + `test_customscan_lossy_bitmap_still_exact` forces the lossy path | impl |
| 2 | Membership leak across queries on a mid-scan `error!` longjmp (skips End) | HIGH | defensive clear at Begin ENTRY + End clear; back-to-back-query test | impl |
| 3 | Panic across the C boundary in the ~6 callbacks / fn-pointer `.unwrap()` | HIGH | `extern "C-unwind"` + `pg_sys::error!` on every corrupt state; `Some/None` match on fn-pointers, never `.unwrap()` | impl |
| 4 | The node overhead may not beat native post-filter → honest-negative | MEDIUM | the Phase-4 benchmark measures before any claim; honest-negative is a valid terminal (documents arbitrary-WHERE-is-post-filter) | impl |

## Unresolved Questions

- Should the recheck route through `pg_sys::ExecScan` (EPQ-future-proof) or a manual pull+recheck loop? Resolved-at-plan:
  the target query is read-only `SELECT ... ORDER BY <-> LIMIT` where EPQ cannot fire (discovery Q5), so a manual
  pull+recheck is correct and simpler for v1; `ExecScan` routing is a documented future hardening if the node is ever
  composed under row-locking. Decision stays inside T2.1.

## Global DoD

- All new pg_tests GREEN + full suite ≥ 263 tests, 0 failed (droplet `cargo pgrx test`).
- No on-disk format change (no REINDEX); `page.rs` untouched.
- `customscan.rs` stays < 800 LoC; `scan.rs` < 1000 LoC.
- CHANGELOG `[Unreleased]` updated.

## Final Phase: Integration Validation

### Task T4.1 — benchmark (M92 DoD) + council sign-off

#### Why this step

**Action:** benchmark on the droplet (SIFT1M, real neighbors — the M91 tie-density lesson): a NON-label scalar column
with a btree index, `WHERE <scalar> ORDER BY e <-> q LIMIT k` across selectivity, Custom Scan ON (inline-by-bitmap) vs
OFF (native post-filter); then dispatch the councils.

**Reasoning:** the M92 DoD requires a measured inline-vs-post comparison on an arbitrary column; and the executor-
lifecycle + MVCC recheck must be reviewed by the two owning councils before merge.

#### Files to edit

- `benchmarks/m92_arbitrary_where_bench.py` (NEW) — the SIFT arbitrary-WHERE harness.
- `docs/benchmarks/m92-arbitrary-where.{md,json}` (NEW) — the measured artifact.

#### TDD

```
(measurement task — no unit RED test; the gate is the benchmark artifact + the correctness re-assertion)
harness assertion: at every selectivity, the Custom Scan filtered id set == exact seqscan-filtered set (correctness
first); then record recall@10 + QPS for inline-by-bitmap vs post-filter.
```

#### Concurrency tests

(none — the benchmark is a measurement harness.)

#### Acceptance criteria

- Correctness: at every measured selectivity the Custom Scan result equals the exact seqscan-filtered set (oracle: the
  harness diffs the two id sets and fails on any mismatch).
- Performance (M92 DoD): `docs/benchmarks/m92-arbitrary-where.json` records inline-by-bitmap recall@10 ≥ post-filter at
  the selective regime, with hardware + methodology; **honest-negative recorded plainly if the node does not beat
  post-filter** (a valid terminal, not a failure to hide).
- Review: council-rust-pgrx + council-index-storage both return READY_TO_MERGE (or their findings fixed).

#### DoD

- `docs/benchmarks/m92-arbitrary-where.{md,json}` committed with real numbers traceable to a droplet run.
- Councils signed off; NOT a QPS-superiority claim vs ScaNN/AlloyDB (M73/M82 ceiling stated).
