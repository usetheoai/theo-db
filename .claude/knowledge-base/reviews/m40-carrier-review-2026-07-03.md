# Review — M40 carrier head-to-head (re-scoped from anisotropic loss)

**Date:** 2026-07-03
**Slug:** m40-carrier
**Milestone:** M40
**Verdict:** READY_TO_MERGE
**Scope:** benchmark-only (harness + honest measurement). No Rust production code changed.

## Why this is benchmark-scoped (re-scope trail)

M40 was requested as "ScaNN anisotropic quantization loss". The discover-phase **ceiling probe**
(`docs/benchmarks/m40-ceiling-probe.md`) falsified the premise before any build: in our IVFFlat→quantized-rank→
exact-f32-rerank pipeline, the recall ceiling is the **carrier** (probes move recall +17 points; the quantizer
choice moves ~0 because the f32 rerank equalizes any quantizer ranking). Building anisotropic loss would have been
sunk cost on the wrong bottleneck (`SEM WORKAROUNDS`). Owner-approved re-scope → carrier head-to-head.

## Findings by dimension

| Dimension | Result | Evidence |
|---|---|---|
| **Functional (harness works)** | PASS | `benchmarks/run_m40_carrier.py` ran clean at n=2k and n=50k against `theo-db:m39`; produces recall×QPS per AM per knob + a matched-QPS verdict. |
| **Test** | PASS | `benchmarks/tests/test_run_m40_carrier.py` (integration) passes: both AMs present, recall∈[0,1], qps>0, verdict token, ≥1 matched row. |
| **Measurement honesty** | PASS | Verdict `THEODB_IVFFLAT_WINS` on synthetic, with an explicit caveat that random-gaussian is the worst case for a graph and the answer needs SIFT1M. No superiority claim (`public-copy.md`). |
| **Statistical rigor** | PASS | 3 runs; recall from exact brute-force GT (`theodb_bench.recall`); QPS wall-clock. `analysis-golden-rule §A1` (≥3 runs). |
| **Lint** | PASS | `ruff check` clean on the new scripts. |
| **CHANGELOG / ROADMAP** | PASS | `[Unreleased] § Changed` documents the re-scope + outcome; ROADMAP M40 `[x]` with the honest verdict + SIFT1M caveat. |
| **Bug caught + fixed in review** | — | Test initially referenced a table name mismatched from the spec helpers' module `_TABLE`; fixed (test aligned to `run_m40_carrier._TABLE`). Re-run green. |

## Hard gates

- Failing tests → none (structure test green). No secrets. On `develop` (not `main`). No `Co-Authored-By`.
  CHANGELOG updated. Benchmark-only → no build/release gate.

## Benchmark requirement (standing directive)

Satisfied: the milestone IS a reproducible benchmark with data (n=50k, 3 runs, exact GT) and an honest verdict.
The measurement points to concrete next work (theodb_hnsw QPS optimization; SIFT1M for the real carrier verdict).

## Verdict rationale

No BLOCKER, no HIGH. The deliverable (a validated, reproducible carrier head-to-head harness + an honest,
caveated measurement) is sound and mergeable. **READY_TO_MERGE.**

## Release recommendation

Benchmark-only (no product code) → fold into the next real release; no dedicated version cut (M32/M33 precedent).
