# M26 — Vector Index Access Method: recall + latency evidence

**Milestone:** M26 (`theodb_ivfflat` / `theodb_hnsw` persisted Postgres index AMs)
**Date:** 2026-07-01
**Plan:** `.claude/knowledge-base/plans/m26-vector-index-am-plan.md`
**Image:** `theo-db:m26` · PostgreSQL 17 · pgrx 0.16.1

Measurement-first (the blueprint's anti-sunk-cost stance, TheoDB rule 5): every claim below comes from an actual
run against the container. Reproduction commands are in § 4.

---

## 1. What was built (DoD coverage)

| DoD item | Status | Evidence |
|---|---|---|
| `IndexAmRoutine` registered (all hooks) | ✅ | `theodb_ivfflat` + `theodb_hnsw` in `pg_am` |
| `CREATE INDEX … USING theodb_{ivfflat,hnsw}` persisted to pages (not rebuild-per-query) | ✅ | `pg_relation_size(idx) > 0`; § 3 latency proves build-once |
| Planner pushdown `ORDER BY <-> LIMIT k` (amcanorderbyop + amcostestimate) | ✅ | `EXPLAIN` shows an Index Scan (§ 2) |
| Incremental INSERT/DELETE maintenance + VACUUM | ✅ | `test_index_am.py::test_incremental_insert_delete_vacuum` |
| Reproducible benchmark: recall@k parity + latency vs full-scan+rebuild | ✅ | § 2 (recall), § 3 (latency) |
| Coexistence with the SQL-callable function (no M20–M22 break) | ✅ | M20–M22 suites: **61 passed** against `theo-db:m26` |

Scope (honest): both AMs ship the **l2** operator class (`theodb_{ivfflat,hnsw}_l2_ops`). The cosine/ip operator
classes are a documented follow-up — the metric is baked into the persisted blob, but opclass→metric resolution
at build time needs a catalog lookup pgrx 0.16 does not expose via `get_opfamily_name` (see ADR
`docs/adr/0010-m26-index-am-scope.md`).

## 2. Recall@k parity

The persisted index answers `ORDER BY embedding <-> $1 LIMIT k` at parity with a brute-force sequential scan.
Asserted green in `benchmarks/tests/test_index_am.py` (200-row corpus, dim 8, both AMs):

- `test_index_scan_returns_correct_neighbors` (theodb_ivfflat): recall@5 ≥ 4/5 vs brute force.
- `test_hnsw_am_persists_pushes_down_and_recalls` (theodb_hnsw): recall@5 ≥ 4/5 vs brute force.
- `EXPLAIN` confirms an **Index Scan** on the theodb AM for the ORDER-BY-LIMIT query in both.

## 3. Latency — persisted index vs full-scan + rebuild (the DoD comparison)

Corpus: 5 000 rows, `vector(128)`, random. Query: nearest-10 to an existing row. `EXPLAIN (ANALYZE, TIMING ON)`,
3 runs each, warm cache.

| Approach | Mean ± std | Note |
|---|---:|---|
| **Rebuild-per-query** `theodb.ivfflat_knn(...)` (reads the whole corpus + k-means **every call**) | **1372 ± 26 ms** | the pre-M26 baseline (the SQL-callable function) |
| **Persisted `theodb_ivfflat` Index Scan** (build once at CREATE INDEX; scan reads the persisted index) | **86 ± 5 ms** | M26 |

**The persisted index is ~16× faster than rebuild-per-query** — the DoD's stated comparison ("latência
índice-persistido vs full-scan+rebuild"). Building once and persisting to pages removes the per-query k-means
rebuild, which dominated the old path (1.3 s → 86 ms).

### Honest caveat + optimization path

At this scale a **plain pgvector-style seq scan** (which does NOT rebuild) runs the same query in ~1.5 ms — faster
than our 86 ms persisted Index Scan. The reason is a known limitation of the current MVP persistence (plan ADR-1):
`amrescan` **deserializes the entire index blob from pages on every scan** (O(N) — for 5 000×128 that is ~2.5 MB
of f32 reconstructed into `Vec<Vec<f32>>` per query). The index therefore wins decisively against the
rebuild-per-query function (the DoD baseline) but not yet against a non-rebuilding seq scan at small/medium N.

The optimization path (a follow-up, measurement-first): read only the needed pages per scan (centroid page +
probed-list pages) instead of the whole blob, and/or cache the deserialized index across scans in a
relation-scoped cache. This turns the scan from O(N) deserialize into O(probes·list_size) — the point at which the
index also beats seq scan. Deferred honestly rather than claimed.

## 4. Reproduction

```bash
docker build -t theo-db:m26 .
docker run -d --name theo-db-m26 -e POSTGRES_PASSWORD=postgres -p 5435:5432 theo-db:m26
# wait for healthy, then:
PGPORT=5435 PGHOST=localhost PGUSER=postgres PGPASSWORD=postgres \
  python3 -m pytest benchmarks/tests/test_index_am.py -q          # 6/6: register, persist, EXPLAIN, recall, maint, hnsw

# latency (inside the container):
docker exec theo-db-m26 psql -U postgres -c "
  CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;
  CREATE TABLE bench (id bigint, embedding vector(128));
  INSERT INTO bench SELECT g, ('['||(SELECT string_agg(random()::text,',') FROM generate_series(1,128))||']')::vector
    FROM generate_series(1,5000) g;
  CREATE INDEX bench_idx ON bench USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops);"
# rebuild-per-query:
docker exec theo-db-m26 psql -U postgres -c "EXPLAIN (ANALYZE) SELECT * FROM theodb.ivfflat_knn('bench','embedding', ARRAY[(SELECT embedding FROM bench WHERE id=1)], 10);"
# persisted index:
docker exec theo-db-m26 psql -U postgres -c "SET enable_seqscan=off; EXPLAIN (ANALYZE) SELECT id FROM bench ORDER BY embedding <-> (SELECT embedding FROM bench WHERE id=1) LIMIT 10;"
```
