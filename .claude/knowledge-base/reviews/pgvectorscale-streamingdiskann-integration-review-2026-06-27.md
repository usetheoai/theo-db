# Review — pgvectorscale StreamingDiskANN integration (M2 DoD-2)

**Date:** 2026-06-27
**Verdict:** READY_TO_MERGE
**Slug:** pgvectorscale-streamingdiskann-integration
**Scope:** M2 **DoD-2 only** (advanced ANN index available + measured vs HNSW). M2 milestone stays `[ ]` — **DoD-3 (embeddings SQL function) is a separate slice** and must land before the milestone checkbox flips.
**Plan:** `.claude/knowledge-base/plans/pgvectorscale-streamingdiskann-integration-plan.md`
**Commits reviewed:** `0c688f3` (feat), `abf56d3` (review fixes), `8e0a6fb` (evidence re-stamp)
**Evidence:** `docs/benchmarks/2026-06-27-pgvector-cosine.json` (sha `abf56d3`), `docs/decisions/m2-index-decision.md`

## Method

Three independent specialist agents reviewed the committed slice in parallel:
1. **Build/Infra** — multi-stage Dockerfile correctness, supply-chain pins, M0 preservation, D1/D3.
2. **Cross-validation** — plan ↔ implementation ↔ evidence coherence; honest scope.
3. **Domain (ANN/vector benchmarking)** — scientific correctness of the DiskANN measurement and the index decision.

## Severity matrix

| # | Sev | Finding | Status |
|---|---|---|---|
| H-1 | HIGH | `query_rescore` capped at `min(sls,500)` → fabricated 0.916 plateau; top recall (0.971) cited but not in the reproducible artifact (measurement-first / ADR 0002 violation, leaked to CHANGELOG) | **FIXED** `abf56d3`+`8e0a6fb` |
| H-2 | HIGH | (Domain, same root as H-1) rescore cap is an asymmetric bias against DiskANN — its competitive recall region was truncated by the harness, not by DiskANN | **FIXED** `abf56d3` |
| M-1 | MED | Integration diskann recall bound silently weakened from plan's ≥0.90 to ≥0.80 | **FIXED** `abf56d3` (restored ≥0.90; verified green) |
| M-2 | MED | Build: builder base unpinned (digest) while runtime pinned; Rust toolchain floats → non-reproducible artifact | **FIXED** `abf56d3` (shared `ARG BASE_IMAGE` digest; Rust pinned 1.91.0) |
| M-3 | MED | "Dataset artifact" framing omitted the scale axis (n=5000 ≪ DiskANN's billion-scale design envelope) | **FIXED** `abf56d3` (two-axis framing added) |
| M-4 | MED | 0.971 datapoint not reproducible from committed harness | **FIXED** — now in the sweep (`sls=1000`) with measured QPS, stamped at producing sha |
| L-1 | LOW | Plan Goal prose names `*-diskann-*.json`; artifact is `*-pgvector-cosine.json` (binding AC globs `*-pgvector-*.json` — matches) | Advisory — plan prose only; impl correct |
| L-2 | LOW | `which cargo` empty + `smoke PASSED` asserted in prose, not auto-captured in CI | Advisory — verified manually this session; CI assertion is a follow-up |
| L-3 | LOW | `eps=1e-3` recall threshold is metric-scale absolute (fine for cosine; both indexes use same eps → fair) | Advisory — documented |
| LIC | MED | `vectorscale.so` static-links pgvectorscale's Rust crate tree; `Cargo.lock` carries no license fields → AGPL-free not proven | **Tracked** as pre-release D1 gate in decision doc (this is a dev image, not a release) |

**0 BLOCKER. Both HIGH findings RESOLVED (not merely mitigated) with re-verified evidence.**

## What was fixed and how it was verified

- **Root-cause fix (H-1/H-2):** `_diskann_spec` now scales `query_rescore` with `sls` up to pgvectorscale's engine ceiling (`query_rescore` max = 1000), replacing the arbitrary 500 cap. The recall×QPS curve is now honest end-to-end; the 0.971 top point is measured **with QPS** and reproducible (`python -m theodb_bench --index both --metric cosine --seed 42 --n 5000 --dim 128 --k 10`).
- **Honesty hardening (Rule 3 + public-copy §4):** QPS proven load-dependent (~2.5× swing between a quiet run and one under build load). The decision doc now states QPS is **relative** — the stable, load-independent signal is the *shape* (HNSW ~3–4× QPS at equal recall; DiskANN −42% index size, uniquely reaches 0.971). No absolute throughput is published.
- **Reproducible build (M-2):** base image pinned by digest for **both** stages via shared `ARG BASE_IMAGE`; Rust toolchain pinned to 1.91.0; `cargo-pgrx` pinned + `--locked`. (`cargo pgrx install` does not accept `--locked`; crate-tree reproducibility comes from the committed `Cargo.lock` at the pinned commit — documented inline.)
- **Re-verification on the repinned image:** `docker build` EXIT=0 → runtime has **no Rust** (`cargo`/`rustc` absent) → `vectorscale-0.9.0.so` present → `smoke.sh` **SMOKE PASSED** (M0 intact) → `CREATE EXTENSION vectorscale` returns 0.9.0 → **47 tests pass** (incl. diskann integration ≥0.90 against the container) → `ruff` clean → `vulture` clean (exit 0).

## Reviewer-confirmed strengths (do not regress)

- Recall is distance-thresholded on **exact** projected distances (`db.py` re-computes `<=>` on stored vectors), so SBQ's approximate ordering cannot inflate/deflate recall — the single most important correctness choice for a quantized index.
- Ground-truth rounded to float32 to match pgvector `float4`; cosine oracle matches `<=>`; queries held out from the corpus; same metric/k/ground-truth for both indexes (apples-to-apples).
- Multi-stage isolation correct (no Rust toolchain in the 445 MB runtime); D3 honored (pgvectorscale as-is, commit `57c88b7`, no fork).
- Index decision is honestly **deferred** to a realistic dataset — choosing either index from synthetic gaussian would itself violate ADR 0002.

## Cycle-review hard gates

- Tests green on branch ✓ (47 passed) · No new secrets ✓ · On `develop` (not `main`) ✓ · No `Co-Authored-By` trailer ✓ · CHANGELOG `[Unreleased]` updated ✓.

## Verdict rationale

Per `rules/cycle-review.md`: READY_TO_MERGE = no BLOCKER and ≤2 HIGH with documented mitigation. Here the 2 HIGH (one shared root cause) are **fixed and re-verified with reproducible evidence**, the MEDIUMs are resolved or tracked (LIC → pre-release gate), and the LOWs are advisory. The slice is functionally complete and honest.

**Next:** M2 **DoD-3** (embeddings SQL function from a configurable model) is the remaining M2 slice. The M2 ROADMAP checkbox MUST NOT flip until DoD-3 ships. Follow-up tracked in the decision doc: mirror a real ANN-Benchmarks dataset (sift-128 / glove-100) + HDF5 loader to make the *final* index decision, and run the pre-release D1 license sweep over the Rust crate tree.
