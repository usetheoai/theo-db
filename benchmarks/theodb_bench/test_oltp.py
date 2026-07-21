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


# --- run-to-run stability via coefficient of variation (single-system dispersion) ---

def test_cv_low_for_stable_runs():
    # tight runs → low dispersion (a stable engine)
    assert m.coefficient_of_variation([1000.0, 1001.0, 999.0, 1000.5]) < 1.0


def test_cv_higher_for_noisy_runs():
    stable = m.coefficient_of_variation([1000.0, 1001.0, 999.0, 1000.5])
    noisy = m.coefficient_of_variation([1000.0, 1300.0, 800.0, 1100.0])
    assert noisy > stable  # dispersion is monotone in run-to-run spread


def test_cv_errors_on_single_sample():
    with pytest.raises(ValueError):
        m.coefficient_of_variation([1000.0])
