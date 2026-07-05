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


def wait_ready(timeout=60):
    """Poll container-running + pg_isready + a REAL connection (pg_isready can accept
    during recovery — SEPA gotcha 2)."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        running = _docker("inspect", "-f", "{{.State.Running}}", CONTAINER, check=False)
        if running.stdout.strip() == "true":
            ready = _docker("exec", CONTAINER, "pg_isready", "-U", PGUSER, check=False)
            if ready.returncode == 0:
                try:
                    conn = connect()
                    conn.close()
                    return
                except psycopg2.OperationalError:
                    pass  # still in recovery — keep polling
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
