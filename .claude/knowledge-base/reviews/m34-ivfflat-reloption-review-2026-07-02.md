# Review — m34-ivfflat-reloption

**Date:** 2026-07-02 · **Verdict:** READY_TO_MERGE · **Milestone:** M34
**Method:** 3 parallel specialist agents (Rust engine correctness+safety · benchmark methodology+honesty · cross-validation) over `git diff 9395ad5..HEAD`.

## Verdict path

Agent verdicts: engine NEEDS_FIXES · methodology NEEDS_FIXES · cross-validation NEEDS_FIXES. No BLOCKER; 2 HIGH (one code, one doc-contract) + MEDIUM/LOW. All resolved in commit `61e64db`-equivalent (`fix(m34): address /review findings`); re-built + re-validated. Final: **READY_TO_MERGE**.

## Findings & resolutions

| # | Sev | Finding | Resolution |
|---|---|---|---|
| 1 | HIGH | `main_index_pages` (INSERT path: aminsert→pending_layout) did NOT version-gate before parsing v2 offsets → a v1 index would misparse the header + silently drop the INSERTed row | added the `ver != 2` REINDEX guard BEFORE reading v2 offsets (mirrors `read_ivf_meta`); + saturating header arithmetic |
| 2 | HIGH | released v1→v2 structured-format break (v0.27–v0.29 shipped v1) undocumented in the consumer contract (Rule 6) | CHANGELOG `Changed` entry: BREAKING format bump — REINDEX `theodb_ivfflat` built on v0.27–v0.29 |
| 3 | MED | the v2 headline capability (`lists` > single-page ~665) was untested | `test_lists_beyond_single_page_directory` (lists=800 → multi-page dir + INSERT round-trip) |
| 4 | MED | k-means++ equivalence claim had no in-repo regression proof | committed `benchmarks/micro/kmpp_equiv.rs` (byte-identical proof, k=1..100) |
| 5 | MED | CHANGELOG omitted the ~575 s (17×) build trade-off | surfaced in the Added entry (build is single-thread full-corpus k-means; parity is a future lever) |
| 6 | MED | probes=50 "FASTER" rests on a 5.3% margin (mobile CPU, no run-to-run CI) | softened to ≈parity (5% noise); the DoD win is claimed only at probes=100 (robust −10%) |
| 7 | LOW | probes=10 mislabeled "PARITY" (theodb ~10% slower) | relabeled "~10% slower (recall +0.8 pts)" |
| 8 | LOW | `main_index_pages` non-saturating header sum | `saturating_add` |
| 9 | LOW | thermal caveat dropped vs M32 | restored (mobile i7-1355U throttling noted) |
| 10 | LOW | guc.rs palloc'd name/desc via `.as_pg_cstr()` round-trip | `c"..."` literals (zero-alloc) |
| 11 | INFO | plan named `run_m32_sift1m.py`/`test_scale_benchmark.py`; impl used `run_m34_ivfflat.py`/`test_reloption.py` | equivalent-or-better; honest divergence noted |

## Confirmed positives (independently verified by the agents)

- **reloption FFI sound:** `#[repr(C)]` layout + `offset_of!` correct; `rd_options` null-checked; bounds [1,32768] enforced at DDL; `static mut RELOPT_KIND` safe (per-backend process, single-thread `_PG_init`); GUC `.max(1)` + scan `.clamp(1,nlists)` — no zero-probe/overflow.
- **k-means++ rewrite genuinely byte-identical** (agent hand-verified: min associative, same f64 sum order, RNG consumed only on `sum>0`, degenerate branch identical) — proven standalone across k=1..100.
- **v2 page format self-consistent** write↔read↔`main_index_pages` incl. empty-index path; the `ver!=2` REINDEX rejection is the right call for a pre-1.0 engine (with #1 fixed, ALL structured-read paths now gate).
- **Benchmark honest, no cherry-pick:** the isolation fix is confirmed correct + the artifact is from the FIXED run (pgvector sweep varies monotonically, not flattened); full 4-point frontier shown for both (incl. theodb's probes=1 loss + 17× slower build); DoD scoped honestly to the high-recall point; fair single-thread head-to-head.
- **Clean:** 0 Rust warnings; DRY (`DEFAULT_LISTS` single-source, `SCAN_PROBES` removed); commits on develop, ZERO Co-Authored-By; M34→M35 scope split documented honestly; un-planned enabling fixes (k-means init, format v2, harness isolation) honestly attributed as discovery-during-implement (the reloption was inert without them).

## Gate results (image `theo-db:m34`, PG17)

- Build: `cargo pgrx install --release` 0 warnings; `CREATE EXTENSION` smoke OK.
- Reloption: `test_reloption.py` 5/5 (lists honored; probes honored; default preserved; **lists=800 multi-page dir + INSERT**; lists=0 rejected).
- Coexistence: M20-M22 + ann + recall + sbq + index-AM (v2 format) + latency — green (the thin-margin clustered latency test flaked once under host contention, passed on isolated re-run; de-flaked by the M31b min-of-3 floor).
- DoD artifact: `docs/benchmarks/m34-ivfflat-reloption.{md,json}` — theodb_ivfflat p50 ≤ pgvector at 1M (probes=100, recall 0.999); mean±std ≥3 runs; hardware + isolated measurement + honest verdict + repro command. Unaffected by the review fixes (all touch INSERT-path/docs/tests, not the measured scan/build).
