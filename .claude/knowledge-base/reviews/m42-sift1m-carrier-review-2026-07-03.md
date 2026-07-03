# Review — M42 SIFT1M carrier verdict

**Date:** 2026-07-03
**Slug:** m42-sift1m-carrier
**Milestone:** M42
**Verdict:** READY_TO_MERGE
**Scope:** measurement-only (no new code — reran the existing `run_m32_sift1m.py` harness on `theo-db:m41` + a
Pareto ef-sweep on the built index).

## Findings by dimension

| Dimension | Result | Evidence |
|---|---|---|
| **Functional (harness ran)** | PASS | 4-way over real SIFT1M (1M×128, exact GT) completed; artifact `docs/benchmarks/m32-scale-sift1m.json`. Pareto ef-sweep on the built theodb_hnsw index (id-overlap recall@10). |
| **Measurement honesty** | PASS | The headline (theodb_hnsw ~10× vs theodb_ivfflat) is order-of-magnitude, far beyond variance. The vs-pgvector margin (~1.7–2.8×) is explicitly qualified as "needs mean±std + independent repro before a hard public claim" (`public-copy.md` §4). No unqualified "faster than pgvector" claim shipped. |
| **Caveats surfaced** | PASS | Build time (24 min@1M — the real weakness, M41 optimized scan not build); best-of-N QPS single-machine; 200-query sample. All in the doc + CHANGELOG + ROADMAP. |
| **Inverts prior verdict honestly** | PASS | Explicitly frames this as vindicating the M40 honesty caveat (synthetic favored ivfflat; real structured data favors the graph) — not hiding the reversal. |
| **Statistical basis** | PASS-with-caveat | Exact GT from ANN-Benchmarks neighbors; best-of-3-runs (harness). The 10× vs ivfflat is unambiguous; the vs-pgvector margin is flagged for mean±std confirmation. Honest. |
| **CHANGELOG / ROADMAP** | PASS | `[Unreleased] § Changed` + ROADMAP M42 `[x]` with numbers + caveats + next-work. |

## Hard gates

- No failing tests (measurement, no code). No secrets. On `develop`. No `Co-Authored-By`. CHANGELOG updated.
  Benchmark-only → no build/release gate on code.

## Benchmark requirement (standing directive)

Satisfied at the highest level: real SIFT1M (1M×128), exact ground truth, a full recall×QPS Pareto curve, and an
honest verdict — the trustworthy carrier answer the M40 caveat demanded. This is the strongest vector evidence in
the project to date.

## Verdict rationale

No BLOCKER. A real-data measurement with a strong, honest positive result (the graph carrier wins on structured
data), all comparative claims correctly qualified per `public-copy.md`, caveats surfaced. **READY_TO_MERGE.**

## Release recommendation

Benchmark/docs-only (no product code) → fold into the next real release; no dedicated version cut (M32/M40 precedent).
The actionable next work (theodb_hnsw build-time optimization; mean±std + repro of the pgvector margin) is recorded.
