"""M130 — DB-free unit tests for the HTAP driver: BenchBase summary parser + tpmC/QphH proxy derivation + the OLAP
result-consistency oracle + the run-to-run dispersion (CV) wiring + the docker-absent skip path."""
import pytest

import run_m130_htap as m


# --- BenchBase combined summary parser (T2.1) ----------------------------

def test_parse_summary_extracts_throughput_and_goodput():
    s = {"Benchmark Type": "tpcc,chbenchmark", "Throughput (requests/second)": 221.71,
         "Goodput (requests/second)": 82.94, "Measured Requests": 26826, "isolation": "TRANSACTION_READ_COMMITTED"}
    out = m.parse_benchbase_summary(s)
    assert out["throughput_rps"] == 221.71 and out["goodput_rps"] == 82.94
    assert out["measured_requests"] == 26826 and out["isolation"] == "TRANSACTION_READ_COMMITTED"


def test_parse_summary_errors_on_missing_throughput():
    with pytest.raises(ValueError):
        m.parse_benchbase_summary({"Benchmark Type": "tpcc"})


def test_parse_summary_errors_on_zero_throughput():
    with pytest.raises(ValueError):
        m.parse_benchbase_summary({"Throughput (requests/second)": 0})


# --- per-type mean-throughput from results.csv (T2.1) --------------------

def test_mean_throughput_from_results_csv():
    csv = ("Time (seconds),Throughput (requests/second),Average Latency (millisecond)\n"
           "0,34.800,77.561\n5,43.600,63.814\n")
    assert m.mean_throughput(csv) == pytest.approx((34.8 + 43.6) / 2)


# --- dual metric proxy derivation (T2.1) ---------------------------------

def test_derive_dual_metric_is_labeled_proxy():
    # NewOrder rate 10 rps → tpmC-proxy = 10*60 = 600 ; analytical rate 2 rps → QphH-proxy = 2*3600 = 7200
    dm = m.derive_dual_metric(neworder_rps=10.0, analytical_rps=2.0)
    assert dm["tpmc_proxy"] == 600.0 and dm["qphh_proxy"] == 7200.0
    assert "PROXY" in dm["label"] and "NOT audited" in dm["label"]


# --- OLAP result-consistency oracle (T3.1) -------------------------------

def test_olap_consistency_flags_sql_error_as_inconsistent():
    # a query that ERRORS (executor returns None) → INCONSISTENT (real SQL incompatibility)
    queries = {"Q1": "select 1", "Q6": "bad sql"}
    def executor(sql):
        return [(1, 2)] if "select" in sql else None
    res = m.olap_result_consistency(queries, executor)
    assert res["per_query"]["Q1"] == "PASS"
    assert res["per_query"]["Q6"] == "INCONSISTENT"
    assert res["inconsistent"] == ["Q6"] and res["all_consistent"] is False


def test_olap_consistency_empty_result_is_valid_pass():
    # an EMPTY result (e.g. a date-literal filter that matches nothing) is a VALID analytical answer → PASS
    res = m.olap_result_consistency({"Q6": "select ... where never"}, lambda sql: [])
    assert res["per_query"]["Q6"] == "PASS" and res["all_consistent"] is True


def test_olap_consistency_flags_ragged_arity():
    res = m.olap_result_consistency({"Q1": "x"}, lambda sql: [(1, 2), (3,)])  # ragged rows
    assert res["per_query"]["Q1"] == "INCONSISTENT"


def test_olap_consistency_all_pass():
    queries = {"Q1": "a", "Q2": "b"}
    res = m.olap_result_consistency(queries, lambda sql: [(1,), (2,)])
    assert res["all_consistent"] is True and res["inconsistent"] == []


# --- CH query loader (the OLAP oracle's query set) -----------------------

def test_load_ch_queries_parses_marked_sql(tmp_path):
    f = tmp_path / "q.sql"
    f.write_text("-- header\n-- @Q1\nSELECT 1\nFROM t;\n\n-- @Q2\nSELECT 2;\n")
    q = m.load_ch_queries(str(f))
    assert set(q) == {"Q1", "Q2"}
    assert "SELECT 1" in q["Q1"] and "FROM t" in q["Q1"] and q["Q2"].startswith("SELECT 2")


def test_load_ch_queries_loads_all_22_real():
    import os
    p = os.path.join(os.path.dirname(os.path.abspath(m.__file__)), "htap", "chbenchmark_queries.sql")
    q = m.load_ch_queries(p)
    assert len(q) == 22 and set(q) == {f"Q{i}" for i in range(1, 23)}


# --- run-to-run dispersion (CV) reused from M129 -------------------------

def test_cv_reused_from_m129():
    # the driver reuses the M129 coefficient_of_variation (no re-implementation — parsimony rung 4)
    assert m.coefficient_of_variation([100.0, 101.0, 99.0, 100.5]) < 1.0


# --- benchbase skip path (no docker) -------------------------------------

def test_run_benchbase_skips_cleanly_without_docker(monkeypatch):
    monkeypatch.setattr(m.shutil, "which", lambda _: None)  # simulate no docker
    r = m.run_benchbase("abc123", 4, 4, 120, "/tmp/x", "eclipse-temurin:23-jdk")
    assert r["status"] == "BENCHBASE_SKIPPED" and "docker" in r["reason"].lower()
