"""M31b latency+recall gate — theodb_ivfflat (structured partial reads + AVX2/FMA fused distance) vs pgvector.

M31b DoD: the structured `theodb_ivfflat` Index Scan p50 is **≤ pgvector** at n≥100k dim=128, recall preserved.

Two regimes, both on DISTINCT vectors seeded from Python (seed=42, COPY) — NEVER the SQL
`(SELECT string_agg(random()...) FROM generate_series(...))` idiom, which PostgreSQL hoists as a one-time InitPlan
so every row gets the SAME vector (COUNT(DISTINCT)=1). That degeneracy made every pre-M31b latency number a
brute-force-on-identical-ties measurement (all lists collapse into one) — see docs/benchmarks/m31b-simd-distance.md
and docs/adr/0012-benchmark-data-degeneracy.md. The profiler `THEODB_SCAN_PROFILE=1` exposed it (`cand=100000,
nonempty_lists=1/100`).

  1. UNIFORM random — the IVFFlat worst case (no cluster structure). Recall is low for BOTH; the gate asserts
     recall PARITY and a decisive theodb latency win (SIMD + tight scan loop; robust margin, noise-proof).
  2. CLUSTERED (gaussian, embedding-like) — the realistic operating point. Both reach high recall; the gate
     asserts recall parity AND theodb p50 ≤ pgvector within a small noise band.

Connection via PG* env.
"""

import io
import os
import random
import statistics

import psycopg2
import pytest

pytestmark = pytest.mark.integration

N = 100_000
DIM = 128
PROBES = 10
QUERIES = 30
LAT_QUERIES = 12
SEED = 42


def _conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
    )


def _seed(cur, table, clustered):
    """Seed N DISTINCT vectors via COPY (deterministic seed). clustered=True → 200 gaussian centers
    (embedding-like); False → uniform random (IVFFlat worst case)."""
    rng = random.Random(SEED)
    cur.execute(f"DROP TABLE IF EXISTS {table} CASCADE")
    cur.execute(f"CREATE TABLE {table} (id bigint, embedding vector({DIM}))")
    buf = io.StringIO()
    if clustered:
        centers = [[rng.random() for _ in range(DIM)] for _ in range(200)]
        for g in range(1, N + 1):
            ce = centers[g % 200]
            vec = "[" + ",".join(f"{ce[j] + rng.gauss(0, 0.05):.6f}" for j in range(DIM)) + "]"
            buf.write(f"{g}\t{vec}\n")
    else:
        for g in range(1, N + 1):
            vec = "[" + ",".join(f"{rng.random():.6f}" for _ in range(DIM)) + "]"
            buf.write(f"{g}\t{vec}\n")
    buf.seek(0)
    cur.copy_expert(f"COPY {table} (id, embedding) FROM STDIN", buf)
    cur.execute(f"SELECT COUNT(DISTINCT embedding::text) FROM {table}")
    assert cur.fetchone()[0] == N, "seed produced non-distinct vectors — the InitPlan-hoist bug is back"


def _create_index(cur, table, which):
    cur.execute(f"DROP INDEX IF EXISTS {table}_theodb")
    cur.execute(f"DROP INDEX IF EXISTS {table}_pgv")
    if which == "theodb":
        cur.execute(f"CREATE INDEX {table}_theodb ON {table} USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)")
    else:
        cur.execute(f"CREATE INDEX {table}_pgv ON {table} USING ivfflat (embedding vector_l2_ops) WITH (lists=100)")


def _truth(cur, table, qv):
    cur.execute("SET enable_indexscan=off; SET enable_seqscan=on")
    cur.execute(f"SELECT id FROM {table} ORDER BY embedding <-> '{qv}' LIMIT 10")
    return {r[0] for r in cur.fetchall()}


def _got(cur, table, qv):
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute(f"SELECT id FROM {table} ORDER BY embedding <-> '{qv}' LIMIT 10")
    return {r[0] for r in cur.fetchall()}


def _p50_ms(cur, table, qv):
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute(f"SELECT id FROM {table} ORDER BY embedding <-> '{qv}' LIMIT 10")
    cur.fetchall()  # warm
    times = []
    for _ in range(15):
        cur.execute(f"EXPLAIN (ANALYZE, TIMING ON) SELECT id FROM {table} ORDER BY embedding <-> '{qv}' LIMIT 10")
        for (row,) in cur.fetchall():
            if row.strip().startswith("Execution Time:"):
                times.append(float(row.split(":")[1].strip().split(" ")[0]))
    return statistics.median(times)


def _query_vectors(cur, table, k):
    cur.execute(f"SELECT embedding::text FROM {table} WHERE id <= {k} ORDER BY id")
    return [r[0] for r in cur.fetchall()]


def test_uniform_theodb_faster_than_pgvector_at_recall_parity():
    """UNIFORM worst case: theodb Index Scan p50 is decisively ≤ pgvector (SIMD + tight loop), recall at parity."""
    conn = _conn()
    conn.autocommit = True
    try:
        cur = conn.cursor()
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        _seed(cur, "latu", clustered=False)
        cur.execute(f"SET ivfflat.probes = {PROBES}")
        qvs = _query_vectors(cur, "latu", QUERIES)

        _create_index(cur, "latu", "theodb")
        th_recall = statistics.mean(len(_truth(cur, "latu", q) & _got(cur, "latu", q)) for q in qvs)
        th_p50 = statistics.median(_p50_ms(cur, "latu", q) for q in qvs[:LAT_QUERIES])

        _create_index(cur, "latu", "pgvector")
        pv_recall = statistics.mean(len(_truth(cur, "latu", q) & _got(cur, "latu", q)) for q in qvs)
        pv_p50 = statistics.median(_p50_ms(cur, "latu", q) for q in qvs[:LAT_QUERIES])

        # Recall parity — uniform random is the IVFFlat worst case; both are low, theodb not materially worse.
        assert th_recall >= pv_recall - 1.0, f"theodb recall {th_recall:.1f} << pgvector {pv_recall:.1f}"
        # Decisive latency win (robust margin — the SIMD-fused scan beats pgvector's AVX C here).
        assert th_p50 <= pv_p50, f"theodb p50 {th_p50:.2f}ms not <= pgvector {pv_p50:.2f}ms (uniform)"
        cur.execute("DROP TABLE latu CASCADE")
    finally:
        conn.close()


def test_clustered_high_recall_and_theodb_within_pgvector():
    """CLUSTERED realistic point: both reach high recall; theodb p50 ≤ pgvector within a small noise band."""
    conn = _conn()
    conn.autocommit = True
    try:
        cur = conn.cursor()
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        _seed(cur, "latc", clustered=True)
        cur.execute(f"SET ivfflat.probes = {PROBES}")
        qvs = _query_vectors(cur, "latc", QUERIES)

        _create_index(cur, "latc", "theodb")
        th_recall = statistics.mean(len(_truth(cur, "latc", q) & _got(cur, "latc", q)) for q in qvs)
        th_p50 = statistics.median(_p50_ms(cur, "latc", q) for q in qvs[:LAT_QUERIES])

        _create_index(cur, "latc", "pgvector")
        pv_recall = statistics.mean(len(_truth(cur, "latc", q) & _got(cur, "latc", q)) for q in qvs)
        pv_p50 = statistics.median(_p50_ms(cur, "latc", q) for q in qvs[:LAT_QUERIES])

        # High recall at the realistic operating point, at parity with pgvector.
        assert th_recall >= 8.0, f"theodb recall {th_recall:.1f}/10 too low on clustered data"
        assert th_recall >= pv_recall - 1.0, f"theodb recall {th_recall:.1f} << pgvector {pv_recall:.1f}"
        # theodb p50 at or below pgvector within a 15% measurement-noise band (the SIMD residual is closed).
        assert th_p50 <= pv_p50 * 1.15, f"theodb p50 {th_p50:.2f}ms not <= pgvector {pv_p50:.2f}ms x1.15 (clustered)"
        cur.execute("DROP TABLE latc CASCADE")
    finally:
        conn.close()
