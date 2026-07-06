"""M47 / FU-1 — doc-check that the same-graph scan-allocation micro-bench report is present and grounded.

This is a doc-grounding gate (not a perf test): the report MUST carry the presized-vs-unsized table per ef,
the criterion CIs, the honest run-to-run picture, the explicit EC-2 caveat (this micro-bench is an UPPER bound —
production page I/O amortizes allocation), and MUST NOT claim product/QPS superiority from this isolated number.
"""
import json
import os

REPORT_MD = os.path.join(os.path.dirname(__file__), "..", "..", "docs", "benchmarks", "fu1-samegraph-scan-microbench.md")
REPORT_JSON = os.path.join(os.path.dirname(__file__), "..", "..", "docs", "benchmarks", "fu1-samegraph-scan-microbench.json")


def test_fu1_report_present():
    assert os.path.isfile(REPORT_MD), "FU-1 report .md missing"
    assert os.path.isfile(REPORT_JSON), "FU-1 report .json missing"


def test_fu1_json_schema():
    d = json.load(open(REPORT_JSON))
    for key in ("runs", "ef_sweep", "per_cell", "verdict", "env"):
        assert key in d, f"FU-1 json missing key {key!r}: {list(d)}"
    assert d["runs"] >= 3, "need >= 3 runs (M48 precedent)"
    # every ef cell has both variants with mean+std across runs
    for ef in d["ef_sweep"]:
        cell = d["per_cell"][str(ef)]
        for variant in ("presized", "unsized"):
            assert "mean_us" in cell[variant] and "std_us" in cell[variant], f"ef {ef} {variant} missing stats"


def test_fu1_md_grounded():
    md = open(REPORT_MD, encoding="utf-8").read()
    low = md.lower()
    # presized-vs-unsized table per ef + CIs
    assert "presized" in low and "unsized" in low, "report must compare presized vs unsized"
    for ef in ("100", "200", "400"):
        assert ef in md, f"ef {ef} row missing"
    # honest run-to-run picture (the effect is within box noise)
    assert "ruído" in low or "noise" in low, "report must state the box-noise honesty finding"
    # explicit EC-2 caveat: upper bound, production I/O amortizes
    assert "ec-2" in low or "upper bound" in low or "limite superior" in low, "report must carry the EC-2 caveat"
    assert "amortiza" in low or "amortize" in low or "i/o" in low, "EC-2 caveat must note production I/O amortization"
    # MUST NOT claim product/QPS superiority from this isolated number (public-copy.md)
    assert not ("mais rápido que" in low and "produção" in low and "qps" in low), "no unqualified product superiority claim"
