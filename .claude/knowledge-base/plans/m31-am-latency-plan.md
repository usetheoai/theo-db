---
slug: m31-am-latency
milestone_id: M31
created_at: 2026-07-01
goal: Cut the theodb_ivfflat Index Scan latency from O(N)-per-scan to O(probes) via partial-page reads, proven by benchmarks/tests/test_index_am_latency.py passing (Index Scan p50 <= pgvector ivfflat on n>=100k dim=128, recall@10 preserved) against the container.
---

# M31 — Index AM query-latency optimization (partial-page reads)

## Goal

Cut the `theodb_ivfflat` Index Scan from O(N)-per-scan (whole-blob deserialize) to O(probes) via partial-page
reads, measured by a single observable metric: **`benchmarks/tests/test_index_am_latency.py` passing** — the
`theodb_ivfflat` Index Scan p50 latency **≤ pgvector `ivfflat`** on the same corpus (n ≥ 100k, dim 128) with
recall@10 preserved at parity, all green in the Docker image, evidence recorded in
`docs/benchmarks/m31-am-latency.md`.

## Context

M26 (`ROADMAP.md § M26`) shipped the persisted AMs but `amrescan` deserializes the WHOLE index blob per query
(`theodb_rs/src/am/scan.rs` → `page::read_blob` reads all pages → `Persisted::from_bytes` reconstructs every
vector) — O(N), 86 ms vs pgvector's ~1.5 ms on 5k×128 (ADR 0010's scan-limitation section). M31 (P0 CTO GOTO —
`memory: goto-p0-vector-superiority`) is the direct latency fix + the pre-req for M32's 1M+ scale benchmark.

**Design (blueprint `m31-am-latency`):** SOTA (pgvector) reads only the probed lists' pages (`ivfscan.c:124`
`GetScanItems`), never the whole index. A per-backend deserialized cache would hold O(N)/backend → prohibitive at
1M+, rejected. Chosen: restructure `theodb_ivfflat` persistence into a **meta page** (centroids + a per-list
directory `(first_block, count)`) + **list pages** (each entry `[tid i64, vector f32×dim]`); `amrescan` reads the
meta, picks the `probes` nearest centroids, and reads ONLY those lists' pages — I/O ∝ probes·list_size, not N.

## Baseline Context

### Files that will be touched

| File | LoC today | git sha (last touch) | Why it exists / role in M31 |
|---|---|---|---|
| `theodb_rs/src/am/page.rs` | 360 | `e85ca48` | M26 blob persistence. Add the structured meta+list page layout (per-list directory + list-page reader) alongside the blob path. |
| `theodb_rs/src/am/build.rs` | 240 | `e85ca48` | M26 ambuild (blob). Add the IVFFlat structured-write path (write centroids+directory to meta, each list to list pages). |
| `theodb_rs/src/am/scan.rs` | 100 | `e85ca48` | M26 amrescan (whole-blob). Add the IVFFlat partial-read path (meta → probed centroids → probed list pages only). |
| `theodb_rs/src/am/index.rs` | 78 | `e85ca48` | Persisted enum. Add a structured-IVFFlat scan entry that returns candidates for only the probed lists. |
| `theodb_rs/src/ann/ivf.rs` | 300 | `e85ca48` | IvfflatIndex. Expose `centroids()` + `probe_centroids(q, probes)` so the AM can pick lists without a full search. |
| `benchmarks/tests/test_index_am_latency.py` | 0 (NEW) | — | The Goal-metric latency + recall harness vs pgvector ivfflat. |
| `docs/benchmarks/m31-am-latency.md` | 0 (NEW) | — | Reproducible latency evidence. |
| `docs/adr/0010-m26-index-am-scope.md` | 130 | `58798d1` | Update scan-limitation section — the O(N) scan is closed for IVFFlat (HNSW still blob, per M31 ADR-2). |

### Current callers / dependents

- `am/scan.rs amrescan` calls `page::read_blob` + `Persisted::from_bytes` + `Persisted::search_merged`. M31 adds a
  structured path for the IVFFlat variant; HNSW keeps the blob path (M31 ADR-2).
- `am/build.rs ambuild` calls `IvfflatIndex::build` + `page::write_blob`. M31 adds `page::write_ivf_structured`.
- `page.rs` blob functions (`read_blob`/`write_blob`/`append_pending`/`rewrite_blob`) stay for HNSW + pending; the
  IVFFlat main index moves to the structured layout.

### Domain glossary

- **list page:** a page holding the `(tid, vector)` entries of one (or part of one) IVFFlat inverted list.
- **per-list directory:** meta-page table mapping each centroid index → `(first list-page block, entry count)`.
- **probed lists:** the `probes` centroids nearest the query — only their list pages are read at scan time.
- **partial-page read:** reading only the pages a query needs (meta + probed lists), not the whole index.

### Architecture boundaries affected

Per `rules/architecture.md`: `ann/ivf.rs` stays pure (adds `centroids()`/`probe_centroids` — no pg types);
`am/page.rs` owns the structured page layout (pg_sys); `am/scan.rs`/`build.rs` orchestrate. No pg_sys leak into `ann/`.

## Prior Art & Related Work

- Blueprint `m31-am-latency-blueprint.md` (this cycle) — the peer I/O finding + the partial-page design.
- pgvector `src/{ivfflat.h,ivfscan.c,ivfbuild.c}` — the page-structured IVFFlat reference (meta page + list pages +
  probed-only scan).
- M26 (`am/*`) — the persistence/scan/vacuum infra this reuses.

## ADRs

### ADR-1 — Partial-page reads via a per-list page directory (not a per-backend cache)

**Decision:** restructure the `theodb_ivfflat` main index into a meta page (centroids + per-list directory) + list
pages; `amrescan` reads only the probed lists' pages. **Rationale:** matches pgvector's page-structured IVFFlat
(bounded per-query I/O + memory), scales to M32's 1M+, reuses M26's page/buffer/WAL infra. **Rejected
alternatives:** (a) a relation-scoped deserialized cache — holds O(N)/backend, prohibitive at 1M+, would be
re-work at scale; (b) keep the blob + lazily deserialize substrings — still needs a directory + selective reads,
so it collapses into this design with a worse memory profile.

### ADR-2 — IVFFlat first; HNSW partial-page reads deferred

**Decision:** scope M31 to the DEFAULT `theodb_ivfflat`; `theodb_hnsw` stays on the M26 blob path (still correct,
just O(N) per scan) with partial-page HNSW reads deferred to a follow-up. **Rationale:** IVFFlat → pages is
natural (lists = pages); HNSW-on-pages (graph traversal reading node pages on demand) is a materially larger,
higher-risk build (pgvector's hnsw is a large C effort) that deserves its own milestone. **Rejected:** doing both
now — doubles the FFI risk + scope for the P0 latency win, against measurement-first. Documented in ADR 0010.

### ADR-3 — Measurement-first: no latency claim without the harness

**Decision:** the latency win is asserted ONLY by the reproducible harness vs pgvector on n ≥ 100k. **Rationale:**
`public-copy.md` + TheoDB rule 5 — performance is a claim, not an opinion. **Rejected:** shipping the restructure
without the head-to-head number (the whole point of the P0 is the measured win).

## Dependency Graph

```
Phase 1 (page.rs: structured meta+list layout — write + partial read primitives)
   ↓
Phase 2 (build.rs: ambuild writes the structured IVFFlat; ivf.rs centroids/probe_centroids)   ← depends on 1
   ↓
Phase 3 (scan.rs: amrescan reads meta + only probed list pages; pending merge preserved)      ← depends on 1,2
   ↓
Phase 4 (maintenance: aminsert/VACUUM fold work on the structured layout; coexistence)        ← depends on 2,3
   ↓
Phase 5 (latency benchmark vs pgvector n>=100k + recall parity; ADR 0010 update) — Final Validation
```

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | 0.16.1 | Rust | Page/buffer/WAL FFI already used in M26. No new dep. |

### New — to be introduced

(none — reuses M26's `am/page.rs` primitives + `ann/wire.rs` codec; the structured layout is new page bookkeeping,
not a new dependency.)

### Removed

(none.)

## Phase 1 — Structured meta + list page layout

### T1.1 — Meta page (centroids + per-list directory) + list-page write/read primitives
#### Why this step
Partial-page reads (DoD) require the index laid out so a scan can find + read only the probed lists. The meta page
holds the centroids and, per centroid, the `(first_block, count)` of its list; list pages hold the entries.
#### Files to edit
- `theodb_rs/src/am/page.rs` (NEW fns) — `write_ivf_structured(rel, centroids, lists_of_entries)` (meta page:
  magic/dim/metric/nlists + centroid vectors + directory; then each list's `[tid,vector]` entries across chained
  list pages, recording `(first_block, count)`); `read_ivf_meta(rel)` (centroids + directory); `read_ivf_list(rel,
  first_block, count, dim)` (entries of ONE list). WAL-logged via the existing GenericXLog helpers.
#### TDD
- `test_index_am.py::test_structured_ivf_roundtrip` (integration, via a built index): build over a known corpus,
  assert a probed-list read returns exactly that list's `(tid,vector)` entries and the centroids match.
- Rust `#[pg_test]` where pure: directory encode/decode round-trip.
#### Concurrency tests
(none — single-threaded) — page writes use the existing per-buffer exclusive-lock + GenericXLog discipline; the
VACUUM fold vs scan concurrency is already serialized by `am/lock.rs` (M26), reused unchanged in Phase 4.
#### Failure scenarios
- Corrupt directory (block/count out of range) → typed `Err` (bounds-checked reader), never an OOB page read.
- Short list page → typed `Err`, surfaced as a clean AM error.
#### Acceptance criteria
- `read_ivf_list` returns exactly the entries `write_ivf_structured` wrote for that list (`assert_eq` on tids).
- `pg_relation_size(idx) > 0` after a structured build; meta page parses (magic + nlists match).
#### DoD
- `pytest test_index_am.py::test_structured_ivf_roundtrip` green in the image; file ≤ 500 LoC.

## Phase 2 — ambuild writes structured IVFFlat

### T2.1 — Structured build path + `ivf.rs` centroid accessors
#### Why this step
`ambuild` must persist the IVFFlat in the structured layout so the scan can read partially. `ivf.rs` must expose
its centroids + a `probe_centroids(q, probes)` so the AM picks lists WITHOUT a full in-memory search.
#### Files to edit
- `theodb_rs/src/ann/ivf.rs` — `pub(crate) fn centroids(&self) -> &[Vec<f32>]`; `pub(crate) fn probe_centroids(&self,
  q, probes) -> Vec<usize>` (indices of the `probes` nearest centroids, reusing the existing sort); `pub(crate) fn
  list_entries(&self) -> Vec<Vec<(i64, Vec<f32>)>>` (per-list (tid,vector) for the writer).
- `theodb_rs/src/am/build.rs` — `ambuild` (ivfflat) builds `IvfflatIndex` then calls `page::write_ivf_structured`
  (instead of `write_blob`). HNSW `ambuild_hnsw` unchanged (blob).
#### TDD
- `test_index_am.py::test_build_persists_not_rebuild` (updated): structured build persists; `EXPLAIN` still Index
  Scan; recall preserved.
- Rust `#[pg_test]`: `probe_centroids` returns the correct nearest-centroid indices for a known corpus.
#### Concurrency tests
(none — single-threaded) — build runs under CREATE INDEX's lock.
#### Acceptance criteria
- Structured index builds over n ≥ 100k without error; `probe_centroids(q, 10)` returns 10 indices ordered by
  centroid distance (`assert_eq` vs a brute-force centroid sort).
#### DoD
- `pytest test_index_am.py::test_build_persists_not_rebuild` green; ≤ 500 LoC per file.

## Phase 3 — amrescan partial-page read

### T3.1 — Read meta + only probed list pages; merge pending; return TIDs by distance
#### Why this step
The latency win (DoD): `amrescan` computes the probed centroids from the meta page, reads ONLY those lists' pages,
scores their entries + the pending region, and returns TIDs by distance — O(probes·list_size), not O(N).
#### Files to edit
- `theodb_rs/src/am/scan.rs` — for the IVFFlat AM: `read_ivf_meta` → `probe_centroids(query, SCAN_PROBES)` →
  `read_ivf_list` per probed centroid → score with the metric → merge pending (`read_pending`, unchanged) → sort.
  HNSW keeps the blob path.
- `theodb_rs/src/am/index.rs` — dispatch: IVFFlat uses the structured reader; HNSW the blob reader.
#### TDD
- `test_index_am.py::test_index_scan_returns_correct_neighbors` (kept): recall@5 ≥ 4/5 via the structured path.
- `test_index_am_latency.py::test_scan_reads_bounded_pages` (NEW): on n ≥ 100k, the Index Scan buffer reads
  (`EXPLAIN (ANALYZE, BUFFERS)`) are ≪ the total index pages (proves partial, not whole-blob, read).
#### Concurrency tests
(none — single-threaded) — scans take the `am/lock.rs` SHARE lock (M26) so a concurrent VACUUM fold
cannot rewrite mid-read; correctness of concurrent scan/insert rides on that lock + buffer locks (reused).
#### Failure scenarios
- NULL query vector → `SK_ISNULL` guard (M26) → empty scan (no NULL deref).
- Corrupt meta/list page → typed `Err` → clean AM error.
#### Acceptance criteria
- recall@10 preserved (≥ parity vs the M26 blob path on the same corpus); `EXPLAIN (ANALYZE, BUFFERS)` shows the
  Index Scan reads far fewer buffers than a whole-blob scan would (bounded by probes).
#### DoD
- `pytest test_index_am.py::test_index_scan_returns_correct_neighbors` + `test_scan_reads_bounded_pages` green.

## Phase 4 — Maintenance + coexistence on the structured layout

### T4.1 — aminsert pending + VACUUM fold rebuild the structured index; M20-M26 intact
#### Why this step
DoD "sem regressão": the M26 incremental maintenance (aminsert → pending; VACUUM → fold) must keep working with
the new structured main index. INSERT still appends to pending (unchanged); VACUUM rebuilds the structured layout.
#### Files to edit
- `theodb_rs/src/am/build.rs` — `vacuum_rebuild` for the structured IVFFlat: enumerate structured entries +
  pending, drop dead, rebuild `IvfflatIndex`, `write_ivf_structured` (replacing `rewrite_blob` for IVFFlat).
- `theodb_rs/src/am/page.rs` — structured rewrite that reinits meta + list pages (reuse the in-place reinit).
#### TDD
- `test_index_am.py::test_incremental_insert_delete_vacuum` (kept): INSERT found via pending, DELETE filtered,
  VACUUM folds into the structured layout, index still correct.
#### Concurrency tests
(none — single-threaded) — the `am/lock.rs` advisory lock (SHARE scan/insert, EXCLUSIVE vacuum,
M26) serializes the structured rewrite against readers/writers; reused unchanged.
#### Failure scenarios
- VACUUM mid-crash → each structured page write is WAL-logged (GenericXLog), replayed independently.
#### Acceptance criteria
- `test_incremental_insert_delete_vacuum` green on the structured layout; M20-M22 suites (61) + M26 tests still green.
#### DoD
- `pytest test_index_am.py` (all) + coexistence suites green in the image.

## Phase 5 — Final Integration Validation + latency benchmark

### T5.1 — Latency + recall harness vs pgvector ivfflat (n ≥ 100k) + ADR update
#### Why this step
Measurement-first (ADR-3): the P0 latency win is a claim only with the reproducible head-to-head vs pgvector.
#### Files to edit
- `benchmarks/tests/test_index_am_latency.py` (NEW) — seed n ≥ 100k dim 128; build `theodb_ivfflat` + pgvector
  `ivfflat`; `EXPLAIN (ANALYZE)` p50 over ≥ 30 queries each; assert `theodb p50 ≤ pgvector p50 * 1.5` (parity
  band) AND recall@10 ≥ parity; record both.
- `docs/benchmarks/m31-am-latency.md` (+ `.json`) — before (M26 blob 86 ms) / after (structured) / pgvector, mean±std.
- `docs/adr/0010-m26-index-am-scope.md` — scan-limitation section updated: O(N) scan closed for IVFFlat (number), HNSW still blob.
#### Concurrency tests
(none — single-threaded) — benchmark is sequential queries.
#### Acceptance criteria
- `theodb_ivfflat` Index Scan p50 ≤ pgvector ivfflat p50 (within the parity band) on n ≥ 100k dim 128; recall@10 ≥
  parity; numbers recorded with methodology; clippy `-D warnings` clean; CHANGELOG updated.
#### DoD
- `pytest test_index_am_latency.py` green in the image; `docs/benchmarks/m31-am-latency.{md,json}` written.

## Coverage Matrix

| # | M31 DoD item | Task(s) |
|---|---|---|
| 1 | Scan reads only necessary pages (meta + probed lists), not the whole blob | T1.1 + T3.1 |
| 2 | Benchmark: Index Scan p50 ≤ pgvector on n ≥ 100k dim ≥ 128, recall@k preserved | T3.1 (recall) + T5.1 (latency) |
| 3 | No regression: `test_index_am.py` + M20–M22 coexistence green; maintenance intact | T4.1 |
| 4 | ADR 0010's scan-limitation sections updated (O(N) closed for IVFFlat or re-characterized with a number) | T5.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Structured page layout FFI (directory + chained list pages) has bugs | HIGH | Reuse M26's proven buffer/GenericXLog primitives; per-list round-trip test (T1.1) before the scan path; incremental Docker validation | paulohenriquevn |
| Partial read still not ≤ pgvector (const factors: per-page overhead, Rust vs C) | MEDIUM | Measurement-first — if the parity band is missed, ADR-honestly report the residual gap + next lever (e.g. quantized list entries); do NOT fake the number | paulohenriquevn |
| HNSW left on the slow blob path | MEDIUM | Documented (ADR-2 + ADR 0010); IVFFlat is the DEFAULT + the M32 scale target; HNSW partial-page = follow-up milestone | paulohenriquevn |
| n ≥ 100k build/bench time in CI | MEDIUM | Bench is a marked integration test run against the container, not unit CI; dataset generated deterministically | paulohenriquevn |
| Maintenance (pending/VACUUM) regression on the new layout | HIGH | T4.1 re-runs the M26 maintenance tests against the structured layout before merge | paulohenriquevn |

## Failure scenarios

- Corrupt meta directory / short list page → typed `Err` (bounds-checked readers) → clean AM error, never OOB/UB.
- NULL query vector → `SK_ISNULL` guard (M26) → empty scan.
- Concurrent VACUUM fold during a scan → `am/lock.rs` advisory lock (M26) serializes; no torn read.
- Empty index / empty list → 0 results, no crash (bounds checks).

## Unresolved Questions

- Will the Rust structured Index Scan actually beat pgvector's C ivfscan at n ≥ 100k, or only reach a parity band?
  — **Resolved empirically by T5.1**; if a residual gap remains it is reported honestly in the ADR with the next
  optimization lever (quantized list entries / SIMD distance), not hidden.
- Optimal `SCAN_PROBES` default for the parity band at n ≥ 100k — tuned in T5.1 against the recall/latency curve.

## Global DoD

- [ ] `benchmarks/tests/test_index_am_latency.py` green in the Docker image (p50 ≤ pgvector band + recall parity).
- [ ] `test_index_am.py` (all) green — structured build/scan/maintenance, no regression.
- [ ] M20–M22 coexistence suites green.
- [ ] `docs/benchmarks/m31-am-latency.{md,json}` records before/after/pgvector with methodology.
- [ ] ADR 0010's scan-limitation sections updated; `cargo pgrx` builds; clippy `-D warnings` clean; every changed file ≤ 500 LoC.
- [ ] CHANGELOG `[Unreleased]` updated; no `Co-Authored-By`.
