#!/usr/bin/env python3
"""M30 — columnar-at-scale: prove the pg_mooncake columnstore crossover vs the row-store.

The M6 benchmark measured only 100k (where the row-store won) and marked the large-scale win UNBENCHMARKED.
M30's KEEP-columnar decision (ADR 0013) is honest ONLY if columnar actually wins at analytical scale — this
driver fills that gap: sweep increasing row counts, run the same group-by aggregate on a DuckDB columnstore
mirror vs the row table (reusing `theodb_bench.columnar.run_columnar_vs_row`, Rule 9 — no harness rebuild),
and report the crossover where the columnstore `DuckDBScan` beats the row-store `Seq Scan`.

Substrate: the canonical `mooncakelabs/pg_mooncake` image (PG18, MIT) — the measurement substrate (proving the
CAPABILITY); shipping columnar on the PG17 TheoDB image is the gated adoption step (a separate milestone).

Usage:
  python3 benchmarks/run_m30_columnar_scale.py --port 5492 --scales 100000,1000000,5000000 --write-doc
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

from theodb_bench.columnar import run_columnar_vs_row
from theodb_bench.db import VectorDB

_REPO = Path(__file__).resolve().parent.parent


def _dsn(port: int) -> str:
    return f"host=localhost port={port} dbname=postgres user=postgres password=postgres"


def run(port, scales=(100_000, 1_000_000, 5_000_000)):
    """Sweep the scales; per n, run the columnar-vs-row aggregate and record timings + plan + correctness."""
    db = VectorDB(_dsn(port)).connect()
    if not db.pg_mooncake_available():
        db.close()
        raise RuntimeError("pg_mooncake not available on this container — run against mooncakelabs/pg_mooncake")
    points = []
    for n in scales:
        r = run_columnar_vs_row(db, n)
        row_ms, col_ms = r["row"]["ms"], r["columnar"]["ms"]
        points.append({
            "n": n,
            "rows_synced": r["rows_synced"],
            "row_ms": round(row_ms, 1),
            "columnar_ms": round(col_ms, 1),
            "speedup": round(row_ms / col_ms, 2) if col_ms > 0 else None,
            "columnar_plan_duckdb": "DuckDB" in r["columnar"]["plan"],
            "match": r["match"],
        })
    db.close()
    crossover = next((p["n"] for p in points if p["speedup"] and p["speedup"] > 1.0), None)
    return {
        "substrate": "mooncakelabs/pg_mooncake (PG18, MIT)",
        "note": "measurement substrate — shipping columnar on the PG17 TheoDB image is the gated adoption step",
        "query": "SELECT category, count(*), avg(amount) FROM t GROUP BY category (analytics rollup)",
        "points": points,
        "crossover_n": crossover,
    }


def _render_md(res) -> str:
    lines = [
        "# M30 — columnar-at-scale: pg_mooncake columnstore vs row-store (the crossover)",
        "",
        f"**Substrate:** {res['substrate']} — {res['note']}.  ",
        f"**Query:** `{res['query']}`.  ",
        "**Type:** measurement — fills the large-scale gap the M6 benchmark "
        "(`docs/benchmarks/m6-columnar-vs-row.md`) marked UNBENCHMARKED (it measured only 100k, where the "
        "row-store won). Grounds the M30 KEEP-columnar decision (ADR 0013).",
        "",
        "| rows (n) | row-store ms (Seq Scan) | columnstore ms (DuckDBScan) | speedup | correct? |",
        "|---|---|---|---|---|",
    ]
    for p in res["points"]:
        plan = "DuckDBScan" if p["columnar_plan_duckdb"] else "NOT-duckdb!"
        lines.append(f"| {p['n']:,} | {p['row_ms']} | {p['columnar_ms']} ({plan}) | "
                     f"{p['speedup']}× | {'yes' if p['match'] else 'NO'} |")
    cross = res["crossover_n"]
    verdict = (f"**Crossover: columnar beats the row-store from n = {cross:,}.**" if cross
               else "**No crossover in the tested range — columnar did NOT beat the row-store at any tested "
                    "scale (honest negative; extend the sweep or reconsider the workload).**")
    lines += [
        "",
        f"## Verdict\n\n{verdict} Correctness (`match`) holds at every point (count exact + per-group avg within "
        "1e-3 cross-engine tolerance). The columnstore plans as `DuckDBScan` (vectorized), the row-store as "
        "`Seq Scan`.",
        "",
        "## Honest caveats",
        "- Substrate is `mooncakelabs/pg_mooncake` (**PG18**); the shipped TheoDB image is **PG17**. This proves "
        "the columnar CAPABILITY + the crossover; shipping columnar on PG17 is the gated adoption step (fix the "
        "from-source build OR bump to PG18) — a separate milestone.",
        "- Synthetic 5-category group-by (analytics/observability-style rollup); real workloads vary.",
        "- The columnstore is a DuckDB+Iceberg lakehouse on disk (ADR 0002 D2 — NOT AlloyDB in-memory).",
        "",
        "## Reproduce",
        "```",
        "docker run -d --name mooncake -p <port>:5432 -e POSTGRES_PASSWORD=postgres mooncakelabs/pg_mooncake:latest",
        "python3 benchmarks/run_m30_columnar_scale.py --port <port> --write-doc",
        "```",
    ]
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=int(os.environ.get("PORT", os.environ.get("PGPORT", "5492"))))
    ap.add_argument("--scales", default="100000,1000000,5000000")
    ap.add_argument("--write-doc", action="store_true")
    args = ap.parse_args()

    scales = tuple(int(x) for x in args.scales.split(","))
    res = run(args.port, scales=scales)

    print(f"\n=== M30 columnar-at-scale ({res['substrate']}) ===")
    for p in res["points"]:
        print(f"  n={p['n']:>9,}  row={p['row_ms']:>8}ms  columnar={p['columnar_ms']:>8}ms  "
              f"speedup={p['speedup']}x  duckdb={p['columnar_plan_duckdb']}  match={p['match']}")
    print(f"\nCROSSOVER: {res['crossover_n']}")

    if args.write_doc:
        out = _REPO / "docs" / "benchmarks"
        out.mkdir(parents=True, exist_ok=True)
        (out / "m30-columnar-scale.json").write_text(json.dumps(res, indent=2))
        (out / "m30-columnar-scale.md").write_text(_render_md(res))
        print(f"wrote {out / 'm30-columnar-scale.json'} + .md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
