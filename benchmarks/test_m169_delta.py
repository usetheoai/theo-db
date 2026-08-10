"""M169 T4.1 — unit tests for the delta between the baseline run and the post-fix re-run.

Pure logic, no box: the delta takes both runs' records as data. What it must REFUSE is the whole point — a delta
that silently compares two runs made under different conditions produces a number attributed to the code when it
belongs to the environment, which is the exact failure this milestone spent T1.2 documenting.
"""
from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import m169_delta as d


def _run(ok_qs, *, routed_fail=(), unrouted_fail=(), n=43):
    """A run where `ok_qs` completed; failures are split by whether the aggregate path routed."""
    recs = []
    for i in range(n):
        if i in ok_qs:
            recs.append({"q": i, "verdict": "ok", "agg_routed": True, "elapsed_s": 1.0})
        elif i in routed_fail:
            recs.append({"q": i, "verdict": "error:XX000", "agg_routed": True, "elapsed_s": 1.0,
                         "error": "byte array offset overflow"})
        else:
            recs.append({"q": i, "verdict": "timeout", "agg_routed": i in unrouted_fail and False, "elapsed_s": 1.0})
    return recs


def _hdr(**over):
    h = dict(label="x", n_queries=43, timeout_s=300, work_mem="256MB")
    h.update(over)
    return h


def _box(**over):
    f = dict(nproc=16, mem_gb=31, so_md5="aaaa", data_directory="/srv/m169data", hits_rows=99_997_497)
    f.update(over)
    return {"facts": f}


# ---------- the refusals: a delta is only a delta when the conditions match -------------------------------------
def test_delta_refuses_when_the_timeout_ceiling_differs():
    """Completion under a 300 s ceiling and under a 600 s ceiling are different measurements. Subtracting them
    reports the ceiling change as if it were the fix."""
    before = (_hdr(timeout_s=300), _run(range(28)), _box())
    after = (_hdr(timeout_s=600), _run(range(31)), _box(so_md5="bbbb"))
    try:
        d.render(before, after)
        raise AssertionError("should have refused")
    except d.IncomparableRuns as e:
        assert "teto" in str(e)


def test_delta_refuses_when_the_box_is_not_the_same():
    """ADR-3 requires both phases on the SAME box; a different core count or RAM makes the delta a mix of code
    and machine, and neither can be isolated afterwards."""
    before = (_hdr(), _run(range(28)), _box(nproc=16))
    after = (_hdr(), _run(range(31)), _box(nproc=8, so_md5="bbbb"))
    try:
        d.render(before, after)
        raise AssertionError("should have refused")
    except d.IncomparableRuns as e:
        assert "box" in str(e)


def test_delta_refuses_when_the_corpus_row_count_differs():
    before = (_hdr(), _run(range(28)), _box())
    after = (_hdr(), _run(range(31)), _box(hits_rows=1_000_000, so_md5="bbbb"))
    try:
        d.render(before, after)
        raise AssertionError("should have refused")
    except d.IncomparableRuns as e:
        assert "corpus" in str(e)


def test_delta_refuses_when_the_binary_did_not_change():
    """The whole claim is that the FIX moved the number. Same `so_md5` on both sides means one of the two runs
    used the wrong binary — the single most likely operator error, and it reads as 'the fix did nothing'."""
    before = (_hdr(), _run(range(28)), _box(so_md5="aaaa"))
    after = (_hdr(), _run(range(31)), _box(so_md5="aaaa"))
    try:
        d.render(before, after)
        raise AssertionError("should have refused")
    except d.IncomparableRuns as e:
        assert "binário" in str(e)


# ---------- the delta itself -------------------------------------------------------------------------------------
def test_delta_reports_both_counts_and_the_difference():
    md = d.render((_hdr(), _run(range(28)), _box()),
                  (_hdr(), _run(range(31)), _box(so_md5="bbbb")))
    assert "28/43" in md and "31/43" in md and "+3" in md


def test_delta_attributes_only_the_queries_that_route_through_the_aggregate():
    """A query that completes without ever entering the columnar aggregate is not evidence of this fix. The
    artifact has to separate them, or the headline credits the milestone with work it did not do."""
    # `range(28)` contém o 20 — passá-lo também em `routed_fail` fazia o ramo `ok` vencer e o teste não
    # exercitava nada. O fixture tem de EXCLUIR as três das que completam antes.
    ok_before = [q for q in range(28) if q not in (20, 33, 34)]
    before = (_hdr(), _run(ok_before, routed_fail=(20, 33, 34)), _box())
    after = (_hdr(), _run(ok_before + [20, 33, 34]), _box(so_md5="bbbb"))
    md = d.render(before, after)
    assert "3 atribuíveis" in md
    for q in ("q20", "q33", "q34"):
        assert q in md, q
    assert "0 NÃO atribuíveis" in md


def test_delta_reads_exactly_what_the_runner_writes_and_returns_the_right_code(tmp_path, capsys):
    """A fronteira arquivo -> main(), que os testes puros acima NÃO atravessam.

    Esta é a lacuna que já custou um CRITICAL nesta mesma sessão: o summarizer usava `json.load` (documento
    único) contra um arquivo JSONL, e a quebra só apareceria DEPOIS das horas de corrida, porque nenhum teste
    ligava o formato real ao ponto de entrada."""
    import json as _json

    def write(label, ok_n, md5):
        with open(tmp_path / f"{label}.jsonl", "w") as fh:
            fh.write(_json.dumps({"header": {"label": label, "n_queries": 43, "timeout_s": 300,
                                             "work_mem": "256MB"}}) + "\n")
            for i in range(43):
                v = "ok" if i < ok_n else "error:XX000"
                fh.write(_json.dumps({"q": i, "verdict": v, "agg_routed": True, "elapsed_s": 1.0}) + "\n")
        with open(tmp_path / f"{label}-box.json", "w") as fh:
            _json.dump({"facts": {"nproc": 16, "mem_gb": 31, "so_md5": md5,
                                  "data_directory": "/srv/m169data", "hits_rows": 99_997_497}}, fh)

    write("b", 28, "aaaa")
    write("a", 31, "aaaa")          # MESMO binário → tem de recusar
    argv = [str(tmp_path / x) for x in ("b.jsonl", "b-box.json", "a.jsonl", "a-box.json")]
    sys.argv = ["m169_delta.py", *argv]
    assert d.main() == 1
    assert "binário é o MESMO" in capsys.readouterr().err

    write("a", 31, "bbbb")          # binário diferente → publica
    sys.argv = ["m169_delta.py", *argv]
    assert d.main() == 0
    assert "28/43 → 31/43 (+3)" in capsys.readouterr().out


def test_delta_names_the_regressions_not_only_the_gains():
    """A query that completed BEFORE and fails AFTER is the most important line in the document, and the one a
    'how many more pass?' summary hides."""
    before = (_hdr(), _run(range(28)), _box())
    after = (_hdr(), _run([q for q in range(28) if q != 5] + [30, 31, 32]), _box(so_md5="bbbb"))
    md = d.render(before, after)
    assert "regress" in md.lower()
    assert "q05" in md or "q5" in md
