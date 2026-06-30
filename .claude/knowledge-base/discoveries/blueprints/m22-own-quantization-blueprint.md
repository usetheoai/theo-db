# Blueprint: M22 — Own scalar quantization (SBQ) in Rust

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS (89.0, weighted 99.7; sole caveat: citation-density metric — densely cited in absolute terms). Scored 2026-06-30 by /discover-confidence. Synthesized 2026-06-30 from the m22-own-quantization
> discovery plan (v1.1). Sources: pgvectorscale SBQ (the quantizer we match — PostgreSQL License, safe) and
> vectorchord RaBitQ (a quantization datapoint — **AGPL-3.0, study-only, NOT borrowable**). **Migration decision:
> COEXISTENCE, measurement-first; own SBQ-style quantizer with f32 rerank, ZERO new deps.**

**Slug:** `m22-own-quantization`
**Owner:** paulohenriquevn
**Created:** 2026-06-30

## Context

M22 (`ROADMAP-v2.md:128`) requires an **own scale + quantization index in Rust** (target DiskANN/SBQ-quality),
substituting pgvectorscale **only** with **recall parity AND a measured memory profile** (`ROADMAP-v2.md:135`);
else an honest ADR keeps pgvectorscale (anti-sunk-cost). Risk MÁXIMO — the most expensive milestone of v2;
measurement-first is the rigorous guard-rail. M21 shipped own HNSW + IVFFlat ANN search in Rust
(`theodb_rs/src/ann/`, coexistence, SQL-callable) reusing the M20 f32 distance kernel. M22 adds **quantization**:
a compressed bit representation that trades a controlled recall loss for a large memory reduction. The recall
harness (`benchmarks/theodb_bench/recall.py:61`) is reused; memory is measured as **bytes/vector** (a computed
formula, not `pg_relation_size`, for the SQL-callable form).

## Objective

Decide whether TheoDB can build an own SBQ-quality quantizer + quantized ANN search in Rust that reaches
pgvectorscale recall@k parity at a comparable memory profile, and how (coexistence vs substitution). **Decision
reached:** build an own **SBQ-style** quantizer (per-dimension mean threshold, n bits/dim, bit-packed) feeding a
**Hamming-distance** search over the M21 HNSW/IVF with an optional **full-precision f32 rerank** (M20 kernel);
gate recall@k + bytes/vector vs pgvectorscale; keep "retain pgvectorscale" a valid outcome. **ZERO new
dependencies** (pure `std` bit ops) — and RaBitQ is **not** borrowed (AGPL).

## Coverage Corner 1 — Integration Tests

How pgvectorscale tests SBQ + how M22 reuses `theodb_bench` for a recall + memory gate.

**pgvectorscale** — SBQ is tested via a compressed-index scaffold: `CREATE INDEX … WITH (storage_layout =
memory_optimized, num_neighbors = N)` (the SBQ path), at `num_bits_per_dimension = 1` default, dim 1536/768,
metric cosine (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/tests.rs:8-62`).
A sized variant asserts the index fits a byte budget (`tests.rs:31-40`). Tests are `#[pg_test]`.

**M22 reuse of the existing TheoDB harness (no rebuild), two gates:**
1. **Recall gate** — `gt_idx, gt_d = brute_force_ground_truth(corpus, queries, k=10, metric='l2')`
   (`recall.py:41`); build own SBQ index over the corpus; per query collect the (reranked) distances →
   `recall_at_k(gt_d, run, k=10)` (`recall.py:61`); compare to pgvectorscale's compressed index recall at matched
   bits/dim. Gate: `own_recall ≥ pgvectorscale_recall − tol` (tolerance band, ADR D3).
2. **Memory gate** — **computed bytes/vector** (EC-3 — NOT `pg_relation_size`, which is M22b on-disk): own SBQ =
   `ceil(dim · bits / 8)` vs f32 `4·dim` vs pgvectorscale `quantized_size_bytes` (same formula). Gate: own
   bytes/vector ≤ pgvectorscale bytes/vector at matched bits/dim (they match by construction).

## Coverage Corner 2 — Dependencies

What the quantization pulls in + licenses (TheoDB D1 forbids AGPL — only Apache/MIT/BSD/PostgreSQL).

| Dependency | Version (ref) | License | Purpose | D1-clean? |
|---|---|---|---|---|
| pgvectorscale (SBQ ref) | — | **PostgreSQL License** | The SBQ quantizer we LEARN from | ✅ safe to study + reimplement |
| **vectorchord / rabitq** | workspace | **AGPL-3.0 / ELv2 (dual)** | RaBitQ quantizer | ❌ **AGPL — study-only, MUST NOT borrow code** |
| rkyv | 0.7.43 | MIT | pgvectorscale on-disk node serialization | ✅ (not needed by M22) |
| zerocopy | 0.8.48 | Apache-2.0/MIT | vectorchord layout | ✅ (not needed by M22) |
| simdeez / simd | 1.0.8 / path | Apache-2.0/MIT | SIMD distance | ✅ (not needed by M22 — perf, deferred) |

**Critical D1 finding (Q6):** vectorchord is **AGPL-3.0/ELv2**
(`.claude/knowledge-base/references/vectorchord/Cargo.toml`; `rabitq/Cargo.toml` inherits via
`license.workspace = true`). Per TheoDB rule 2 (D1), AGPL is **forbidden in the distribution** — so RaBitQ code is
**study-only**, never copied. pgvectorscale is **PostgreSQL License** (permissive) — safe to learn from and
reimplement. **M22 own SBQ needs ZERO external deps:** the quantizer is pure f32 arithmetic + bit-packing into
`u64` + `popcount` (Rust `u64::count_ones`, std) — parsimony rungs 2/5. No `rand` (training is deterministic mean
accumulation), no SIMD (perf, deferred to a later milestone).

## Coverage Corner 3 — Tools

The memory layout (Q4) + how the quantizer integrates with ANN search (Q5).

### Memory layout (Q4)

- **bytes/vector formula:** `quantized_size_bytes(dim, bits) = ceil(dim·bits / 64) · 8`
  (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs:37-50`),
  i.e. ≈ `ceil(dim·bits/8)` bytes packed into `u64` store words (`SbqVectorElement = u64`,
  `BITS_STORE_TYPE_SIZE = 64`, `sbq/mod.rs:31-32`).
- **f32 baseline:** `4·dim` bytes. **Compression at 1 bit/dim ≈ 32×** (dim=1024: 4096 → 128 bytes).
- **Node layout:** `ClassicSbqNode { heap_item_pointer, bq_vector: Vec<u64>, neighbor_index_pointers, … }`
  (rkyv-archived, `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/node.rs:26-33`)
  — quantized vector in the graph node; full f32 vector kept in the heap (for rerank).
- **M22 measurement:** compute `own_bytes_per_vec = ceil(dim·bits/8)` in-process and compare (EC-3).

### Search integration (Q5)

- **Distance during search = Hamming on bit codes:** `distance_xor_optimized(a, b)` (XOR + popcount), via
  `SbqSearchDistanceMeasure` / `SbqNodeDistanceMeasure`
  (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/mod.rs:33,145-189`;
  `sbq/storage.rs:174-179,330-354`). Quantized vectors live in the graph nodes; traversal uses Hamming (cheap).
- **Rerank EXISTS (Q3 honesty resolved):** `get_full_distance_for_resort()`
  (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/storage.rs:304-328`)
  re-fetches the original f32 vector from the heap and recomputes the exact distance for the top candidates — the
  Hamming-search + f32-rerank pattern is what recovers recall.
- **M22 design (TheoDB, labeled a design sketch):** quantize the corpus with the own SBQ quantizer; build the M21
  HNSW/IVF over the **bit codes** using Hamming distance for traversal; keep the full f32 vectors; **rerank** the
  top-(k·over_fetch) candidates with the M20 f32 kernel (`crate::vec`) and return top-k. This reuses M20 (distance)
  + M21 (graph) — the only new code is the quantizer + the Hamming measure + the rerank glue.

## Coverage Corner 4 — Techniques

The SBQ quantizer (Q1), RaBitQ (Q2, study-only), and the recall-vs-memory tradeoff (Q3).

### SBQ (Q1) — pgvectorscale (the model)

- **Training:** per-dimension running `mean` (and `m2` variance for multi-bit) via Welford's online algorithm
  (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs:115-148`);
  `start_training`/`finish_training` (`:104,:150`).
- **Quantize (1-bit default):** `bit[d] = (x[d] > mean[d]) ? 1 : 0`, packed
  `res[i/64] |= 1 << (i%64)` (`quantize.rs:52-62`); default `num_bits_per_dimension = 1`
  (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/meta_page.rs:81`).
- **Quantize (n-bit):** z-score `(x−mean)/std`, unary-encoded into `n` bits, packed at `d·n+j` (`quantize.rs:63-89`).

```
train:    for each sample: Welford update mean[d] (+ m2[d] if bits>1)
quantize: for d in 0..dim: bits>1 ? unary(zscore(x[d])) : (x[d] > mean[d]); pack into u64 words
search:   hamming(query_code, node_code) = popcount(query_code XOR node_code)
rerank:   for top (k·overfetch): exact = f32_dist(full[node], full[query]); resort; return top-k
```

### RaBitQ (Q2) — vectorchord (STUDY-ONLY, AGPL)

`code(vector)` = sign bits (`vector[i].is_sign_positive()`,
`.claude/knowledge-base/references/vectorchord/crates/rabitq/src/bit.rs:88-124`); `code_metadata` = 4 correction
factors (`dis_u_2`, `factor_cnt`, `factor_ip`, `factor_err`, `bit.rs:20-86`); `preprocess` = asymmetric query LUT
(high-precision query vs 1-bit DB, `bit.rs:126-132`); `pack_code` into `u64` (`bit.rs:135-152`). More complex than
SBQ (rotation + correction factors + LUT). **Not borrowed** — AGPL (Corner 2). SBQ is ~2× simpler and permissive.

### Recall-vs-memory tradeoff (Q3)

- More bits/dim → higher recall + more memory; 1 bit/dim ≈ 32× compression. Hamming-only search loses recall;
  the **f32 rerank** of top candidates recovers it (storage.rs:304-328) — recall@k parity at 1–2 bits/dim is
  plausible **with rerank + over-fetch**. Parity is a **tolerance band** (ADR D3): own recall@k ≥ pgvectorscale −
  tol at matched bits/dim, own bytes/vector ≤ pgvectorscale bytes/vector (equal by the shared formula). Honest:
  without rerank, low-bit recall degrades — so M22 implements rerank.

## Cross-cutting Comparison

| Dimension | pgvectorscale SBQ (model) | vectorchord RaBitQ (study-only) | M22 implication |
|---|---|---|---|
| Quantizer | per-dim mean threshold, 1–8 bits | rotation + sign + 4 factors + LUT | Implement **SBQ-style** (simpler, permissive) |
| License | PostgreSQL (permissive) | **AGPL-3.0** | RaBitQ NOT borrowable (D1); SBQ safe |
| Search distance | Hamming (XOR+popcount) | asymmetric LUT estimate | Hamming over the M21 graph |
| Recall recovery | f32 rerank from heap | 4-factor correction | f32 rerank with the M20 kernel |
| Memory | `ceil(dim·bits/8)` bytes | 1-bit + metadata | same formula; measured bytes/vector |
| Deps | rkyv, simdeez | simd, zerocopy, rabitq | **ZERO** (std bit ops) |
| Storage | quantized in node, f32 in heap | packed u64 | quantized codes + kept f32 for rerank |

## ADRs

### D1 — Own SBQ-style quantizer (NOT RaBitQ): permissive + simpler

**Decision:** implement an own **SBQ-style** scalar quantizer (per-dimension mean threshold, configurable
`bits_per_dim`, bit-packed into `u64`) in Rust. Do NOT borrow RaBitQ.

**Rationale:** RaBitQ lives in vectorchord which is **AGPL-3.0/ELv2** (Corner 2 / Q6) — forbidden in the TheoDB
distribution (rule 2 / D1). pgvectorscale SBQ is PostgreSQL-licensed (safe to learn from) and ~2× simpler to
implement (Q2 contrast). KISS + Don't-Reinvent-but-license-clean.

**Alternatives considered:** port RaBitQ (rejected — AGPL contamination, a release blocker); product quantization
(rejected — heavier, codebook training, YAGNI for the SBQ-parity target).

**Consequences:** the quantizer is pure permissive own code; RaBitQ stays a study reference only.

### D2 — Hamming search + full-precision f32 rerank (reuse M20 + M21)

**Decision:** search the M21 HNSW/IVF over the bit codes using **Hamming distance** (`popcount(a XOR b)`), then
**rerank** the top `k·over_fetch` candidates with the M20 f32 kernel (`crate::vec`) and return top-k.

**Rationale:** this is exactly pgvectorscale's pattern (Hamming traversal + `get_full_distance_for_resort`,
storage.rs:304-328) and the only way 1-bit recall reaches parity. Reuses M20 (distance) + M21 (graph) — Rule 9.

**Alternatives considered:** Hamming-only, no rerank (rejected — recall too low at 1 bit/dim); dequantized
estimate distance (rejected — more complex than Hamming + rerank, no recall benefit).

**Consequences:** keep the full f32 vectors alongside the codes; the rerank over-fetch factor is a tunable knob.

### D3 — Coexistence, measurement-first, recall + memory gate, anti-sunk-cost

**Decision:** M22 ships an own SBQ quantizer + quantized ANN search as SQL-callable functions (coexistence — no
pgvectorscale/pgvector touched), gated by (a) recall@k parity (tolerance band, with rerank) and (b) bytes/vector
memory profile, both vs pgvectorscale SBQ, measured + reproducible in `docs/benchmarks/`. If parity at a
comparable memory profile is NOT reached → honest ADR **retain pgvectorscale** (anti-sunk-cost); the milestone
delivers the measurement, not a regression.

**Rationale:** the M22 DoD (`ROADMAP-v2.md:135`) is measurement-first; mirrors the M21 user-approved scope.

**Alternatives considered:** full StreamingDiskANN AM (rejected — M22b, multi-month); force substitution
(rejected — violates measurement-first).

**Consequences:** the acceptance metric is the recall+memory benchmark; "retain pgvectorscale" is first-class.

### D4 — Zero new dependencies (std bit ops); SQL-callable scope (planner AM = M22b)

**Decision:** the quantizer uses only `std` (f32 arithmetic, `u64` packing, `u64::count_ones`) — no new crate.
The deliverable is SQL-callable (build + quantized search functions); the on-disk planner-integrated AM is M22b.

**Rationale:** parsimony rungs 2/5 (a bit-packer is std); avoids AGPL + SIMD churn; matches the M21 measurement-
first SQL-callable scope the user chose.

**Alternatives considered:** add `simdeez` for SIMD popcount (rejected — perf, YAGNI now; `u64::count_ones` is
hardware popcount already); add `rkyv` for serialization (rejected — batch SRF needs no persistence, M21 D1).

**Consequences:** no dependency/license risk; M22b owns persistence + planner integration.

## Recommendations

1. **Implement own SBQ-style quantizer in Rust (per-dim mean threshold, `bits_per_dim`, u64-packed), per ADR
   D1** — permissive, ~2× simpler than RaBitQ, ZERO deps. (feeds Q1/Q2/Q6)
2. **Search with Hamming over the M21 HNSW/IVF + full-precision f32 rerank (M20 kernel), per ADR D2** — the
   recall-recovery pattern. (feeds Q3/Q5)
3. **Gate recall@k (tolerance band, with rerank) AND bytes/vector vs pgvectorscale, reusing `theodb_bench`, per
   ADR D3** — measurement-first; "retain pgvectorscale" valid on miss. (feeds Q4/Q7)
4. **Zero new dependencies; SQL-callable scope; AGPL avoided (RaBitQ study-only), per ADR D4** — planner AM is
   M22b. (feeds Q6 + the migration decision)
5. **Measure both metrics honestly:** recall@k (own vs pgvectorscale) + bytes/vector (computed formula, not
   `pg_relation_size`); document the rerank over-fetch and the bits/dim sweep. (feeds Q4/Q7, EC-3)

## Blocked questions (if any)

(none — all 7 questions answered with citations.)
