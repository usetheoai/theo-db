#!/usr/bin/env bash
# M128 — ClickBench entry runner for TheoDB (thin wrapper; the real driver is benchmarks/run_m128_clickbench.py).
# Follows the ClickBench per-db contract: create.sql + queries.sql + a results.json of raw [t1,t2,t3] triples.
# Honest scope: subsampled hits on a self-hosted box (NOT canonical c6a) — see docs/benchmarks/m128-clickbench-columnar.md.
set -euo pipefail
PYTHONPATH="${PYTHONPATH:-benchmarks}" python3 benchmarks/run_m128_clickbench.py "$@"
