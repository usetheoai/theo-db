"""M48 crash-safety tests for the theodb index AMs (plan m48-am-crash-safety).

These are REAL crash tests: they SIGKILL the postgres container (power-loss analog — the
pattern of core's src/test/recovery/t/013_crash_restart.pl, blueprint §Q6) and assert the
post-recovery contract:

  * selective durability — LOGGED control table keeps its rows; UNLOGGED resets to empty
  * writability — INSERT + index scan work after recovery (issue #46: pre-fix this fails
    with "truncated meta page" because GenericXLog never WAL-logs the INIT fork; the fix
    under test is ``wal_log_init_fork`` in theodb_rs/src/am/build.rs — log_newpage_range
    over the INIT fork, called as the last step of both ambuildempty callbacks)

Container contract: a dedicated container (default ``theodb-m48-verify`` on port 55448)
whose lifecycle THIS file owns — each test performs its own kill/restart cycle
(independence, testing.md §3). Never point these tests at a shared/long-lived container.
"""
import os
import subprocess
import time

import psycopg2
import pytest

CONTAINER = os.environ.get("THEODB_CRASH_CONTAINER", "theodb-m48-verify")
PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55448")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "theodb")


def _docker(*args, check=True):
    return subprocess.run(["docker", *args], check=check, capture_output=True, text=True)


def connect():
    return psycopg2.connect(
        host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres",
        connect_timeout=5,
    )


def wait_ready(timeout=90):
    """Poll container-running + pg_isready + TWO consecutive real connections that can run a query — an abort()
    crash restarts the whole postmaster in place, and a single successful connect can still race the tail of
    crash recovery (a backend accepted mid-recovery then killed). Requiring two stable query round-trips a
    beat apart makes the crash tests robust when run back-to-back (SEPA gotcha 2, hardened)."""
    deadline = time.time() + timeout
    stable = 0
    while time.time() < deadline:
        running = _docker("inspect", "-f", "{{.State.Running}}", CONTAINER, check=False)
        if running.stdout.strip() == "true":
            ready = _docker("exec", CONTAINER, "pg_isready", "-U", PGUSER, check=False)
            if ready.returncode == 0:
                try:
                    conn = connect()
                    conn.cursor().execute("SELECT 1")
                    conn.close()
                    stable += 1
                    if stable >= 2:
                        return
                    time.sleep(0.5)
                    continue
                except (psycopg2.OperationalError, psycopg2.errors.InternalError_):
                    stable = 0  # a failed round-trip resets the stability counter
        time.sleep(1)
    raise TimeoutError(f"container {CONTAINER} not ready after {timeout}s")


def crash_and_restart():
    """SIGKILL the postmaster (PID 1 of the container) — the power-loss analog. A clean
    `docker stop`/`restart` would checkpoint and mask issue #46 (SEPA gotcha: RED falso-verde)."""
    _docker("kill", "-s", "KILL", CONTAINER)
    # container may need a beat to reach the stopped state before start is accepted
    for _ in range(10):
        state = _docker("inspect", "-f", "{{.State.Running}}", CONTAINER, check=False)
        if state.stdout.strip() == "false":
            break
        time.sleep(0.5)
    _docker("start", CONTAINER)
    wait_ready()


@pytest.fixture()
def conn():
    wait_ready()
    c = connect()
    c.autocommit = True
    yield c
    try:
        c.close()
    except Exception:
        pass


def _setup_control(cur, name):
    cur.execute(f"DROP TABLE IF EXISTS {name}")
    cur.execute(f"CREATE TABLE {name} (id int)")
    cur.execute(f"INSERT INTO {name} VALUES (1)")


def _assert_post_crash_contract(table, index_am, control):
    """Post-recovery asserts shared by the three #46 tests (selective durability +
    writability). Reconnect from scratch — pre-crash connections are dead."""
    conn2 = connect()
    conn2.autocommit = True
    cur = conn2.cursor()
    try:
        # selective durability: LOGGED control kept its committed row
        cur.execute(f"SELECT count(*) FROM {control}")
        assert cur.fetchone()[0] == 1, "LOGGED control table lost committed data"
        # UNLOGGED resets to empty — a VALID empty state, not an error
        cur.execute(f"SELECT count(*) FROM {table}")
        assert cur.fetchone()[0] == 0, "UNLOGGED table should reset to empty on crash recovery"
        # writability: INSERT goes through the index aminsert path (issue #46 fails HERE
        # pre-fix: 'truncated meta page' — the INIT fork never reached the WAL)
        cur.execute(f"INSERT INTO {table} VALUES (2, '[5,6,7,8]')")
        # index scan works and sees the new row
        cur.execute(f"SET enable_seqscan = off")
        cur.execute(f"SELECT id FROM {table} ORDER BY v <-> '[5,6,7,8]' LIMIT 1")
        assert cur.fetchone()[0] == 2, "index scan after recovery did not return the inserted row"
    finally:
        cur.execute(f"DROP TABLE IF EXISTS {table}")
        cur.execute(f"DROP TABLE IF EXISTS {control}")
        conn2.close()


def test_unlogged_index_survives_crash_restart(conn):
    """#46 (hnsw): UNLOGGED table + theodb_hnsw index survives SIGKILL + recovery."""
    cur = conn.cursor()
    _setup_control(cur, "m48_ctrl_hnsw")
    cur.execute("DROP TABLE IF EXISTS m48_unlogged_hnsw")
    cur.execute("CREATE UNLOGGED TABLE m48_unlogged_hnsw (id int, v vector(4))")
    cur.execute(
        "CREATE INDEX m48_unlogged_hnsw_idx ON m48_unlogged_hnsw USING theodb_hnsw (v theodb_hnsw_l2_ops)"
    )
    cur.execute("INSERT INTO m48_unlogged_hnsw VALUES (1, '[1,2,3,4]')")
    # no CHECKPOINT here — an explicit or implicit checkpoint would materialize the INIT
    # fork and mask the missing WAL (SEPA gotcha 1); kill immediately
    crash_and_restart()
    _assert_post_crash_contract("m48_unlogged_hnsw", "theodb_hnsw", "m48_ctrl_hnsw")


def test_unlogged_ivfflat_survives_crash_restart(conn):
    """#46 (ivfflat): same contract for the IVF AM (blob INIT fork path)."""
    cur = conn.cursor()
    _setup_control(cur, "m48_ctrl_ivf")
    cur.execute("DROP TABLE IF EXISTS m48_unlogged_ivf")
    cur.execute("CREATE UNLOGGED TABLE m48_unlogged_ivf (id int, v vector(4))")
    cur.execute(
        "CREATE INDEX m48_unlogged_ivf_idx ON m48_unlogged_ivf USING theodb_ivfflat (v theodb_ivfflat_l2_ops)"
    )
    cur.execute("INSERT INTO m48_unlogged_ivf VALUES (1, '[1,2,3,4]')")
    crash_and_restart()
    _assert_post_crash_contract("m48_unlogged_ivf", "theodb_ivfflat", "m48_ctrl_ivf")


def test_alter_set_unlogged_survives_crash(conn):
    """#46 via EC-7: ALTER TABLE SET UNLOGGED is the second production path that calls
    ambuildempty — same post-recovery contract."""
    cur = conn.cursor()
    _setup_control(cur, "m48_ctrl_alter")
    cur.execute("DROP TABLE IF EXISTS m48_alter_unlogged")
    cur.execute("CREATE TABLE m48_alter_unlogged (id int, v vector(4))")
    cur.execute(
        "CREATE INDEX m48_alter_unlogged_idx ON m48_alter_unlogged USING theodb_hnsw (v theodb_hnsw_l2_ops)"
    )
    cur.execute("INSERT INTO m48_alter_unlogged VALUES (1, '[1,2,3,4]')")
    cur.execute("ALTER TABLE m48_alter_unlogged SET UNLOGGED")
    crash_and_restart()
    _assert_post_crash_contract("m48_alter_unlogged", "theodb_hnsw", "m48_ctrl_alter")


# --- T2.3: deterministic crash-injection in the VACUUM fold (issue #47 gate + ADR 0014 fail-loud proof) ---
#
# The theodb.test_crash_after_pages / test_crash_phase GUCs make the fold abort() the backend at a chosen point.
# abort() is a real backend crash: the postmaster terminates all backends and runs crash recovery IN PLACE (the
# container stays up), so these tests just wait for recovery and reconnect — no docker kill needed.

HNSW = ("theodb_hnsw", "theodb_hnsw_l2_ops")
IVF = ("theodb_ivfflat", "theodb_ivfflat_l2_ops")


def _seed_index(cur, table, am, opclass, n=600, dim=8):
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    for i in range(n):
        cur.execute(f"INSERT INTO {table} VALUES (%s, %s)", (i, str([float(i + j * 1000) for j in range(dim)])))
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING {am} (v {opclass})")


def _run_expecting_crash(setup_sql, crash_sql):
    """Run `crash_sql` after `setup_sql` on a fresh connection, expecting the backend to be killed by the
    fold's abort(). Then wait for the postmaster's in-place crash recovery to finish."""
    c = connect()
    c.autocommit = True
    cur = c.cursor()
    for s in setup_sql:
        cur.execute(s)
    with pytest.raises(psycopg2.OperationalError):
        cur.execute(crash_sql)
        cur.execute("SELECT 1")  # force the socket to notice the dead backend
    try:
        c.close()
    except Exception:
        pass
    wait_ready()


def _live_ids_via_seqscan(table, query, k):
    c = connect(); c.autocommit = True
    cur = c.cursor()
    cur.execute("SET enable_indexscan = off"); cur.execute("SET enable_bitmapscan = off")
    cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT {k}", (str(query),))
    out = [r[0] for r in cur.fetchall()]
    c.close()
    return out


@pytest.mark.parametrize("am,opclass", [HNSW, IVF])
def test_crash_mid_fold_pre_pivot_leaves_old_generation(am, opclass):
    """#47 gate: a crash while writing the new generation (before the pivot) leaves block 0 pointing at the OLD
    generation. The scan is FAIL-LOUD: it either returns the correct live neighbours OR raises a typed REINDEX
    error — NEVER a silently-wrong result. (When the fold extends at the tail, the crash leaves orphan body pages
    inside the old pending range; read_pending rejects them fail-loud. Fully-clean recovery here is the same M55
    window as the reclaim — ADR 0014.)"""
    wait_ready()
    table = f"m48_crash_pre_{am}"
    c = connect(); c.autocommit = True
    _seed_index(c.cursor(), table, am, opclass)
    c.cursor().execute(f"DELETE FROM {table} WHERE id % 10 < 3")
    c.close()
    _run_expecting_crash(["SET theodb.test_crash_after_pages = 2"], f"VACUUM {table}")

    query = [40.0 + j * 1000 for j in range(8)]
    c = connect(); c.autocommit = True
    cur = c.cursor()
    cur.execute("SET enable_seqscan = off")
    try:
        cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT 10", (str(query),))
        idx_top = [r[0] for r in cur.fetchall()]
        # Branch A — usable: correct live neighbours, no deleted id.
        assert all(i % 10 >= 3 for i in idx_top), f"{am}: deleted id after pre-pivot crash: {idx_top}"
        truth = set(_live_ids_via_seqscan(table, query, 10))
        assert len(set(idx_top) & truth) >= 8, f"{am}: recall collapsed after pre-pivot crash"
    except psycopg2.errors.InternalError_ as e:
        # Branch B — fail-loud: typed REINDEX/orphan error, never a silently-wrong scan.
        msg = str(e).lower()
        assert "reindex" in msg or "orphan" in msg or "corrupt" in msg, f"{am}: non-fail-loud error: {e}"
    c.close()
    c2 = connect(); c2.autocommit = True
    c2.cursor().execute(f"DROP TABLE {table}")
    c2.close()


@pytest.mark.parametrize("am,opclass", [HNSW, IVF])
def test_crash_post_pivot_new_generation_then_revacuum_converges(am, opclass):
    """#47 gate: a crash right after the pivot (before reclaim) — block 0 already points at the NEW generation,
    so the gen data is never silently wrong. The scan is fail-loud (consistent OR typed REINDEX, because the
    un-reclaimed leftover still sits in the pending range), and REINDEX (which rebuilds from the heap, ignoring
    the stale index pages) HEALS it to a clean, correct state. A plain re-VACUUM does NOT heal — it reads the
    same polluted pending and fails loud too; REINDEX is the honest recovery. Fully-clean-on-crash is M55
    (ADR 0014 — the crash window may require REINDEX, but never corrupts)."""
    wait_ready()
    table = f"m48_crash_post_{am}"
    c = connect(); c.autocommit = True
    _seed_index(c.cursor(), table, am, opclass)
    c.cursor().execute(f"DELETE FROM {table} WHERE id % 10 < 3")
    c.close()
    _run_expecting_crash(["SET theodb.test_crash_phase = 1"], f"VACUUM {table}")

    query = [40.0 + j * 1000 for j in range(8)]
    c = connect(); c.autocommit = True
    cur = c.cursor()
    cur.execute("SET enable_seqscan = off")
    try:
        cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT 10", (str(query),))
        assert all(i % 10 >= 3 for i in [r[0] for r in cur.fetchall()]), f"{am}: deleted id post-pivot"
    except psycopg2.errors.InternalError_ as e:
        assert "reindex" in str(e).lower() or "orphan" in str(e).lower(), f"{am}: non-fail-loud: {e}"
    # REINDEX (GUC off) heals the index — it rebuilds from the heap, discarding the stale index pages.
    cur.execute("SET theodb.test_crash_phase = 0")
    cur.execute(f"REINDEX INDEX {table}_idx")
    cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT 10", (str(query),))
    healed = [r[0] for r in cur.fetchall()]
    assert all(i % 10 >= 3 for i in healed), f"{am}: REINDEX did not heal: {healed}"
    truth = set(_live_ids_via_seqscan(table, query, 10))
    assert len(set(healed) & truth) >= 8, f"{am}: REINDEX healed but recall low: {healed}"
    cur.execute(f"DROP TABLE {table}")
    c.close()


@pytest.mark.parametrize("am,opclass", [HNSW, IVF])
def test_crash_mid_reclaim_is_fail_loud(am, opclass):
    """ADR 0014 proof: a crash DURING the reclaim step is FAIL-LOUD — after recovery the scan is EITHER exactly
    consistent OR raises a typed REINDEX/truncated error. NEVER a silently-wrong result. This is the honest M55
    boundary: the reclaim window may require REINDEX, but never corrupts."""
    wait_ready()
    table = f"m48_crash_reclaim_{am}"
    c = connect(); c.autocommit = True
    cur = c.cursor()
    _seed_index(cur, table, am, opclass)
    cur.execute(f"DELETE FROM {table} WHERE id % 10 < 3")
    cur.execute(f"VACUUM {table}")  # fold 1 (extend) — next fold reuses the low region and runs a reclaim step
    cur.execute(f"DELETE FROM {table} WHERE id % 10 >= 7")
    c.close()
    _run_expecting_crash(["SET theodb.test_crash_phase = 2"], f"VACUUM {table}")

    query = [40.0 + j * 1000 for j in range(8)]
    c = connect(); c.autocommit = True
    cur = c.cursor()
    cur.execute("SET enable_seqscan = off")
    try:
        cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT 10", (str(query),))
        idx_top = [r[0] for r in cur.fetchall()]
        # Branch A — no error: the result MUST be correct live neighbours (no stale/deleted id).
        assert all(3 <= i % 10 < 7 for i in idx_top), f"{am}: mid-reclaim crash gave a silently wrong scan: {idx_top}"
    except psycopg2.errors.InternalError_ as e:
        # Branch B — typed error: MUST be the fail-loud REINDEX/truncated path, not a generic failure.
        msg = str(e).lower()
        assert "reindex" in msg or "truncated" in msg or "corrupt" in msg, f"{am}: non-fail-loud error: {e}"
    c.close()
    c2 = connect(); c2.autocommit = True
    c2.cursor().execute(f"DROP TABLE {table}")
    c2.close()


def _suguard_setup(cur, table):
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector(8))")
    for i in range(200):
        cur.execute(f"INSERT INTO {table} VALUES (%s, %s)", (i, str([float(i)] * 8)))
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING theodb_hnsw (v theodb_hnsw_l2_ops)")
    cur.execute(f"DELETE FROM {table} WHERE id % 3 = 0")


def test_crash_hook_is_superuser_gated():
    """SECURITY (SEPA CRITICAL + review HIGH refutation): abort() is instance-wide (the postmaster treats a
    SIGABRT'd backend as a crash and recovers everything), so the crash-injection hook is superuser-gated
    (`guc.rs`: `if !superuser() { return }`) — a NON-superuser SET + VACUUM must NOT abort.

    Two halves so the gate proof is not vacuous (review LOW-MED):
      (1) POSITIVE CONTROL — a SUPERUSER with the SAME n=200/GUC=1 setup DOES abort (proves the hook is
          reachable: the backend connection drops, then the instance crash-recovers).
      (2) THE GATE — a NON-superuser with the identical setup does NOT abort (fresh query works, no recovery).
    Without (1), a fold that wrote 0 body pages would make (2) pass vacuously."""
    wait_ready()

    # (1) Positive control: as superuser, the hook fires and aborts the backend.
    su = connect(); su.autocommit = True
    cur = su.cursor()
    _suguard_setup(cur, "m48_suguard_pos")
    cur.execute("SET theodb.test_crash_after_pages = 1")
    aborted = False
    try:
        cur.execute("VACUUM m48_suguard_pos")  # superuser → hook NOT gated → abort() → connection dies
    except (psycopg2.OperationalError, psycopg2.errors.AdminShutdown, psycopg2.InterfaceError):
        aborted = True
    try:
        su.close()
    except Exception:  # noqa: BLE001 — the socket may already be dead from the abort
        pass
    assert aborted, "positive control: superuser VACUUM with GUC=1 did NOT abort — the hook is unreachable, so " \
                    "the gate assertion below would be vacuous. Fix the setup so the fold writes >= 1 body page."
    wait_ready()  # the instance crash-recovers after the abort

    # (2) The gate: a NON-superuser doing the exact DoS attempt must NOT crash the instance.
    su2 = connect(); su2.autocommit = True
    c2 = su2.cursor()
    _suguard_setup(c2, "m48_suguard")
    c2.execute("DROP ROLE IF EXISTS m48_attacker")
    c2.execute("CREATE ROLE m48_attacker LOGIN PASSWORD 'x' NOSUPERUSER")
    c2.execute("GRANT ALL ON m48_suguard TO m48_attacker")
    su2.close()

    atk = psycopg2.connect(host=PGHOST, port=PGPORT, user="m48_attacker", password="x", dbname="postgres")
    atk.autocommit = True
    acur = atk.cursor()
    acur.execute("SET theodb.test_crash_after_pages = 1")  # pgrx lets a non-super SET it in-session...
    acur.execute("VACUUM m48_suguard")  # ...but the superuser-gated hook returns early → no abort
    atk.close()

    # The instance is still up (no crash recovery happened): a fresh connection works immediately, no wait.
    check = connect(); check.autocommit = True
    ccur = check.cursor()
    ccur.execute("SELECT 1")
    assert ccur.fetchone()[0] == 1
    ccur.execute("DROP TABLE m48_suguard")
    ccur.execute("DROP TABLE IF EXISTS m48_suguard_pos")
    ccur.execute("DROP ROLE m48_attacker")
    check.close()


# SECURITY POSTURE (matches the SHIPPED code; verified by repro 2026-07-06):
# The crash GUCs are declared GucContext::Suset. pgrx 0.16.1 does NOT reject a non-superuser *in-session* SET
# on a custom GUC, BUT that only affects the attacker's OWN backend, where the hook's explicit
# `if !superuser() { return }` guard (guc.rs) makes it a no-op — closed and PROVEN by the gate test above
# (with a superuser positive control so the proof is not vacuous). The autovacuum-persistence escalation (a
# non-super persisting the GUC where an autovacuum superuser session would read it) is BLOCKED BY POSTGRES:
# `ALTER DATABASE ... SET` and `ALTER ROLE ... SET` of this Suset GUC by a non-superuser BOTH fail with
# "permission denied to set parameter" (repro 2026-07-06 — the review HIGH is refuted). Defence in depth: the
# hooks are still compiled into the production artifact (ADR D6 — injection_points absent in packaged PG17);
# moving them behind a `crash_injection` cargo feature (fail-closed by construction) is a recommended follow-up
# hardening, not a live hole. Full detail in m48-am-crash-safety-followups.md § Security review.
