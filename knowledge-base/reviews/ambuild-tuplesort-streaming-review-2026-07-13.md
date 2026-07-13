# Review — M96 tuplesort-streaming ambuild

**Date:** 2026-07-13 · **Slug:** ambuild-tuplesort-streaming · **Milestone:** M96 · **Verdict:** `READY_TO_MERGE` (fixes applied)

council-rust-pgrx audited the FFI-heavy streaming build (the milestone's risk surface). First pass `NEEDS_FIXES`
(1 HIGH); fixed and re-verified (277 tests GREEN).

## Findings → dispositions

| # | Sev | Finding | Disposition |
|---|---|---|---|
| 1 | HIGH | `sample_callback` / `assign_callback` are invoked by function pointer from inside C (`table_index_build_scan`) but lacked `#[pg_guard]` — a PG `ERROR` longjmp (detoast of a corrupt datum) or a Rust panic (`.unwrap()`) would cross the C boundary without setjmp translation | **FIXED** — `#[pg_guard]` on both (matching the in-RAM `build_callback`) |
| 2 | LOW | `assign_callback` had no dim-mismatch guard (the in-RAM `build_callback` does) — a wrong-dim row would silently produce a zip-truncated distance | **FIXED** — skip a row whose dim ≠ the centroids' |
| 3 | LOW | `next_list_member().expect(...)` in the streaming writer — a bare panic on a histogram/stream shortfall | **FIXED** — typed `pg_sys::error!` (fail-loud, Rule 8) |
| 4 | INFO | drop-after-`tuplesort_end` ordering relies on `copy=false ⇒ !SHOULDFREE` | ACCEPTED (correct today; the `copy=false` invariant is commented at the call site) |

## Council verified SOUND (against PG source, not comments)

- **The per-row bytea `pfree` is correct + safe** — `tuplesort_puttupleslot` → `ExecCopySlotMinimalTuple` deep-copies (flattens) the bytea into the sorter's context BEFORE returning, so freeing our palloc'd varlena after is not a use-after-free; `Vec<u8>::into_datum` allocates a plain 4-byte-header varlena (never toasted/inline-shared) so `cast_mut_ptr()` targets the right base; no double-free (a virtual slot frees nothing).
- **Slot lifecycle sound** — the EMPTY-flag discipline satisfies `ExecStoreVirtualTuple`'s `Assert(TTS_EMPTY)`; the `copy=false` minimal get-slot is decoded out before the next `gettupleslot`; the drops after `tuplesort_end` are safe because `!SHOULDFREE`.
- **WAL/buffer lifecycle is the proven `GenericXLog` pattern** — no buffer/relcache leak; `pg_sys::error!` longjmps don't durably leak the sorter (its maincontext is a child of the build context, reclaimed by xact/portal cleanup).
- **Directory math byte-identical to the in-RAM v5 writer** (`code_len = cnt*8 + nblocks*pairs*32`) — same magic 5, same blob order → no REINDEX. Histogram vs stream count agree by construction.
- **Dispatch cannot take a wrong layout path** (exact flag gate) and cannot do worse than pre-M96 on a mis-estimate (falls back to in-RAM = same OOM). `index_vector_dim` guards `natts < 1`.

## Gates

- **277 tests GREEN, 0 failed** (droplet, re-run after fixes) — incl. 4 M96 tests: the tuplesort FFI roundtrip + a 50k-row external spill; the streaming build's recall in the ANN band vs an exact seqscan; the streamed index's scan stable across re-runs (durable pages).
- **Memory-bound measurement (T3.1, `docs/benchmarks/m96-streaming-build.md`):** the streaming peak is FLAT — 1M peak **0.65 GB** (1.264× base), 3M peak **0.62 GB** (0.404× base): the peak did not grow while N tripled → the `O(maintenance_work_mem + sample)` bound, MEASURED. A per-row bytea leak (peak 1.84→0.65 GB at 1M) was found BY the measurement and fixed. vs the M88 in-RAM 4.21×-base baseline (OOM at 30M). 30M/100M peaks honestly PROJECTED from the flat curve (never fabricated — Rule 5); the single-threaded assignment wall-clock (parallel-assign is the deferred follow-up) makes a direct 100M build impractical here.
- plan-confidence SHIPPABLE 95.6. No page-format change (no REINDEX); ≤mwm in-RAM fast-path byte-identical. No commits to main; no Co-Authored-By; CHANGELOG updated.

## Honest scope (shipped, documented)

- v5 plain-f32 path only; SQ8/v6, label/v7, SOAR keep the in-RAM build (exact flag dispatch — never a silent wrong path). Streaming v6/v7 + parallel assignment are documented follow-ups.
- Streaming is recall-EQUAL (bounded-sample training → different centroids), not bit-identical to the in-RAM build; the fast-path stays byte-identical.

## Verdict

`READY_TO_MERGE`. The corpus is never materialized — the O(mwm) memory bound is MEASURED flat, removing the M88/M89
1×-corpus wall that OOM'd at 30M. The FFI is verified memory-safe (the HIGH `#[pg_guard]` gap fixed). Proceed to `/release`.
