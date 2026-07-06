# /review — M49 cosine + inner-product opclasses

Date: 2026-07-06 · Slug: `cosine-ip-opclasses` · milestone_id: M49 · Range: `98afb58..HEAD` (after v0.39.0)

## Verdict: READY_TO_MERGE (after review-fix commit `d746389`)

Two domain specialists; both READY_TO_MERGE after fixes.

## Reviewers + findings

**council-rust-pgrx: READY_TO_MERGE** — FFI verified sound against pgrx-pg-sys 0.16.1 signatures + pgvector
precedent: `resolve_metric` (index_getprocid/OidFunctionCall0Coll, RegProcedure==InvalidOid, Datum→u8 tag) all
correct; the 0-arg support procs are fmgr-callable; the fused kernels are pure safe Rust with an OOB-proof length
assert; NaN ordered by total_cmp; all 4 build callbacks under #[pg_guard]. 2 LOW/INFO nits (dead is_l2 plumbing,
stale comments) — non-blocking.

**council-index-storage: NEEDS_FIXES → READY_TO_MERGE** — engineering sound (opclass registration matches ADR-1,
metric-consistency chain closed build↔scan↔vacuum, Design B faithfully raw-store, crash-safe by construction,
partial-read invariant preserved). 5 findings, all resolved:
- **[BLOCKER-1]** crash-safety asserted in the artifact but no committed test → **FIXED**: added
  `test_am_crash.py::test_cosine_crash_safe` (build cosine, SIGKILL, recover, assert top-5 under `<=>` identical).
- **[HIGH-1]** "parity vs pgvector" but only same-metric seqscan measured → **FIXED**: reframed to the EXACT
  seqscan oracle (stronger gate); pgvector head-to-head filed as follow-up.
- **[HIGH-2]** IVF recall gap partly non-spherical k-means, not purely list-probing → **FIXED**: honest caveat +
  spherical-k-means follow-up filed in `backlog.md`.
- **[MEDIUM-1/2]** stale comments (Scored NaN, ADR 0010 L2-only) → **FIXED**.

## Evidence (M49 image theodb:m49-p3)
- Opclass register + pushdown: 4 passed (cosine `<=>` / ip `<#>`, both AMs, EXPLAIN Index Scan).
- Metric resolution (ADR-1): 2 passed (cosine order ≠ L2, both AMs).
- Fused zero-alloc kernels + L2 regression: cosine/ip 6 + L2 (ann_index+index_am) 34 = 40 passed.
- Recall@10 vs exact same-metric seqscan oracle: HNSW cosine/ip = 1.0000; IVF cosine/ip = 0.89/0.83 (all ≥ 0.80).
- Crash-safety: `test_cosine_crash_safe` 1 passed (cosine top-5 identical pre/post SIGKILL — metric preserved).

## Hard gates
Failing tests: NONE. No secrets; no main commit; no Co-Authored-By; CHANGELOG updated per phase.

## Tracked follow-ups (not blockers)
IVF spherical k-means (backlog.md); AVX2 for IP/cosine kernels (backlog.md); pgvector recall head-to-head.

**Verdict:** READY_TO_MERGE
