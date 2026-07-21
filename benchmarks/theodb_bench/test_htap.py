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

def test_olap_consistency_flags_empty_result():
    queries = {"Q1": "select 1", "Q6": "select 2"}
    # Q6 returns empty → INCONSISTENT; Q1 returns a well-shaped result → PASS.
    def executor(sql):
        return [(1, 2)] if "1" in sql else []
    res = m.olap_result_consistency(queries, executor)
    assert res["per_query"]["Q1"] == "PASS"
    assert res["per_query"]["Q6"] == "INCONSISTENT"
    assert res["inconsistent"] == ["Q6"] and res["all_consistent"] is False


def test_olap_consistency_all_pass():
    queries = {"Q1": "a", "Q2": "b"}
    res = m.olap_result_consistency(queries, lambda sql: [(1,), (2,)])
    assert res["all_consistent"] is True and res["inconsistent"] == []


# --- run-to-run dispersion (CV) reused from M129 -------------------------

def test_cv_reused_from_m129():
    # the driver reuses the M129 coefficient_of_variation (no re-implementation — parsimony rung 4)
    assert m.coefficient_of_variation([100.0, 101.0, 99.0, 100.5]) < 1.0


# --- benchbase skip path (no docker) -------------------------------------

def test_run_benchbase_skips_cleanly_without_docker(monkeypatch):
    monkeypatch.setattr(m.shutil, "which", lambda _: None)  # simulate no docker
    r = m.run_benchbase("abc123", 4, 4, 120, "/tmp/x", "eclipse-temurin:23-jdk")
    assert r["status"] == "BENCHBASE_SKIPPED" and "docker" in r["reason"].lower()
