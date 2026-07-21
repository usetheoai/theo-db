# ClickBench entry for TheoDB (theodb_columnar) — M128

TheoDB's ClickBench per-database entry (adopt-and-wrap, ADR-0050). Runs the 43 ClickBench queries over a
`theodb_columnar` table + a byte-identical result A/B vs heap (the correctness oracle ClickBench lacks).

## Provenance & licence (D1)

`create.sql` and `queries.sql` come from ClickBench (github.com/ClickHouse/ClickBench, **CC-BY-NC-SA**). They are
**fetched at runtime** by `run_m128_clickbench.py` (`ensure_entry_sql`), **NOT vendored** into this Apache-2.0 tree
(the D1 permissive gate bars CC-BY-NC-SA from the distributed artifact) — `create.sql`/`queries.sql`/`results.json`
are git-ignored. The `hits` dataset (also CC-BY-NC-SA) is likewise streamed CI-only, never vendored.

Only `benchmark.sh` + `template.json` + this README are ours (Apache-2.0). `create.sql` is auto-adapted to
`USING theodb_columnar` on fetch.

## Run

```
PYTHONPATH=benchmarks python3 benchmarks/run_m128_clickbench.py --n 1000
```

Measured evidence: `docs/benchmarks/m128-clickbench-columnar.md`. The vectorized-agg CustomScan pushdown (`--agg`)
is gated on issue #135 (planner hang on the wide real hits table); the default measured path is columnar storage.
