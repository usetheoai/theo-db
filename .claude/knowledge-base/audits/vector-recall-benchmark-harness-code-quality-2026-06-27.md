# Code-Quality Audit — vector-recall-benchmark-harness

**Date:** 2026-06-27 · **Slug:** vector-recall-benchmark-harness · **Commit:** `84aead5`
**Verdict:** **PASS**

## Scope

`benchmarks/theodb_bench/` (Python package, 7 modules) + `benchmarks/tests/` (6 test files, 42 tests). Re-audited after cycle-review fixes.

## Detectors run (real, this audit)

| Detector | Tool | Result |
|---|---|---|
| Dead code (D1) | `vulture benchmarks/theodb_bench --min-confidence 80` | **clean** (exit 0, zero findings) |
| Lint + unused imports/vars | `ruff check benchmarks/` | **All checks passed** |
| Symbol fabrication (D2) | tests import + exercise every public symbol; real CLI run executes the full path | **none** — any fabricated symbol would fail at import (36/36 pass) + the real benchmark ran end-to-end |
| Cross-package wiring (D3) | manual | every export wired: `recall`/`dataset`/`metrics`/`db` ← `harness` ← `__main__` (CLI); tests exercise each |
| File size | `wc -l` | every module ≤ 500 (max 123 `db.py`) |
| Coverage | `pytest --cov` | **98%** total; `recall.py`/`metrics.py` (critical paths) **100%** |

## Wiring triad (the harness feature)

- **Caller:** `theodb_bench/__main__.py` `main()` — the CLI exercises dataset→ground-truth→build→query→recall→report end-to-end.
- **Integration test:** `tests/test_integration.py` — runs against the real `theo-db:dev` container (marker `integration`).
- **Runtime metric / observable:** the JSON+markdown report at `docs/benchmarks/2026-06-27-pgvector-l2.json` with measured `recall_at_k` ∈ [0,1] and `qps > 0` — the gate artifact (ADR 0002).

## Notes

- `code-quality-languages.txt` has Python not formally enabled and the skill auto-detects manifests at repo root only (the harness `pyproject.toml` is under `benchmarks/`), so the standalone `/code-quality` auto-run is a NOOP for this package — the equivalent detectors (vulture/ruff) were run directly and recorded above.
- `psycopg2` (LGPL) is dev/CI-only and not in the distributed image (D1 distribution constraint N/A; see plan § Dependencies).
