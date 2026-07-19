---
slug: symqg-inpg-am
created_at: 2026-07-17
goal: ship the theodb_symqg in-PG index AM and prove ≥1.5× QPS vs theodb_hnsw at matched recall on SIFT1M
---

# Plan: theodb_symqg — in-PG SymphonyQG quantized-graph index AM

> **Version 1.1** (edge-cases absorbed: EC-1 build-cancel, EC-2 row-spans-page, EC-3 query-dim guard + SHOULD-TEST/DOCUMENT items) — Productize the E2 off-PG spike (measured under `docs/benchmarks/`: recall parity + 1.8–2.66× faster than exact traversal on SIFT1M, scalar 1-bit sign) into a real PostgreSQL index Access Method, `theodb_symqg`. The AM persists a co-located quantized graph (per vertex: neighbor IDs + 1-bit sign RaBitQ codes + nr/w factors) to index pages, builds it by reusing our own `HnswIndex` for the base adjacency + `encode_sign`, and scans it with the spike-validated beam search reading pages per hop. The make-or-break the spike could NOT answer — the per-hop random-page-read tax that a standalone lib avoids — is settled by an in-PG A/B against our own `theodb_hnsw`. Clean-room from the paper (arXiv:2411.12229); the NTUITIVE-licensed C++ is study-only (D5).

## Goal

> Enable vector-search users to `CREATE INDEX … USING theodb_symqg` so that graph ANN search runs on a persisted co-located quantized graph, measured by the SIFT1M in-PG A/B benchmark returning **theodb_symqg QPS ≥ 1.5× theodb_hnsw at matched recall@10 ≥ 0.95**.

## Context

The vector pillar's warm-QPS ceiling is paradigm-bound for a PG extension (M73/M82/ADR-0035–0036) — but E1 MEASURED that the warm bottleneck is **Stage-1 graph/list traversal** (random memory access + exact-distance cost), not Stage-2 refinement. E2 discovery + spike (blueprint `symphonyqg-graph-quant`, benchmark under `docs/benchmarks/`, commit e2c6e3b) showed SymphonyQG attacks exactly that: fold a quantized distance estimate INTO the traversal (co-located neighbor codes + no re-rank). The spike proved the mechanism own-code off-PG — **recall parity (0.998) + 1.8–2.66× faster + 15–27× fewer exact distances on SIFT1M** — but off-PG (pure in-RAM; no heap/WAL/MVCC on the search path). The blueprint's spike-first gate is now MET; the next gate is the in-PG AM, where each graph hop is a random page read. This plan builds that AM and settles the page-tax question with a measured A/B.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/am/mod.rs` | 360 | `7a5d798` | Registers the AM handlers (`theodb_ivfflat`, `theodb_hnsw` amhandlers) + `IndexAmRoutine` wiring | Existing handlers keep registering; add a NEW handler without touching theirs |
| `theodb_rs/src/am/build.rs` | 1551 | `87a3bd2` | `ambuild`/`ambuild_hnsw` — builds the index (HnswIndex / IVF) + packs to pages | `ambuild_hnsw` path unchanged; new `ambuild_symqg` added beside it |
| `theodb_rs/src/am/scan.rs` | 1130 | `30a6c87` | `amrescan` + `scan_hnsw_structured`/`scan_ivf_structured` — page-reading search | HNSW/IVF scan paths unchanged; new `scan_symqg_structured` dispatched by AM |
| `theodb_rs/src/am/options.rs` | 417 | `87a3bd2` | Reloption parsing per AM (lists, refine, rabitq_bits, …) | Existing reloptions unchanged; add symqg reloptions (degree_bound, ef_construction) |
| `theodb_rs/src/am/hnsw_page.rs` | 3351 | `7a5d798` | Persisted HNSW graph page layout (meta, element, neighbors, pack) — the persistence PATTERN to mirror | Read-only reference; NOT edited (new `symqg_page.rs` mirrors its shape) |
| `theodb_rs/src/am/page/mod.rs` | 972 | `30a6c87` | Shared page-write helpers (`write_item`, `write_chunks`, GenericXLog, pending region) | Reuse verbatim; no signature change |
| `theodb_rs/src/ann/symqg_spike.rs` | 396 | `9b69ca0` | The off-PG spike: `SignCode`, `encode_sign`, `estimate_sign`, `SymqgSpike`, `SearchStat`, `exact_beam_search` | Search/estimator logic reused by the in-PG scan; the pure algorithm stays in `ann/` |
| `theodb_rs/src/am/symqg_page.rs` (NEW) | 0 | — | (file to create) per-vertex page layout: meta + rows `[nbr_ids][sign_codes][factors]` | — |
| `theodb_rs/src/lib.rs` | — | `e2c6e3b` | Module registry + extension bootstrap | Add `mod` for the new page module if not folded into `am/` |
| `benchmarks/e2_symqg_inpg.py` (NEW) | 0 | — | (file to create) in-PG A/B harness vs theodb_hnsw | — |

### Current callers / dependents

- **Symbol:** `theodb_hnsw_amhandler` in `am/mod.rs` — called by PG via the `CREATE ACCESS METHOD … HANDLER` SQL; no Rust callers. The new `theodb_symqg_amhandler` follows the same PG-invoked contract (no internal callers to break).
- **Symbol:** `SymqgSpike::search` / `encode_sign` / `estimate_sign` in `ann/symqg_spike.rs` — callers: the `#[pg_extern] symqg_spike_bench` in `bench_symqg.rs:1` (the off-PG measurement). The in-PG scan will add a second consumer; the pure functions stay `pub(crate)` and unchanged in behavior.
- **Symbol:** `page::write_item` / `write_chunks` / `read_ivf_list_bytes` in `page/mod.rs` — callers: `am/build.rs`, `am/scan.rs`, `am/page/ivf.rs` (many). Reused as-is; no signature change.
- **External (other repos):** no — index AMs are consumed only via SQL DDL; no cross-repo Rust API.

### Domain glossary

- **AM (Access Method)** — a PostgreSQL index type; a Rust `IndexAmRoutine` of callbacks (`ambuild`, `aminsert`, `amrescan`, `amgettuple`, `amvacuumcleanup`) dispatched by PG.
- **Co-located graph** — each vertex's page row stores its own neighbors' quantization codes (replicated), so scoring a vertex's neighbors is a local read, not N scattered reads.
- **1-bit sign code** — `u[d] = sign(P·(x−c)[d]) ∈ {−1,+1}` (the SymphonyQG quantization; our multi-bit codec is degenerate at bits=1).
- **Factors** — per-neighbor `nr = ‖x−c‖`, `w = ⟨u,o'⟩`, used by the RaBitQ estimator to recover `‖q−x‖²` from the code.
- **GenericXLog** — PG's WAL-logging API for custom index pages (crash-safety).
- **degree_bound R** — every vertex has exactly R edges (a multiple of 32 for FastScan alignment).

### Architecture boundaries affected

- `ann/` (pure domain — zero `pg_sys`) → the estimator/search logic lives here (`symqg_spike.rs`), reused by `am/`. Preserved: the DIP seam (domain declares `Fn()` for interrupts; `am/` injects `pgrx::check_for_interrupts!`), per `rules/architecture.md § 2`.
- `am/` (infrastructure — the PG boundary) → the new page layout + AM callbacks. New handler registered at the composition root (`am/mod.rs`), never deep in domain code.

## Prior Art & Related Work

- **Internal blueprint** — `knowledge-base/discoveries/blueprints/symphonyqg-graph-quant-blueprint.md § 2` (data layout, Algorithm-1 query, build) and `§ 5` (the spike-first gate this plan's off-PG predecessor already MET).
- **Internal benchmark** — the E2 off-PG SIFT1M verdict committed under `docs/benchmarks/` (commit e2c6e3b: recall parity 0.998 + 1.8–2.66× faster + 15–27× fewer exact distances; the 1-bit-sign finding).
- **Internal reference (persistence pattern)** — `theodb_rs/src/am/hnsw_page.rs:622` (`pack`), `:417` (`decode_element`), `:532` (`decode_neighbors`) — the HNSW persisted-graph page layout this AM mirrors for a co-located row.
- **Reference project (study-only, D5)** — `knowledge-base/references/SymphonyQG/symqglib/qg/qg.hpp:60` (per-vertex row `RawData + QuantizationCodes + Factors + neighborIDs`) + `:26` (the `triple_x/factor_dq/factor_vq` factor struct). NTUITIVE non-commercial → design studied, never copied.
- **External literature** — SymphonyQG, arXiv:2411.12229 (SIGMOD'25) — the algorithm (Fast JL rotation, NSG-refined degree-R graph, Algorithm-1 beam search). Algorithm/math freely studiable; code license-gated.

## Objective

- [ ] Sub-goal 1 — a persisted co-located page layout (`symqg_page.rs`): meta + per-vertex rows, GenericXLog-logged, round-trips byte-identical (pack→read).
- [ ] Sub-goal 2 — `ambuild_symqg`: build `HnswIndex` base adjacency + `encode_sign` per parent + pack to pages; `CREATE INDEX … USING theodb_symqg` succeeds on SIFT1M.
- [ ] Sub-goal 3 — `scan_symqg_structured`: the spike-validated beam search reading rows per hop, returning correct top-k; recall@10 matches the off-PG spike within 1pp on the same data.
- [ ] Sub-goal 4 — reloptions (`degree_bound`, `ef_construction`) + `amvacuumcleanup` (rebuild) + crash-safety (GenericXLog) present and proven.
- [ ] Sub-goal 5 — in-PG A/B benchmark (`benchmarks/e2_symqg_inpg.py`) vs `theodb_hnsw`: **theodb_symqg QPS ≥ 1.5× at matched recall@10 ≥ 0.95** OR an honest measured negative (the page-tax verdict).

## ADRs

### D1 — Mirror the theodb_hnsw AM, not a fresh design
**Decision:** register `theodb_symqg_amhandler` in `am/mod.rs` and add `ambuild_symqg`/`scan_symqg_structured` beside the HNSW ones, reusing `page/mod.rs` write helpers and the `hnsw_page.rs` layout shape.
**Rationale:** the HNSW AM already solves build/scan/WAL/VACUUM/reloptions for a persisted graph (`rules/architecture.md`, Rule 9 don't-reinvent).
**Alternatives considered:** (a) extend `theodb_hnsw` with a co-located mode — rejected: pollutes a shipped AM with an experimental layout, risks its crash-safety; (b) a fresh AM from scratch — rejected: re-derives solved plumbing.
**Consequences:** fastest correct path; constrains the layout to the existing page-helper vocabulary.

### D2 — Per-vertex co-located row [nbr_ids][sign_codes][factors], degree padded to a multiple of 32
**Decision:** store each vertex's R neighbors' 1-bit sign codes + factors + IDs contiguously (SymphonyQG replication).
**Rationale:** makes neighbor scoring a single local page read (the whole point; blueprint § 2).
**Alternatives considered:** (a) codes on a shared page separate from adjacency (like v5 storage separation) — rejected: reintroduces the scattered read E2 removes; (b) no replication (code stored once per vertex) — rejected: then scoring a parent's neighbors needs N scattered reads.
**Consequences:** index grows (replicated codes, ~1–2× raw-vector overhead) — the measured trade-off vs the speed win.

### D3 — Reuse the pure ann/symqg_spike.rs estimator + search; the AM only feeds it page bytes
**Decision:** `scan_symqg_structured` decodes a vertex row into `SignCode`s and calls the SAME `estimate_sign` + beam logic validated off-PG.
**Rationale:** DIP (`architecture.md § 2`) — domain logic stays pure and already-tested; the AM is the adapter.
**Alternatives considered:** reimplement the search in `am/` — rejected: duplicates validated logic, diverges (DRY).
**Consequences:** the off-PG recall parity transfers by construction; only the page-read cost is new.

### D4 — 1-bit SIGN code, scalar estimator first; the SIMD FastScan kernel is a separate follow-up
**Decision:** ship the scalar sign estimator (already ≥1.5× off-PG).
**Rationale:** the spike proved scalar sign alone meets the gate; the kernel is an additional multiplier, not a prerequisite (blueprint § 5, the E2 off-PG benchmark verdict).
**Alternatives considered:** block on the SIMD kernel — rejected: gates the whole AM on the hardest component; anti-sunk-cost.
**Consequences:** the AM's headroom is larger than measured once the kernel lands.

### D5 — Clean-room from the paper; never copy the NTUITIVE C++
**Decision:** implement from arXiv:2411.12229 + our own code.
**Rationale:** the permissive-license policy (`CLAUDE.md` Rule 9 / PRD §11 gate); a port would contaminate Apache-2.0 and fail the release license gate.
**Alternatives considered:** port the C++ — rejected: non-commercial license, illegal + self-defeating.
**Consequences:** identical algorithm, legally clean.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| **Page-tax may erase the win** — off-PG was pure in-RAM; in-PG each hop is a random page read the standalone lib avoids. The whole gate may come back negative. | High | This is the explicit measured question (Phase 5). Co-location keeps the 32-neighbor scoring on ONE page (mitigates); if still negative, record the honest negative (like M74). | vector |
| **Index size grows** (replicated codes, ~1–2× raw-vector overhead) — opposite of E1's memory win. | Medium | Measure size in the A/B; document the size↔speed trade-off; 1-bit keeps it minimal vs multi-bit. | vector |
| **Build cost** — HNSW build + per-parent encode is O(N·R·D²) with the dense rotate (spike: 814s+720s at 1M). | Medium | Acceptable one-time cost for the spike/AM; Fast-JL O(D log D) is a later lever (out of scope, noted). | vector |
| **Build memory ceiling** (EC-10) — replicated codes are O(N·R) resident (~4.5 GB at 1M/R=32); billion-scale exceeds commodity RAM. | Medium | Document the ceiling; streaming encode + Fast-JL are the follow-up levers (out of scope). | vector |
| **Crash mid-build / torn page** — a new page format must be WAL-safe. | High | GenericXLog for every page write (reuse `page/mod.rs` helpers, the proven `theodb_hnsw` pattern); crash test in Phase 4. | vector |
| **pgrx unsafe/FFI across the C boundary** in the AM callbacks (panic-across-C). | Medium | Mirror the existing `theodb_hnsw` callback signatures (`extern "C-unwind"`, `pg_guard`); no new FFI shapes; council-rust-pgrx review at `/review`. | vector |

## Unresolved Questions

- Q1 — Does the per-hop random page read erase the 1.8–2.66× off-PG win? (The core gate — answered only by Phase 5's in-PG A/B.)
- Q2 — What `degree_bound R` (32/64/128) best trades index size vs recall/QPS in-PG? (Swept in Phase 5.)
- Q3 — RESOLVED (EC-9): mirror `theodb_hnsw` — post-build INSERT → pending region scored EXACT at scan; DELETE → tombstone-at-scan + full rebuild at `amvacuumcleanup`; the co-located graph is IMMUTABLE between VACUUM rebuilds (no incremental code insertion). Documented in T2.1/T4.1.
- Q4 — Should the base graph be our HNSW layer-0 (spike's first cut) or a proper NSG degree-R refinement (paper)? (v1 uses HNSW layer-0 per the spike; NSG refinement is a follow-up if recall/QPS underperforms.)

## Dependency Graph

```
Phase 1 (page layout + codec round-trip) ──▶ Phase 2 (ambuild_symqg) ──▶ Phase 3 (scan_symqg_structured)
                                                     │                          │
                                                     ▼                          ▼
                                              Phase 4 (reloptions + WAL/VACUUM)  │
                                                     └──────────────┬───────────┘
                                                                    ▼
                                                     Phase 5 (in-PG A/B benchmark — the gate)
                                                                    ▼
                                                     Final Phase: Integration Validation
```

Phases 1→2→3 are sequential (layout before build before scan). Phase 4 depends on Phase 2 (needs a build to VACUUM). Phase 5 depends on 3+4.

---

## Phase 1: Persisted co-located page layout + codec round-trip

**Objective:** define + round-trip the `theodb_symqg` page format (meta + per-vertex rows) so pack→read is byte-identical.

### T1.1 — `symqg_page.rs`: meta + per-vertex row encode/decode

#### Objective
A page layout: meta page (magic, version, dim, degree_bound, n, entry_point, rotation codebook, gen_base) + per-vertex rows `[nbr_ids: i64×R][sign_codes: packed bits ×R][factors: (nr,w) f32×2×R]`, written via `page/mod.rs` helpers.

#### Why this step (action + reasoning)
1. **What this step does** — introduce `SymqgMeta`, `pack_symqg(idx, &SymqgSpike) -> Packed`, `decode_symqg_meta`, `decode_symqg_row` in a new `am/symqg_page.rs`.
2. **Why it is necessary now** — the build (Phase 2) and scan (Phase 3) both depend on a stable on-disk format; defining + round-trip-testing it FIRST (per `hnsw_page.rs` which did the same) prevents a format bug from surfacing only at scan time (the E1 lesson: a read/write dispatch gap tanked recall). Cite D2 (layout), Baseline row `hnsw_page.rs` (the mirror).

#### Evidence
`hnsw_page.rs:622` (`pack`) + `:417` (`decode_element`) show the exact pattern for a persisted graph row; `page/mod.rs:` `write_item`/`write_chunks` are the WAL-safe writers reused. Reference layout: `references/SymphonyQG/symqglib/qg/qg.hpp:60`.

#### Files to edit
```
theodb_rs/src/am/symqg_page.rs (NEW) — SymqgMeta, pack_symqg, decode_symqg_meta, decode_symqg_row
theodb_rs/src/am/symqg_page.rs — #[cfg(test)] round-trip test (co-located with the module, per rules/testing.md § 5)
theodb_rs/src/am/mod.rs — `mod symqg_page;` declaration only
```

#### Deep file dependency analysis
- `symqg_page.rs` (NEW) — depends on `page/mod.rs` write helpers (Baseline row) + `ann/symqg_spike.rs::SignCode` (the code to serialize). No downstream yet (Phase 2/3 consume it).
- `am/mod.rs` — add one `mod` line; existing handlers untouched (Invariant).

#### Deep Dives
- Data structures: `SymqgMeta { magic:u32, version:u32, dim:u32, degree_bound:u32, n:u32, entry:u32, gen_base:u32, rotation_codebook_npages:u32 }`; row = `R×i64 ids ‖ ceil(R·D/8) sign-bit bytes ‖ R×(nr:f32, w:f32)`.
- **EC-2 MUST-FIX (row spans pages):** a row can exceed one 8 KB page at high dim × degree (dim=768, R=128 ⇒ ~12 KB sign bytes). Rows are written via the CHUNKED writer + a per-vertex offset directory (the v5/v6 `dir` pattern in `page/ivf.rs`), NOT a single `write_item`; `decode_symqg_row` reads a row across pages via the directory offset. (SIFT dim=128/R=32 ≈ 4 KB fits, but the format must be correct for high-dim, not just SIFT.)
- Invariants: pack→decode is byte-identical (round-trip test); degree padded to multiple of 32 with sentinel ids for empty slots.
- Edge cases: vertex with < R real neighbors (pad with a sentinel id skipped at scan, EC-5); vertex identical to a parent → `nr=0` sign code, `estimate_sign` returns `qc2` (EC-6); empty index (`ambuildempty`, EC-4).
- Negative cases: truncated/corrupt row bytes → `decode_symqg_row` returns a typed `Err`, never a panic/OOB (EC-7, the E1 decoder discipline).

#### Pseudo-code / Signatures
```pseudocode
fn pack_symqg(idx: &HnswIndex, spike: &SymqgSpike, degree_bound: usize) -> Packed
  meta = SymqgMeta{ magic, version=1, dim, degree_bound, n, entry, ... }
  for p in 0..n:
    row = concat(pad_ids(neighbors(p), R), pack_sign_bits(codes(p)), factors(p))
    append row to region
  return Packed{ meta_bytes, rotation_codebook, rows }
# round-trip: decode_symqg_row(pack_symqg(...).row(p)) == (ids, codes, factors) for all p
```

#### Tasks
1. Define `SymqgMeta` + encode/decode.
2. Implement `pack_symqg` reusing `page/mod.rs` writers.
3. Implement `decode_symqg_meta` + `decode_symqg_row`.
4. Sign-bit packing helper (R×D bits → bytes) + unpack.

#### TDD
```
RED:  symqg_page_meta_round_trips() — decode_symqg_meta(encode) == original
RED:  symqg_page_row_round_trips() — decode_symqg_row(pack row) yields identical ids/codes/factors
RED:  symqg_page_pads_partial_degree() — a vertex with < R neighbors round-trips with sentinel-skipped slots
RED:  symqg_page_row_spans_multiple_pages() — EC-2: a dim=768/R=128 row (>8KB) packs+decodes byte-identical across pages
RED:  symqg_encode_sign_zero_residual() — EC-6: x==parent ⇒ nr=0,w=0 and estimate_sign returns exactly qc2 (no div-by-zero)
RED:  symqg_decode_truncated_row_errs() — EC-7: a short byte slice yields a typed Err, not a panic/OOB
GREEN: implement pack/decode
REFACTOR: extract the sign-bit pack/unpack if duplicated
VERIFY: cargo pgrx test pg17 symqg_page  (or cargo test --lib symqg_page after `cargo pgrx install`)
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] pack→decode byte-identical for meta + rows (round-trip tests green)
- [ ] partial-degree vertex handled
- [ ] Pass: size — `symqg_page.rs` ≤ 500 lines (split if larger)
- [ ] Pass: lint — `cargo clippy` clean on the file

#### DoD
- [ ] Round-trip tests passing
- [ ] `cargo pgrx install --release` compiles
- [ ] File-size budget respected

---

## Phase 2: `ambuild_symqg` — build + persist

**Objective:** `CREATE INDEX … USING theodb_symqg` builds the base graph, encodes co-located sign codes, and persists via Phase-1 pack.

### T2.1 — `ambuild_symqg` + handler registration

#### Objective
Register `theodb_symqg_amhandler` (mirror `theodb_hnsw_amhandler`) and implement `ambuild_symqg`: heap-scan → `HnswIndex::build_owned` → `SymqgSpike::build` (sign codes) → `pack_symqg` → write pages.

#### Why this step (action + reasoning)
1. **What this step does** — add the AM handler + build callback beside the HNSW one.
2. **Why it is necessary now** — the scan (Phase 3) needs a persisted index to read; build must exist first. Reusing `HnswIndex::build_owned` (Baseline row) + `SymqgSpike::build` (spike) is Rule 9 (don't reinvent) + D3. Cite D1, D3.

#### Evidence
`am/build.rs:368` (`ambuild_hnsw`) + `am/mod.rs:74` (`theodb_hnsw_amhandler`) are the exact templates; `ann/symqg_spike.rs:100` (`SymqgSpike::build`) is the encoder.

#### Files to edit
```
theodb_rs/src/am/mod.rs — theodb_symqg_amhandler + CREATE ACCESS METHOD SQL (mirror hnsw)
theodb_rs/src/am/build.rs — ambuild_symqg + ambuildempty_symqg
theodb_rs/src/ann/symqg_spike.rs — expose a pack-friendly accessor over sign_codes/rot_vec if needed (pub(crate))
```

#### Deep file dependency analysis
- `am/mod.rs` — add handler; existing handlers untouched (Invariant). Routine callbacks point at `ambuild_symqg` + `scan_symqg_structured` (Phase 3) + `amvacuumcleanup` (Phase 4).
- `am/build.rs` — new `ambuild_symqg` reuses `HnswIndex::build_owned` (already called by `ambuild_hnsw:382`) + `pack_symqg` (Phase 1).
- `ann/symqg_spike.rs` — may add `pub(crate) fn codes_of(p)` / `rot_vec()` accessors (read-only); `SymqgSpike::search` unchanged.

#### Deep Dives
- Invariant: the `TableAmRoutine`/`IndexAmRoutine` is allocated per the pgrx pattern the HNSW handler uses (`am/mod.rs:74`) — no dangling routine (memory note `tableam-rd-tableam-topmemcontext` is TAM-specific; IndexAM uses pgrx's `PgBox` pattern already proven for `theodb_hnsw`).
- **EC-1 MUST-FIX (build cancellation):** the per-parent sign-encode loop (~N·R iterations, 32M at 1M/R=32) MUST call `pgrx::check_for_interrupts!()` every ~4096 vertices — a plain loop ignores `pg_cancel_backend` (the exact E1 k-means bug where only a postmaster kill worked).
- Edge cases: empty relation (`ambuildempty_symqg`, EC-4); non-L2 opclass → `error!` at build (the sign estimator is L2-only, mirror the v8 build guard `build.rs:208`).

#### Tasks
1. Add `theodb_symqg_amhandler` + `CREATE ACCESS METHOD` SQL (mirror `am/mod.rs:64-74`).
2. Implement `ambuild_symqg` (heap-scan → build → encode → pack → write).
3. `ambuildempty_symqg`.
4. L2-only build guard.

#### TDD
```
RED:  symqg_ambuild_creates_scannable_index() — CREATE INDEX on a small table succeeds + pg_relation_size > 0 (pg_test)
RED:  symqg_ambuild_rejects_non_l2() — non-L2 opclass errors at build
RED:  symqg_ambuild_empty_then_scan_returns_empty() — EC-4: build on 0 rows, scan returns 0, no panic
RED:  symqg_ambuild_responds_to_cancel() — EC-1: a cancel signal during a large build is honored within one check_for_interrupts window (not postmaster-kill)
GREEN: implement ambuild_symqg + handler + check_for_interrupts! every ~4096 vertices in the encode loop
REFACTOR: share the heap-scan corpus assembly with ambuild_hnsw if trivially factorable (else leave — DRY vs coupling)
VERIFY: cargo pgrx test pg17 symqg_ambuild
```

#### Concurrency tests

(none — single-threaded). The base-graph build reuses the proven parallel `ann/hnsw_parallel` path (race-freedom covered by its `hnsw_parallel_build_produces_valid_searchable_graph` test); this task adds no new shared-state mutation.

#### Acceptance Criteria
- [ ] `CREATE INDEX … USING theodb_symqg` succeeds on SIFT-shaped data
- [ ] `pg_relation_size` > 0; meta decodes
- [ ] non-L2 opclass errors clearly
- [ ] Pass: size — changed regions keep files ≤ 500 lines where feasible (build.rs is already large; add a focused fn, do not grow unboundedly)

#### DoD
- [ ] Build tests green; `cargo pgrx install` compiles; CHANGELOG updated

---

## Phase 3: `scan_symqg_structured` — page-reading beam search

**Objective:** search reads per-vertex rows and runs the spike-validated beam search; recall@10 matches the off-PG spike within 1pp.

### T3.1 — scan dispatch + page-reading traversal

#### Objective
`amrescan`/`amgettuple` dispatch to `scan_symqg_structured`: decode entry, beam search where each popped vertex's row is read from a page, neighbors estimated via `estimate_sign`, top-k exact returned.

#### Why this step (action + reasoning)
1. **What this step does** — port `SymqgSpike::search` to read page rows instead of in-RAM `Vec`s.
2. **Why it is necessary now** — this is the whole point (search on the persisted graph) and the source of the page-tax the A/B measures. Reusing the validated search shape (D3) means recall transfers; only the row-read is new. Cite D3, Baseline row `scan.rs:208` (`scan_hnsw_structured` — the page-reading search template).

#### Evidence
`scan.rs:208` (`scan_hnsw_structured`) shows page-reading graph traversal in an AM; `ann/symqg_spike.rs` `search` is the validated algorithm; `page/mod.rs` `read_ivf_list_bytes` the page reader.

#### Files to edit
```
theodb_rs/src/am/scan.rs — scan_symqg_structured + dispatch in amrescan (beside scan_hnsw_structured:198)
theodb_rs/src/am/symqg_page.rs — decode_symqg_row used per hop (read a vertex's neighbors+codes+factors)
```

#### Deep file dependency analysis
- `scan.rs` — add `scan_symqg_structured`; the existing `amrescan:108` dispatch grows one arm (like the v8 dispatch `scan.rs:276`). HNSW/IVF arms untouched (Invariant).
- `symqg_page.rs` — `decode_symqg_row` (Phase 1) called per popped vertex.

#### Deep Dives
- Algorithm: exactly `SymqgSpike::search` (validated), but `rot_vec[p]` and `codes[p]` come from `decode_symqg_row(read_page(p))` instead of a `Vec`. Rotate-query-once still applies; `q_r = rot_q − P·x_p` where `P·x_p` is read from the row (or recomputed — decide by measurement).
- Invariant: top-k are the k smallest EXACT among popped (no re-rank), matching the spike.
- Edge cases: pending rows (Phase 4) scored exact; sentinel neighbor slots skipped; entry-point read once.

#### Pseudo-code / Signatures
```pseudocode
fn scan_symqg_structured(rel, query, ef) -> topk
  meta = decode_symqg_meta(read_meta_page(rel))
  if query.len() != meta.dim: error!("query dim != index dim")   # EC-3 MUST-FIX: validate at boundary (Rule 8)
  ef = max(ef, k)                                                 # EC-8: clamp beam >= k
  rot_q = rotate(query)            # once
  beam search (spike logic):
    pop min-estimate p
    row = decode_symqg_row(read_page(rel, p))     # the per-hop page read (the tax)
    exact_p = L2(query, row.raw_or_refetch)       # 1 exact/popped
    for (nb, code, nr, w) in row: est = estimate_sign(...); push
  return k smallest exact
```

#### Tasks
1. `scan_symqg_structured` reading rows per hop.
2. Dispatch arm in `amrescan`.
3. Wire `amgettuple` to stream the top-k (mirror hnsw).

#### TDD
```
RED:  symqg_scan_recall_matches_spike() — on a fixed small corpus, in-PG top-10 == off-PG SymqgSpike::search top-10 (±1pp) (pg_test)
RED:  symqg_scan_returns_k() — LIMIT k returns exactly k ordered by distance
RED:  symqg_scan_query_dim_mismatch_errs() — EC-3: a wrong-dim query yields a typed error, not a panic/OOB
RED:  symqg_scan_ef_below_k_clamps() — EC-8: ef_search=1, LIMIT 10 still returns 10 ordered rows
GREEN: implement scan_symqg_structured + dispatch
REFACTOR: factor the row-decode-to-SignCode adapter
VERIFY: cargo pgrx test pg17 symqg_scan
```

#### Concurrency tests

(none — single-threaded). PG runs one backend per scan; index pages are read-only via the buffer manager — no mutation on the scan path.

#### Acceptance Criteria
- [ ] in-PG recall@10 matches the off-PG spike within 1pp on the same data
- [ ] `ORDER BY e <-> q LIMIT k` returns k rows correctly ordered
- [ ] Pass: lint clean

#### DoD
- [ ] Scan tests green; recall parity with the spike documented

---

## Phase 4: Reloptions + WAL/crash-safety + VACUUM

**Objective:** `degree_bound`/`ef_construction` reloptions, GenericXLog crash-safety, and `amvacuumcleanup` (rebuild) present and proven.

### T4.1 — reloptions + vacuum + crash test

#### Objective
Add `WITH (degree_bound=N, ef_construction=M)` parsing; `amvacuumcleanup` rebuilds dropping dead tuples (mirror `vacuum_rebuild_hnsw_structured`); a crash-during-build test asserts no torn page.

#### Why this step (action + reasoning)
1. **What this step does** — the production-hardening triad (options, VACUUM, WAL) every shipped AM needs.
2. **Why it is necessary now** — an index that is not crash-safe or VACUUM-able is not shippable (the invariants `theodb_hnsw` already holds). Cite D1, Baseline row `build.rs:619` (`vacuum_rebuild_hnsw_structured`).

#### Evidence
`am/options.rs` (reloption parsing per AM); `am/build.rs:619` (`vacuum_rebuild_hnsw_structured`); `page/mod.rs` GenericXLog usage. Memory `#46/#47 durabilidade` — crash-safety is proven via check-crash harnesses (the pattern to reuse).

#### Files to edit
```
theodb_rs/src/am/options.rs — degree_bound + ef_construction reloptions (mirror the ivfflat/hnsw reloption pattern)
theodb_rs/src/am/build.rs — vacuum_rebuild_symqg (mirror vacuum_rebuild_hnsw_structured)
theodb_rs/theodb_rs.control / SQL — opclass for theodb_symqg (vector_l2_ops)
```

#### Deep file dependency analysis
- `options.rs` — add a reloption struct + parse entries (mirror `TheodbIvfflatOptions`); existing options untouched.
- `build.rs` — `vacuum_rebuild_symqg` reuses `HnswIndex::build_owned` over live tuples (like the hnsw vacuum).

#### Deep Dives
- Invariant (crash-safety): every page write goes through GenericXLog (`page/mod.rs` helpers) — no raw `PageAddItem` without WAL. Crash test: kill mid-build, recover, assert meta decodes OR index is cleanly absent (no torn state).
- Edge cases: `degree_bound` not a multiple of 32 → round up + warn; `ef_construction < degree_bound` → clamp.

#### Tasks
1. reloptions parse + defaults (degree_bound=32, ef_construction=200).
2. `vacuum_rebuild_symqg`.
3. opclass SQL (`vector_l2_ops`).
4. crash-during-build harness (reuse the `#46/#47` check-crash pattern).

#### TDD
```
RED:  symqg_reloptions_parse() — WITH (degree_bound=64) is read back as 64 (pg_test)
RED:  symqg_vacuum_drops_dead() — after DELETE + VACUUM, dead tids not returned
RED:  symqg_crash_during_build_no_torn_page() — kill mid-build; recovery leaves a decodable-or-absent index (crash harness)
GREEN: implement reloptions + vacuum + WAL discipline
REFACTOR: none expected
VERIFY: cargo pgrx test pg17 symqg_reloptions symqg_vacuum + the crash harness script
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] reloptions round-trip; VACUUM drops dead tuples; crash test leaves no torn page
- [ ] opclass registered; `CREATE INDEX … (e vector_l2_ops)` works

#### DoD
- [ ] Phase-4 tests green incl. crash harness

---

## Phase 5: in-PG A/B benchmark — the gate

**Objective:** measure `theodb_symqg` vs `theodb_hnsw` on SIFT1M in-PG; report QPS at matched recall + index size, warm AND cold.

### T5.1 — `benchmarks/e2_symqg_inpg.py`

#### Objective
Load SIFT1M, build both `theodb_symqg` and `theodb_hnsw` on the same table, sweep search params, measure recall@10 vs official GT + QPS (best-of-3) + `pg_relation_size` + buffers/query, WARM and COLD (drop-caches).

#### Why this step (action + reasoning)
1. **What this step does** — the measured A/B that answers the page-tax question (Q1).
2. **Why it is necessary now** — the Goal's metric is exactly this benchmark; without it there is no verdict (Rule 5: performance is a claim only with a `docs/benchmarks/` artifact). Cite D4, Goal.

#### Evidence
The E1 harness `benchmarks/e1_rabitq_bench.py` + `e1_cold_perquery.py` are the exact templates (load fvecs, build, sweep, warm+cold). Off-PG target: the E2 spike verdict under `docs/benchmarks/` (commit e2c6e3b).

#### Files to edit
```
benchmarks/e2_symqg_inpg.py (NEW) — the A/B harness (mirror e1_rabitq_bench.py)
docs/benchmarks/e2-symqg-inpg-verdict.md (NEW) — the measured verdict + honest framing
docs/benchmarks/e2-symqg-inpg-verdict.json (NEW) — raw data
```

#### Deep file dependency analysis
- New bench script; no production code. Consumes the two AMs (Phases 2–4).

#### Deep Dives
- Method: same table, two indexes; `SET theodb_symqg.ef_search` / `theodb_hnsw.ef_search` swept; recall@10 vs `sift_groundtruth.ivecs`; QPS best-of-3; warm (shared_buffers large) + cold (256MB + drop_caches per query, the E1 pattern).
- Invariant (honesty, Rule 5): report matched-recall QPS (not cherry-picked points); note warm AND cold; state the off-PG→in-PG delta honestly.

#### Tasks
1. Load SIFT + build both indexes on one table.
2. Sweep + measure recall/QPS/size/buffers, warm.
3. Cold (drop-caches) pass.
4. Write the verdict doc + JSON.

#### TDD
```
RED:  (benchmark harness — validated by producing E2_INPG_RESULT lines with recall in [0,1] and QPS > 0; no unit TDD for a measurement script, per rules/testing.md § 4 "don't test framework/measurement glue")
GREEN: the harness runs end-to-end on SIFT1M and emits the verdict
VERIFY: python3 benchmarks/e2_symqg_inpg.py on the droplet
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] EC-11: N equals the GT base size (1,000,000) — a subset yields a false recall ceiling (the spike's N=200k trap)
- [ ] Verdict doc reports theodb_symqg vs theodb_hnsw QPS at matched recall@10 ≥ 0.95, warm AND cold, with index sizes
- [ ] **GATE:** theodb_symqg QPS ≥ 1.5× theodb_hnsw at matched recall — OR an honest measured negative documented (the page-tax verdict)
- [ ] No performance claim without the benchmark artifact (Rule 5, public-copy.md)

#### DoD
- [ ] Verdict doc + JSON committed; CHANGELOG updated with the measured result

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Persisted co-located page layout | T1.1 | `symqg_page.rs` meta+rows, round-trip tested |
| 2 | Build path persists the graph | T2.1 | `ambuild_symqg` reuses HnswIndex + encode_sign + pack |
| 3 | Scan reads pages + beam search | T3.1 | `scan_symqg_structured` reuses spike search, recall parity |
| 4 | Reloptions + WAL + VACUUM | T4.1 | degree_bound/ef reloptions, GenericXLog, vacuum_rebuild_symqg, crash test |
| 5 | In-PG A/B gate (≥1.5× vs hnsw) | T5.1 | `e2_symqg_inpg.py` + verdict doc; page-tax answered |
| 6 | License compliance (clean-room) | T2.1, T5.1 | own-code from paper; NTUITIVE C++ study-only (D5) |
| 7 | 1-bit sign correctness in-PG | T1.1, T3.1 | round-trip codes + recall-parity-with-spike test |

**Coverage: 7/7 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cargo pgrx test pg17` green
- [ ] Zero type errors / lint — `cargo clippy` clean on changed files
- [ ] File-size budget respected (per `rules/architecture.md`; `build.rs`/`scan.rs` already large — add focused fns, do not balloon)
- [ ] CHANGELOG.md updated under `[Unreleased]`
- [ ] Backward compatibility — existing `theodb_hnsw`/`theodb_ivfflat` AMs unchanged and green
- [ ] Runtime-metric proof — the A/B benchmark emits real recall/QPS numbers on SIFT1M (not just compiles)
- [ ] Plan archived after `/review` READY_TO_MERGE + merge

## Failure scenarios (external I/O = index page reads)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| index pages (PG buffer mgr / disk) | torn page after crash mid-build | crash harness (kill mid-`ambuild_symqg`, recover) — reuse the `#46/#47` check-crash pattern | recovery leaves a decodable meta OR a cleanly-absent index; never a half-written graph read as valid |
| index pages (disk) | cold read (out-of-RAM) per hop | Phase-5 cold pass: shared_buffers=256MB + drop_caches | search still correct; QPS measured (the page-tax number) |
| heap (VACUUM) | dead tuple after DELETE | `symqg_vacuum_drops_dead()` | `amvacuumcleanup` rebuild excludes dead tids |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** the AM works end-to-end on SIFT1M, not just in unit tests.

### Execution
```
cargo pgrx test pg17            # unit + pg_test (page round-trip, build, scan recall, reloptions, vacuum)
cargo clippy --release          # zero warnings on changed files
<crash harness script>          # crash-during-build recovery
python3 benchmarks/e2_symqg_inpg.py   # the in-PG A/B on SIFT1M (warm + cold)
```

### Acceptance Criteria
- [ ] All test suites green (unit + pg_test)
- [ ] Recall@10 parity in-PG vs off-PG spike (±1pp) proven
- [ ] Crash harness: no torn page
- [ ] Benchmark emits the measured A/B verdict (≥1.5× gate met OR honest negative)
- [ ] Zero lint warnings

### If Validation Fails
1. Separate plan-caused failures from pre-existing.
2. Fix all plan-caused failures; re-run.
3. If the ≥1.5× gate is not met, that is a VALID measured outcome — document the honest negative in the verdict doc (the page-tax verdict), do NOT force a green.
