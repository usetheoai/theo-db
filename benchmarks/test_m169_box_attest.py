"""M169 unit tests — the box-attestation gate for T1.1's acceptance criteria.

Pure logic, NO box and NO database: `attest` judges a `BoxFacts` and `collect` takes its runners injected, so
both are deterministic with zero I/O. Same shape as the M164 guards in `run_m128_clickbench.py` — and for the
same reason: a check that needs the real box cannot be a regression test.

Why this exists: T1.1's four acceptance criteria are all MANUAL checkboxes today — nothing fails when the box is
wrong. The two failures that already cost this project a run are exactly that shape: `unattended-upgrades`
restarting PostgreSQL mid-COPY (M162), and a row count trusted from a load log instead of from the table.

Both lenses of `testing.md § 4.1` are covered: edge cases (values exactly at each threshold) AND negative cases
(the collector failing entirely and handing `attest` its sentinels — the single most likely real invocation).
"""
from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import m169_box_attest as a


def _facts(**over) -> a.BoxFacts:
    """A fully-conforming box; each test overrides one field to prove the guard bites on that field alone."""
    env = dict(nproc=16, mem_gb=31, loadavg1=0.12, unattended_state="masked",
               hits_rows=a.HITS_ROWS, hits_heap_rows=a.HITS_ROWS, tsv_rows=a.HITS_ROWS,
               disk_free_gb=a.MIN_DISK_FREE_GB + 50, so_md5="a6ab650771f00b5a", data_directory="/srv/m169data")
    env.update(over)
    return a.BoxFacts(**env)


# ---------- the conforming case ---------------------------------------------------------------------------------
def test_attestation_passes_on_a_conforming_box():
    v = a.attest(_facts())
    assert v.ok is True
    assert v.failures == []


# ---------- the corpus constant is not allowed to drift from the harness's ---------------------------------------
def test_hits_rows_agrees_with_the_harness_constant():
    """The literal is duplicated (importing the harness would drag psycopg2 into a pure test). This pins the two
    so a correction to one cannot silently leave the other behind."""
    try:
        import run_m128_clickbench as h
    except ImportError:  # psycopg2 absent — the duplication check is unavailable, not failed
        import pytest
        pytest.skip("run_m128_clickbench not importable (psycopg2 absent)")
    assert a.HITS_ROWS == h.HITS_TOTAL_ROWS


# ---------- AC-1: the row count comes from the TABLE, and must equal the FILE ------------------------------------
def test_attestation_rejects_a_corpus_that_is_not_the_clickbench_one():
    # exercises the FIRST branch (tsv_rows != HITS_ROWS) — previously unreachable from any test
    v = a.attest(_facts(tsv_rows=1_000_000, hits_rows=1_000_000, hits_heap_rows=1_000_000))
    assert v.ok is False
    assert any(f.startswith(a.ID_TSV_WRONG) for f in v.failures)


def test_attestation_rejects_a_table_that_disagrees_with_the_tsv():
    # the M162 false-100M shape: the table holds fewer rows than the source file and nothing noticed
    v = a.attest(_facts(hits_rows=1_000_000))
    assert v.ok is False
    assert any(f.startswith(a.ID_COPY_LOST) for f in v.failures)


# ---------- AC-1b: the heap twin must exist AND match — the gap that lost the table twice ------------------------
def test_attestation_rejects_a_missing_heap_twin():
    v = a.attest(_facts(hits_heap_rows=0))
    assert v.ok is False
    assert any(f.startswith(a.ID_HEAP_ABSENT) for f in v.failures)


def test_attestation_rejects_a_heap_twin_that_disagrees_with_the_columnar():
    v = a.attest(_facts(hits_heap_rows=a.HITS_ROWS - 17))
    assert v.ok is False
    assert any(f.startswith(a.ID_HEAP_MISMATCH) for f in v.failures)


# ---------- AC-2/3/4: the box is the one ADR-3 declared, idle, and not self-restarting ---------------------------
def test_attestation_rejects_an_undersized_box():
    assert a.attest(_facts(nproc=8)).ok is False
    assert a.attest(_facts(mem_gb=15)).ok is False


def test_attestation_rejects_a_loaded_box():
    v = a.attest(_facts(loadavg1=2.08))
    assert v.ok is False
    assert any(f.startswith("box_busy") for f in v.failures)


def test_attestation_rejects_unmasked_unattended_upgrades():
    for state in ("enabled", "disabled", "static", "unknown"):
        v = a.attest(_facts(unattended_state=state))
        assert v.ok is False, state
        assert any(f.startswith("unattended_upgrades_live") for f in v.failures)


def test_attestation_rejects_insufficient_disk_for_the_heap_rebuild():
    v = a.attest(_facts(disk_free_gb=40))
    assert v.ok is False
    assert any(f.startswith("disk_insufficient") for f in v.failures)


# ---------- NEGATIVE lens: the collector failed entirely -----------------------------------------------------------
def test_unreachable_collector_is_reported_as_such_not_as_a_data_defect():
    """The single most likely real invocation: psql down / wrong port / TSV path wrong. Reporting that as 'the
    COPY lost rows' sends the operator hunting a phantom bug — the #132 defect this project already paid for."""
    v = a.attest(_facts(hits_rows=a.UNREACHABLE, hits_heap_rows=a.UNREACHABLE, tsv_rows=a.UNREACHABLE))
    assert v.ok is False
    assert any(f.startswith(a.ID_UNREACHABLE) for f in v.failures)
    assert not any(f.startswith(a.ID_COPY_LOST) for f in v.failures), "infra != data defect"


# ---------- EDGE lens: exactly at each threshold --------------------------------------------------------------------
def test_thresholds_are_inclusive_exactly_where_the_plan_says():
    assert a.attest(_facts(nproc=a.MIN_NPROC)).ok is True            # ">= 16" passes at 16
    assert a.attest(_facts(mem_gb=a.MIN_MEM_GB)).ok is True          # ">= 30" passes at 30
    assert a.attest(_facts(disk_free_gb=a.MIN_DISK_FREE_GB)).ok is True
    # the plan says loadavg "< 2", so exactly 2.0 must FAIL
    assert a.attest(_facts(loadavg1=a.MAX_LOADAVG1)).ok is False
    assert a.attest(_facts(loadavg1=a.MAX_LOADAVG1 - 0.01)).ok is True


# ---------- every failure, not just the first ------------------------------------------------------------------------
def test_attestation_reports_every_failure_not_just_the_first():
    v = a.attest(_facts(nproc=8, loadavg1=9.0, unattended_state="enabled"))
    assert len(v.failures) >= 3


# ---------- the artifact header must carry every judged field (ADR-3 comparability) -----------------------------------
def test_every_judged_field_reaches_the_artifact_header():
    """ADR-3 requires Phase 1 and Phase 4 to run on the same box, and the header is the only evidence of that. A
    field that is checked but not published breaks the comparison silently."""
    v = a.attest(_facts())
    for key in ("nproc", "mem_gb", "loadavg1", "unattended_state", "hits_rows", "hits_heap_rows",
                "tsv_rows", "disk_free_gb", "so_md5", "data_directory"):
        assert key in v.facts, key


# ---------- collect(): the MAPPING layer, with fakes — no box, no database ---------------------------------------------
def test_collect_maps_a_healthy_box_into_facts():
    calls = []

    def fake_sh(cmd, timeout=60):
        calls.append((cmd, timeout))
        if "nproc" in cmd:
            return "16"
        if "free -g" in cmd:
            return "31"
        if "loadavg" in cmd:
            return "0.12"
        if "unattended" in cmd:
            return "masked"
        if cmd.startswith("wc -l"):
            return f"{a.HITS_ROWS}"
        if "df -BG" in cmd:
            return "283"
        if "md5sum" in cmd:
            return "a6ab650771f00b5a"
        if "data_directory" in cmd:
            return "/srv/m169data"
        return None

    box = a.collect("/tmp/hits.tsv", sh=fake_sh, sh_any_rc=fake_sh, psql_int=lambda _sql: a.HITS_ROWS)
    assert a.attest(box).ok is True
    assert box.data_directory == "/srv/m169data"
    assert box.so_md5 == "a6ab650771f00b5a"
    # the wc must get the long ceiling: 60 s over a 69.7 GB corpus times out on a CONFORMING box
    wc_timeouts = [t for c, t in calls if c.startswith("wc -l")]
    assert wc_timeouts == [a.WC_TIMEOUT_S], wc_timeouts
    # df must target PGDATA's filesystem, not "/" — the heap rebuild writes there
    assert any("df -BG" in c and "/srv/m169data" in c for c, _ in calls)


def test_collect_maps_a_failing_runner_to_fail_closed_values():
    """A runner that always fails must produce a verdict that REFUSES, and must not invent plausible numbers."""
    box = a.collect("/tmp/hits.tsv", sh=lambda cmd, timeout=60: None,
                    sh_any_rc=lambda cmd, timeout=60: None, psql_int=lambda _sql: a.UNREACHABLE)
    v = a.attest(box)
    assert v.ok is False
    assert box.loadavg1 == 99.0        # the fail-closed sentinel, not 0.0 which would look idle
    assert box.unattended_state == "unknown"
    assert box.tsv_rows == a.UNREACHABLE
    assert any(f.startswith(a.ID_UNREACHABLE) for f in v.failures)


# ---------- quick mode: "não perguntei" é um terceiro estado, distinto de 0 e de UNREACHABLE ----------------------
def test_quick_mode_does_not_judge_the_dataset_it_did_not_ask_about():
    """The closing header of a read-only run asks 'did anything run alongside?', not 'is the data still there?'.
    Skipping the ~40-minute dataset checks there is right; letting SKIPPED be read as 'absent' would invent a
    failure, and letting it be read as 'fine' would invent a pass."""
    box = a.BoxFacts(nproc=16, mem_gb=31, loadavg1=0.1, unattended_state="masked",
                     hits_rows=a.SKIPPED, hits_heap_rows=a.SKIPPED, tsv_rows=a.SKIPPED,
                     disk_free_gb=a.MIN_DISK_FREE_GB + 10)
    v = a.attest(box)
    assert v.ok is True
    assert not any(f.startswith(a.ID_HEAP_ABSENT) for f in v.failures)
    assert not any(f.startswith(a.ID_UNREACHABLE) for f in v.failures)


def test_quick_mode_still_judges_box_fitness():
    """Quick mode skips the DATASET, never the contamination check — that is the only reason it exists."""
    box = a.BoxFacts(nproc=16, mem_gb=31, loadavg1=7.5, unattended_state="masked",
                     hits_rows=a.SKIPPED, hits_heap_rows=a.SKIPPED, tsv_rows=a.SKIPPED,
                     disk_free_gb=a.MIN_DISK_FREE_GB + 10)
    v = a.attest(box)
    assert v.ok is False
    assert any(f.startswith("box_busy") for f in v.failures)


def test_collect_quick_skips_the_expensive_calls_entirely():
    """Not merely 'ignores the result' — the ~40 minutes must not be SPENT."""
    calls = []

    def fake_sh(cmd, timeout=60):
        calls.append(cmd)
        return {"nproc": "16"}.get(cmd, "masked" if "unattended" in cmd else
                                   "0.1" if "loadavg" in cmd else
                                   "31" if "free -g" in cmd else
                                   "300" if "df -BG" in cmd else
                                   "/srv/m169data" if "data_directory" in cmd else "abc123")

    psql_calls = []

    def fake_psql_int(sql):
        psql_calls.append(sql)
        return a.HITS_ROWS

    box = a.collect("/tmp/hits.tsv", sh=fake_sh, sh_any_rc=fake_sh, psql_int=fake_psql_int, quick=True)
    assert box.hits_rows == a.SKIPPED
    assert psql_calls == [], "quick mode must not run count(*) at all"
    assert not any(c.startswith("wc -l") for c in calls), "quick mode must not run wc -l on 69.7 GB"


# ---------- o buraco que tornava o escape hatch inerte no próprio caso de uso ----------------------------------
def test_absent_relation_is_distinct_from_unreachable_and_from_empty():
    """Três estados, três vereditos. `ALLOW_MISSING_HEAP` existe para tolerar UM deles; se `hits_heap` ausente
    fosse reportado como 'não consegui olhar', a tolerância nunca dispararia — que era exatamente o caso."""
    absent = a.attest(_facts(hits_heap_rows=a.ABSENT))
    assert any(f.startswith(a.ID_HEAP_ABSENT) for f in absent.failures)
    assert not any(f.startswith(a.ID_UNREACHABLE) for f in absent.failures)

    unreachable = a.attest(_facts(hits_heap_rows=a.UNREACHABLE))
    assert any(f.startswith(a.ID_UNREACHABLE) for f in unreachable.failures)
    assert not any(f.startswith(a.ID_HEAP_ABSENT) for f in unreachable.failures)


def test_a_heap_with_the_wrong_population_is_never_reported_as_absent():
    """O buraco perigoso: uma tolerância que casasse a substring 'hits_heap' engoliria a divergência de
    contagem — que é PIOR que ausência, porque o A/B compararia duas populações diferentes e passaria."""
    v = a.attest(_facts(hits_heap_rows=50_000_000))
    assert any(f.startswith(a.ID_HEAP_MISMATCH) for f in v.failures)
    assert not any(f.startswith(a.ID_HEAP_ABSENT) for f in v.failures)


# ---------- a costura real: _count_rows -> _psql -> _sh, só o _sh falsificado -------------------------------------
def test_count_rows_wires_to_psql_with_the_long_timeout(monkeypatch):
    """Os testes de `collect` injetam `psql_int` FALSO, então `_count_rows` e `_psql` nunca eram exercitados —
    e foi assim que um `_psql(sql, timeout)` chamando uma função de um argumento só passou verde e só quebrou na
    box, depois do rsync. Este teste falsifica APENAS o `_sh`, deixando a cadeia real montada."""
    calls = []

    def fake_sh(cmd, timeout=60):
        calls.append((cmd, timeout))
        return "t" if "to_regclass" in cmd else "99997497"

    monkeypatch.setattr(a, "_sh", fake_sh)
    assert a._count_rows("public.hits") == 99997497
    # a sonda de existência usa o teto curto; a contagem usa o longo (um count(*) a 100M leva ~35 min)
    assert calls[0][1] == a.SH_TIMEOUT_S
    assert calls[1][1] == a.COUNT_TIMEOUT_S


def test_count_rows_reports_absent_without_counting(monkeypatch):
    calls = []

    def fake_sh(cmd, timeout=60):
        calls.append(cmd)
        return "f" if "to_regclass" in cmd else None

    monkeypatch.setattr(a, "_sh", fake_sh)
    assert a._count_rows("public.hits_heap") == a.ABSENT
    assert len(calls) == 1, "uma relação ausente não deve custar um count(*)"


def test_count_rows_reports_unreachable_when_the_probe_itself_fails(monkeypatch):
    monkeypatch.setattr(a, "_sh", lambda cmd, timeout=60: None)
    assert a._count_rows("public.hits") == a.UNREACHABLE


def test_a_masked_unit_is_read_as_masked_even_though_systemctl_exits_nonzero():
    """`systemctl is-enabled` devolve exit 1 para `masked` — o estado É o stdout, e o código de saída apenas o
    repete. Ler não-zero como "não consegui rodar" transformou uma unidade CORRETAMENTE mascarada em `unknown`,
    reprovando o gate pelo motivo errado. Terceira vez neste milestone que um returncode estrito rotula mal um
    comando que funcionou."""
    def sh_strict(cmd, timeout=60):
        return None            # o `_sh` normal: não-zero vira None

    def sh_any(cmd, timeout=60):
        return "masked" if "is-enabled" in cmd else None

    box = a.collect("/tmp/x", sh=sh_strict, sh_any_rc=sh_any, psql_int=lambda _s: a.SKIPPED, quick=True)
    assert box.unattended_state == "masked"
    assert not any(f.startswith("unattended_upgrades_live") for f in a.attest(box).failures)
