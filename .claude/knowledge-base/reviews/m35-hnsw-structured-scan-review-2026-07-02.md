# Review — m35-hnsw-structured-scan

**Date:** 2026-07-02 · **Verdict:** READY_TO_MERGE · **Milestone:** M35
**Method:** 3 parallel specialist agents (Rust engine correctness + FFI safety · benchmark methodology + scientific honesty · cross-validation plan↔impl↔artifact) over commit `f003a7b`.

## Verdict path

Agent verdicts: Rust-engine **READY_TO_MERGE** · benchmark **NEEDS_FIXES** (honesty framing) · cross-validation **READY_TO_MERGE**. No BLOCKER. The benchmark NEEDS_FIXES surfaced 2 HIGH honesty defects (both fixed) + MEDIUM disclosures; the Rust review surfaced a MEDIUM FFI hardening + a comment trap (both fixed). All resolved in `57d7618` + `88eac77`; container rebuilt (0 warnings), re-validated. Final: **READY_TO_MERGE**.

## Findings & resolutions

| # | Sev | Finding | Resolution |
|---|---|---|---|
| 1 | HIGH | "recall preserved" was measured against a weak 0.90 bar; the ef=40 headline point (recall 0.927) is BELOW the M32 blob's 0.964 — a recall regression advertised as preserved | verdict now measured against the blob's 0.964: the matched-recall point is `ef_search=100` (recall 0.979 ≥ 0.964), 100 QPS; ef=40 labeled honestly as a recall drop |
| 2 | HIGH | "~194×" compared mismatched recall points (ef=40 recall 0.93 vs blob 0.964), inflating the speedup ~3× | honest headline is **~61× at preserved recall** (ef=100, 100.4 QPS / blob 1.6); "up to ~194× if recall drops to 0.93" is labeled as such |
| 3 | MED | flat-in-N measured on 32-dim synthetic at 50k/200k (not SIFT1M, not re-validated at 1M) — undisclosed | disclosed explicitly in the artifact md |
| 4 | MED | ~17.5 min 1M build (slower than the blob) not in prose | build trade-off prose line added |
| 5 | MED | blob baseline is a 50-query sample vs M35's 1000 — undisclosed denominator mismatch | noted in the artifact + verdict JSON |
| 6 | MED | FFI: a cross-dim query reaches the SIMD scorer's length assertion → bare panic across C-unwind | `scan_hnsw_structured` dim guard → typed `pg_sys::error!` (mirrors pgvector "different vector dimensions"); only fires on a non-empty index (empty carries dim=0 + short-circuits) |
| 7 | LOW | `META_LEN` comment said `= 47` but the constant is 45 (comment trap) | comment corrected to 45 (constant was always 45 — encode/decode agree) |
| 8 | LOW | driver docstring said flat-in-N was "p50 at 250k vs 1M" but impl measures pages-read at 50k/200k | docstring corrected |
| 9 | LOW | CHANGELOG "recall preserved" unqualified | qualified (≥0.964 at ef≥100; ~61×) |
| 10 | INFO | p95 rendered full-precision | formatted `.2f` |
| 11 | MED (traceability) | ADR-3 executed as "legacy blob still reads" not "reject with REINDEX" — a locked plan ADR changed inside `/implement` | honestly documented (CHANGELOG Changed § + commit body); DoD-compliant + arguably better (backward-compat, no data loss for a pre-1.0 index); logged as a caveat, not a defect |
| 12 | LOW (coverage) | plan's single-node graph test named but absent | `test_single_node_graph` added (`88eac77`) |

## Confirmed positives (independently verified by the agents)

- **Byte codec CORRECT (Rust agent, byte-level):** every `encode_meta`/`decode_meta` offset maps exactly (45 bytes, no off-by-one); element offsets + `decode_element` bounds-check before slicing; neighbor per-layer slice (`start=(level-lc)·m`, ground `m0` at `level·m`) agrees between encode + decode and matches pgvector `HNSW_NEIGHBOR_TUPLE_SIZE`; `(0,0)` sentinel handled.
- **Packer overflow-safe (the prime BLOCKER candidate — checked SAFE):** `used + ITEMID + maxalign(size) ≤ USABLE(8168)` is *exactly* `PageAddItemExtended`'s fit condition → a neighbor tuple the packer accepts always fits at write time (no assert-panic). Element chunking places node i at the analytic `(1+i/ipp, 1+i%ipp)`.
- **Level cap CORRECT:** `nbr_size(32, m=16, m0=32)=3268 B` < one page; `HNSW_MAX_LEVEL=32` applied in build.
- **FFI safety CORRECT:** `read_page_item_at` bounds-checks block + offno and copies out before `UnlockReleaseBuffer` (no dangling pointer); `extend_page_with_items`/`reinit_page_with_items` have the correct GenericXLog lifecycle + paired extension lock; no buffer leak.
- **Traverse CORRECT:** mirrors `HnswIndex::search` (greedy upper ef=1 with strict `<` decrease → terminates; ground ef_search; `(blk,off)` visited-dedup); `entry_level<0` guarded; corrupt pointer → typed `Err`, tag bytes prevent element/neighbor confusion; every `decode_*` bounds-checks.
- **VACUUM rewrite CORRECT:** reinit in place + empty leftover pages; new meta `pending_start()` stays consistent with `read_pending` whether the graph shrinks or grows.
- **Benchmark substance honest:** the O(N)→O(ef·M) win is real + reproducible (dated raw run matches the curated artifact); recall math is the ANN-Benchmarks standard; flat-in-N uses the correct pages-read metric with an honest p50-cache-effect note; distinct seeded vectors (no M31b degeneracy); framing is internal ("vs our own blob"), no competitor/superiority claim (public-copy clean).
- **DoD:** all 3 ROADMAP M35 checkboxes satisfied (structured layout + VACUUM/INSERT/DELETE intact; on-demand O(ef·M) + QPS≥50 at 1M at preserved recall; graph integrity + coexistence + reproducible benchmark). `milestone_id: M35` present. Commit on develop, ZERO Co-Authored-By.

## Gate results (image `theo-db:m35`, PG17)

- Build: `cargo pgrx install --release` 0 warnings; `CREATE EXTENSION` smoke OK.
- M35 integration: `test_hnsw_structured.py` 6/6 (true-kNN recall preserved; INSERT fold; DELETE+VACUUM; ef_search GUC honored + default 64; single-node; empty/NULL safe).
- Codec units: 4 `#[pg_test]` (pack address analytic; neighbor slice == in-memory every layer; empty; corrupt-meta → Err).
- Coexistence: M20–M22 + ann + reloption + sbq + latency — 57 green (theodb_ivfflat untouched).
- DoD artifact: `docs/benchmarks/m35-hnsw-structured-scan.{md,json}` — at 1M, matched-recall (ef=100, recall 0.979 ≥ blob 0.964) 100 QPS = **~61× the O(N) blob** (up to ~194× at recall 0.93); pages-read flat-in-N (2742→2962, 1.08× while N×4 = O(ef·M)); hardware + repro + honest trade-offs.
