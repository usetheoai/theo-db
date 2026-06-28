"""M6 columnar/HTAP measurement test (pg_mooncake). Gated on pg_mooncake being available (the throwaway
packaging/Dockerfile.columnar substrate); skips cleanly otherwise (no silent green)."""
import os

import pytest

from theodb_bench.columnar import run_columnar_vs_row
from theodb_bench.db import DBUnavailableError, VectorDB

pytestmark = pytest.mark.integration


def _dsn() -> str:
    return (
        f"host={os.environ.get('PGHOST', 'localhost')} "
        f"port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} "
        f"user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )


@pytest.fixture
def db():
    d = VectorDB(_dsn()).connect()
    d.ping()
    yield d
    d.close()


def _require_mooncake(db):
    if not db.pg_mooncake_available():
        pytest.skip("pg_mooncake not available — run against packaging/Dockerfile.columnar "
                    "(canonical pg_mooncake distribution)")


# module-scoped single run reused across assertions (the seed+mirror is the expensive part)
@pytest.fixture(scope="module")
def result():
    d = VectorDB(_dsn()).connect()
    d.ping()
    try:
        if not d.pg_mooncake_available():
            pytest.skip("pg_mooncake not available")
        yield run_columnar_vs_row(d, n=100000)
    finally:
        d.close()


def test_columnar_skips_without_extension(db):
    if db.pg_mooncake_available():
        pytest.skip("pg_mooncake IS available here — absent-path covered on the shipped image")
    with pytest.raises(DBUnavailableError) as exc:
        db.ensure_mooncake_extension()  # fails loud, not a silent NULL
    assert "pg_mooncake" in str(exc.value)


def test_columnar_mirror_matches_row(result):
    # correctness: the columnstore mirror aggregate equals the row-store aggregate
    assert result["match"], (
        f"mirror != row\nrow={result['row']['result']}\ncol={result['columnar']['result']}"
    )
    assert len(result["columnar"]["result"]) == 5  # 5 categories


def test_columnar_uses_duckdb_plan(result):
    # DoD-2: the mirror query plans as DuckDBScan; the row query as Seq Scan
    assert "DuckDBScan" in result["columnar"]["plan"], result["columnar"]["plan"][:300]
    assert "Seq Scan" in result["row"]["plan"], result["row"]["plan"][:300]


def test_columnar_reports_timings(result):
    assert result["row"]["ms"] > 0 and result["columnar"]["ms"] > 0


def test_columnar_create_mirror_nonexistent_base_raises(db):
    _require_mooncake(db)
    db.ensure_mooncake_extension()
    with pytest.raises(DBUnavailableError):  # typed error, not silent
        db.create_columnstore_mirror("cs_nope", "does_not_exist_base")
