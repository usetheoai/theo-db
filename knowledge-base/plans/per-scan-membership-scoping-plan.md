---
slug: per-scan-membership-scoping
milestone_id: M94
created_at: 2026-07-13
goal: Scope the vecfilter membership per Custom Scan node via pull-window swap-discipline so a UNION ALL of two filtered vector queries returns the exact union of the per-branch seqscan-filtered results (replacing the M93 fail-loud guard), with 263+ tests green and the single-scan path byte-identical.
---

# M94 — per-scan membership scoping (unblocks filtered UNION/self-join/Append)

## Context

The M92/M93 review's convergent BLOCKER (councils rust-pgrx B1 + index-storage HIGH-1): the vecfilter membership is a
single per-backend `thread_local` slot, but the executor runs every node's `BeginCustomScan` (Init) before any pull
(Exec), and the AM reads the membership lazily at scan time — so two vecfilter nodes in one plan (`UNION` / self-join
/ partitioned `Append`) cross-contaminate → silently wrong results. M93 shipped the councils' accepted stopgap:
**fail-loud** on the second node (`m93_concurrent_vecfilter_fails_loud`). M94 delivers the real fix the councils
prescribed: per-scan membership scoping.

## Goal

Scope the vecfilter membership per Custom Scan node via pull-window swap-discipline so a `UNION ALL` of two filtered
vector queries returns the exact union of the per-branch seqscan-filtered results (replacing the M93 fail-loud guard),
with 263+ tests green and the single-scan path byte-identical.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Role |
|---|---:|---|---|
| `theodb_rs/src/am/customscan.rs` | ~530 | `85a01dd` (v0.80.1) | the Custom Scan Provider — the membership side channel (`SCAN_MEMBERSHIP` thread_local, `set_membership`/`membership`/`has_membership`), the node callbacks, the fail-loud guard, the xact-abort callback, the pg_tests |

### Current callers / dependents (from code read)

- `SCAN_MEMBERSHIP: thread_local RefCell<Option<Rc<Membership>>>` — the single active slot. Written by
  `set_membership` (from `begin_custom_scan`, `rescan_custom_scan`, `end_custom_scan`, the xact callback, and the
  v1a tests); read by `membership()`/`has_membership()` from `scan.rs` (`amrescan` :~146, `scan_ivf_aq_split` :~503,
  `scan_ivf_aq_split_v7` :~619) — the reads happen INSIDE the child's synchronous execution (amrescan fills the heap;
  amgettuple re-searches), i.e. inside our `ExecProcNode(vector_child)` / `ExecReScan(vector_child)` call windows.
- `begin_custom_scan` — currently: fail-loud if `has_membership()` (the M93 guard), MultiExec → materialize →
  `set_membership(Some(...))`, `st.membership_active = true`.
- `exec_custom_scan` — `pg_sys::ExecProcNode(st.vector_child)` (a per-output-tuple pull).
- `rescan_custom_scan` — clear-then-set + MultiExec re-derive.
- `end_custom_scan` — `set_membership(None)` + `ExecEndNode` both children.
- `xact_clear_membership` — clears on `XACT_EVENT_ABORT`/`PARALLEL_ABORT` (registered in `init()`); **gap: no
  SUBXACT abort callback** — a PL/pgSQL `EXCEPTION` handler aborts only the subtransaction, skipping the clear.
- pg_test `m93_concurrent_vecfilter_fails_loud` — asserts the fail-loud error on a UNION; REPLACED by the new
  correctness test in this plan.

### Domain glossary

- **pull window** — the synchronous span of one `ExecProcNode(vector_child)` (or `ExecReScan`) call; ALL AM work
  (amrescan / amgettuple / iterative re-search) for that pull happens inside it — a backend is single-threaded.
- **swap-discipline** — save the active slot, install this node's membership, pull, restore the saved value.
  Re-entrant by construction (stack discipline): a SubPlan inside the child's Filter that runs another vecfilter
  node nests correctly.
- **registry** — a thread-local `HashMap<usize, Rc<Membership>>` keyed by the node pointer, holding each node's
  membership across pulls. Rust-side storage (normal Drop) — NEVER an `Rc` inside the `palloc0`'d `VecFilterState`
  (Postgres frees that memory without running Drop → the HashSets would leak).

### Architecture boundaries affected

`customscan.rs` only (interface layer). No AM change (`scan.rs` keeps reading `membership()` — the swap makes the
read per-scan-correct). No page-format change. GUC default-OFF unchanged.

## Prior Art & Related Work

- The M92/M93 review — the councils' prescribed fix, verbatim: *"snapshot the membership into the vector child's
  scan … so two nodes each carry their own set"* / *"scope the membership to the specific index scan"*
  (`knowledge-base/reviews/custom-scan-node-integration-review-2026-07-13.md`). The pull-window swap achieves the
  same isolation without touching the AM's ScanState (smaller diff, same guarantee — the AM's reads only ever occur
  inside the owning node's pull window).
- Postgres executor: `ExecProcNode` is synchronous; `nodeAppend.c`/`nodeUnion` interleave child pulls — the swap
  discipline is exactly what makes interleaving safe.
- The M93 xact-abort callback (extended here with the SUBXACT sibling).

## ADRs

### ADR M94-1 — pull-window swap-discipline over AM ScanState plumbing

**Decision:** keep the AM reading a thread-local `membership()`, but make the Custom Scan node install its OWN
membership only for the duration of each child pull (`swap → pull → restore`), with per-node storage in a
thread-local registry keyed by the node pointer.

**Rejected alternatives:** (a) *plumb the membership into the AM's `ScanState`* (a setter keyed by IndexScanDesc) —
REJECTED: requires a new cross-layer API between the node and the AM internals, touches `scan.rs` state lifecycle,
and still needs a channel to find the right ScanState from the node (the node holds a `PlanState`, not the
IndexScanDesc). The swap achieves the identical isolation guarantee (the AM only reads during the owning pull window,
single-threaded) with a smaller, interface-layer-only diff (KISS). (b) *keep fail-loud forever* — REJECTED: filtered
UNION/self-join/Append are legitimate query shapes; refusing them is a capability gap vs the milestone goal.

### ADR M94-2 — registry in a thread_local, never Rust-droppable data in palloc

**Decision:** node → membership mapping lives in `thread_local HashMap<usize, Rc<Membership>>`; `VecFilterState`
keeps only POD fields.

**Rejected alternative:** *store the `Rc<Membership>` in `VecFilterState`* — REJECTED: the struct is `palloc0`'d and
freed by memory-context reset without running Rust `Drop` → the `Rc` refcount never decrements → the (potentially
tens-of-MB) HashSets leak per query.

### ADR M94-3 — clear on BOTH xact and subxact abort

**Decision:** register `RegisterSubXactCallback` (clearing on `SUBXACT_EVENT_ABORT_SUB`) alongside the existing xact
callback; both clear the ACTIVE slot and the registry.

**Rejected alternative:** *xact-abort only (status quo)* — REJECTED: a PL/pgSQL `EXCEPTION` handler aborts only the
subtransaction; the top-level xact continues, so a stale ACTIVE slot/registry entry from the failed sub-block would
survive into subsequent queries of the same transaction.

## Dependency Graph

```
Phase 1 (registry + swap-discipline + subxact clear; remove fail-loud) ──> Phase 2 (correctness gate: UNION/self-join/abort tests + full suite) ──> Phase 3 (review + release)
```

## Phase 1 — registry + swap-discipline

### Task T1.1 — per-node registry, pull-window swap, subxact clear, remove the fail-loud guard

#### Why this step

**Action:** (a) add `NODE_MEMBERSHIPS: thread_local RefCell<HashMap<usize, Rc<Membership>>>` + a
`swap_active(Option<Rc<Membership>>) -> Option<Rc<Membership>>` helper; (b) `begin_custom_scan`: store the
materialized membership in the registry under `node as usize` (NOT the active slot); remove the fail-loud guard;
(c) `exec_custom_scan`: look up the node's membership → `let prev = swap_active(mine)` → `ExecProcNode` →
`swap_active(prev)` → return the slot; (d) `rescan_custom_scan`: update the registry entry (re-derive via MultiExec)
and wrap `ExecReScan(vector_child)` in the same swap window; (e) `end_custom_scan`: remove the registry entry (drops
the sets); (f) register `RegisterSubXactCallback` clearing ACTIVE + registry on `SUBXACT_EVENT_ABORT_SUB`, and extend
the xact callback to clear the registry too.

**Reasoning:** the councils' BLOCKER is Init-phase overwrite of a single slot; per ADR M94-1 the pull window is the
correct isolation boundary (all AM reads happen inside it, single-threaded), and save/restore makes nesting
(SubPlan-in-Filter) correct by stack discipline. The registry (ADR M94-2) avoids the palloc/Drop leak; the subxact
callback (ADR M94-3) closes the EXCEPTION-handler leak the xact-only callback misses.

#### Files to edit

- `theodb_rs/src/am/customscan.rs` — the membership section, the 4 node callbacks, `init()`, the tests module.

#### Deep file dependency analysis

`scan.rs` is UNTOUCHED — `membership()`/`has_membership()` keep their signatures; the swap makes their reads
per-scan-correct. The v1a tests call `set_membership` directly (the ACTIVE slot) — that API stays for tests. The
`m93_concurrent_vecfilter_fails_loud` pg_test is DELETED (replaced by T2.1's correctness tests). `xact_clear_membership`
grows a registry clear; a sibling subxact callback is added.

#### TDD

```
test_m94_union_two_filtered_scans_correct (pg_test):
  GIVEN a table with a btree-indexed scalar `cat` + a vector index, vecfilter ON
  WHEN (SELECT id ... WHERE cat=1 ORDER BY e<->q LIMIT 3) UNION ALL (SELECT id ... WHERE cat=2 ... LIMIT 3)
  THEN the result equals the union of the two exact seqscan-filtered top-3 sets (each branch filtered by ITS OWN
       membership — the sets are disjoint by construction, so any cross-contamination shows as a wrong id).
test_m94_interleaved_rescan_correct (pg_test):
  GIVEN the same table
  WHEN a nested-loop shape re-scans a filtered vector scan with a changing outer (LATERAL over 2 cat values)
  THEN each inner result equals its exact seqscan-filtered set (the rescan window re-derives the right membership).
```

#### Concurrency tests

(none — single-threaded: a Postgres backend serves one query at a time; the swap window is synchronous within the
backend. The thread_locals are per-backend by construction. No cross-thread state is added.)

#### Acceptance criteria

- `test_m94_union_two_filtered_scans_correct` asserts the UNION id multiset equals the union of the two exact
  seqscan-filtered top-3 lists (oracle: `assert_eq` on sorted ids).
- `test_m94_interleaved_rescan_correct` asserts each LATERAL inner result equals its exact seqscan set (oracle:
  per-branch `assert_eq`).
- The M93 fail-loud test is removed; grep confirms no `concurrent filtered vector scans` error remains reachable.

#### DoD

- `cargo pgrx test` green for the new tests (droplet).
- Existing M92/M93 pg_tests (arbitrary-where-equals-seqscan, pending-rechecked, lossy-block, v1a membership, inert)
  still green — the single-scan path is byte-identical.

## Phase 2 — correctness gate

### Task T2.1 — abort-leak test + full-suite no-regression

#### Why this step

**Action:** a pg_test that fails a filtered vector query inside a PL/pgSQL `EXCEPTION` block (forcing a subxact
abort mid-scan), then runs a PLAIN vector query (no WHERE) and asserts it returns the unfiltered result (no stale
membership filtering it). Then the full suite.

**Reasoning:** the subxact-abort leak is the one path where the swap's restore is skipped by a longjmp (ADR M94-3);
this test proves the callback closes it. The full suite is the no-regression gate (Global DoD).

#### Files to edit

- `theodb_rs/src/am/customscan.rs` — tests module.

#### TDD

```
test_m94_subxact_abort_clears_membership (pg_test):
  GIVEN vecfilter ON and a filtered vector query wrapped in a plpgsql block whose later statement RAISEs
  WHEN the EXCEPTION handler catches (subxact abort) and a plain vector query runs afterward
  THEN the plain query returns the full unfiltered top-k (a stale membership would starve it).
```

#### Concurrency tests

(none — single-threaded, as above.)

#### Acceptance criteria

- `test_m94_subxact_abort_clears_membership` asserts the post-abort plain scan returns ≥ k rows matching the
  no-membership baseline (oracle: compare to the same query before any vecfilter ran).

#### DoD

- Full `cargo pgrx test` GREEN (≥ 264 tests, 0 failed) on the droplet; GUC-off path byte-identical.
- CHANGELOG `[Unreleased]` updated.

## Failure scenarios

- **Mid-pull `pg_sys::error!` longjmp skips the swap restore:** the ACTIVE slot holds this node's membership until
  the xact/subxact abort callback clears it (ADR M94-3). The abort callback runs before any new statement in the
  backend, so no later scan reads the stale value. Test: `test_m94_subxact_abort_clears_membership`.
- **MultiExec returns a non-TIDBitmap on rescan:** unchanged M93 behavior — `pg_sys::error!` fail-loud.
- **Registry entry missing at exec (corrupt lifecycle):** `exec_custom_scan` treats a missing entry as
  `pg_sys::error!` (fail-loud, never a silent unfiltered scan under a WHERE).

(No external I/O touched.)

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Per-node membership storage without palloc/Drop leak | T1.1 | thread_local registry keyed by node ptr (ADR M94-2) |
| 2 | Each node's pulls see only ITS membership (UNION/self-join/Append) | T1.1 | pull-window swap-discipline (ADR M94-1) |
| 3 | Rescan re-derives + uses the right membership | T1.1 | registry update + swap window around ExecReScan |
| 4 | Nested vecfilter (SubPlan in Filter) correct | T1.1 | save/restore stack discipline |
| 5 | Fail-loud guard removed (capability delivered) | T1.1 | guard deleted; UNION test replaces the fail-loud test |
| 6 | Subxact-abort leak closed | T1.1, T2.1 | RegisterSubXactCallback clear (ADR M94-3) + abort test |
| 7 | Zero regression (single-scan byte-identical; GUC-off untouched) | T2.1 | full suite ≥ 264 GREEN |
| 8 | sign-off council-rust-pgrx | T2.1 | the council review is dispatched at integration validation (part of T2.1's DoD chain); findings fixed before `/release` |

**Coverage: 8/8 gaps covered (100%)**

## Drawbacks & Risks

| # | Risk | Severity | Mitigation | Owner |
|---|---|---|---|---|
| 1 | A child-execution path outside the pull window (membership not active when the AM reads) | HIGH | the AM reads only inside amrescan/amgettuple, which run inside ExecProcNode/ExecReScan — both wrapped; the UNION + LATERAL tests catch any missed window | impl |
| 2 | Per-pull overhead (TLS lookup + 2 swaps per output tuple) | LOW | ~k pulls per LIMIT-k query; trivially amortized vs the Stage-1 scan; spot-check vs the M92 benchmark numbers | impl |
| 3 | Registry growth if End is skipped repeatedly (longjmp) | MEDIUM | xact/subxact abort callbacks clear the WHOLE registry (not just ACTIVE) | impl |

## Unresolved Questions

(none — every decision is resolved at plan time; the design is the councils' prescribed fix realized at the pull-window boundary.)

## Global DoD

- Full suite ≥ 264 tests, 0 failed (droplet `cargo pgrx test pg17`); GUC-off path byte-identical.
- No page-format change; `scan.rs` untouched.
- `customscan.rs` stays < 800 LoC.
- CHANGELOG `[Unreleased]` updated.

## Final Phase: Integration Validation

- Full suite GREEN on the droplet (the correctness gate — UNION/LATERAL/abort tests + all M92/M93 tests).
- Spot-check: re-run one M92 benchmark point (1% selectivity) and assert recall/QPS within noise of the v0.80.1
  numbers (the swap must not perturb the single-scan path).
- Review: council-rust-pgrx (the owning council for the executor-lifecycle/thread-local surface); findings fixed
  before `/release`.
