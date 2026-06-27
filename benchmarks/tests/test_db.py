"""Unit tests for the DB adapter (no container needed — boundary errors + SQL shape)."""
import pytest

from theodb_bench.db import DBUnavailableError, VectorDB


def test_connect_raises_dbunavailable_on_bad_dsn():
    # port 1 is unbound -> connect fails fast -> typed error with the DSN context in the message
    db = VectorDB("host=127.0.0.1 port=1 dbname=x user=x connect_timeout=1")
    with pytest.raises(DBUnavailableError, match="cannot connect"):
        db.connect()


def test_operation_without_connect_raises_dbunavailable():
    db = VectorDB("host=127.0.0.1 port=1 dbname=x")
    with pytest.raises(DBUnavailableError, match="not connected"):
        db.ping()


def test_topk_sql_shape_l2():
    sql = VectorDB._topk_sql("items", "embedding", 5, "l2")
    assert "ORDER BY" in sql
    assert "<->" in sql
    assert "LIMIT 5" in sql
    assert "::vector" in sql


def test_topk_sql_shape_cosine():
    sql = VectorDB._topk_sql("items", "embedding", 10, "cosine")
    assert "<=>" in sql
    assert "LIMIT 10" in sql


def test_op_for_metric():
    assert VectorDB._op_for_metric("l2") == "<->"
    assert VectorDB._op_for_metric("cosine") == "<=>"


def test_op_for_unknown_metric_raises():
    with pytest.raises(ValueError):
        VectorDB._op_for_metric("manhattan")
