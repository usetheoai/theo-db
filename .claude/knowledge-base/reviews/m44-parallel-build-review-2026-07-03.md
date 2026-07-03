# Review — M44 parallel theodb_hnsw build

**Date:** 2026-07-03
**Slug:** m44-parallel-build
**Milestone:** M44
**Verdict:** READY_TO_MERGE
**Scope:** Rust production code — `theodb_rs/src/ann/{hnsw_parallel.rs (new), hnsw.rs, mod.rs}` (concurrent HNSW build).

## Change

The theodb_hnsw graph build runs concurrently (`std::thread::scope` + per-node `RwLock`), dispatched on corpus
size (< 4096 sequential/deterministic; ≥ parallel). No new dependency; NON-DETERMINISTIC build accepted (racy
insert; recall parity is the gate).

## Findings by dimension (focused concurrency audit)

| Dimension | Result | Evidence |
|---|---|---|
| **Data-race freedom** | PASS (SOUND) | All neighbor mutation via `.write()`, reads via `.read()`; `vectors`/`levels` read-only (`&`, never `&mut`); `AtomicUsize` counter with correct `Relaxed` (unique-index distribution; happens-before comes from the RwLocks). |
| **Deadlock-freedom** | PASS | Confirmed NO nested cross-node locking: `insert_node` holds at most one node lock at a time (search read-locks + clones + drops; linking write-locks one `nb` at a time, released each iteration). `select_from` runs under `nb`'s write lock but reads ONLY `vectors` (no neighbor lock) → no A→B/B→A cycle possible. |
| **Lost-update (pruning)** | PASS | `push(node)` + `select_from` prune are under the SAME `nbn` write guard (no drop-then-relock) → no lost update on prune. |
| **Panic-safety (C boundary)** | PASS | `std::thread::scope` re-raises a worker panic on join → propagates through `HnswIndex::build` → `ambuild_hnsw` (`#[pg_guard] extern "C-unwind"`) → PG ereport. No UB, no longjmp over Rust frames. Poison handled (`unwrap_or_else(|e| e.into_inner())`). |
| **scope + borrow** | PASS | Worker closures borrow (no `move`) `&neighbors/&state/&counter/vectors/levels` (Sync); scope joins before the borrow ends. No `'static`, no `Arc`, no escaping borrow. |
| **Determinism/recall + promotion** | PASS | Stale `(ep, max_level)` snapshot is safe (max_level monotonic; connected graph); entry promotion double-checks under the write lock (`if level > st.1`) → no promotion race. Recall parity gated (Δ +0.0055 @50k; pg_tests self-recall ≥19/20 + recall@10 ≥0.85). |
| **nthreads edges** | PASS | `min(cpus, n)` with n ≥ 4096; excess threads get `node ≥ n` and break (no OOB); AtomicUsize overflow impossible. |

## Honest LOW finding (non-blocker, within the accepted envelope)

**Back-link lost-update:** the node's own forward-set assignment (`nn[layer] = selected.clone()`) OVERWRITES the
list, so an in-flight back-link another thread pushed can be clobbered — a LOGICAL lost-update (NOT a data race:
serialized under the node's write lock). Effect: slightly fewer in-edges → a minor recall effect, well inside the
M44 accepted racy-build envelope (ADR D2) and empirically covered by the recall-parity gate (Δ +0.0055). Per the
reviewer + Esforço≠Complexidade/YAGNI, NOT fixed (recall is green; a merge-instead-of-overwrite fix adds
complexity without proven need). **Documented in code** (`hnsw_parallel.rs` forward-set comment) for future
maintainers + here.

## Hard gates

- Failing tests → none (8/8 `test_index_am.py`; live 6000-node parallel build valid). No secrets. On `develop`.
  No `Co-Authored-By`. CHANGELOG updated. Build compiled clean; no `unsafe` added.

## Benchmark requirement (standing directive)

Satisfied: A/B build-time with data (3-sample back-to-back mean±std @50k, std bands separated + 1M confirmation),
recall parity — the honest oracle. Build 2.82× @50k / 1.95× @1M (8.4min → 4.3min).

## Verdict rationale

No BLOCKER. Race-free + deadlock-free + panic-safe BY CONSTRUCTION (Rust RwLock/scope, no unsafe). Recall
preserved (parity gate + AM tests). Build 2.82× (controlled) faster. The LOW back-link finding is within the
explicitly-accepted non-determinism envelope, documented, and empirically harmless. **READY_TO_MERGE.**

## Release recommendation

Product code (Rust) with a measured, recall-preserving build-time win — a legitimate release candidate. Human decides.
