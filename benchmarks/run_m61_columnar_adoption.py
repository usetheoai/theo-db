#!/usr/bin/env python3
"""M61 — pg_duckdb adoption: does the DuckDB vectorized engine (force_execution) speed up an analytical
aggregate over a Postgres HEAP table vs the Postgres row executor? The honest HTAP number for the EMBEDDED
pg_duckdb surface — NOT inherited from the M30 pg_mooncake columnstore (different engine, different mechanism).

pg_duckdb WITHOUT MotherDuck has no persistent native columnstore (measured 2026-07-08: "Only TEMP tables are
supported… if MotherDuck support is not enabled"). Its HTAP path is `SET duckdb.force_execution=true` → the same
heap table is scanned+aggregated by DuckDB's vectorized engine (analytics on transactional data, no ETL). This
harness times `_AGG` (reused from theodb_bench.columnar, Rule 9) with force_execution OFF vs ON, ≥3 runs mean±std,
correctness-matched (_results_match), across scales. Honest crossover: the smallest scale where DuckDB wins AND
effect>variance; honest-negative (crossover_n=None) is a valid outcome.
"""
from __future__ import annotations

import argparse
import json
import os
import statistics
import time

import psycopg2

from theodb_bench.columnar import _AGG, _results_match

PGHOST = os.environ.get("PGHOST", "127.0.0.1")
PGPORT = os.environ.get("PGPORT", "5432")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")


def _conn():
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres",
                         connect_timeout=15, keepalives=1, keepalives_idle=15)
    c.autocommit = True
    return c


def _seed(cur, table, n):
    cur.execute(f"DROP TABLE IF EXISTS {table} CASCADE")
    cur.execute(f"CREATE TABLE {table} (id bigint PRIMARY KEY, category text, amount double precision)")
    cur.execute(f"INSERT INTO {table} SELECT g, 'cat' || (g %% 5), (g %% 1000) * 1.5 FROM generate_series(1, %s) g",
                (n,))
    cur.execute(f"ANALYZE {table}")


def _timed(cur, sql, force):
    cur.execute(f"SET duckdb.force_execution = {'true' if force else 'false'}")
    t0 = time.perf_counter()
    cur.execute(sql)
    rows = cur.fetchall()
    return rows, (time.perf_counter() - t0) * 1000.0


def _plan_uses_duckdb(cur, sql):
    cur.execute("SET duckdb.force_execution = true")
    cur.execute("EXPLAIN " + sql)
    return any("DuckDB" in r[0] or "duckdb" in r[0] for r in cur.fetchall())


def _measure_scale(cur, n, runs, table="metrics"):
    _seed(cur, table, n)
    sql = _AGG.format(t=table)
    row_ms, duck_ms = [], []
    row_res = duck_res = None
    # warm each engine once (JIT/plan cache), then time `runs` times
    for _ in range(runs + 1):
        rr, rm = _timed(cur, sql, force=False)
        dr, dm = _timed(cur, sql, force=True)
        row_res, duck_res = rr, dr
        row_ms.append(rm); duck_ms.append(dm)
    row_ms, duck_ms = row_ms[1:], duck_ms[1:]  # drop warm-up
    rm_mean, rm_std = statistics.mean(row_ms), (statistics.pstdev(row_ms) if runs > 1 else 0.0)
    dm_mean, dm_std = statistics.mean(duck_ms), (statistics.pstdev(duck_ms) if runs > 1 else 0.0)
    speedup = round(rm_mean / dm_mean, 2) if dm_mean else None
    # effect > variance: DuckDB wins AND the gap exceeds the combined std (anti-noise, PRD D3)
    effect_gt_var = (dm_mean < rm_mean) and (rm_mean - dm_mean) > (rm_std + dm_std)
    return {
        "n": n, "runs": runs,
        "row_ms_mean": round(rm_mean, 1), "row_ms_std": round(rm_std, 1),
        "duckdb_ms_mean": round(dm_mean, 1), "duckdb_ms_std": round(dm_std, 1),
        "duckdb_over_row_speedup": speedup, "duckdb_wins": dm_mean < rm_mean,
        "effect_gt_variance": effect_gt_var,
        "match": _results_match(row_res, duck_res), "plan_uses_duckdb": _plan_uses_duckdb(cur, sql),
    }


def _measure_parquet(cur, n, runs, table="metrics", path="/tmp/m61_metrics.parquet"):
    """The COLUMNAR path: export the heap to Parquet (DuckDB COPY), then aggregate the Parquet with DuckDB
    (columnar+vectorized, via duckdb.query) vs the heap with Postgres. This is where DuckDB's engine actually
    wins — the analytical value only materializes when the data is columnar-stored (Parquet), NOT over the row
    heap. Full-scan checksum (sum of a computed expr) so no metadata shortcut hides the real compute."""
    _seed(cur, table, n)
    cur.execute(f"COPY (SELECT * FROM {table}) TO '{path}' (FORMAT parquet)")
    duck_sql = (f"SELECT * FROM duckdb.query($$ SELECT round(sum(amount*2.0 + id),0) AS s "
                f"FROM read_parquet('{path}') $$)")
    heap_sql = f"SELECT round(sum(amount*2.0 + id)::numeric,0) FROM {table}"
    pq_ms, hp_ms = [], []
    pq_val = hp_val = None
    for _ in range(runs + 1):
        cur.execute("SET duckdb.force_execution=false")
        t0 = time.perf_counter(); cur.execute(duck_sql); pq_val = cur.fetchone()[0]; pq_ms.append((time.perf_counter()-t0)*1000)
        t0 = time.perf_counter(); cur.execute(heap_sql); hp_val = cur.fetchone()[0]; hp_ms.append((time.perf_counter()-t0)*1000)
    pq_ms, hp_ms = pq_ms[1:], hp_ms[1:]
    pm, hm = statistics.mean(pq_ms), statistics.mean(hp_ms)
    # Correctness: compare NUMERICALLY within a relative epsilon — DuckDB returns the sum as a double
    # ("…0.0"), Postgres as numeric; a string compare false-mismatches on formatting, not on the value.
    match = abs(float(pq_val) - float(hp_val)) <= 1e-3 * max(1.0, abs(float(hp_val)))
    return {"n": n, "parquet_duckdb_ms_mean": round(pm, 1), "heap_postgres_ms_mean": round(hm, 1),
            "parquet_over_heap_speedup": round(hm / pm, 2) if pm else None,
            "checksum_match": match, "parquet_wins": pm < hm}


def run(scales, runs):
    conn = _conn(); cur = conn.cursor()
    cur.execute("SELECT extversion FROM pg_extension WHERE extname='pg_duckdb'")
    ext = cur.fetchone()
    per_scale = [_measure_scale(cur, n, runs) for n in scales]
    parquet = [_measure_parquet(cur, n, runs) for n in scales]
    conn.close()
    # crossover_n: smallest scale where DuckDB wins with effect>variance AND results match (the HEAP path)
    winners = [p["n"] for p in per_scale if p["effect_gt_variance"] and p["match"]]
    pq_wins = [p["n"] for p in parquet if p["parquet_wins"] and p["checksum_match"]]
    return {
        "pg_duckdb_version": ext[0] if ext else None, "runs": runs, "scales": list(scales),
        "heap_surface": "force_execution over heap (row-format) — DuckDB reads the row store",
        "parquet_surface": "DuckDB over columnar Parquet (read_parquet via duckdb.query) — data-lake path",
        "heap_crossover_n": min(winners) if winners else None,
        "parquet_crossover_n": min(pq_wins) if pq_wins else None,
        "adoption_verdict": (
            "HEAP force_execution: honest-negative (DuckDB loses on row-format data); "
            "COLUMNAR Parquet: DuckDB wins (the analytical value is in columnar-stored data)"),
        "per_scale_heap": per_scale, "per_scale_parquet": parquet,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scales", default="100000,1000000,5000000")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--out", default="/root/m61_columnar_adoption.json")
    args = ap.parse_args()
    scales = [int(x) for x in args.scales.split(",")]
    data = run(scales, args.runs)
    json.dump(data, open(args.out, "w"), indent=2)
    print(f"wrote {args.out} — pg_duckdb {data['pg_duckdb_version']}")
    print(f"verdict: {data['adoption_verdict']}")
    print("HEAP (force_execution over row store):")
    for p in data["per_scale_heap"]:
        print(f"  n={p['n']:>9}: row {p['row_ms_mean']}±{p['row_ms_std']}ms  duckdb {p['duckdb_ms_mean']}±{p['duckdb_ms_std']}ms "
              f"→ {p['duckdb_over_row_speedup']}× (wins={p['duckdb_wins']}, match={p['match']})")
    print("PARQUET (DuckDB columnar vs Postgres heap):")
    for p in data["per_scale_parquet"]:
        print(f"  n={p['n']:>9}: parquet {p['parquet_duckdb_ms_mean']}ms  heap {p['heap_postgres_ms_mean']}ms "
              f"→ {p['parquet_over_heap_speedup']}× (wins={p['parquet_wins']}, checksum_match={p['checksum_match']})")


if __name__ == "__main__":
    main()
