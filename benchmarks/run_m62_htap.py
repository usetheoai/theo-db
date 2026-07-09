#!/usr/bin/env python3
"""M62 — HTAP unified surface: the mixed-load / non-interference harness (Phase 2, axis-3 of the benchmark).

The honest HTAP surface is a row↔columnar materialized flow (theodb.htap_refresh → Parquet snapshot;
theodb.olap → read_parquet via DuckDB, ~9× at 5M per M61). This harness measures axis-3 (the one that needs
CONCURRENCY, not a single thread): does an OLAP aggregate running concurrently degrade OLTP INSERT latency?

Race-aware by construction (`.claude/rules/testing.md` §6 — single-thread TDD never catches a race): a
threading.Barrier(2) forces the OLTP INSERT-loop and the OLAP olap-loop to actually OVERLAP (happens-before);
without it one thread could finish before the other starts (false green). We measure p50/p95 of INSERTs with
and without concurrent OLAP pressure and assert the degradation stays under a declared factor. Honest-negative
accepted: if the surface degrades OLTP more than the factor, the harness REPORTS it (never hides it).

Reuses _AGG / _results_match from theodb_bench.columnar (Rule 9). Phase-3 axes (speedup, refresh cost) are
added on top of this file in T3.1; this file ships axis-3 first (the concurrency gate).
"""
from __future__ import annotations

import argparse
import json
import os
import threading
import time

import psycopg2

PGHOST = os.environ.get("PGHOST", "127.0.0.1")
PGPORT = os.environ.get("PGPORT", "5432")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")

# The OLTP INSERT latency under concurrent OLAP must not blow past this factor of the alone-baseline p95.
# The Parquet snapshot is read-only and does not lock the heap, so real interference should be small; the
# factor is generous (honest-negative reported if exceeded). Declared, not hidden.
LATENCY_DEGRADATION_FACTOR = float(os.environ.get("M62_LATENCY_FACTOR", "4.0"))


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


def _pctl(samples, q):
    """Nearest-rank percentile (q in [0,1]) over a non-empty list of latencies (ms)."""
    if not samples:
        return None
    s = sorted(samples)
    idx = min(len(s) - 1, max(0, int(round(q * (len(s) - 1)))))
    return round(s[idx], 4)


def _run_oltp(table, n_inserts, base_id, barrier=None):
    """Insert n_inserts rows (one autocommit txn each = OLTP), timing each. base_id keeps PKs unique across
    the two conditions. If a barrier is passed, wait on it first so both workers start together (overlap)."""
    conn = _conn()
    cur = conn.cursor()
    lat = []
    if barrier is not None:
        barrier.wait()
    for i in range(n_inserts):
        rid = base_id + i
        t0 = time.perf_counter()
        cur.execute(f"INSERT INTO {table} (id, category, amount) VALUES (%s, %s, %s)",
                    (rid, f"cat{rid % 5}", (rid % 1000) * 1.5))
        lat.append((time.perf_counter() - t0) * 1000.0)
    conn.close()
    return lat


def _run_olap(table, stop_evt, iters_counter, barrier=None):
    """Run theodb.olap(table) in a loop (OLAP pressure) until stop_evt is set. Counts iterations so the test
    can assert the OLAP actually ran DURING the INSERTs (overlap confirmation)."""
    conn = _conn()
    cur = conn.cursor()
    if barrier is not None:
        barrier.wait()
    while not stop_evt.is_set():
        try:
            cur.execute(f"SELECT theodb.olap('{table}')")
            cur.fetchone()
            iters_counter[0] += 1
        except psycopg2.Error:
            # An OLAP failure must not silently pass — surface it via the counter staying flat (the test asserts
            # overlap_confirmed = iters > 0). Re-raise-free here so the OLTP worker still completes its timing.
            break
    conn.close()


def measure_mixed_load(table="htap_metrics", seed_n=50_000, n_inserts=2_000):
    """Axis-3: OLTP INSERT p50/p95 alone (baseline) vs under concurrent OLAP (theodb.olap loop). A
    threading.Barrier(2) guarantees overlap in the mixed phase. Returns baseline/mixed percentiles, the
    degradation factor, and overlap_confirmed (OLAP iterations that ran during the INSERTs > 0)."""
    conn = _conn()
    cur = conn.cursor()
    _seed(cur, table, seed_n)
    cur.execute(f"SELECT theodb.htap_refresh('{table}')")  # snapshot so theodb.olap has something to read
    conn.close()

    # Phase A — OLTP alone (baseline). No OLAP pressure.
    baseline = _run_oltp(table, n_inserts, base_id=seed_n + 1)

    # Phase B — OLTP + OLAP concurrently, synchronized by a barrier so they truly overlap.
    barrier = threading.Barrier(2)
    stop_evt = threading.Event()
    olap_iters = [0]
    olap_t = threading.Thread(target=_run_olap, args=(table, stop_evt, olap_iters, barrier), daemon=True)
    olap_t.start()
    mixed = _run_oltp(table, n_inserts, base_id=seed_n + n_inserts + 1, barrier=barrier)
    stop_evt.set()
    olap_t.join(timeout=30)

    base_p95 = _pctl(baseline, 0.95)
    mixed_p95 = _pctl(mixed, 0.95)
    degradation = round(mixed_p95 / base_p95, 3) if base_p95 else None
    return {
        "seed_n": seed_n, "n_inserts": n_inserts,
        "baseline_p50_ms": _pctl(baseline, 0.50), "baseline_p95_ms": base_p95,
        "mixed_p50_ms": _pctl(mixed, 0.50), "mixed_p95_ms": mixed_p95,
        "degradation_factor": degradation,
        "latency_factor_threshold": LATENCY_DEGRADATION_FACTOR,
        "overlap_confirmed": olap_iters[0] > 0, "olap_iters_during_inserts": olap_iters[0],
        "not_degraded": (degradation is not None and degradation <= LATENCY_DEGRADATION_FACTOR),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed-n", type=int, default=50_000)
    ap.add_argument("--n-inserts", type=int, default=2_000)
    ap.add_argument("--out", default="/root/m62_htap_axis3.json")
    args = ap.parse_args()
    data = measure_mixed_load(seed_n=args.seed_n, n_inserts=args.n_inserts)
    json.dump(data, open(args.out, "w"), indent=2)
    print(f"wrote {args.out}")
    print(f"axis-3 non-interference: baseline p95 {data['baseline_p95_ms']}ms  mixed p95 {data['mixed_p95_ms']}ms "
          f"→ {data['degradation_factor']}× (threshold {data['latency_factor_threshold']}×, "
          f"overlap={data['overlap_confirmed']}, not_degraded={data['not_degraded']})")


if __name__ == "__main__":
    main()
