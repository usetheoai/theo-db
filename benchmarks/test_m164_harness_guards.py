"""M164 unit tests — the three false-green/infra guards added to run_m128_clickbench.py.

Pure logic, NO database and NO real box: every guard takes its environment as an argument (rows-in-file, the
route+identity pair, disk/RAM bytes) so the tests are deterministic with zero I/O (plan ADR-1). These are the
regression proofs for the three M160-M162 gotchas: a stale sample served as a bigger one, a declined A/B arm passing
as a trivial diverged=0, and an undersized box loading anyway.
"""
from __future__ import annotations

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_m128_clickbench as m


# ---------- B1: sample_is_fresh (count integrity) --------------------------------------------------------------------
def test_sample_is_fresh_rejects_1m_cache_for_100m():
    # the exact M162 false-100M: a 1,000,000-row cache MUST NOT satisfy a --n 100,000,000 request
    assert m.sample_is_fresh(1_000_000, 100_000_000) is False


def test_sample_is_fresh_accepts_exact():
    assert m.sample_is_fresh(1_000_000, 1_000_000) is True


def test_sample_is_fresh_tolerates_systematic_off_by_one():
    # HITS_TOTAL_ROWS // n integer division can admit ONE extra row before the head cap — that is fresh
    assert m.sample_is_fresh(1_000_001, 1_000_000) is True
    # but 50 extra is a different sample size, not the off-by-one — stale
    assert m.sample_is_fresh(1_000_050, 1_000_000) is False


def test_sample_is_fresh_zero_rows_is_stale():
    # an empty/truncated cache is never fresh for a positive request
    assert m.sample_is_fresh(0, 1_000_000) is False


# ---------- B2: classify_ab (routing integrity — no trivial diverged=0) ----------------------------------------------
def test_classify_ab_declined_identical_is_trivial():
    # the "ON não roteou" guard: identical but the ON arm DECLINED → the A/B proves nothing about the pushdown
    assert m.classify_ab(False, True) == "declined_trivial"


def test_classify_ab_routed_identical_is_meaningful():
    assert m.classify_ab(True, True) == "routed_identical"


def test_classify_ab_diverged_regardless_of_routing():
    # a real divergence is a bug whether or not it routed
    assert m.classify_ab(True, False) == "diverged"
    assert m.classify_ab(False, False) == "diverged"


def test_classify_ab_na_when_unknown():
    assert m.classify_ab(None, None) == "n/a"
    assert m.classify_ab(True, None) == "n/a"


# ---------- C: preflight_sizing (disk BLOCKS, larger-than-RAM WARNS) --------------------------------------------------
def test_preflight_blocks_when_disk_too_small():
    # estimated on-disk sample exceeds the safe fraction of free disk → BLOCK (pure waste to attempt)
    r = m.preflight_sizing(100_000_000, disk_free_bytes=1_000_000_000, ram_bytes=64 * 2**30)
    assert r["block"] is True
    assert any("disk" in reason.lower() for reason in r["reasons"])


def test_preflight_warns_but_not_blocks_larger_than_ram():
    # fits disk but the in-DB working set exceeds RAM headroom → WARN, never BLOCK (larger-than-RAM is intentional, ADR-2)
    r = m.preflight_sizing(100_000_000, disk_free_bytes=10 * 2**40, ram_bytes=8 * 2**30)
    assert r["block"] is False
    assert r["warn"] is True
    assert any("ram" in reason.lower() or "memory" in reason.lower() for reason in r["reasons"])


def test_preflight_ok_when_both_fit():
    r = m.preflight_sizing(1_000_000, disk_free_bytes=10 * 2**40, ram_bytes=64 * 2**30)
    assert r["block"] is False
    assert r["warn"] is False


def test_preflight_disk_block_takes_precedence_over_ram():
    # a run that fits neither is a BLOCK (disk), not merely a WARN
    r = m.preflight_sizing(100_000_000, disk_free_bytes=1_000_000_000, ram_bytes=1_000_000_000)
    assert r["block"] is True


# ---------- wiring: _ensure_sample uses the count-check (fresh path, no network) -------------------------------------
def test_ensure_sample_accepts_fresh_cache_without_streaming(tmp_path):
    # integration of the B1 glue: a cache whose row count matches n is accepted (returns True) WITHOUT re-streaming.
    # Proves sample_is_fresh is actually wired into _ensure_sample's early-return, end-to-end, with no DB and no network.
    cache = tmp_path / "hits_sample.tsv"
    cache.write_text("".join(f"row{i}\n" for i in range(500)))
    mtime_before = cache.stat().st_mtime
    assert m._ensure_sample(str(cache), 500) is True
    # unchanged: the fresh cache was reused, not overwritten by a stream
    assert cache.stat().st_mtime == mtime_before
    assert cache.read_text().count("\n") == 500
