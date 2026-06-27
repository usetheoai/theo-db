# Review — M2 Vetorial / IA (todos os 4 DoDs)

**Date:** 2026-06-27
**Verdict:** READY_TO_MERGE
**Scope:** M2 milestone — all four DoDs. (ROADMAP M2 checkbox stays `[ ]`; the flip is the post-merge release step.)
**Commits:** `0c688f3` → `ba98af3` (9 commits on `develop`, unpushed).

## DoD status (evidence-backed)

| DoD | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | Harness recall@k/latency/QPS/build/memory, reproducible, **in CI**, over **ANN-Benchmarks reference datasets**, published | ✅ (CI wired; first live run on push) | `docs/benchmarks/2026-06-27-glove-25-angular.json` (real glove, sha `c421550`); `.github/workflows/ci.yml` |
| 2 | ANN index beyond HNSW available, **chosen by evidence** | ✅ | DiskANN available + `docs/decisions/m2-index-decision.md` (HNSW default on real-data evidence) |
| 3 | SQL function to generate embeddings from a configurable model (local/remote) | ✅ | `sql/30-theodb-embed.sql` + `tools/embedding_server.py`; 11 integration tests (real model) |
| 4 | Fork Policy (D3) honored | ✅ | pgvectorscale as-is `57c88b7`, no fork |

## Method

Two review rounds, three independent specialist agents each (security/correctness, cross-validation, test-auditor) — round 1 on the DoD-2 slice (separate report `pgvectorscale-streamingdiskann-integration-review-2026-06-27.md`), round 2 on the DoD-1-real-dataset + DoD-3 slices (this report).

## Severity matrix (round 2 — DoD-1 real dataset + DoD-3)

| # | Sev | Finding | Status |
|---|---|---|---|
| H1 | HIGH | `theodb.embed` is a server-side SSRF primitive granted to PUBLIC, unhardened | **FIXED** `ba98af3` — REVOKE FROM PUBLIC (verified `has_function_privilege(public)=f`), http(s)-scheme check, no-redirect opener, threat-model COMMENT |
| H-1 | HIGH | DoD-1 "in CI" wired but never executed (all commits unpushed) | **Documented mitigation** — first push (the release step) triggers CI; M2 checkbox MUST NOT flip until that run is green. Honestly disclosed in CHANGELOG + decision doc. |
| M1 | MED | `except urllib.error.URLError` missed timeout (`OSError`) + non-JSON 200 (`ValueError`) → untyped traceback | **FIXED** `ba98af3` — broadened except; sqlstate `38000` for endpoint failures, `22023` for config |
| M2 | MED | API key in session GUC (log/SHOW leak) | **Documented** caveat in function COMMENT |
| M3 | MED | Synchronous one-HTTP-per-row blocking | **Documented** caveat in COMMENT + `docs/sql-embeddings.md` |
| M4/T-M1/M2 | MED | Endpoint failure paths untested (timeout/unreachable/5xx/malformed/empty/NULL content) | **FIXED** `ba98af3` — 6 new negative tests (unreachable, bad scheme, NULL content, empty/malformed/non-JSON response) |
| M-1 | MED | CI benchmark gate is capped HNSW-only; decision-grade DiskANN sweep is out-of-CI | **Accepted + documented** — CI proves the harness runs + HNSW recall high; the DiskANN decision sweep is committed, re-runnable by the documented command |
| M-2 | MED | DoD-3 CI test has a runtime model-download dependency (HuggingFace) | **Accepted** — `fastembed` in requirements; first CI run validates; fl/cache risk disclosed |
| L1 | LOW | `load_hdf5_subsample` lacked lower-bound validation | **FIXED** `ba98af3` — `n>=1`/`n_queries>=1` guard + test |
| L-1 | LOW | "equal/surpass ScaNN" aim unproven | **Honest `UNBENCHMARKED`** — needs Cohere-768 at scale (future); HNSW chosen because it dominates both measured datasets |
| L-2/LIC | LOW | D1 license sweep over the Rust crate tree | **Tracked** as pre-release gate (PRD §11) in decision doc — dev image, not a release |

**0 BLOCKER. Both HIGH resolved (H1 fixed with verification; H-1 documented-mitigated, closes at release). MEDIUMs fixed or tracked. LOWs advisory.**

## Final verification (hardened, rebuilt image)

- `docker build` EXIT=0 → image **469 MB** (+24 MB plpython3, no torch) → runtime has no Rust.
- `smoke.sh` → **SMOKE PASSED** (M0 pgvector intact).
- `vectorscale 0.9.0` loads; `theodb.embed/2` baked via initdb; `has_function_privilege(public, …)=f` (REVOKE effective).
- **70 tests pass** (unit + all integration: HNSW/DiskANN + embed real-model + 7 negative cases).
- `ruff` + `vulture` clean.
- Real evidence (no mock): glove-25 HNSW dominates DiskANN on every axis (recall 0.996 vs 0.933, QPS ~20×, build ~11×, index 20.55 < 22.77 MB); embed returns real 384-d vectors with genuine semantics (paraphrase `<=>` < unrelated).

## Reviewer-confirmed strengths

- Distance-thresholded recall on exact projected distances (quantization can't game recall); float32-matched ground-truth; held-out queries.
- Anti-mock-theater: `MissVectorDB` proves recall is propagated; embed semantic assertion would fail on a hash/mock.
- AlloyDB-anchored embed design (DB calls a configurable endpoint; lean image); honest about every gap.

## Cycle-review hard gates

Tests green ✓ · No new secrets ✓ · On `develop` ✓ · No `Co-Authored-By` ✓ · CHANGELOG updated ✓.

## Verdict rationale

Per `rules/cycle-review.md`: READY_TO_MERGE = no BLOCKER and ≤2 HIGH with documented mitigation. Both HIGH are resolved (one fixed+verified, one documented-mitigated and structurally closed by the release push). All four M2 DoDs are functionally complete with real, reproducible evidence.

**Before the M2 checkbox flips (release step):** (1) push `develop` and confirm the first CI run is green (closes H-1); (2) run the D1 `cargo-deny`/`loop-check-licence` sweep over the Rust crate tree (PRD §11). Both are explicitly tracked.
