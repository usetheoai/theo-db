# Plan: theodb_symqg FastScan 1-bit SIMD sign kernel

> **Version 1.1** (2026-07-18 — absorbed EC-1/2/3 MUST-FIX from `reviews/symqg-fastscan-1bit-edge-cases-2026-07-18.md`: added the FastScan eligibility dispatch D5 + degree>32 chunking + EC-4/5/6 tests) — The E2 in-PG A/B (`docs/benchmarks/e2-symqg-inpg-verdict.md`) measured `theodb_symqg` 2.6–3.9× SLOWER than `theodb_hnsw` at matched recall, warm — the per-hop bottleneck is the 32 scalar sign-dot estimates (O(32·dim) per popped vertex). This plan replaces that scalar loop with a batched FastScan 1-bit SIMD kernel that reuses the already-tested `vec/ah.rs::ah_score_block` (LUT16-pshufb, block32) to score all 32 neighbours' sign-codes in one SIMD pass, then re-runs the same SIFT1M A/B. Measurement-first: the deliverable is the measured number (gate met OR honest-negative), and recall parity is a hard gate.

## Goal

> "Enable `theodb_symqg` vector search to score a vertex's 32 neighbour sign-codes via a batched FastScan 1-bit SIMD kernel so that per-hop estimate cost drops, measured by the SIFT1M in-PG A/B (`benchmarks/e2_symqg_inpg.py`) returning **theodb_symqg QPS ≥ 1.5× theodb_hnsw at matched recall@10 ≥ 0.95**."

Measurement-first note (D4): the QPS target is the aspiration the slice chases; the **deliverable** is the measured A/B (met OR an honest-negative, consistent with the E2 v2 verdict and M73/M74). Recall parity (within 1.5 pp of the current scalar scan) is a **hard** gate — a QPS win bought by a recall regression is rejected.

## Context

The E2 verdict (`docs/benchmarks/e2-symqg-inpg-verdict.md`) settled the page-tax gate: contiguous packing folded the index 5.66× (7828→1383 MB) and lifted QPS 2.3× (32→73 @ recall 0.95), but `theodb_hnsw` is still 2.6–3.9× faster at matched recall. The residual gap is scan maturity + the per-hop cost: each popped vertex decodes a 1408-byte co-located row and runs **32 scalar sign-estimates** (`estimate_sign`, `scan.rs:350`), each an O(dim) dot `⟨q_r, u⟩` over `u ∈ {−1,+1}^dim`. That 32·dim term dominates the per-hop cost.

The off-PG spike (`docs/benchmarks/e2-symqg-spike.md`) showed the co-located traversal is algorithmically 1.8–2.66× faster than a reference — in RAM, scalar. SymphonyQG's paper closes the in-PG gap with **FastScan**: the 1-bit sign dot reformulates exactly as a LUT16-pshufb scan (group 4 sign-dims → 16 sign patterns → a per-query LUT of signed sums). Our `vec/ah.rs::ah_score_block` is exactly that LUT16-pshufb kernel, already tested (memory `ah-batched-kernel-exists`). This plan wires it into the symqg scan.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/vec/ah.rs` | 328 | `7a5d798` (2026-07-16) | AH LUT16-pshufb scoring kernel (M59); `build_lut16`, `Lut16` (private fields), `ah_score_block` (pub(crate)), `ah_score_block32` (AVX2), `Lut16::dequantize`, `LUT_MAX=127` | `ah_score_block` signature + block32 layout contract MUST stay unchanged (existing AQ v4+ callers in `ivf_aqah.rs`/`scan.rs` depend on it); domain layer — NO `pg_sys` (`architecture.md § 1`) |
| `theodb_rs/src/vec/ah_tests.rs` | — | `7a5d798` (2026-07-16) | `#[pg_test]` suite for `ah.rs` (split for the 500-LoC budget) | test-only; new sign-LUT tests append here |
| `theodb_rs/src/am/page/symqg.rs` | 359 | `39e5487` (2026-07-18) | v2 co-located page layout; `pack_row` (signs region), `pack_symqg`, `decode_row`, `SYMQG_VERSION=2`, `row_bytes` | `row_bytes(dim,degree)` value MUST stay unchanged (contiguous packing arithmetic in `mod.rs`/`scan.rs` depends on it); pure `#[test]` codec must stay standalone-rustc runnable |
| `theodb_rs/src/am/scan.rs` | 1254 | `39e5487` (2026-07-18) | `gather_symqg_candidates` (beam search, `:277`) calls `estimate_sign` per neighbour (`:350`) via the `read_row` closure (`:313`) | scan output ordering (sqrt-L2 scale) + recall MUST be preserved (E1 qc2-scale lesson); `am` may depend on `vec`, never the reverse |
| `theodb_rs/src/ann/symqg_spike.rs` | 433 | `39e5487` (2026-07-18) | `SignCode{u,nr,w}`, `encode_sign` (pub(crate)), `estimate_sign` (pub(crate)), off-PG `SymqgSpike` | `estimate_sign` stays as the scalar oracle for the FastScan parity test; `SignCode` fields stay |
| `benchmarks/e2_symqg_inpg.py` | 93 | `39e5487` (2026-07-18) | in-PG A/B harness (build both AMs, sweep ef, recall@10 + QPS + size) | reused unchanged as the Goal metric harness |

### Current callers / dependents

- **Symbol:** `ah_score_block(lut, codes, n, out)` in `vec/ah.rs:290`
  - **Callers (production):** `theodb_rs/src/ann/ivf_aqah.rs` (AQ v4+ scan), `theodb_rs/src/am/scan.rs` (AQ gathers)
  - **Callers (tests):** `theodb_rs/src/vec/ah_tests.rs`
  - **External:** no — `pub(crate)`, in-crate only.
- **Symbol:** `estimate_sign(code, q_r, qc2)` in `ann/symqg_spike.rs`
  - **Callers (production):** `theodb_rs/src/am/scan.rs:350` (the loop this plan accelerates)
  - **Callers (tests):** `theodb_rs/src/ann/symqg_spike.rs` (spike traversal + tests)
  - **External:** no.
- **Symbol:** `pack_row` / `decode_row` / `read_symqg_row` in `am/page/symqg.rs`
  - **Callers (production):** `am/build.rs` (`pack_symqg`→`pack_row`), `am/scan.rs` (`read_symqg_row`), `am/page/mod.rs` (`main_index_pages`)
  - **Callers (tests):** `am/page/symqg.rs` pure `#[test]` block.

### Domain glossary

- **sign-code (1-bit RaBitQ)** — per-neighbour `u ∈ {−1,+1}^dim` = `sign(P·(x−c))`, plus scalars `nr` (residual norm) and `w = ⟨u,o'⟩`; stored co-located in the vertex row.
- **q_r** — the rotated query residual `rot_q − rot_p` at the popped vertex `p`; the estimate dots it against each neighbour's `u`.
- **Lut16** — a per-query lookup table: `m` subspaces × 16 int8 partial scores + an affine (`scale`,`bias`) to dequantize an integer accumulator back to f32. Built by `build_lut16` for AQ; this plan adds `build_sign_lut16` for the sign case.
- **block32 (FastScan `bbs`)** — subspace-pair-major transposed code layout: `codes[pair*32 + v]` holds vector `v`'s packed nibble-pair for subspace-pair `pair`; `ah_score_block` scores 32 vectors/instruction via `pshufb`.
- **FastScan sign LUT** — for a group of 4 sign-dims, the 4-bit sign pattern (16 possibilities) indexes the signed sum `Σ_{d in group} q_r[d]·(±1)`; `m = dim/4` groups reproduce `⟨q_r,u⟩` exactly (up to int8 requant).

### Architecture boundaries affected

- `vec/` (domain, no `pg_sys`) — `build_sign_lut16` is pure domain math added here; `ah_score_block` reused unchanged. Preserves `architecture.md § 1` (domain has no PG dependency).
- `am/scan.rs` (interface, PG boundary) → depends on `vec/ah.rs` (inward). No new reverse dependency. `am/page/symqg.rs` (PG boundary) build-time repack.

## Dependencies

**No new external dependency (Rule 9 — reuse before add).** This slice reuses only existing in-crate code:

| Dependency | Version | Status | Rule 9 justification |
|---|---|---|---|
| `theodb_rs::vec::ah` (in-crate) | current | existing | The LUT16-pshufb `ah_score_block` kernel + `Lut16` + int8 requant already exist and are tested (`ah-batched-kernel-exists`); this slice adds only `build_sign_lut16` in the same module. |
| `std::arch::x86_64` (stdlib) | rustc pinned | existing | AVX2 intrinsics already used by `ah.rs`; no new crate. |

No `Cargo.toml` change. `/deps-audit` verdict: PASS (no declared dep added → no CVE surface).

## Prior Art & Related Work

- **Internal blueprint:** `knowledge-base/discoveries/blueprints/symphonyqg-graph-quant-blueprint.md` — the SymphonyQG co-located-graph + FastScan investigation that motivated E2.
- **Internal benchmark (input):** `docs/benchmarks/e2-symqg-inpg-verdict.md` — v2 packed: symqg 73 vs hnsw 287 QPS @ recall 0.95 (gap 3.9×); names the FastScan kernel as the next lever.
- **Internal benchmark:** `docs/benchmarks/e2-symqg-spike.md` — off-PG scalar 1.8–2.66× advantage (proves the algorithmic headroom the in-PG FastScan chases).
- **In-repo reuse (Rule 9):** `theodb_rs/src/vec/ah.rs` — the LUT16-pshufb `ah_score_block` kernel + block32 machinery + int8 requant, already built and tested (memory `ah-batched-kernel-exists`). This plan adds ONLY the sign-LUT builder + wiring, not a new kernel.
- **External literature:** SymphonyQG, arXiv:2411.12229 (SIGMOD'25) — the co-located-graph + FastScan design (STUDY-ONLY, NTUITIVE license; clean-room, never copied — D1). RaBitQ, arXiv:2405.12497 — the 1-bit estimator whose FastScan-LUT reformulation this plan implements.

## Objective

- [ ] `build_sign_lut16(q_r) -> Lut16` in `vec/ah.rs` builds the sign LUT; `sign_lut_dequant_within_tol` #[pg_test] asserts |dequantized-dot − exact ⟨q_r,u⟩| ≤ 1 requant step.
- [ ] A FastScan estimate path scores 32 neighbours via `ah_score_block` + a per-neighbour scalar finalize (`qc2 + nr² − 2nr·dot/w`), parity-tested against `estimate_sign` within tolerance.
- [ ] `page/symqg.rs` v3 packs sign-codes block32-nibble; `symqg_row_block32_round_trips` standalone `rustc --test` passes AND `row_bytes(128,32)==1408` unchanged.
- [ ] `scan.rs::gather_symqg_candidates` uses the FastScan path + `with_page_item`; `symqg_scan_recall_preserved_inpg` #[pg_test] top-k set-equal (recall delta ≤ 0.5 pp).
- [ ] FastScan eligibility dispatch (D5): FastScan for `dim%4==0 && dim/4≤258` (+ `⌈degree/32⌉` chunks); scalar `estimate_sign` fallback otherwise — correct for ALL dims/degrees (1536-dim, `degree_bound=64`).
- [ ] `benchmarks/e2_symqg_inpg.py` re-run emits `E2AB_DONE`; v3 recall@10 within 1.5 pp of v2 scalar (hard gate); symqg/hnsw QPS ratio recorded as a number in the verdict.

## ADRs

### D1 — Reuse `ah_score_block` via a new `build_sign_lut16`, not a bespoke 1-bit kernel

- **Decision:** Express the 1-bit sign dot `⟨q_r,u⟩` as a LUT16-pshufb scan (group 4 sign-dims → 16 sign patterns → signed-sum LUT) and reuse the existing `ah_score_block` unchanged; add only `build_sign_lut16`.
- **Rationale:** Unbreakable Rule 9 (don't reinvent) + `parsimony-ladder.md` rung 4 (reuse installed capability). The pshufb kernel + block32 layout + int8 requant are already built and tested; the reformulation is exact (a group's 16 patterns are all the possible signed sums). De-risks the slice.
- **Alternatives considered:** (a) a bespoke AVX2 popcount/XOR-based 1-bit kernel — rejected: more `unsafe` code, unvalidated, no correctness oracle, and the LUT16 reformulation gives the same throughput reusing tested code. (b) keep the scalar loop, only add `with_page_item` — rejected: the 32·dim estimate is the dominant term; copy-free reads alone give ~1.2× (insufficient).
- **Consequences:** Enables 32-neighbours/pass SIMD scoring reusing tested code; constrains the estimate to int8-requant precision (D3 gates recall).

### D2 — block32-nibble layout at BUILD time (bump SYMQG_VERSION 2→3)

- **Decision:** Repack a vertex's 32 neighbour sign-codes into block32-nibble transposed layout when `pack_symqg` writes the row; bump `SYMQG_VERSION` 2→3.
- **Rationale:** Build-once / read-many (measurement-first) — the transpose is paid once at `CREATE INDEX`, not per hop. `row_bytes` is unchanged (32×⌈dim/8⌉ = pairs×32 bits either way), so the contiguous-packing arithmetic (`mod.rs`, `scan.rs`) is untouched.
- **Alternatives considered:** repack per-scan-hop from the existing v2 bit-packed layout — rejected: a per-hop transpose negates the kernel win (it re-introduces an O(32·dim) rearrange). Keeping v2 and reading both layouts — rejected: dead complexity, the AM is pre-release (REINDEX is free).
- **Consequences:** Existing v2 symqg indexes need REINDEX (version gate fails loud — the established pattern); no on-disk size change.

### D3 — int8 requant precision is a hard RECALL gate, measured not assumed

- **Decision:** The FastScan estimate uses `Lut16`'s int8 requant (bounded error); recall parity (within 1.5 pp of the v2 scalar scan) is a hard gate verified by the A/B + a per-neighbour tolerance unit test — not assumed from the paper's bound.
- **Rationale:** Honesty (Rule 3) + measurement-first (`CLAUDE.md` rule 5). The scalar `estimate_sign` uses a full f64 dot; the FastScan int8 LUT is lossy. Recall must be measured, not inferred.
- **Alternatives considered:** trust the RaBitQ error bound and skip the recall gate — rejected: assumptions are not evidence; the E1/E2 lessons are that measured reality differs from the model.
- **Consequences:** A recall regression > 1.5 pp fails the slice (loop back), even if QPS improves.

### D4 — Goal metric is the QPS gate; deliverable is the measured A/B (met OR honest-negative)

- **Decision:** The plan chases `symqg QPS ≥ 1.5× hnsw @ recall 0.95` but ships the measured number either way, as an honest-negative if the gate is not met.
- **Rationale:** Measurement-first mandate; the E2 v2 verdict shipped an honest-negative and it was the valuable deliverable. `public-copy.md` — no QPS-win claim without the benchmark.
- **Alternatives considered:** only merge if the gate is met — rejected: an honest-negative (how far a permissive PG extension can close the gap) is itself North-Star evidence (M73/M74 precedent).
- **Consequences:** `/review` reads the Goal as the target; a measured-negative does not invalidate the correctly-wired kernel + preserved recall.

### D5 — FastScan eligibility dispatch with a scalar fallback (absorbs EC-1/2/3)

- **Decision:** The scan dispatches per index: use the FastScan kernel ONLY when `dim % 4 == 0 && dim/4 ≤ 258` (int16-accumulator-safe, `ah.rs:25`); for `degree > 32`, loop `⌈degree/32⌉` block32 chunks. When `dim % 4 != 0` OR `dim/4 > 258`, fall back to the scalar `estimate_sign` path, reconstructing each neighbour's `u` from the block32 codes. The v3 block32 layout is ALWAYS written (one format); the dispatch is a scan-time choice from `dim`/`degree` (both known at scan start).
- **Rationale:** `theodb`'s public surface accepts arbitrary `vector(N)` dims and a `degree_bound` reloption up to 512 (`options.rs MAX=512`). The block32/int16 kernel has hard limits: `m = dim/4 ≤ 258` (else i16 overflow → silent recall corruption on 1536-dim OpenAI embeddings, EC-1) and `n ≤ 32` per block (`ah.rs:292`, else panic on `degree_bound=64`, EC-3). Fail-safe (Rule 8): the scalar path is always correct; FastScan is a pure optimization gated on eligibility. 768-dim (BERT, `m=192`) and 128/256/512/1024 are eligible; 1536 falls back (no regression, just no speedup).
- **Alternatives considered:** (a) reject ineligible dims/degrees at `CREATE INDEX` — rejected: a vector DB must index 1536-dim embeddings; refusing them is worse than a scalar fallback. (b) two on-disk layouts (block32 for eligible, bit-packed for ineligible) — rejected: dead complexity; block32 can reconstruct `u` for the scalar path with an O(dim) un-transpose (same cost the scalar path already pays).
- **Consequences:** Correct recall for ALL dims/degrees; FastScan speedup only on the eligible common case. `decode_row` must expose both the raw block32 codes (FastScan) and a per-neighbour `u` reconstruction (scalar fallback).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| int8 requant of `q_r` regresses recall below the 1.5 pp tolerance | Medium | D3 recall gate: A/B recall@10 within 1.5 pp of v2 scalar + a per-neighbour tolerance unit test; fail loud, loop back | impl |
| QPS target (1.5× hnsw) may not be met — only reach parity | Medium (honest) | Measurement-first (D4): accept + document the honest-negative; the scalar→SIMD estimate is ~4-8× on the dominant term, ~2-4× overall (estimate) | impl |
| Index format bump v2→v3 forces REINDEX of existing symqg indexes | Low | Version gate in `SymqgMeta::decode` fails loud with a REINDEX message (existing pattern); AM is pre-release | impl |
| Per-hop `build_sign_lut16` adds O(dim·16/4) = ~4·dim work per hop | Low | It replaces 32·dim scalar work per hop (8× more); net win. Verified by the A/B QPS (if the LUT-build dominates, QPS won't move — the gate catches it) | impl |
| `unsafe` AVX2 path already exists but a wrong sign-LUT could feed it garbage | Low | `ah_score_block` asserts layout (`codes.len()==pairs*32`, `n<=32`) ALWAYS (release too); the scalar `ah_score_block_scalar` fallback is the correctness oracle; parity test forces both | impl |
| Ineligible dims/degrees (1536-dim OpenAI, `degree_bound=64`) exceed the block32/int16 kernel limits → silent corruption / panic (EC-1/2/3) | High | D5 eligibility dispatch: FastScan only when `dim%4==0 && dim/4≤258`; `⌈degree/32⌉` chunks for degree>32; scalar `estimate_sign` fallback otherwise (always correct) | impl |

## Unresolved Questions

- Q1 — Does the int8 requant keep recall@10 within 1.5 pp of the v2 scalar scan at the ef needed for recall 0.95? (Answered by the A/B — the D3 gate.)
- Q2 — Is the per-hop LUT-build overhead negligible vs the batched-estimate win, or does it eat the gain? (Answered by the A/B QPS — Q4 drawback.)
- Q3 — For `degree_bound ≠ 32`, does the block32 kernel handle n<32 (sentinel-padded) correctly? (Resolved at plan time: `ah_score_block` asserts `n≤32`; padded slots compute a garbage estimate that the scan ignores because `ordinal == SENTINEL_ORD`. For `degree>32` the row's neighbours already cap at `degree`; a >32-neighbour block would need chunking — out of scope, `degree_bound` default is 32 and the A/B uses 32.)

## Dependency Graph

```
Phase 1 (vec/ah.rs kernel — pure, off-PG testable)
   │
   ▼
Phase 2 (page/symqg.rs v3 layout + scan.rs wiring — depends on Phase 1 kernel)
   │
   ▼
Phase 3 (Integration Validation — in-PG SIFT1M A/B re-measure)
```

Sequential — Phase 2 consumes Phase 1's `build_sign_lut16` + FastScan estimate; Phase 3 measures the wired result.

---

## Phase 1: FastScan 1-bit sign kernel (pure domain, `vec/ah.rs`)

**Objective:** Add `build_sign_lut16` + a batched sign-estimate that reuses `ah_score_block`, proven correct against the scalar `estimate_sign` oracle off-PG.

### T1.1 — `build_sign_lut16(q_r) -> Lut16` + FastScan sign-estimate

#### Objective
Add a pure LUT builder that turns `q_r` into a `Lut16` of signed-sum partials, and a batched estimate that scores 32 sign-codes via `ah_score_block` then finalizes per neighbour.

#### Why this step (action + reasoning — ReAct discipline)

**What this step does:** Adds `build_sign_lut16(q_r: &[f32]) -> Lut16` in `vec/ah.rs` (16 signed-sums per 4-dim group, one global int8 affine requant mirroring `build_lut16:94-105`), plus a helper that calls `ah_score_block(lut, block32_codes, n, out)` and finalizes each neighbour's estimate `qc2 + nr² − 2nr·(lut.dequantize(out[k])/w_k)`.

**Why it is necessary now:** The per-hop 32·dim scalar dot (`scan.rs:350`) is the measured bottleneck (D1, `docs/benchmarks/e2-symqg-inpg-verdict.md`). The LUT16 reformulation is exact and reuses the tested `ah_score_block` (Rule 9), so the kernel must exist and be proven off-PG BEFORE wiring it into the PG scan (Phase 2) — a pure test isolates the requant-tolerance risk (D3) from the page/scan plumbing.

#### Evidence
`vec/ah.rs:60` (`build_lut16` — the affine-requant template), `vec/ah.rs:290` (`ah_score_block` contract), `vec/ah.rs:49` (`Lut16::dequantize`), `ann/symqg_spike.rs` `estimate_sign` (the scalar oracle), `docs/benchmarks/e2-symqg-inpg-verdict.md` (the 32·dim bottleneck).

#### Files to edit
```
theodb_rs/src/vec/ah.rs — add build_sign_lut16 + fn sign_estimate_block (reuses ah_score_block); remove/keep #![allow(dead_code)] as callers land
theodb_rs/src/vec/ah_tests.rs — RED #[pg_test]s: sign-LUT dequant tolerance + FastScan-vs-estimate_sign parity
```

#### Deep file dependency analysis
- `vec/ah.rs` (Baseline row 1): today holds `build_lut16` (AQ/L2) + `ah_score_block`. This task ADDS `build_sign_lut16` (sign case) + `sign_estimate_block`; it does NOT touch `ah_score_block`'s signature (AQ callers `ivf_aqah.rs`/`scan.rs` unaffected). `Lut16` private fields are accessible in-file, so the builder constructs it directly.
- `vec/ah_tests.rs`: appends `#[pg_test]`s; no production caller.

#### Deep Dives
- **`build_sign_lut16` algorithm:** `m = dim/4` groups. For group `g` and pattern `p ∈ 0..16` (4 sign bits, bit `b`=1 ⇒ `+q_r[4g+b]`, 0 ⇒ `−`), partial `= Σ_{b<4} q_r[4g+b]·(bit? +1:−1)`. Track global min/max over all `m·16` partials; affine-requant to `[0,127]` (`scale=(hi-lo)/127`, `bias=lo`) exactly as `build_lut16:94-105`. `Lut16{m, tables, scale, bias}`.
- **`sign_estimate_block`:** input `lut`, `codes` (block32-nibble, `pairs*32` bytes), `n` (≤32), per-neighbour `nr[], w[]`, scalar `qc2`. Call `ah_score_block(lut, codes, n, out_i32)`; for each k<n: `dot_k = lut.dequantize(out[k])`; `est_k = qc2 + nr[k]² − 2·nr[k]·(dot_k / w[k])` (guard `w[k]==0 || nr[k]==0 ⇒ qc2 + nr[k]²`, matching `estimate_sign`).
- **Invariants:** `ah_score_block` layout asserts hold (`codes.len()==pairs*32`, `out.len()==n`); the accumulator never overflows i16 (`m=dim/4=32 ≤ 258`, `32·127<32767`).
- **Edge cases:** all-equal partials (degenerate range) → `build_lut16`'s `max(f32::MIN_POSITIVE)` guard reused; `w==0`/`nr==0` neighbour → early scalar branch (parity with `estimate_sign`); empty `q_r` → typed `Err` (fail-fast, Rule 8).

#### Pseudo-code / Signatures
```pseudocode
fn build_sign_lut16(q_r: &[f32]) -> Result<Lut16, String>
  -- precondition: q_r.len() % 4 == 0 (dim multiple of 4; SIFT 128 ✓)
  m = q_r.len() / 4
  partials = []          -- m*16 signed sums
  for g in 0..m:
    for p in 0..16:
      s = Σ_{b<4} q_r[4g+b] * (if bit(p,b) {+1} else {-1})
      partials.push(s)
  (lo,hi) = (min,max) partials
  scale = (hi-lo).max(MIN_POS)/127 ; tables = partials.map(v -> round((v-lo)/scale).clamp(0,127) as i8)
  Ok(Lut16{ m, tables, scale, bias: lo })

# Example (dim=4, q_r=[1,-2,0.5,3]): pattern 0b1111 (all +) partial = 1-2+0.5+3 = 2.5 (the max)
#                                    pattern 0b0000 (all -) partial = -2.5 (the min)
```

#### Tasks
1. Add `build_sign_lut16` to `vec/ah.rs` (mirror `build_lut16`'s requant).
2. Add `sign_estimate_block` (calls `ah_score_block`, finalizes per neighbour).
3. Append RED `#[pg_test]`s to `ah_tests.rs`; implement to GREEN.

#### TDD
```
RED:  sign_lut_dequant_within_tol() — build_sign_lut16(q_r); for random 1-bit codes, |lut.dequantize(ah_score_block(...)) − exact ⟨q_r,u⟩| ≤ scale (one requant step). MUST fail before build_sign_lut16 exists.
RED:  sign_fastscan_matches_estimate_sign() — for 32 random SignCodes, sign_estimate_block estimates match estimate_sign within 2·scale tolerance (rank-order identical on a sorted sample).
RED:  sign_lut_empty_q_r_errs() — build_sign_lut16(&[]) returns typed Err (fail-fast).
RED:  sign_fastscan_matches_estimate_sign_mixed_sign_qr() [EC-4] — mixed-sign q_r (so lo<0) still matches estimate_sign within tolerance (the affine map handles negative partials; assert, don't assume — build_lut16's partials are never negative).
RED:  sign_lut_degenerate_range_all_zero_qr() [EC-6] — build_sign_lut16(&[0.0;128]) is a valid Lut16 (max==min guard) and sign_estimate_block returns qc2+nr² per neighbour.
RED:  build_sign_lut16 backstop — debug_assert!(m<=258) inside build_sign_lut16 (the D5 dispatch guarantees eligibility; the assert is the belt-and-suspenders backstop).
GREEN: implement build_sign_lut16 + sign_estimate_block.
REFACTOR: factor the shared affine-requant with build_lut16 only if it does not couple the AQ and sign paths (DRY vs KISS — duplicate the ~6-line requant if extraction couples them).
VERIFY: cargo pgrx test (droplet) — sign_* tests; + a standalone `rustc --test` extraction of the pure LUT math (T1.1 pattern) for a droplet-free fast check.
```

#### Concurrency tests

(none — single-threaded)
`build_sign_lut16`/`sign_estimate_block` are pure functions over owned slices; no shared state.

#### Acceptance Criteria
- [ ] `build_sign_lut16` + `sign_estimate_block` exist and the 3 RED tests pass GREEN.
- [ ] FastScan estimates match `estimate_sign` within 2·scale on a random sample (parity oracle).
- [ ] Pass: size — `vec/ah.rs` ≤ 500 lines after the addition (currently 328; budget `architecture.md`).
- [ ] Pass: lint — `cargo clippy` zero warnings on `vec/ah.rs`.

#### DoD
- [ ] Tasks completed; sign_* tests green (droplet `cargo pgrx test` + standalone pure check).
- [ ] Zero clippy warnings on changed files.
- [ ] `vec/ah.rs` ≤ 500 LoC.

---

## Phase 2: block32 layout + scan wiring (`page/symqg.rs`, `scan.rs`)

**Objective:** Pack neighbour sign-codes block32-nibble at build time (v3) and wire the FastScan estimate + copy-free reads into the beam search.

### T2.1 — v3 block32-nibble sign-code layout in `pack_row`/`decode_row`

#### Objective
Change `pack_row` to write the 32 neighbours' sign-codes in block32-nibble transposed layout; bump `SYMQG_VERSION` 2→3; keep `row_bytes` and everything else identical.

#### Why this step (action + reasoning)

**What this step does:** In `pack_row` (`page/symqg.rs`), replace the per-neighbour bit-packed signs region `[R × ⌈dim/8⌉]` with the block32-nibble layout `codes[pair*32 + v]` (`pairs = ceil((dim/4)/2)`), and mirror it in `decode_row`/`read_symqg_row` so the scan reads the block directly. Bump `SYMQG_VERSION`.

**Why it is necessary now:** `ah_score_block` requires the block32 transposed layout (D2, `ah.rs:196-202`). Doing the transpose at build time (once) is the measurement-first choice; a per-hop transpose would negate the kernel win (Q4). `row_bytes` is unchanged (both layouts pack the same 32·dim bits), so the contiguous-packing arithmetic added in the E2 verdict work (`mod.rs`, `scan.rs`) is untouched.

#### Evidence
`page/symqg.rs` `pack_row` (signs region), `SYMQG_VERSION=2`, `row_bytes`; `vec/ah.rs:196-202` (block32 layout contract); `docs/benchmarks/e2-symqg-inpg-verdict.md` (v2 packing that this extends).

#### Files to edit
```
theodb_rs/src/am/page/symqg.rs — pack_row: block32-nibble signs region; decode_row/read_symqg_row: return the raw block32 bytes for the scan; SYMQG_VERSION 2→3; version-gate REINDEX message; update pure #[test]s
```

#### Deep file dependency analysis
- `page/symqg.rs` (Baseline row 3): today `pack_row` writes `[rot][ord][signs bit-packed][factors]`. This task changes ONLY the signs region encoding to block32-nibble (same byte count). `decode_row` must expose the block32 bytes (the scan feeds them to `ah_score_block`) AND still recover per-neighbour `nr,w,ordinal`. `read_symqg_row` (called by `scan.rs`) returns a struct carrying the block32 code bytes + the `(ord, nr, w)` per neighbour.
- Callers: `build.rs::pack_symqg`→`pack_row` (build), `scan.rs::read_row` (scan), `mod.rs::main_index_pages` (uses `row_bytes` only — unchanged).

#### Deep Dives
- **block32-nibble packing:** `m = dim/4` groups per neighbour; group `g`'s 4 sign bits form a nibble `∈0..16`; two groups pack into one byte (low nibble = group `2p`, high = `2p+1`); transposed so `codes[pair*32 + v]` = neighbour `v`'s byte for subspace-pair `pair`. For dim=128: `m=32`, `pairs=16`, region = `16*32 = 512` bytes = the current signs-region size (32×16). `row_bytes` unchanged.
- **degree > 32 (EC-3):** the codes region holds `⌈degree/32⌉` consecutive block32 blocks; block `b` covers neighbours `[32b, 32b+32)`. degree is a multiple of 32 (`degree_bound_from_relation` rounds up), so blocks are full-width. Region size = `⌈degree/32⌉ · pairs · 32` bytes — still equal to the v2 `degree · ⌈dim/8⌉` (same bit count), so `row_bytes` is unchanged for any degree.
- **Reconstruct helper (D5 scalar fallback):** `decode_row` exposes the raw block32 codes AND `neighbour_u(v) -> Vec<i8>` that un-transposes block lane `v` back to `u ∈ {−1,+1}^dim` — O(dim) per neighbour, used by the scalar `estimate_sign` path when the index is FastScan-ineligible (dim%4≠0 || dim/4>258).
- **Invariants:** `row_bytes(dim,degree)` value identical (contiguous packing depends on it); sentinel-padded neighbours (< degree real, padded LAST per `pack_row`) occupy the tail lanes — their nibble block is all-zero and their estimate is ignored (`ordinal == SENTINEL_ORD`).
- **Edge cases:** `dim` not a multiple of 4 → block32 not written for the sign groups; the layout stores whole-byte sign bits and the scan takes the scalar path (D5) — the codec still round-trips the raw sign bits; `degree < 32` → block still 32-wide, tail lanes are sentinels.

#### Pseudo-code / Signatures
```pseudocode
# in pack_row, signs region (replaces bit-packed loop):
for v in 0..degree:                      # neighbour (block lane)
  for pair in 0..pairs:                  # subspace-pair
    g0 = 2*pair ; g1 = 2*pair+1
    lo = nibble_of_group(neighbours[v].u, g0)   # 4 sign bits -> 0..16
    hi = if g1<m { nibble_of_group(neighbours[v].u, g1) } else { 0 }
    signs[pair*32 + v] = lo | (hi<<4)
# decode_row returns SymqgRow{ rot, block32_codes: Vec<u8>, neigh: Vec<(ord,nr,w)> }
```

#### Tasks
1. Rewrite `pack_row` signs region to `⌈degree/32⌉` block32-nibble blocks; add `nibble_of_group`.
2. Update `decode_row`/`SymqgRow` to expose block32 codes + per-neighbour `(ord,nr,w)` + `neighbour_u(v)` reconstruction (D5 scalar fallback).
3. Bump `SYMQG_VERSION`=3; version-gate with REINDEX message.
4. Update the pure `#[test]` codec (round-trip block32 → recover the same sign bits; degree>32 multi-block).

#### TDD
```
RED:  symqg_row_block32_round_trips() — pack_row(block32) then decode recovers the SAME per-neighbour sign bits + nr/w + ordinals (sentinel-skipped). MUST fail before the layout change.
RED:  symqg_row_block32_degree64_multiblock() [EC-3] — degree=64 packs 2 block32 blocks; decode recovers all 64 neighbours' sign bits + neighbour_u(v) matches for v in {0,31,32,63}.
RED:  symqg_row_neighbour_u_reconstructs() [D5] — neighbour_u(v) un-transposes the block32 back to the exact ±1 vector the neighbour was encoded with.
RED:  symqg_v3_version_gate() — decoding a v2 meta magic/version yields a typed REINDEX Err.
RED:  symqg_row_bytes_unchanged() — row_bytes(128,32) == 1408 AND row_bytes(128,64) == 2·(block bytes) (the contiguous-packing invariant holds for multi-block).
GREEN: implement block32 pack/decode (multi-block) + neighbour_u + version bump.
REFACTOR: None expected (keep the codec flat).
VERIFY: standalone `rustc --test` on the pure codec (T1.1 pattern — no pgrx link needed).
```

#### Concurrency tests

(none — single-threaded)
Build-time packing + read-time decode are single-threaded per relation (CREATE INDEX / scan hop).

#### Acceptance Criteria
- [ ] block32 round-trip + version-gate + `row_bytes` invariant tests pass (standalone rustc).
- [ ] `row_bytes(128,32)` unchanged at 1408.
- [ ] Pass: size — `page/symqg.rs` ≤ 500 lines (currently 359).
- [ ] Pass: lint — `cargo clippy` clean on `page/symqg.rs`.

#### DoD
- [ ] Codec round-trip green (standalone rustc); version gate fails loud on v2.
- [ ] `page/symqg.rs` ≤ 500 LoC; clippy clean.

### T2.2 — Wire FastScan estimate + copy-free reads into `gather_symqg_candidates`

#### Objective
Replace the per-neighbour `estimate_sign` loop in the beam search with `build_sign_lut16` (once per hop) + `sign_estimate_block` over the row's block32 codes, and read rows copy-free via `with_page_item`.

#### Why this step (action + reasoning)

**What this step does:** In `scan.rs::gather_symqg_candidates`, per popped vertex: build `q_r`, `qc2`, then `build_sign_lut16(&q_r)` once, `sign_estimate_block(lut, row.block32_codes, n, nr[], w[], qc2)` → 32 estimates in one SIMD pass; admit to the beam as today. Read the row through `with_page_item` (borrow, no `to_vec`).

**Why it is necessary now:** This is the production caller that realizes the Phase-1 kernel's win (D1) — the wiring triad pillar (a). Copy-free reads (`with_page_item`, `mod.rs:886`) remove the per-hop `Vec` alloc the first-cut scan pays (`docs/benchmarks/e2-symqg-inpg-verdict.md` "first-cut symqg scan"). The estimate ORDER and sqrt-L2 scale MUST be preserved (E1 lesson) so recall is unchanged modulo the requant (D3).

#### Evidence
`scan.rs:277` (`gather_symqg_candidates`), `scan.rs:350` (the `estimate_sign` loop being replaced), `scan.rs:313` (`read_row` closure), `mod.rs:886` (`with_page_item`).

#### Files to edit
```
theodb_rs/src/am/scan.rs — gather_symqg_candidates: build_sign_lut16 per hop + sign_estimate_block; with_page_item copy-free row read; keep the sqrt-L2 recorded-distance scale
```

#### Deep file dependency analysis
- `scan.rs` (Baseline row 4): `gather_symqg_candidates` today calls `estimate_sign` 32×/hop via `read_row` (which `to_vec`s the row). This task swaps the estimate to the batched kernel and the read to `with_page_item`. The popped-vertex EXACT distance (`qc2.sqrt()`, `scan.rs:345`) and the pending-region scoring stay identical (recall-neutral).
- Callers: `amrescan` dispatch arm (`scan.rs:206`) — unchanged (still calls `scan_symqg_structured`).

#### Deep Dives
- **Eligibility dispatch (D5):** at scan start compute `fastscan_ok = dim%4==0 && dim/4<=258`. If eligible: per hop `lut = build_sign_lut16(&q_r)`, then loop `⌈degree/32⌉` block32 chunks calling `sign_estimate_block` (n = real neighbours in the chunk). If ineligible (e.g. 1536-dim): per hop reconstruct each neighbour's `u` via `row.neighbour_u(v)` and call the scalar `estimate_sign` (correct, un-accelerated). The dispatch is decided ONCE (dim/degree known at scan start), not per hop.
- **Per-hop flow (eligible):** pop `(est_p, p)` → `row = with_page_item(rel, block(p), off, nblocks, |bytes| decode_row(bytes))` → `q_r = rot_q − row.rot`, `qc2 = ‖q_r‖²` → record `(tids[p], qc2.sqrt())` → `lut = build_sign_lut16(&q_r)` → for chunk in 0..⌈degree/32⌉: `sign_estimate_block(&lut, &row.block32_codes[chunk], n_real_in_chunk, nr, w, qc2, &mut out)` → admit each real neighbour's estimate to the beam.
- **Invariants:** estimate ORDER identical to scalar (same admit rule); recorded distances on the sqrt-L2 scale (E1); `visited` dedup unchanged; real neighbours occupy lanes `0..n_real` (sentinels padded last), so `n = n_real` reads the right lanes.
- **Edge cases:** vertex with 0 real neighbours (all sentinel) → `n_real=0`, skip `sign_estimate_block`, no admits, no panic (EC-5); `w==0`/`nr==0` neighbour → `sign_estimate_block`'s scalar branch (parity); ineligible dim → scalar `estimate_sign` from `neighbour_u` (EC-1/2).

#### Tasks
1. Compute `fastscan_ok` once (D5); branch the hop between FastScan and scalar `estimate_sign`.
2. Eligible path: `build_sign_lut16` once/hop + loop `⌈degree/32⌉` chunks of `sign_estimate_block`; admit real neighbours.
3. Ineligible path: reconstruct `u` via `row.neighbour_u(v)` + scalar `estimate_sign` (unchanged semantics).
4. Switch the row read to `with_page_item` (copy-free) on both paths.

#### TDD
```
RED:  symqg_scan_recall_preserved_inpg() — in-PG #[pg_test], 128-dim distinct corpus: FastScan scan returns the SAME top-k as the v2 scalar scan (recall unchanged) within the requant tolerance. MUST fail before wiring.
RED:  symqg_scan_all_sentinel_block_no_admit() [EC-5] — a vertex whose neighbours are all sentinel (n_real=0) yields no admits, no panic (sign_estimate_block not called).
RED:  symqg_scan_ineligible_dim_falls_back() [EC-1/2] — a 130-dim (or a >1032-dim) index takes the scalar path and returns correct top-k (no overflow/panic, recall identical to pre-FastScan).
GREEN: wire the D5 dispatch + FastScan chunk loop + scalar fallback + with_page_item.
REFACTOR: None expected.
VERIFY: cargo pgrx test (droplet) symqg_scan_* ; + the psql PG-as-theo integration smoke (CREATE INDEX on vector(128) AND vector(130) + ORDER BY <->).
```

#### Concurrency tests

(none — single-threaded)
The scan holds SHARE locks per page (`with_page_item` RAII pin); one beam search per backend, no shared mutable state introduced.

#### Acceptance Criteria
- [ ] FastScan scan top-k set-equals the v2 scalar scan on a 5k distinct corpus — asserted by `symqg_scan_recall_preserved_inpg` #[pg_test] (recall delta ≤ 0.5 pp).
- [ ] Pillar (a): `grep -c with_page_item theodb_rs/src/am/scan.rs` ≥ 1 AND `python3 skills/implement/scripts/check_wiring.py --symbol sign_estimate_block` reports PASS.
- [ ] Pass: size — `scan.rs` stays ≤ its budget (currently 1254 — pre-existing; the diff must not grow it materially, add a helper if needed).
- [ ] Pass: lint — clippy clean on the changed `scan.rs` region.

#### DoD
- [ ] `cargo pgrx test symqg_scan_` exits 0 (droplet); the psql smoke `SELECT id FROM t ORDER BY e <-> q LIMIT 10` returns the 10 exact-nearest ids.
- [ ] `cargo clippy` zero warnings AND `check_wiring.py --symbol build_sign_lut16` PASS (no dead new symbol).

---

## Phase 3: Integration Validation — in-PG SIFT1M A/B (MANDATORY)

**Objective:** Re-measure the Goal metric on SIFT1M and record the honest verdict.

### T3.1 — Re-run the SIFT1M in-PG A/B + write the verdict

#### Objective
Rebuild `theodb_symqg` (v3 FastScan) on a droplet, run `benchmarks/e2_symqg_inpg.py`, and record QPS + recall@10 vs `theodb_hnsw`.

#### Why this step (action + reasoning)

**What this step does:** Provision a droplet, `cargo pgrx install --release`, run the existing A/B harness (N=1M, degree=32), collect the ef sweep for both AMs, compare at matched recall, write `docs/benchmarks/e2-symqg-fastscan-verdict.{md,json}`.

**Why it is necessary now:** The Goal metric IS this A/B (D4). Measurement-first: no QPS claim without the artifact (`public-copy.md`). Recall parity is the D3 hard gate.

#### Evidence
`benchmarks/e2_symqg_inpg.py` (the harness), `docs/benchmarks/e2-symqg-inpg-verdict.md` (the v2 baseline: symqg 73 vs hnsw 287 @ recall 0.95).

#### Files to edit
```
docs/benchmarks/e2-symqg-fastscan-verdict.md (NEW) — measured QPS/recall verdict
docs/benchmarks/e2-symqg-fastscan-verdict.json (NEW) — raw sweep
CHANGELOG.md — [Unreleased] entry for the FastScan kernel + verdict
```

#### Deep file dependency analysis
- `benchmarks/e2_symqg_inpg.py` (Baseline row 6): reused unchanged. The verdict docs are new artifacts (no callers).

#### Deep Dives
- **Method:** same as v2 (2GB shared_buffers, best-of-3 warm, 200 queries, official GT). Report v2 scalar vs v3 FastScan side-by-side + hnsw.
- **Decision rule:** recall within 1.5 pp of v2 scalar (D3, hard) → else FAIL (loop to Phase 1/2). QPS ≥ 1.5× hnsw @ recall 0.95 → gate MET; else honest-negative (D4).

#### Tasks
1. Provision droplet, rsync crate, `cargo pgrx install --release`.
2. Run the A/B (N=1M, degree=32); collect E2AB_* lines.
3. Write the verdict `.md`/`.json`; update CHANGELOG; destroy droplet.

#### TDD
```
RED:  (measurement task — no unit test; the A/B harness IS the assertion). The recall gate is the RED: if v3 recall regresses > 1.5 pp vs v2, the slice FAILs and loops back.
GREEN: verdict artifact written with real measured numbers.
REFACTOR: None.
VERIFY: benchmarks/e2_symqg_inpg.py emits E2AB_DONE; recall within tolerance; verdict cites real numbers.
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `benchmarks/e2_symqg_inpg.py` emits `E2AB_DONE` with ≥ 5 `E2AB_RESULT` rows each for v3 symqg + hnsw (real measured QPS + recall@10).
- [ ] v3 recall@10 within 1.5 pp of v2 scalar at matched ef (D3 hard gate).
- [ ] The verdict `.md` states the measured symqg/hnsw QPS ratio at recall@10 ≥ 0.95 as a number (e.g. `X.Yx`), sourced from the `.json` sweep; no ratio appears without the sweep row.
- [ ] CHANGELOG `[Unreleased]` updated (Rule 6).

#### DoD
- [ ] `git show --stat HEAD` lists `docs/benchmarks/e2-symqg-fastscan-verdict.{md,json}`; `doctl compute droplet list` shows no symqg droplet remaining.
- [ ] CHANGELOG updated.

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Batched 1-bit sign estimate reusing `ah_score_block` (not a new kernel) | T1.1 | `build_sign_lut16` + `sign_estimate_block` reuse the tested LUT16-pshufb kernel |
| 2 | FastScan estimate correctness vs scalar `estimate_sign` | T1.1 | parity `#[pg_test]` within requant tolerance |
| 3 | block32-nibble sign-code layout at build time (v3), `row_bytes` unchanged | T2.1 | `pack_row`/`decode_row` block32; `SYMQG_VERSION`=3; row_bytes-invariant test |
| 4 | Wire the kernel + copy-free reads into the beam search | T2.2 | `gather_symqg_candidates` uses `build_sign_lut16`+`sign_estimate_block`+`with_page_item` |
| 5 | Recall parity (int8 requant gate) | T1.1 (unit) + T2.2 (in-PG) + T3.1 (A/B) | tolerance test + in-PG top-k match + A/B recall within 1.5 pp |
| 6 | Measured QPS vs hnsw (Goal metric) | T3.1 | SIFT1M A/B verdict, honest-negative accepted |
| 7 | REINDEX gate for the v2→v3 format bump | T2.1 | version-gate fails loud with REINDEX message |
| 8 | High-dim overflow (1536-dim OpenAI, `dim/4>258`) → scalar fallback (EC-1) | T2.2 (D5), T1.1 | eligibility dispatch + `symqg_scan_ineligible_dim_falls_back` + `debug_assert!(m<=258)` |
| 9 | `dim % 4 != 0` → scalar fallback (EC-2) | T2.2 (D5) | eligibility dispatch; codec still round-trips raw sign bits |
| 10 | `degree_bound > 32` → `⌈degree/32⌉` block32 chunks, no panic (EC-3) | T2.1, T2.2 | multi-block layout + chunk loop + `symqg_row_block32_degree64_multiblock` |

**Coverage: 10/10 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed.
- [ ] All tests passing — `cargo pgrx test` (droplet, `#[pg_test]`) + standalone `rustc --test` (pure codec/LUT) green.
- [ ] Zero clippy warnings on changed files — `cargo clippy`.
- [ ] File-size budget respected — `vec/ah.rs` ≤ 500, `page/symqg.rs` ≤ 500 (`architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6).
- [ ] Backward compatibility: v2 symqg indexes fail loud with a REINDEX message (no silent misread).
- [ ] Plan-specific: recall@10 within 1.5 pp of the v2 scalar scan (D3 hard gate); QPS vs hnsw measured + recorded honestly (D4).
- [ ] Runtime-metric proof — the A/B QPS/recall numbers are observed from a real 1M in-PG run, not compiled-only.
- [ ] Plan archived to `knowledge-base/plans/completed/` after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (when I/O external)

```
(none — no external I/O touched)
```
The change is pure in-process kernel + index page format + a benchmark. The in-PG scan reads pages through the existing WAL/buffer path (unchanged); no HTTP/DB-driver/queue/socket added.

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate the FastScan kernel works in the real in-PG scan + measure the Goal metric.

### Execution
```
cargo pgrx test                 # (droplet) #[pg_test] — sign_* kernel + symqg_scan_* recall parity
rustc --test (pure codec/LUT)   # droplet-free fast check of the LUT math + block32 codec
cargo clippy                    # zero warnings on changed files
benchmarks/e2_symqg_inpg.py     # (droplet) SIFT1M A/B — QPS + recall@10, the Goal metric
```

### Acceptance Criteria
- [ ] All `#[pg_test]` + standalone tests green.
- [ ] Zero clippy warnings on changed files.
- [ ] Recall@10 within 1.5 pp of the v2 scalar scan (D3).
- [ ] A/B completed (E2AB_DONE); QPS vs hnsw recorded in the verdict.
- [ ] Runtime-metric proof — the A/B numbers come from a real 1M run.
- [ ] Failure scenarios green — N/A (none — no external I/O touched).

### If Validation Fails
1. A recall regression > 1.5 pp → the int8 requant (D3) is too lossy → loop to Phase 1 (revisit the LUT precision / affine range) BEFORE claiming the slice done.
2. QPS not improved → the LUT-build-per-hop overhead (Q2/drawback) ate the win → profile; either amortize the LUT or accept the honest-negative (D4).
3. Pre-existing failures (unrelated AMs) logged, not blocking.
