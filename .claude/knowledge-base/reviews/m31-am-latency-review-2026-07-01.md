# /review — M31 index AM latency (structured partial-page reads)

**Date:** 2026-07-01
**Slug:** m31-am-latency
**Diff scope:** `e85ca48..HEAD` (theodb_rs/src/{am,ann} + benchmarks + docs + ROADMAP)
**Verdict:** READY_TO_MERGE

## Round 1 — 2 focused agents (the M26 page/buffer/WAL primitives were reviewed in M26; M31 reuses them)

| Agent | Verdict |
|---|---|
| structured correctness (layout / directory / dispatch / maintenance) | HAS_BUG (1 BLOCKER) |
| FFI safety + re-scoped-DoD honesty | SOUND_AND_HONEST (0 BLOCKER/HIGH) |

### Findings + resolution

| Sev | Finding | Resolution (commit `d50c7eb`) |
|---|---|---|
| **BLOCKER** | `scan_ivf_structured` early-returned on empty centroids BEFORE folding pending → a structured index built (or vacuumed) empty then INSERTed-into silently returned zero neighbors (regression vs M26 blob path) | Removed the early return — the probe loop is empty when there are no centroids and the pending fold still runs. New regression test `test_insert_into_empty_built_index_is_found` (green). |
| LOW | `main_index_pages` `total += npages` could wrap on a corrupt meta | `saturating_add` (defense-in-depth) |
| LOW | latency gate ran at N=50k while the DoD headline is 100k | bumped the gate to N=100k |
| LOW | duplicate `### Changed` in CHANGELOG | removed |
| INFO | untrusted `dim` drives the scratch alloc; bounded indirectly by `read_ivf_meta`'s centroid-region check | accepted (bounded; noted) |

### Confirmed CORRECT (correctness agent)

Directory↔pages arithmetic (no off-by-one), write vs rewrite item-sequence identity, `peek_magic` dispatch totality (structured vs blob, both formats), byte-parsing = `encode_list`, scratch-buffer scoring produces identical distances, VACUUM fold folds+drops-dead with no lost/duplicated entry, empty index/list no crash.

### Confirmed SOUND + HONEST (FFI agent)

Every new byte-slice read is bounds-checked (typed `Err` in parsers, safe `break` in the hot scan loop); capacity allocations guarded behind length checks; buffers released before any `Err`/`error!` — no OOB/UB. The re-scope (ADR 0011) is textbook-honest: records the real 2.7×-behind number, does NOT falsify the benchmark, defers `≤ pgvector` to a properly-sequenced M31b (before M32), and the gate proves the re-scoped claims rather than false-passing.

## Evidence

- `cargo clippy --features pg17 --tests -- -D warnings`: clean.
- Full image `theo-db:m31` builds; **51 tests green** (index-AM incl. empty-built-index regression + 100k latency + M20–M22 coexistence).
- Benchmark (`docs/benchmarks/m31-am-latency.{md,json}`): structured Index Scan ~38 ms vs M26 O(N) ~1700 ms = **~45×**; ~2.7× behind pgvector (honest; residual = scalar-vs-SIMD → M31b). Recall preserved.

## Verdict: READY_TO_MERGE (re-scoped DoD per ADR 0011)

0 BLOCKER (the one found was fixed + regression-tested), 0 HIGH. The re-scoped M31 DoD (O(N)-per-scan closed via
structured partial reads · correctness + maintenance intact · latency far below the O(N) regime and within a
documented band of pgvector) is met by evidence. The `≤ pgvector` latency parity is honestly deferred to M31b
(SIMD distance), sequenced before M32. No faked benchmark, no undocumented gap.
