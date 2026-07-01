"""M31 latency gate — theodb_ivfflat structured partial-page reads vs the M26 O(N) baseline and vs pgvector.

Re-scoped DoD (CTO decision 2026-07-01): M31 proves the O(N)-per-scan gap is CLOSED (structured partial reads)
with correctness preserved; latency is measured honestly against pgvector (theodb is ~2-3x behind pgvector's
AVX-SIMD C — closing that residual is M31b's SIMD-distance slice, ADR 0011). This test therefore asserts:
  1. the structured index answers ORDER BY <-> LIMIT k at recall parity with a brute-force scan, AND
  2. the Index Scan p50 is FAR below an O(N)-per-scan latency (a hard ceiling that a whole-index deserialize
     would blow), AND is within a documented band of pgvector — NOT that it beats pgvector (M31b).

Connection via PG* env.
"""

import os
import statistics

import psycopg2
import pytest

pytestmark = pytest.mark.integration

N = 100_000
DIM = 128
PROBES = 10
QUERIES = 15
# Re-scoped ceilings (CTO 2026-07-01): the O(N) M26 path would be ~1 s+ at this N; a structured partial read is
# an order of magnitude under that. We assert well under the O(N) regime, and within ~4x of pgvector (honest band).
O_N_CEILING_MS = 600.0
PGVECTOR_BAND = 4.0


def _conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
    )


def _p50_ms(cur, index_hint_sql, qv):
    times = []
    for _ in range(QUERIES):
        cur.execute(index_hint_sql)
        cur.execute(
            f"EXPLAIN (ANALYZE, TIMING ON) SELECT id FROM lat ORDER BY embedding <-> '{qv}' LIMIT 10"
        )
        for (row,) in cur.fetchall():
            if row.strip().startswith("Execution Time:"):
                times.append(float(row.split(":")[1].strip().split(" ")[0]))
    return statistics.median(times)


def test_structured_latency_closes_on_and_within_pgvector_band():
    conn = _conn()
    conn.autocommit = True
    try:
        cur = conn.cursor()
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        cur.execute("DROP TABLE IF EXISTS lat CASCADE")
        cur.execute(f"CREATE TABLE lat (id bigint, embedding vector({DIM}))")
        cur.execute(
            f"INSERT INTO lat SELECT g, ('['||(SELECT string_agg((random())::text, ',') "
            f"FROM generate_series(1,{DIM}))||']')::vector FROM generate_series(1,{N}) g"
        )
        cur.execute("CREATE INDEX lat_theodb ON lat USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)")
        cur.execute(f"CREATE INDEX lat_pgv ON lat USING ivfflat (embedding vector_l2_ops) WITH (lists=100)")
        cur.execute(f"SET ivfflat.probes = {PROBES}")
        cur.execute("SELECT embedding::text FROM lat WHERE id = 1")
        qv = cur.fetchone()[0]

        # (1) recall parity: structured index top-10 overlaps the brute-force top-10.
        cur.execute("SET enable_indexscan=off; SET enable_seqscan=on")
        cur.execute(f"SELECT id FROM lat ORDER BY embedding <-> '{qv}' LIMIT 10")
        truth = {r[0] for r in cur.fetchall()}
        cur.execute("DROP INDEX lat_pgv")  # force the theodb index for the recall + theodb-latency measurement
        cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
        cur.execute(f"SELECT id FROM lat ORDER BY embedding <-> '{qv}' LIMIT 10")
        got = {r[0] for r in cur.fetchall()}
        assert len(truth & got) >= 8, f"structured recall too low: {got} vs {truth}"

        # (2a) theodb structured p50 is far below the O(N) regime (a whole-index deserialize would exceed this).
        theodb_p50 = _p50_ms(cur, "SET enable_seqscan=off", qv)
        assert theodb_p50 < O_N_CEILING_MS, f"theodb p50 {theodb_p50}ms is in the O(N) regime (>= {O_N_CEILING_MS})"

        # (2b) and within a documented band of pgvector (honest — pgvector's SIMD C is faster; M31b closes it).
        cur.execute(f"CREATE INDEX lat_pgv ON lat USING ivfflat (embedding vector_l2_ops) WITH (lists=100)")
        cur.execute("DROP INDEX lat_theodb")
        pgv_p50 = _p50_ms(cur, "SET enable_seqscan=off", qv)
        assert theodb_p50 <= pgv_p50 * PGVECTOR_BAND, (
            f"theodb p50 {theodb_p50}ms not within {PGVECTOR_BAND}x of pgvector {pgv_p50}ms"
        )
        cur.execute("DROP TABLE lat CASCADE")
    finally:
        conn.close()
