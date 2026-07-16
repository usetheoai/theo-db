# ADR 0044 — M103: vector + columnar in one substrate (Lance-inspired co-residence)

- Status: Accepted
- Date: 2026-07-16
- Deciders: TheoDB core (CYCLE M103; sign-off council-vector-ann + council-index-storage + council-benchmark)
- Tags: vector, columnar, co-residence, filtered-ann, column-pruning, north-star
- Supersedes / relates to: ADR 0035 (M73 North-Star vector verdict — the paradigm ceiling), ADR 0037 (M82 AM IVF/AQ),
  ADR 0042 (M99 own-code columnar TAM), ADR 0033 (positioning: AI-native/HTAP is the moat, not vector speed)

## Context

The pillar shipped the columnar substrate (M99-M101) and the AI plan-ops (M102) as separate access methods from the
IVF vector index (M60-M89) + the filtered-ANN path (M90-M95). The Lance insight is **co-residence**: store the vector
index (IVF partition-id + the raw vector) AS columns alongside the scalar/analytical columns, so a scalar-prefiltered
vector top-k + an analytical projection compose in ONE column-pruned scan. The RowAddrMask already existed (M92
`Membership`), the exact rerank kernel + tie-break already existed (`vec::l2_dist_from_bytes`, `am/scan.rs::Scored`),
the IVF partitioning already existed (`ann/ivf.rs`). M103's novelty is the **co-resident layout + the byte-identity
recall proof**, not a new index.

## Decision

Store the vector index as columnar columns (`tid int8`, `part_id int4`, `label int4`, `vec bytea`) co-resident with
the analytical columns in a `theodb_columnar` table (own code — Lance is a file-format design study only, D1 license
gate). `theodb.vindex_assign` materializes the real IVF partition per row (DoD 1); `theodb.vindex_knn_columnar` runs
the scalar prefilter (label mask) → IVF probe → **exact rerank** → top-k, reading ONLY the 4 index columns via a
column-pruned `decode_columns` (DoD 2/4). The result is proven byte-identical to the exact filtered search.

### D1 — Reuse the exact `Scored` tie-break + `l2_dist_from_bytes`; byte-identity by construction

The columnar filtered top-k and the byte-identity oracle both rerank with `vec::l2_dist_from_bytes` and order with
`d.total_cmp(o.d).then(tid.cmp(o.tid))` (the M90-M95 `am/scan.rs::Scored` comparator). **Rejected:** an Arrow-native
f32 dot product (different summation order → last-ULP drift → different tie-break under distance ties → gate failure;
council-vector-ann trap #2). Rationale: only the identical kernel + tie-break guarantees byte-identity.

### D2 — Full-probe byte-identity vs exact filtered brute-force IS the recall-equivalence proof

At full probe (probes ≥ nlist) the candidate set equals every masked row, so the columnar filtered top-k is
byte-identical to the exact filtered brute-force — proving the co-resident layout loses NO recall (recall = 1.0,
identical to the canonical exact search). **Rejected:** comparing against a live `theodb_ivfflat` build inside the
test (larger harness; the exact-brute-force oracle is a STRONGER statement — the true top-k, not agreement with
another approximate path). The `part_id` column carries the real IVF partition, so reduced-probe is a real IVF
approximation (tested for ordering); the GATE claim is full-probe byte-identity.

### D3 — Column pruning is the measured cost win; latency invariant to analytical width

`vindex_knn_columnar` decodes only `tid`/`part_id`/`label`/`vec`; the analytical columns are never decoded.
**Measured:** knn latency 79.43 ms (narrow, 1 payload col) vs 79.27 ms (wide, 16 payload cols) — ratio **0.998** —
while the wide index is **4.67× larger on disk**. The analytical width does not touch the vector scan.

### D4 — Honest ceiling: cost/scale/composability only; recall equal-by-construction, NO QPS-vs-ScaNN

The benchmark reports column pruning + the composed filtered-knn+aggregation; recall is stated as equal-by-construction
(the GATE), never a win; NO QPS-vs-ScaNN claim. **Rejected:** any "faster ANN" / recall / QPS framing (Rule 5,
public-copy.md, the M73/M74 paradigm ceiling from ADR 0035). Co-residence does not close the paradigm gap.

## Consequences

- **Measured (docs/benchmarks/m103-vector-columnar.*):** column-pruning ratio 0.998 (latency invariant to analytical
  width, 4.67× on-disk difference); composed filtered-knn + aggregation in one plan (225.7 ms).
- **Proven (pg_test, 312 GREEN, +5 M103):** the byte-identity GATE (`m103_full_probe_byte_identical_to_exact_filtered`),
  the scalar prefilter (label mask), reduced-probe ordering, empty-mask handling, and the end-to-end columnar
  co-residence + composition (`m103_columnar_coresident_filtered_topk_matches_exact_and_composes`).
- **Honest boundary:** recall equal-by-construction (not a claim); out-of-RAM at billion-scale is the honest
  projection, not measured; the reduced-probe IVF-over-columnar path is exercised in-memory (the columnar query
  path proves the full-probe GATE). Lance is a file-format → this is a materialized side-store index, NOT a
  replacement for the transactional row-store (dual-store consistency: slice-1 is a static snapshot; incremental
  maintenance over immutable segments is a documented follow-up).
- **Deferred:** a native DataFusion `ORDER BY vec LIMIT k` planner node fused with aggregation (slice-1 composes at
  the function-scan level); reduced-probe over columnar-stored centroids; incremental index maintenance.

## Cross-references

- Plan: `knowledge-base/plans/m103-vector-columnar-unified-substrate-plan.md`
- Benchmark: `docs/benchmarks/m103-vector-columnar.{md,json}`
- Code: `theodb_rs/src/vindex.rs`; reused: `am/scan.rs::Scored`, `vec::l2_dist_from_bytes`, `am/customscan.rs::Membership`,
  `ann/ivf.rs`, `am/columnar.rs::decode_columns` (column-pruned)
- Prior ADRs: 0035 (paradigm ceiling), 0037 (IVF/AQ AM), 0042 (columnar TAM), 0033 (positioning)
