# Implementation Summary: m22-own-quantization

**Date:** 2026-06-30 · **Plan:** `.claude/knowledge-base/plans/m22-own-quantization-plan.md` (SHIPPABLE 96.8)
**Verdict:** IMPLEMENTATION_COMPLETE · **Branch:** develop · **Scope:** SQL-callable measurement-first (M21 precedent)

## What shipped

TheoDB's own **SBQ scalar quantizer** + quantized ANN search in Rust (`theodb.sbq_knn`, `theodb.sbq_bytes_per_vector`),
at **recall@k parity with pgvectorscale SBQ AND a memory profile** (bytes/vector), proven by a reproducible benchmark.

| Task | Deliverable | Commit |
|---|---|---|
| T1.1 | SBQ quantizer (`theodb_rs/src/sbq.rs`: train mean threshold, 1-bit + n-bit quantize, u64 pack, Hamming, bytes_per_vector) | `466d3a5` |
| T2.1 | `sbq::knn` (IVFFlat carrier + Hamming + f32 rerank) + SQL externs + extension_sql + REVOKE; `IvfflatIndex::candidate_positions` (additive M21) | `466d3a5` |
| T2.2 | Container integration tests (`benchmarks/tests/test_sbq_index.py`) | `61cf919` |
| T3.1 | Recall+memory parity benchmark + doc (`benchmarks/bench_sbq_index.py`, `docs/benchmarks/m22-sbq-parity.md`) | `61cf919` |

## Gates (all green)

- **Rust compiles** — `cargo pgrx install --release --features pg17` ✓
- **Lint** — `cargo clippy --release --features pg17 -- -D warnings` CLEAN (fixed 5 `manual_range_contains`) ✓
- **Algorithm unit proof** — standalone prototype 6/6 (`rustc --test`): SBQ recall@10 = 0.86 at 1-bit+over_fetch=8 (dim 16); `#[pg_test]` mod locks the contract ✓
- **Container integration** — `pytest benchmarks/tests/test_sbq_index.py` → **10 passed** (recall with rerank, parity gate, bytes/vector compression, 22023 negatives, empty queries, REVOKE incl. private extern) ✓
- **Benchmark** — `bench_sbq_index.py` → **PARITY_REACHED**: memory own=pgvectorscale (8 bytes/vec at dim 32, 16× vs f32); recall pgvectorscale diskann SBQ=0.6278, own 0.625→0.855 across the over_fetch/probes sweep (mean±std, 3 runs) ✓
- **CHANGELOG** `[Unreleased] § Added` updated ✓ · **No new dependency** (deps-audit PASS_WITH_CAVEATS; RaBitQ AGPL **avoided**) ✓

## Honest notes (Rule 3)

- **Memory is PARITY with pgvectorscale, not a win over it** (EC-1): own bytes/vector `ceil(dim·bits/64)·8` is the
  identical formula → memory parity + ~16-32× vs f32. The substantive differentiator is **recall with rerank**.
- **License (D1):** vectorchord/RaBitQ is **AGPL-3.0** → study-only, NOT borrowed. The own SBQ is permissive
  std-only code (zero deps). This is the headline dep finding of the milestone.
- **Scope:** SQL-callable; the candidate-gen reuses the M21 IVFFlat f32 carrier + reads f32 for rerank, so this
  scope reads f32 at search time — the STORAGE memory metric (codes) is the honest win; the runtime memory win
  (search touching only codes) requires the on-disk AM = **M22b**.
- **#[pg_test] not run in CI** (M18-M21 limitation) — Python container suite + prototype are the always-on proofs.
- **Coexistence:** pgvectorscale `diskann`/SBQ, pgvector, M21 `ann/`, embed/hybrid/import untouched.
