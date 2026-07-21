"""M129 — DB-free unit tests for the OLTP driver's TPS/NOPM parsers + the significance wiring."""
import pytest

import run_m129_oltp as m


# --- pgbench TPS parser (T1.1) -------------------------------------------

def test_parse_pgbench_tps_extracts_float():
    out = ("transaction type: <builtin: TPC-B (sort of)>\n"
           "number of transactions actually processed: 12345\n"
           "latency average = 6.5 ms\n"
           "tps = 1234.567890 (without initial connection time)\n")
    assert m.parse_pgbench_tps(out) == pytest.approx(1234.56789)


def test_parse_pgbench_tps_errors_on_garbage():
    with pytest.raises(ValueError):
        m.parse_pgbench_tps("pgbench: error: connection failed")


# --- HammerDB NOPM parser (T2.1) -----------------------------------------

def test_parse_hammerdb_nopm_extracts_int():
    out = "Vuser 1:TEST RESULT : System achieved 45678 NOPM from 105432 PostgreSQL TPM"
    assert m.parse_hammerdb_nopm(out) == 45678


def test_parse_hammerdb_nopm_errors_on_garbage():
    with pytest.raises(ValueError):
        m.parse_hammerdb_nopm("Vuser 1: Timing test period complete")


# --- hammerdb skip path (no docker) --------------------------------------

def test_hammerdb_skips_cleanly_without_docker(monkeypatch):
    monkeypatch.setattr(m.shutil, "which", lambda _: None)  # simulate no docker
    r = m.run_hammerdb(4, 4, 1, 2)
    assert r["status"] == "HAMMERDB_SKIPPED" and "docker" in r["reason"].lower()


# --- significance wiring: run-to-run stability is NOT significant ---------

def test_significance_over_stable_runs_not_significant():
    from theodb_bench.significance import paired_significance
    a, b = [1000.0, 1001.0], [1000.5, 999.5]  # stable → tiny diffs
    sig = paired_significance(a, b)
    assert sig["p_permutation"] > 0.05  # not significant → stable engine
