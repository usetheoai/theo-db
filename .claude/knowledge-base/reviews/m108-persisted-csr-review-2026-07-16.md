---
slug: m108-persisted-csr-am
milestone_id: M108
date: 2026-07-16
cycle: review
---

# /review — M108 persisted-CSR graph structure

**Verdict:** READY_TO_MERGE (2 HIGH + 1 MEDIUM fixed)

Independent adversarial review (council-index-storage) — regenerated the graph + re-measured 3 baseline formulations independently. Core claims held under attack.

## Verified sound
- **Crash-safety:** the CSR as a `bytea` in an ordinary heap table IS WAL-logged + crash-safe + MVCC by PostgreSQL — aborted `graph_build` → no visible tuple; committed → survives replay byte-identical; no torn/half-written window. The "estrutura CSR sobre a edge-table" scope (vs full index-AM) is honestly disclosed.
- **Cache correct:** `Rc<Csr>` cloned inside the `RefCell` borrow, released before `expand()` — no borrow held across SPI, no use-after-free; keyed by oid, `built_at`-epoch invalidation.
- **Benchmark honest:** fair `UNION`-dedup indexed baseline (not strawman); SPI-overhead + cold/warm + release/debug all disclosed; 16× is a conservative floor.
- **Security clean:** identifiers `%I`-quoted (injection-safe), functions REVOKE'd from PUBLIC.

## Findings — FIXED
- **[HIGH-1] OID-reuse stale row → silent wrong answer.** A `graph_csr` row survived DROP of its edge relation; a reused OID would serve the old graph's CSR. → **FIXED:** added an `sql_drop` event trigger (`theodb.graph_on_drop`) that deletes the orphaned row. Proven by `m108_drop_removes_persisted_csr` (after DROP, 0 orphans).
- **[HIGH-2] clock_timestamp cache fix untested.** → **CLARIFIED + confirmed:** `m108_refold_folds_new_edges` IS the same-txn regression test — it failed with `now()` (txn-constant epoch → stale cache) and passes with `clock_timestamp()` (E2≠E1 within one txn → reload). Comment updated to make this explicit.
- **[MEDIUM] count-oracle not injective.** The scale bench used `count==count`. → **FIXED:** upgraded to a **set-hash** oracle (`bit_xor(hashint8(node))`) comparing the reachable SET, as the blueprint mandated.

## Findings — accepted (tracked)
- **[MEDIUM] No SIGABRT+WAL-replay crash test.** The bytea-in-heap durability is sound by construction (Postgres guarantee); a dedicated crash harness is a documented follow-up (the claim rests on Postgres's WAL, not an executed replay — honestly noted).
- **[LOW]** sub-µs epoch collision (impossible for a 274ms rebuild); `%s` edge_rel splice (mitigated by REVOKE + `::regclass`); `catch_unwind` idiom in the error test.

## Hard gates
✅ no BLOCKER · ✅ no secrets · ✅ no main commit · ✅ commit-trailer policy honored · ✅ CHANGELOG updated · 330 pg_tests GREEN (+5 M108, 0 regression).

**READY_TO_MERGE.** Crash-safety sound, cache correct, benchmark honest (16×, fair baseline, caveats disclosed), security clean; the 2 HIGH silent-wrong-answer hazards fixed with tests.
