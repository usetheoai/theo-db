"""M169 unit tests — the baseline artifact generator.

The AC of T1.2 demands an artifact carrying `so_md5`, `nproc`, `free` and `loadavg`. The facts exist in the two
box-attestation JSONs, but nothing turned them into the document — so the criterion could not be ticked without
someone hand-assembling it, which is how provenance goes missing.

The interesting tests here are the REFUSALS. A generator that happily emits a report with `so_md5: unknown`, or
without the reproduction command, produces an artifact that LOOKS complete and is not — and an artifact is read
long after anyone remembers what was in it.
"""
from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import m169_baseline_report as r


def _box(**over):
    facts = dict(nproc=16, mem_gb=31, loadavg1=0.23, unattended_state="masked",
                 hits_rows=99_997_497, hits_heap_rows=-3, tsv_rows=99_997_497, disk_free_gb=283,
                 so_md5="a6ab650771f00b5a0d66af2220709168", data_directory="/srv/m169data")
    facts.update(over)
    return {"ok": True, "failures": [], "facts": facts}


def _records(n_ok=19, n=43):
    recs = [{"q": i, "verdict": "ok" if i < n_ok else "timeout", "elapsed_s": 1.0, "agg_routed": True}
            for i in range(n)]
    return recs


def _header(**over):
    h = dict(label="baseline-100m", n_queries=43, timeout_s=300, work_mem="256MB",
             gucs=["SET theodb.enable_columnar_agg = on"], gucs_effective={"theodb.enable_columnar_agg": "on"})
    h.update(over)
    return h


# ---------- the report carries the provenance the AC names -------------------------------------------------------
def test_report_carries_every_provenance_field_the_ac_demands():
    md = r.render(_header(), _records(), _box(), _box(loadavg1=0.31))
    for token in ("so_md5", "a6ab650771f00b5a", "nproc", "16", "loadavg", "0.23", "0.31"):
        assert token in md, token


def test_report_states_the_completion_count_which_is_the_whole_deliverable():
    md = r.render(_header(), _records(n_ok=19), _box(), _box())
    assert "19/43" in md


def test_report_carries_a_reproduction_command_next_to_the_numbers():
    """Global DoD: toda alegação de número tem comando de reprodução ao lado."""
    md = r.render(_header(), _records(), _box(), _box())
    assert "m169_baseline_100m.sh" in md
    assert "m169_baseline_summarize.py" in md


# ---------- the agg_routed discriminator — what makes the number ATTRIBUTABLE -------------------------------------
# Written AFTER the section it covers (my TDD lapse, stated rather than hidden). Each assertion targets a string
# that exists ONLY in that section, so removing it turns these red.
def _mixed_records():
    """Shaped like the real 100M run: failures on BOTH sides of the discriminator."""
    recs = [{"q": i, "verdict": "ok", "elapsed_s": 1.0, "agg_routed": True} for i in range(43)]
    for q in (20, 33, 34):   # ROUTED — the overflow this milestone exists to remove
        recs[q].update(verdict="error:XX000", agg_routed=True, error="byte array offset overflow")
    for q in (17, 21, 22):   # UNROUTED — PostgreSQL's row executor; no columnar change moves them
        recs[q].update(verdict="timeout", agg_routed=False,
                       error="canceling statement due to statement timeout")
    return recs


def test_report_separates_failures_by_whether_the_aggregate_path_routed():
    """An aggregate count mixes two classes of failure. A query that never enters the columnar path fails for a
    reason no change to that path can move — counting it alongside the target inflates the milestone's scope."""
    md = r.render(_header(), _mixed_records(), _box(), _box())
    assert "3 falhas COM roteamento agregado" in md
    assert "3 falhas SEM roteamento" in md


def test_report_names_which_queries_are_in_the_milestones_scope():
    """The count alone is not actionable — the artifact has to say WHICH queries the fix must move, on the line
    that declares the routed class."""
    md = r.render(_header(), _mixed_records(), _box(), _box())
    routed_line = md.split("falhas COM roteamento agregado")[1].split("\n")[0]
    for q in ("q20", "q33", "q34"):
        assert q in routed_line, q
    assert "q17" not in routed_line, "unrouted failure leaked into the in-scope list"


def test_report_carries_the_error_text_of_each_failure():
    md = r.render(_header(), _mixed_records(), _box(), _box())
    assert "byte array offset overflow" in md


def test_report_refuses_to_present_the_m162_count_as_a_comparison_baseline():
    """Two runs in different MEMORY REGIMES are not comparable query-by-query: a delta between them mixes the
    effect of the code with the effect of the box, and neither can be isolated afterwards. The caveat has to sit
    where the number is read, not in a commit message nobody opens."""
    md = r.render(_header(), _mixed_records(), _box(), _box())
    assert "não é base de comparação válida" in md
    assert "page cache" in md, "the caveat must name the regime difference, not just assert it"


# ---------- the REFUSALS — an artifact that looks complete and is not ---------------------------------------------
def test_report_refuses_to_emit_with_an_unidentifiable_binary():
    """`so_md5: unknown` means nobody can say WHICH binary produced these numbers. The project already paid for
    that: an oracle passed against the OLD `.so` because the postmaster had not been restarted."""
    try:
        r.render(_header(), _records(), _box(so_md5="unknown"), _box())
    except r.IncompleteProvenance as e:
        assert "so_md5" in str(e)
    else:
        raise AssertionError("um artefato sem identidade do binário não pode ser emitido")


def test_report_refuses_when_the_run_did_not_reach_every_query():
    """The non-vacuity gate again, at the document boundary: a truncated run must not become a published number."""
    try:
        r.render(_header(), _records(n=30), _box(), _box())
    except r.IncompleteProvenance as e:
        assert "30/43" in str(e)
    else:
        raise AssertionError("uma corrida truncada não pode virar artefato")


def test_report_refuses_when_the_binary_changed_mid_run():
    try:
        r.render(_header(), _records(), _box(), _box(so_md5="deadbeef" * 4))
    except r.IncompleteProvenance as e:
        assert "so_md5" in str(e)
    else:
        raise AssertionError("dois binários numa medição não podem virar um artefato")


# ---------- honesty about what was NOT measured --------------------------------------------------------------------
def test_report_says_the_ab_was_not_run_when_the_heap_twin_was_absent():
    """`hits_heap_rows = ABSENT` means no byte-identity oracle ran. The artifact must SAY that, because a reader
    who does not see the caveat assumes correctness was checked."""
    md = r.render(_header(), _records(), _box(hits_heap_rows=-3), _box())
    assert "n/a" in md.lower() or "não executad" in md.lower()
    assert "byte-identical" not in md


def test_report_flags_placeholder_gucs_in_the_header():
    """A GUC the server does not know was SET without effect — the run measured a configuration that does not
    exist, and the artifact must not present it as configuration."""
    h = _header(gucs_effective={"theodb.enable_columnar_agg": "on",
                                "theodb.enable_columnar_agg_stream": "PLACEHOLDER — o servidor não conhece"})
    md = r.render(h, _records(), _box(), _box())
    assert "PLACEHOLDER" in md


# ---------- round trip: the generator reads what the runner writes ---------------------------------------------------
def test_report_reads_the_files_the_pipeline_actually_produces(tmp_path):
    jsonl = tmp_path / "baseline-100m.jsonl"
    with open(jsonl, "w") as fh:
        fh.write(json.dumps({"header": _header()}) + "\n")
        for rec in _records():
            fh.write(json.dumps(rec) + "\n")
    before = tmp_path / "b.json"
    after = tmp_path / "a.json"
    before.write_text(json.dumps(_box()))
    after.write_text(json.dumps(_box(loadavg1=0.31)))

    md = r.build(str(jsonl), str(before), str(after))
    assert "19/43" in md
    assert "a6ab650771f00b5a" in md
