# Review — vector-recall-benchmark-harness

**Date:** 2026-06-27 · **Slug:** vector-recall-benchmark-harness · **Verdict:** **READY_TO_MERGE**
**Commits reviewed:** `84aead5`…`caa656f` (feat + evidence + review-fixes) on `develop`.
**Method:** 4 parallel specialist agents (architecture, test-auditor, cross-validation, domain/ANN-correctness) read the real code + plan + evidence.

## Verdict rationale

`READY_TO_MERGE`: **0 BLOCKER**, **2 HIGH — both fixed** (not merely mitigated), MEDIUMs addressed, LOW/INFO accepted with rationale. Hard gates clear: tests green (42/42), no secrets, on `develop` (not `main`), no `Co-Authored-By` trailer, CHANGELOG updated.

## Hard gates

| Gate | Status |
|---|---|
| Tests green on branch | ✅ 42/42 (unit + integration vs real container) |
| No new secrets | ✅ (the `postgres/postgres` default is the documented M0 smoke password) |
| Not committed to `main` | ✅ `develop` |
| No `Co-Authored-By` | ✅ |
| CHANGELOG updated | ✅ |

## Severity matrix (consolidated)

| Sev | Finding | Resolution |
|---|---|---|
| BLOCKER | — | none |
| HIGH | CHANGELOG cited stale numbers contradicting the committed JSON (Rule 5/6) | **FIXED** — CHANGELOG now cites recall ~0.84/~0.96 + points to the sha-stamped JSON as exact source |
| HIGH | recall distance-threshold/eps semantic was prose-only (tie test == perfect-match test) | **FIXED** — added `test_recall_counts_by_distance_not_identity` + `_within_eps_counts_as_hit` + `_outside_eps_is_miss` |
| MEDIUM | float64 oracle vs pgvector float4 storage (comparative-claim precision) | **FIXED** — `recall.py` rounds GT to float32 to match the SUT |
| MEDIUM | error-typing contract half-applied (raw psycopg2.Error leaked) | **FIXED** — `db._cursor` translates operational errors → `DBUnavailableError`; docstring corrected |
| MEDIUM | percentiles pooled cold+warm while QPS used warm best-of-N | **FIXED** — untimed warmup run; report self-describes methodology |
| MEDIUM | report not self-describing (no n_queries/host/methodology) | **FIXED** — added to JSON+MD |
| MEDIUM | harness recall test tautological (all-hit fake only) | **FIXED** — added `MissVectorDB` (recall 0.0) + report round-trip test |
| MEDIUM | `ip` metric half-wired (latent inverted-threshold trap) | **FIXED** — removed `ip` from `_OPS` until recall handles the `<#>` sign convention |
| MEDIUM | n_queries<1 guard untested | **FIXED** — `test_dataset_zero_queries_raises` |
| LOW | IVFFlat capability not yet exercised (only HNSW run) | **ACCEPTED** — harness is index-agnostic; IVFFlat/pgvectorscale are explicit future slices (honest scope in summary) |
| LOW | identifier interpolation unquoted (operator-controlled config, not injection) | **ACCEPTED** — defense-in-depth note; not a vuln for a benchmark tool |
| LOW | cosine zero-vector guard absent | **ACCEPTED** — unreachable from seeded gaussian; tracked for the future real-dataset slice |
| LOW | private helpers tested directly; integration "and"-smell | **ACCEPTED** — pragmatic at this size |
| INFO | single-thread QPS (= inverse-latency, not saturated throughput) | **ACCEPTED** — matches ANN-Benchmarks `batch=False` protocol; report says "best-of-N (warm)" |

## Domain correctness (the load-bearing axis) — verified CORRECT

The domain specialist traced the full path and confirmed (no inflation bug): recall@k is genuinely distance-thresholded (not id-overlap); ground-truth is exact and now in the same float32 unit the DB returns; query↔ground-truth ordering aligned; `assert_index_used` + `SET enable_seqscan=off` ensures the index (not seqscan) is measured; QPS/latency measured correctly client-side.

## Evidence (the gate artifact)

`docs/benchmarks/2026-06-27-pgvector-l2.json` (sha `651bf65`): HNSW recall@10 ≈ 0.84 (ef_search=40) / ≈ 0.96 (ef_search=100), QPS ~1.7k–3.7k, seed=42 n=5000 dim=128 — the recall×QPS tradeoff, **measured and reproducible**, no `UNBENCHMARKED`.

## Remaining follow-ups (non-blocking, next slices)

- Exercise IVFFlat + pgvectorscale StreamingDiskANN once the image carries them.
- Mirror a real ANN-Benchmarks dataset (sift-128) locally for cross-system comparison.
- Optionally pin `max_parallel_maintenance_workers=0` for bit-deterministic recall.
