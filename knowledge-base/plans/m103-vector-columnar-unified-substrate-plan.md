---
slug: m103-vector-columnar-unified-substrate
milestone_id: M103
created_at: 2026-07-16
goal: Co-reside the IVF vector index (partition-id + AQ code + raw vector) as Arrow columns in the theodb_columnar substrate so a scalar-prefiltered top-k vector search + analytical projection compose in one column-pruned scan, proven byte-identical in recall to the exact filtered search (shared Scored tie-break + l2_dist_from_bytes kernel) and measured for column-pruning cost — never recall, never QPS-vs-ScaNN.
---

# M103 — vector + columnar in one substrate (Lance-inspired co-residence)

## Goal

Store the IVF vector index (partition-id + AQ code + raw f32 vector) as Arrow columns co-resident with the scalar
columns in the `theodb_columnar` substrate, so that `WHERE <scalar> ORDER BY <vector> LIMIT k` + an analytical
projection compose in ONE column-pruned columnar scan (scalar prefilter → RowAddrMask → IVF probe on masked rows →
exact rerank → project analytical columns by row-id). Metric: **the M103 byte-identity GATE pg_test GREEN on pg17
(the columnar filtered top-k is byte-identical — same `(tid, dist)` sequence — to the exact filtered search, sharing
the M90-M95 `Scored` tie-break + `l2_dist_from_bytes` kernel) + a measured `docs/benchmarks/m103-vector-columnar.
{md,json}` column-pruning cost artifact.**

## Context

M99 shipped the columnar TAM; M100 the vectorized executor; M101 the HTAP cache; M102 the AI plan-ops. M103 is the
summit (rung M-ε): the vector index and the scalar/analytical columns live in ONE columnar substrate — the Lance
insight (index as columns) with own code (Apache-2.0 layout study only, D1). The **RowAddrMask already exists**
(M92 `Membership`, `am/customscan.rs`); the **exact rerank kernel + tie-break already exist** (`vec::l2_dist_from_bytes`,
`am/scan.rs::Scored`); the **IVF partitioning + AQ** already exist (`ann/ivf.rs`, `am/aq.rs`). M103's novelty is
**co-residence + the byte-identity proof**, not a from-scratch index. Honest ceiling (GATE): recall is EQUAL by
construction (that is the whole point); the benchmark measures cost/scale (column pruning, out-of-RAM projection) —
**never recall, never the ScaNN QPS gap** (the M73/M74 paradigm ceiling is untouched by co-residence).

## Baseline Context

### Files that will be touched

| File | LoC today | Role | Change |
|---|---|---|---|
| `theodb_rs/src/vindex.rs` | (NEW) | the M103 co-resident vector-index-as-columns + filtered top-k + the byte-identity oracle | NEW — the milestone core |
| `theodb_rs/src/am/df_executor.rs` | ~360 | Arrow bridge (`build_arrow` type map) | ADD `bytea` (OID 17) → `DataType::Binary` (~12 lines) so a raw-vector/AQ-code column decodes to Arrow |
| `theodb_rs/src/lib.rs` | ~120 | module registration | register `mod vindex` |
| `theodb_rs/src/api.rs` | ~700 | SQL surface | ADD the `theodb.vindex_*` wrappers |
| `docs/adr/00NN-m103-vector-columnar-coresidence.md` | (NEW) | the co-residence + gate ADR | NEW |
| `docs/benchmarks/m103-vector-columnar.{md,json}` | (NEW) | column-pruning cost | NEW |

### Current callers / prior art in this repo (reuse, not greenfield)

- `am/scan.rs:28-31` — `Scored { d: f64, tid: i64 }` with `cmp = d.total_cmp(o.d).then(tid.cmp(o.tid))` — the
  LOAD-BEARING tie-break the GATE must reproduce byte-for-byte.
- `vec::l2_dist_from_bytes(query, raw) -> f64` (`vec.rs:263`) + `ip_/cosine_dist_from_bytes` — the exact rerank kernel
  on raw f32 bytes; using the SAME kernel is what makes byte-identity hold.
- `am/customscan.rs:38` — `Membership { exact, lossy }` — the RowAddrMask (scalar prefilter → admission set).
- `ann/ivf.rs` — `kmeanspp` (`:140`), `assign_all_parallel` (`:74`), `centroids` (`:265`) — the IVF partitioning.
- `am/aq.rs` — `AqQuantizer::train` (`:44`), `encode` (`:88`) — the AQ code (stored as a column for DoD 1).
- `am/columnar.rs` — `decode_columns(rel, projection)` (`:662`) — column-pruned decode; the co-residence read path.
- `am/df_executor.rs::build_arrow` (`:44`) — the Arrow bridge; the ONE new-code touch (bytea → Binary).

### Glossary

- **Co-residence** — the vector index (part_id, aq_code, raw vec) and the scalar/analytical columns in ONE columnar table.
- **RowAddrMask** — the admission bitmap from the scalar prefilter (M92 `Membership`); rows the vector search may consider.
- **Byte-identity GATE** — the columnar filtered top-k `(tid, dist)` sequence is IDENTICAL to the exact filtered search.
- **Column pruning** — the filtered-knn scan decodes only (part_id, label, vec, projected analytical) columns, skipping the rest.

### Architecture boundaries

Per `rules/architecture.md`: `vindex.rs` is application logic reusing the domain kernels (`vec`, `ann::ivf`, `am::aq`)
and the columnar infrastructure (`am::columnar` decode). No panic across C (Rule 8). Little-endian raw-vector bytes
(the `vec` kernels' contract).

## Prior Art & Related Work

- **Pillar blueprint (SHIPPABLE 98.8):** `knowledge-base/discoveries/blueprints/single-planner-columnar-ai-blueprint.md`
  Q4 (rung M-ε) — Lance's "index as columns" + the filtered-ANN-as-RowAddrMask composition.
- **Apache-2.0 study:** `lance` (columnar file format with co-resident vector index — layout study only, own code).
- **TheoDB own prior art:** M60-M89 IVF/AQ (`ann/ivf.rs`, `am/aq.rs`, `am/page.rs`), M90-M95 filtered-ANN
  (`am/scan.rs`, `am/customscan.rs`), M99-M101 columnar substrate, ADRs 0035/0037/0042.

## ADRs

### D1 — Reuse the exact `Scored` tie-break + `l2_dist_from_bytes` kernel; byte-identity holds by construction

**Decision:** the columnar filtered top-k reranks with `vec::l2_dist_from_bytes` and orders with the exact
`Scored`-equivalent comparator (`d.total_cmp(o.d).then(tid.cmp(o.tid))`); the byte-identity oracle uses the SAME
kernel + comparator, so the GATE is airtight.
**Alternatives:** an Arrow-native f32 dot product (different summation order → last-ULP drift → different tie-break) —
REJECTED (fails the gate; council-vector-ann trap #2). **Rationale:** the GATE is byte-identity; only the identical
kernel + tie-break guarantees it (`scan.rs:28`, `vec.rs:263`).

### D2 — Full-probe byte-identity vs exact filtered brute-force is the recall-equivalence proof

**Decision:** the GATE asserts that at full probe (probes = nlist) the columnar filtered top-k equals the EXACT
filtered brute-force top-k (all label-matching rows, exact rerank) — byte-identical. This proves the co-resident
layout loses NO recall (recall = 1.0, identical to the canonical exact search), and it shares the M90-M95 comparator.
**Alternatives:** compare against a live `theodb_ivfflat` index build inside the test — REJECTED for slice-1 (large
harness; the exact-brute-force oracle is a STRONGER correctness statement — it proves the true top-k, not just
agreement with another approximate path). **Rationale:** council-vector-ann; recall preserved by construction.

### D3 — Vector index stored AS columnar columns (part_id int4, aq_code bytea, vec bytea, label int4, tid int8)

**Decision:** the IVF partition-id, the AQ code, and the raw f32 vector are stored as Arrow columns co-resident with
the scalar `label` and the analytical columns in a `theodb_columnar` table; the filtered-knn scan reads them via the
column-pruned `decode_columns`.
**Alternatives:** keep the vector index in the index-AM pages (status quo, two separate AMs) — REJECTED (that is the
non-co-resident world M103 exists to unify). **Rationale:** the Lance insight; DoD (1).

### D4 — Honest ceiling: cost/scale/composability only; recall equal-by-construction, no QPS-vs-ScaNN

**Decision:** the benchmark reports column-pruning (bytes decoded reading only the needed columns vs the full row) +
the honest out-of-RAM-at-scale projection; recall is stated as equal-by-construction (the GATE), never as a win, and
NO QPS-vs-ScaNN claim is made.
**Alternatives:** any "faster ANN" / recall / QPS framing — REJECTED (Rule 5, public-copy.md, M73/M74 ceiling).
**Rationale:** the paradigm gap is untouched by co-residence; the win is composability + column-pruned cost.

## Dependency Graph

```
Phase A (co-resident layout: build the vector-index-as-columns + bytea Arrow bridge) ── gates ──▶ Phase B
Phase B (filtered top-k from the columnar columns + the byte-identity GATE vs exact filtered brute-force) ── gates ──▶ Phase C
Phase C (composed WHERE+ORDER BY+agg in one column-pruned scan + the column-pruning cost benchmark)
```

## Phase A — co-resident vector-index-as-columns

### Task A1 — build the co-resident layout + decode a bytea (raw vector / AQ code) column to Arrow

#### Why this step
DoD (1): the vector index must live AS Arrow columns. Build (part_id via IVF kmeans, aq_code via AQ, raw vec) and
store alongside the scalar `label` + `tid`; teach the Arrow bridge to decode a bytea column (the one new-code touch,
council-vector-ann).

#### Files to edit
- `theodb_rs/src/vindex.rs` (NEW) — `struct ColumnarVIndex { tid, part_id, label, vec_raw: Vec<Vec<u8>>, centroids }`
  + `build(vectors, labels, tids, nlist, seed) -> ColumnarVIndex` (reuse `ann::ivf` kmeanspp/assign) + a persist path
  that writes the columns into a `theodb_columnar` table via SPI.
- `theodb_rs/src/am/df_executor.rs` — add `17 => (DataType::Binary, BinaryArray::from_iter(...))` to `build_arrow`.

#### TDD
- RED: `test_bytea_column_decodes_to_arrow_binary` — a `theodb_columnar` table with a bytea column round-trips through
  `decode_columns` → `build_arrow` → a `BinaryArray` with the exact bytes. Fails before the OID-17 arm.
- GREEN: the OID-17 arm; the build/persist.
- REFACTOR: share the little-endian raw-vector encoding with `vec`.

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded) — build + decode are single-backend.

#### Failure scenarios
`## Failure scenarios` — a vector of the wrong dimension → typed error (not a panic); a NULL vec cell → skipped from the index.

#### Acceptance criteria
- A bytea column decodes to an Arrow `BinaryArray` byte-identical; the co-resident layout persists + reads back.

#### DoD
- `cargo pgrx test pg17 m103_bytea_arrow` GREEN on the droplet.

## Phase B — filtered top-k + the byte-identity GATE

### Task B1 — filtered top-k from the columnar columns, byte-identical to exact filtered brute-force

#### Why this step
The DoD (3) GATE + DoD (2) core: scalar prefilter → RowAddrMask → IVF probe on masked rows → exact rerank → top-k,
proven byte-identical to the exact filtered search (shared kernel + tie-break).

#### Files to edit
- `theodb_rs/src/vindex.rs` — `knn_filtered(&self, query, k, probes, label) -> Vec<(i64,f64)>` (mask by label →
  assign query to `probes` nearest centroids → candidates = masked rows in probed partitions → rerank
  `l2_dist_from_bytes` → top-k via the exact `Scored`-equivalent comparator) + `exact_filtered_bruteforce(&self,
  query, k, label) -> Vec<(i64,f64)>` (the oracle: all masked rows, exact rerank, same comparator).

#### TDD
- RED: `test_columnar_filtered_topk_byte_identical_to_exact` — over a fixed seeded dataset, `knn_filtered(full
  probe)` returns the IDENTICAL `(tid, dist)` sequence as `exact_filtered_bruteforce` (byte-for-byte, including
  distance-tie order via the tid tie-break). Fails before the reranker uses the exact kernel + comparator.
- GREEN: the reranker with `l2_dist_from_bytes` + the exact comparator; the mask; the probe restriction.
- REFACTOR: extract the top-k selection to share with the oracle.

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded).

#### Failure scenarios
`## Failure scenarios` — an empty mask (no label-matching row) → empty top-k (not an error); probes > nlist → clamp.

#### Acceptance criteria
- `knn_filtered(full probe)` is byte-identical to `exact_filtered_bruteforce`; the mask returns only label-matching
  rows; at reduced probes the recall vs exact matches the IVF partitioning (documented, not a claim).

#### DoD
- `cargo pgrx test pg17 m103_byte_identity` GREEN.

## Phase C — composed scan + column-pruning benchmark

### Task C1 — `WHERE scalar + ORDER BY vec LIMIT k` + analytical projection in one column-pruned scan + the cost benchmark

#### Why this step
DoD (2) composition + DoD (4) measured cost: one function scan does prefilter + probe + rerank + analytical
projection, reading ONLY the needed columns (column pruning); the benchmark measures the pruning cost, honestly.

#### Files to edit
- `theodb_rs/src/vindex.rs` + `api.rs` — `theodb.vindex_knn_filtered(idx regclass, query float4[], k int, probes int,
  label int) RETURNS TABLE(tid bigint, dist float8)` reading via the column-pruned `decode_columns`; a variant that
  also projects an analytical column by row-id (the composition).
- `docs/benchmarks/m103-vector-columnar.{md,json}` + a harness — bytes decoded reading (part_id, label, vec,
  projected) vs the full row; the honest out-of-RAM-at-scale projection.
- `docs/adr/00NN-m103-vector-columnar-coresidence.md`.

#### TDD
- RED: the harness runs; a result-equivalence cross-check (the composed scan's top-k == the GATE oracle) gates it.
- GREEN: the SQL surface + the pruning measurement.
- REFACTOR: reproducibility (fixed seed).

#### Failure scenarios
`## Failure scenarios` — a non-columnar `idx` argument → typed error; a dimension mismatch query → typed error.

#### Acceptance criteria
- The composed scan returns the filtered top-k + the projected analytical column in one scan; the benchmark shows the
  column-pruning ratio (bytes decoded pruned vs full); honest ceiling stated (recall equal-by-construction, no QPS).

#### DoD
- The harness emits `docs/benchmarks/m103-vector-columnar.{md,json}`; ADR written.

## Coverage Matrix

| Requirement (ROADMAP M103 DoD) | Task(s) |
|---|---|
| (1) vector index as Arrow columns in the columnar substrate | A1 |
| (2) `WHERE scalar + ORDER BY vec` + aggregation in one vectorized plan | B1 (filtered top-k) + C1 (composed scan + projection) |
| (3) result-equivalence of recall vs the exact filtered search (byte-identical) — THE GATE | B1 |
| (4) benchmark cost/scale (column pruning / out-of-RAM), honest | C1 |
| (5) sign-off council-vector-ann + council-index-storage + council-benchmark | Review phase |
| honest boundary (cost/scale/composability, NOT recall, NOT QPS-vs-ScaNN) | D4 (ADR) enforced in the benchmark |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Tie-break / float-determinism drift fails the byte-identity gate | HIGH | reuse the EXACT `Scored` comparator + `l2_dist_from_bytes` (D1); the oracle uses the same (council-vector-ann trap #1/#2) | impl |
| Over-claiming recall / QPS | HIGH | D4 + benchmark states recall equal-by-construction, no QPS-vs-ScaNN (M73/M74 ceiling) | impl |
| Dual-store consistency (row truth vs columnar replica) | MEDIUM | slice-1 is a static materialized index (post-build snapshot); incremental maintenance is a documented follow-up | plan |
| Column-pruning benchmark meaningless if in-RAM small | MEDIUM | measure the deterministic bytes-decoded pruning ratio + state the out-of-RAM-at-scale projection honestly (M57/M88 lesson) | impl |
| bytea Arrow bridge type surface | LOW | one arm (OID 17 → Binary), unit-tested round-trip | impl |

## Unresolved Questions

- **Native DataFusion `ORDER BY vec LIMIT k` plan operator** (fused with aggregation as a real planner node) — the
  ambitious version of DoD (2); slice-1 delivers the composition at the function-scan level (prefilter + probe +
  rerank + projection in one column-pruned scan), the native planner node is deferred (resolved at C1 by scope).
- **Incremental index maintenance over the columnar segments** — slice-1 materializes a static snapshot; live
  maintenance is a follow-up (dual-store consistency risk).

## Failure scenarios

- **Empty mask** (B1) — no label-matching row → empty top-k, not an error.
- **probes > nlist** (B1) — clamp to nlist (full probe).
- **Wrong-dimension query / non-columnar idx** (C1) — typed error, no panic across C.
- **NULL vec cell** (A1) — skipped from the index, never a panic.

## Global DoD

- Phase A–C `cargo pgrx test pg17` GREEN on the droplet; the byte-identity GATE test (B1) GREEN.
- `docs/benchmarks/m103-vector-columnar.{md,json}` present with the measured column-pruning ratio, methodology,
  honest ceiling (recall equal-by-construction, no QPS-vs-ScaNN).
- No callback panics across C; little-endian raw-vector contract respected.
- CHANGELOG `[Unreleased]` updated; no commits to main; no Co-Authored-By trailer; ADR written.
- Files respect the ~500 LoC budget.
- Sign-off: council-vector-ann + council-index-storage + council-benchmark (review phase).

## Final Phase — Integration Validation

- Full `cargo pgrx test pg17` suite GREEN (no regression on M99–M102 + the new M103 tests).
- The columnar filtered top-k == the exact filtered search (byte-identical); the composed scan projects analytical
  columns in one column-pruned pass.
- Benchmark reproducible; honest ceiling stated.
- council-vector-ann + council-index-storage + council-benchmark = READY_TO_MERGE before `/release`.
