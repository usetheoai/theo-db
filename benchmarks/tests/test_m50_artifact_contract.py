"""M50 régua SOTA — contrato do artefato + sanidade do multi-cliente do driver.

Guarda de regressão para o gate do M51: se o driver `run_m50_sota.py` mudar e deixar de emitir
uma chave exigida pelo veredito (as 3 curvas, o load registrado, o sweep multi-cliente), ou se o
artefato committado perder a seção do veredito, estes testes falham. Não toca o banco (Rule: unit,
determinístico) — valida o JSON committado + a forma pura do driver.
"""
import importlib.util
import json
import pathlib
import re

BENCH = pathlib.Path(__file__).resolve().parents[1]
ART_JSON = BENCH.parent / "docs" / "benchmarks" / "m50-sota-ruler.json"
ART_MD = BENCH.parent / "docs" / "benchmarks" / "m50-sota-ruler.md"


def _load_driver():
    spec = importlib.util.spec_from_file_location("run_m50_sota", BENCH / "run_m50_sota.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def test_artifact_json_has_three_way_ruler_and_multiclient():
    """The committed M50 artifact carries the 3-way recall curve + the first DB-throughput sweep."""
    d = json.loads(ART_JSON.read_text())
    # 3-way ruler: the three specs are present with recall curves
    for spec in ("theodb_hnsw", "pgvector_hnsw", "diskann"):
        assert spec in d["per_spec"], f"missing spec {spec}"
        assert "curve" in d["per_spec"][spec], f"{spec} has no recall curve"
        assert len(d["per_spec"][spec]["curve"]) >= 3, f"{spec} curve too thin"
    # first DB-throughput (multi-client) sweep — item 3 of the DoD
    assert "multiclient" in d, "no multi-client QPS sweep (item 3 of M50 DoD)"
    for name in ("theodb_hnsw", "pgvector_hnsw"):
        conns = {s["conns"] for s in d["multiclient"][name]["sweeps"]}
        assert {8, 16} <= conns, f"{name} multi-client sweep missing 8/16 conns"
    # box-load honesty (M46 lesson) — load recorded, guard flag present
    assert "load_pre" in d and "load_per_run" in d and "load_guard_tripped" in d


def test_artifact_md_contains_written_verdict_gate():
    """The .md carries the written verdict paragraph — the formal M51 gate (item 6)."""
    md = ART_MD.read_text()
    assert "gate formal do M51" in md, "written verdict (M51 gate) missing from artifact"
    assert "recall-parity" in md or "paridade de recall" in md
    # honesty caveats about reduced scale must be present, not hidden
    assert "escala reduzida" in md.lower()


def test_md_table_numbers_match_json_no_handedit():
    """§1 table cells (recall/p50/qps) MUST equal the committed JSON per_spec (review F1 regression guard).

    Catches hand-edited numbers that diverge from the reproducible artifact — the exact traceability
    breach the benchmark council flagged. Rows are matched by (index-name-in-a-prior-line, knob)."""
    d = json.loads(ART_JSON.read_text())
    lut = {}  # (spec, knob) -> curve point
    label = {"theodb_hnsw": "theodb_hnsw", "pgvector_hnsw": "pgvector_hnsw", "diskann": "diskann"}
    for spec in label:
        for c in d["per_spec"][spec]["curve"]:
            lut[(spec, c["knob"])] = c
    # parse markdown rows: | **theodb_hnsw** | ef=400 | 0.941 ± 0.007 | 12.19 | 100 | 15.9 |
    row = re.compile(r"\|\s*\**(theodb_hnsw|pgvector_hnsw|diskann)\**\s*\|\s*\**(?:ef|sls)=(\d+)\**\s*\|"
                     r"\s*\**([\d.]+)\s*±\s*[\d.]+\**\s*\|\s*\**([\d.]+)\**\s*\|\s*\**([\d.]+)\**\s*\|")
    matched = 0
    for m in row.finditer(ART_MD.read_text()):
        spec, knob, recall, p50, qps = m.group(1), int(m.group(2)), float(m.group(3)), float(m.group(4)), float(m.group(5))
        c = lut[(spec, knob)]
        assert abs(recall - c["recall_mean"]) <= 0.001, f"{spec} knob={knob}: recall md {recall} != json {c['recall_mean']}"
        assert abs(p50 - c["p50_ms_mean"]) <= 0.05, f"{spec} knob={knob}: p50 md {p50} != json {c['p50_ms_mean']}"
        assert abs(qps - c["qps_1client_mean"]) <= 1.0, f"{spec} knob={knob}: qps md {qps} != json {c['qps_1client_mean']}"
        matched += 1
    assert matched >= 12, f"expected >=12 table rows cross-checked, got {matched}"


def test_driver_multiclient_helper_exists_and_shaped():
    """The DB-throughput helper is wired (item 3 lever); pure-signature sanity only (no DB)."""
    mod = _load_driver()
    assert hasattr(mod, "_multiclient_qps"), "driver lost the multi-client QPS helper"
    assert "diskann" in mod.SPECS and "theodb_hnsw" in mod.SPECS and "pgvector_hnsw" in mod.SPECS
    # cosine opclass wired for theodb (requires M49)
    assert "theodb_hnsw_cosine_ops" in mod.SPECS["theodb_hnsw"]["ddl"]
