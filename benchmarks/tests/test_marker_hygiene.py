"""Coverage-rot guard: a test that needs a live Postgres MUST carry the `integration` marker.

Why this exists: CI runs the "fast, container-free" tier as `pytest -m "not integration"`. A test
module that opens a real connection but forgets the marker is silently pulled into that tier, where
no database exists — so it does not fail fast, it burns its `connect_timeout` and then errors. Five
such modules accumulated (test_am_cosine_ip, test_am_crash, test_am_maintenance,
test_extension_install, test_unified) and turned a job whose own description says "Fast,
container-free" into **999 seconds that ended in failure** — ~80% of the whole CI run, every run.

The failure mode is invisible to a normal review: each module looks correct in isolation, and the
suite is green locally where a dev Postgres happens to be listening. Only the marker/runner
*combination* is wrong. This guard makes the invariant executable so the sixth one cannot land.

HONESTLY HEURISTIC: detection is source-level (regex over `psycopg2.connect(`), not behavioural —
it cannot see a connection opened through an indirection. It is a ratchet against the common case,
not a proof. A module that connects via a helper still needs human judgement.
"""
import pathlib
import re

TESTS_DIR = pathlib.Path(__file__).parent

# A connection to port 1 is a DELIBERATE dead endpoint: those tests assert failure handling
# (typed error, timeout) and are correctly unit-tier — they must NOT be marked integration.
DEAD_ENDPOINT = re.compile(r"port=1[,\s)]")
OPENS_CONNECTION = re.compile(r"psycopg2\.connect\(")
IS_MARKED = re.compile(r"pytestmark\s*=.*integration|@pytest\.mark\.integration")


def _modules_needing_a_live_db():
    """Test modules whose source opens a Postgres connection to a real endpoint."""
    for path in sorted(TESTS_DIR.glob("test_*.py")):
        if path.name == pathlib.Path(__file__).name:
            continue
        source = path.read_text()
        if OPENS_CONNECTION.search(source) and not DEAD_ENDPOINT.search(source):
            yield path, source


def test_every_db_test_module_declares_the_integration_marker():
    """Given a module that opens a live Postgres connection, it declares `integration`.

    Without the marker the module runs in the container-free tier, where it cannot pass — it can
    only time out. The assertion message names each offender so the fix is mechanical.
    """
    unmarked = [
        path.name for path, source in _modules_needing_a_live_db() if not IS_MARKED.search(source)
    ]

    assert unmarked == [], (
        "these modules open a live Postgres connection but do not declare the `integration` "
        f"marker, so `pytest -m 'not integration'` will run them without a database: {unmarked}. "
        "Add `pytestmark = pytest.mark.integration` at module level, or point the test at a dead "
        "endpoint if it is really asserting connection-failure handling."
    )


def test_guard_recognises_a_dead_endpoint_as_unit_tier():
    """Positive control: the guard must NOT demand the marker for deliberate-failure tests.

    If this ever fails, the guard has become over-eager and would force `integration` onto genuine
    unit tests — pushing them out of the fast tier and hiding real regressions behind a container.
    """
    assert DEAD_ENDPOINT.search('psycopg2.connect(host="127.0.0.1", port=1, connect_timeout=1)')
    assert not DEAD_ENDPOINT.search('psycopg2.connect(host=PGHOST, port=PGPORT)')
