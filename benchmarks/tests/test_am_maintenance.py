"""M48 maintenance tests for the theodb index AMs (plan m48-am-crash-safety, phases 2-5).

Covers the non-crash side of the crash-safe fold (issue #47): a VACUUM fold must preserve scan
results and, because it shadow-writes the new generation to fresh pages before pivoting block 0
(never in place), the relation must GROW across a single fold (the in-place rewrite it replaces
kept the size ~constant — that size delta is the structural oracle that distinguishes the two
mechanisms without needing a crash). Later phases add the pending-fold threshold (T3.1) and the
honest cost estimate (T5.1) here.

Uses a plain psycopg2 connection (env PGHOST/PGPORT/...) — these do NOT kill the container, so they
share the module-level connection. Crash tests live in test_am_crash.py.
"""
import os
import re
import subprocess
import threading
import time

import psycopg2
import pytest

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55448")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "theodb")
# When set, the container has THEODB_SCAN_PROFILE=1 and the fold tests read the pages_read runtime metric from
# its server log (the wiring-triad pillar (c) observable). Without it, the pages_read assertions are skipped and
# only correctness is checked.
MAINT_CONTAINER = os.environ.get("THEODB_MAINT_CONTAINER", "")


def _scan_pending_pages(table, query):
    """Run a profiled index scan and return the pending_pages the theodb scan logged — the O(pending) linear scan
    cost that the pending fold eliminates (THEODB_SCAN_PROFILE=1 must be set in the container env). Returns None
    when profiling is unavailable."""
    if not MAINT_CONTAINER:
        return None
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres")
    c.autocommit = True
    cur = c.cursor()
    cur.execute("SET enable_seqscan = off")
    cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT 5", (str(query),))
    cur.fetchall()
    c.close()
    logs = subprocess.run(["docker", "logs", "--tail", "40", MAINT_CONTAINER], capture_output=True, text=True)
    hits = re.findall(r"pending_pages=(\d+)", logs.stdout + logs.stderr)
    return int(hits[-1]) if hits else None


@pytest.fixture()
def conn():
    c = psycopg2.connect(
        host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres",
        connect_timeout=5,
    )
    c.autocommit = True
    yield c
    c.close()


def _make_index(cur, table, am, opclass, n=400, dim=8):
    """Build a structured index over `n` deterministic vectors and return the ground-truth rows."""
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    rows = []
    for i in range(n):
        # Well-separated, distinct vectors (avoid the tie-heavy modular data that makes ANN top-k
        # ambiguous): each id gets a unique point whose distance to a query grows smoothly with id.
        vec = [float(i + j * 1000) for j in range(dim)]
        rows.append((i, vec))
        cur.execute(f"INSERT INTO {table} VALUES (%s, %s)", (i, str(vec)))
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING {am} (v {opclass})")
    return rows


def _index_knn(cur, table, query, k):
    cur.execute("SET enable_seqscan = off")
    cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT {k}", (str(query),))
    return [r[0] for r in cur.fetchall()]


def _seqscan_knn(cur, table, query, k):
    """Ground-truth top-k via seqscan — NEVER use the index as its own oracle (SEPA)."""
    cur.execute("SET enable_indexscan = off")
    cur.execute("SET enable_bitmapscan = off")
    cur.execute("SET enable_seqscan = on")
    cur.execute(
        f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT {k}", (str(query),)
    )
    out = [r[0] for r in cur.fetchall()]
    cur.execute("RESET enable_indexscan")
    cur.execute("RESET enable_bitmapscan")
    return out


def _rel_size(cur, table):
    cur.execute(f"SELECT pg_relation_size('{table}_idx')")
    return cur.fetchone()[0]


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_fold_preserves_scan_results(conn, am, opclass):
    """A VACUUM fold preserves scan correctness AND grows the relation (shadow-write, not in-place)."""
    cur = conn.cursor()
    table = f"m48_fold_{am}"
    _make_index(cur, table, am, opclass, n=400, dim=8)
    query = [40.0 + j * 1000 for j in range(8)]  # near id 40

    size_pre = _rel_size(cur, table)
    cur.execute(f"DELETE FROM {table} WHERE id % 10 < 3")  # ~30% dead
    # INDEX_CLEANUP ON forces ambulkdelete → the fold path. Plain VACUUM on this tiny table (heap < 32 pages)
    # hits PG's index-vacuum bypass (vacuumlazy.c BYPASS_THRESHOLD_PAGES) and never calls ambulkdelete, so the
    # fold would not run. A real (large) table triggers ambulkdelete without the flag; the flag only forces the
    # path deterministically on the small fixture.
    cur.execute(f"VACUUM (INDEX_CLEANUP ON) {table}")
    size_post = _rel_size(cur, table)

    # Correctness (ANN is approximate — assert recall overlap, not exact order): the fold must keep the
    # index returning the true nearest neighbours of the LIVE corpus. A corrupt fold (stale bytes scored
    # as vectors) would collapse this overlap. No dead ids may appear (heap-visibility filter works).
    idx_top = _index_knn(cur, table, query, 10)
    truth = set(_seqscan_knn(cur, table, query, 10))
    assert all(i % 10 >= 3 for i in idx_top), f"{am}: fold returned a deleted id: {idx_top}"
    overlap = len(set(idx_top) & truth)
    assert overlap >= 8, f"{am}: fold degraded recall (overlap {overlap}/10; idx={idx_top} truth={truth})"

    # Structural oracle (the RED/GREEN discriminator): the crash-safe fold shadow-writes the new
    # generation to FRESH pages before pivoting block 0, so the relation GROWS across one fold. The
    # in-place rewrite it replaces kept the size ~constant. (Reclaim / bounded growth is T2.2.)
    assert size_post > size_pre, (
        f"{am}: relation did not grow after fold ({size_pre} -> {size_post}) — "
        "fold appears to still rewrite in place (pre-#47-fix behaviour)"
    )
    cur.execute(f"DROP TABLE {table}")


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_fold_reclaims_pages(conn, am, opclass):
    """T2.2: consecutive folds must NOT grow the relation unboundedly — the second fold reuses the dead
    low region left by the first (generation alternation), so its size does not exceed the first fold's.
    A pure tail-append fold (no reclaim) would grow strictly on every fold."""
    cur = conn.cursor()
    table = f"m48_reclaim_{am}"
    _make_index(cur, table, am, opclass, n=500, dim=8)

    # INDEX_CLEANUP ON forces the fold path on this small fixture (see test_fold_preserves for why plain VACUUM
    # bypasses it). Both folds must actually run for the reclaim invariant to be exercised.
    cur.execute(f"DELETE FROM {table} WHERE id % 10 < 3")
    cur.execute(f"VACUUM (INDEX_CLEANUP ON) {table}")
    size_fold1 = _rel_size(cur, table)

    cur.execute(f"DELETE FROM {table} WHERE id % 10 >= 7")
    cur.execute(f"VACUUM (INDEX_CLEANUP ON) {table}")
    size_fold2 = _rel_size(cur, table)

    # The second fold reuses the region freed by the first ⇒ size does not grow. (Bounded growth; a fully
    # minimal / shrinking reclaim is M55.) Correctness still holds after two folds.
    assert size_fold2 <= size_fold1, (
        f"{am}: relation grew across folds ({size_fold1} -> {size_fold2}) — reclaim did not reuse freed space"
    )
    query = [40.0 + j * 1000 for j in range(8)]
    idx_top = _index_knn(cur, table, query, 5)
    assert all(i % 10 >= 3 and i % 10 < 7 for i in idx_top), f"{am}: stale/deleted id after two folds: {idx_top}"
    cur.execute(f"DROP TABLE {table}")


def _insert_pending(cur, table, start, count, dim=8):
    """Insert `count` rows AFTER the index exists ⇒ they land in the pending region (aminsert append)."""
    for i in range(start, start + count):
        cur.execute(f"INSERT INTO {table} VALUES (%s, %s)", (i, str([float(i + j * 1000) for j in range(dim)])))


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_insert_only_vacuum_folds_pending_above_threshold(conn, am, opclass):
    """T3.1/D3: an insert-only workload (no deletes) accumulates a pending region; a VACUUM must fold it into the
    main structure once it exceeds theodb.vacuum_pending_threshold, so the scan returns to O(structure). Proven
    on two lenses: pages_read (THEODB_SCAN_PROFILE runtime metric) drops, AND the top-k stays correct."""
    cur = conn.cursor()
    table = f"m48_pend_{am}"
    _make_index(cur, table, am, opclass, n=120, dim=8)
    _insert_pending(cur, table, 120, 800)  # enough rows for several pending pages
    query = [60.0 + j * 1000 for j in range(8)]

    pend_before = _scan_pending_pages(table, query)
    if pend_before is None:
        pytest.skip("THEODB_SCAN_PROFILE not enabled — pending_pages metric unavailable")
    assert pend_before >= 2, f"{am}: test needs ≥2 pending pages, got {pend_before}"
    cur.execute("SET theodb.vacuum_pending_threshold = 1")  # well below pending ⇒ folds
    cur.execute(f"VACUUM (INDEX_CLEANUP ON) {table}")  # force index cleanup (PG14+ skips it insert-only) ⇒ pending fold
    pend_after = _scan_pending_pages(table, query)

    # The pending region is now folded into the structure ⇒ the O(pending) scan cost is gone.
    assert pend_after == 0, f"{am}: pending not folded ({pend_before} -> {pend_after} pages)"
    # Correctness: the folded index still returns the true nearest neighbours of the full corpus.
    idx_top = _index_knn(cur, table, query, 10)
    truth = set(_seqscan_knn(cur, table, query, 10))
    assert len(set(idx_top) & truth) >= 8, f"{am}: fold degraded recall: {idx_top} vs {truth}"
    cur.execute(f"DROP TABLE {table}")


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_pending_threshold_boundary(conn, am, opclass):
    """EC-6: strict `>` — pending == threshold does NOT fold; threshold+1 folds. A high threshold leaves the
    pending in place (scan still correct via the pending scan); a low one folds it."""
    cur = conn.cursor()
    table = f"m48_bound_{am}"
    _make_index(cur, table, am, opclass, n=80, dim=8)
    _insert_pending(cur, table, 80, 800)
    query = [50.0 + j * 1000 for j in range(8)]

    p = _scan_pending_pages(table, query)
    if p is None:
        pytest.skip("THEODB_SCAN_PROFILE not enabled — pending_pages metric unavailable")
    assert p >= 2, f"{am}: boundary test needs ≥2 pending pages, got {p}"

    # threshold == pending: strict `>` ⇒ does NOT fold (EC-6). Pending stays exactly p.
    cur.execute(f"SET theodb.vacuum_pending_threshold = {p}")
    cur.execute(f"VACUUM (INDEX_CLEANUP ON) {table}")
    assert _scan_pending_pages(table, query) == p, f"{am}: folded at threshold==pending (should be strict >)"

    # threshold == pending-1: now `p > p-1` ⇒ folds. Pending goes to 0.
    cur.execute(f"SET theodb.vacuum_pending_threshold = {p - 1}")
    cur.execute(f"VACUUM (INDEX_CLEANUP ON) {table}")
    assert _scan_pending_pages(table, query) == 0, f"{am}: did not fold at threshold==pending-1"
    assert len(set(_index_knn(cur, table, query, 10)) & set(_seqscan_knn(cur, table, query, 10))) >= 8
    cur.execute(f"DROP TABLE {table}")


def test_vacuum_cleanup_never_errors_across_states(conn):
    """SEPA MAJOR (EC-3 on the maintenance path): a routine VACUUM must NEVER abort on the pending-fold check.
    Exercise the states a cleanup can hit — freshly built (no pending), below threshold, above threshold, and
    empty — all must complete without error. (A legacy-v1/torn-meta fixture needs a cross-binary build like
    EC-1; pending_page_count swallows that Err to 0 by construction — followup for the fixture.)"""
    cur = conn.cursor()
    table = "m48_vcleanup"
    _make_index(cur, table, "theodb_hnsw", "theodb_hnsw_l2_ops", n=60, dim=8)
    cur.execute(f"VACUUM {table}")  # no pending
    _insert_pending(cur, table, 60, 3)
    cur.execute("SET theodb.vacuum_pending_threshold = 65536")
    cur.execute(f"VACUUM {table}")  # pending below threshold
    _insert_pending(cur, table, 63, 200)
    cur.execute("SET theodb.vacuum_pending_threshold = 2")
    cur.execute(f"VACUUM {table}")  # pending above threshold → folds
    cur.execute(f"DELETE FROM {table}")
    cur.execute(f"VACUUM {table}")  # empty
    cur.execute(f"SELECT 1")  # connection still healthy, no error was raised
    assert cur.fetchone()[0] == 1
    cur.execute(f"DROP TABLE {table}")


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_fold_empty_corpus(conn, am, opclass):
    """EC-5: folding an emptied index (DELETE ALL + VACUUM) yields a valid empty index, still writable."""
    cur = conn.cursor()
    table = f"m48_empty_{am}"
    _make_index(cur, table, am, opclass, n=200, dim=8)
    cur.execute(f"DELETE FROM {table}")
    cur.execute(f"VACUUM (INDEX_CLEANUP ON) {table}")  # force the fold-to-empty path on the small fixture

    # empty scan returns nothing, no error
    assert _index_knn(cur, table, [1.0] * 8, 5) == []
    # still writable: a fresh INSERT is scannable through the index
    cur.execute(f"INSERT INTO {table} VALUES (999, %s)", (str([2.0] * 8),))
    assert _index_knn(cur, table, [2.0] * 8, 1) == [999]
    cur.execute(f"DROP TABLE {table}")


def _connect():
    return psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres",
                            connect_timeout=5)


def test_cancel_interrupts_create_index():
    """T4.1/D4: a long parallel CREATE INDEX must respond to pg_cancel_backend within ~one batch, not run to
    completion. Anti-flake (EC-8): measure a full build baseline first; skip if it is too fast to cancel
    reliably; otherwise cancel a second build after baseline/4 and assert it errors ('canceling statement')
    well before a full build would finish, leaves no index, and a re-CREATE works."""
    su = _connect(); su.autocommit = True
    cur = su.cursor()
    cur.execute("DROP TABLE IF EXISTS m48_cancel")
    cur.execute("CREATE TABLE m48_cancel (id int, v vector(32))")
    cur.execute(
        "INSERT INTO m48_cancel SELECT g, array_fill((g / 97.0)::real, ARRAY[32])::vector "
        "FROM generate_series(1, 500000) g"  # ~6s parallel build on a quiet box — long enough to cancel
    )
    su.close()

    # Baseline: time one full CREATE INDEX (the parallel batched build).
    b = _connect(); b.autocommit = True
    bc = b.cursor()
    t0 = time.time()
    bc.execute("CREATE INDEX m48_cancel_base ON m48_cancel USING theodb_hnsw (v theodb_hnsw_l2_ops)")
    baseline = time.time() - t0
    bc.execute("DROP INDEX m48_cancel_base")
    b.close()
    if baseline < 3.0:
        pytest.skip(f"parallel build too fast ({baseline:.1f}s) to cancel reliably on this box (EC-8)")

    # Cancel run: a worker thread builds; the main thread cancels its backend after baseline/4.
    ca = _connect(); ca.autocommit = True
    acur = ca.cursor()
    acur.execute("SELECT pg_backend_pid()")
    pid = acur.fetchone()[0]
    result = {}

    def _build():
        try:
            acur.execute("CREATE INDEX m48_cancel_x ON m48_cancel USING theodb_hnsw (v theodb_hnsw_l2_ops)")
            result["ok"] = True
        except Exception as e:  # noqa: BLE001 — we assert the specific typed message below
            result["err"] = e

    th = threading.Thread(target=_build)
    start = time.time()
    th.start()
    time.sleep(baseline / 4)
    killer = _connect(); killer.autocommit = True
    killer.cursor().execute(f"SELECT pg_cancel_backend({pid})")
    killer.close()
    th.join(timeout=baseline)
    elapsed = time.time() - start
    ca.close()

    assert "err" in result, f"CREATE INDEX was not cancelled (ran to completion in {elapsed:.1f}s)"
    assert "cancel" in str(result["err"]).lower(), f"unexpected error (not a cancel): {result['err']}"
    assert elapsed < baseline * 0.9, f"cancel latency too high ({elapsed:.1f}s vs baseline {baseline:.1f}s)"

    # No orphan index, and a fresh CREATE INDEX still works.
    chk = _connect(); chk.autocommit = True
    cc = chk.cursor()
    cc.execute("SELECT count(*) FROM pg_class WHERE relname = 'm48_cancel_x'")
    assert cc.fetchone()[0] == 0, "cancelled CREATE INDEX left an orphan index relation"
    cc.execute("CREATE INDEX m48_cancel_x ON m48_cancel USING theodb_hnsw (v theodb_hnsw_l2_ops)")
    cc.execute("DROP TABLE m48_cancel")
    chk.close()


# ---------------------------------------------------------------------------
# T5.1 — honest amcostestimate (planner picks seqscan at small N, index at realistic N)
# ---------------------------------------------------------------------------

def _bulk_make_index(cur, table, am, opclass, n, dim=8):
    """Server-side bulk build of the same deterministic vectors as _make_index (id i → [i, i+1000, ...]) so a
    50k-row table builds in one INSERT ... SELECT, not n round-trips. ANALYZE after so the planner's cost model
    reads real reltuples (amcostestimate keys off (*indexinfo).tuples)."""
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    cur.execute(
        f"INSERT INTO {table} SELECT g, (SELECT array_agg((g + j * 1000)::real ORDER BY j) "
        f"FROM generate_series(0, {dim - 1}) j)::vector FROM generate_series(0, {n - 1}) g"
    )
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING {am} ({'v ' + opclass})")
    cur.execute(f"ANALYZE {table}")


def _explain(cur, table, query, k):
    cur.execute(f"EXPLAIN SELECT id FROM {table} ORDER BY v <-> %s LIMIT {k}", (str(query),))
    return "\n".join(r[0] for r in cur.fetchall())


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_planner_prefers_seqscan_small_n(conn, am, opclass):
    """T5.1/D5: with an honest cost, a 100-row table's ORDER BY <-> LIMIT is cheaper as a seqscan+sort than an
    index scan (the index's startup ≈ its total when ratio≈1). The pre-fix stub returned cost 0 ⇒ index always
    won even here (lying to the planner). Asserts the CORRECT new behaviour: no Index Scan at small N."""
    cur = conn.cursor()
    table = f"m48_cost_small_{am}"
    _bulk_make_index(cur, table, am, opclass, n=100, dim=8)
    plan = _explain(cur, table, [50.0 + j * 1000 for j in range(8)], 5)
    assert "Index Scan" not in plan, f"{am}: small N should NOT use the index (seqscan+sort wins) — plan:\n{plan}"
    cur.execute(f"DROP TABLE {table}")


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_planner_prefers_index_realistic_n(conn, am, opclass):
    """T5.1/D5: at realistic scale (50k rows) the ANN index's ordered scan (ratio≪1 ⇒ tiny startup) beats a
    full seqscan+sort for ORDER BY <-> LIMIT — the pushdown must survive the honest cost. Asserts Index Scan."""
    cur = conn.cursor()
    table = f"m48_cost_big_{am}"
    _bulk_make_index(cur, table, am, opclass, n=50000, dim=8)
    plan = _explain(cur, table, [25000.0 + j * 1000 for j in range(8)], 5)
    assert "Index Scan" in plan, f"{am}: realistic N should use the index — plan:\n{plan}"
    cur.execute(f"DROP TABLE {table}")


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_costestimate_never_errors_on_empty_index(conn, am, opclass):
    """T5.1/EC-3: amcostestimate must NEVER error — a costestimate that error!'d would abort ALL query planning
    (e.g. during a concurrent VACUUM that momentarily makes the meta unreadable). Exercise the tuples==0 /
    freshly-emptied fallback (ratio=1.0) and assert EXPLAIN still emits a plan with no error."""
    cur = conn.cursor()
    table = f"m48_cost_empty_{am}"
    _make_index(cur, table, am, opclass, n=10, dim=8)
    cur.execute(f"DELETE FROM {table}")
    cur.execute(f"ANALYZE {table}")  # reltuples → 0, drives the tuples<=0 fallback branch
    plan = _explain(cur, table, [1.0] * 8, 5)  # must not raise
    assert "->" in plan or "Scan" in plan, f"{am}: EXPLAIN emitted no plan:\n{plan}"
    cur.execute(f"DROP TABLE {table}")
