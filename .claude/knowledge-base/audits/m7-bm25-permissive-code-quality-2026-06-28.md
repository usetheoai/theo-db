# Code-Quality Audit — m7-bm25-permissive

**Date:** 2026-06-28
**Verdict:** PASS (0 hard caps, 0 soft caps)

`run_code_quality.py m7-bm25-permissive` → PASS. Independently verified on the changed surface:
- vulture (dead code) clean; ruff clean on `theodb_bench` + `tests/test_bm25.py`.
- Symbol resolution: imports resolve — proven by 69 unit + BM25 integration tests passing.
- Shell: `bash -n packaging/license-sweep.sh` clean.

## Wiring triad
- `VectorDB.bm25_query`/`create_bm25_index`/`ensure_bm25_extension`/`pg_textsearch_available` — callers: `run_three_retrievers(include_bm25)` + `test_bm25.py`; observable: measured nDCG@10/Recall@100 + skip-path.
- `license-sweep.sh` § (c) — caller: manual + `bm25-measure` CI job; observable: pg_textsearch=permissive / VectorChord-bm25=AGPL verdict lines.
- `packaging/Dockerfile.bm25` — caller: `bm25-measure` CI job; observable: build + BM25 measurement.

PASS → proceed to /review.
