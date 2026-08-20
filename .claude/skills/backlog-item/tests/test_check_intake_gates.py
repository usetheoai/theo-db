"""Os gates G1 e G2 do intake eram mecanizáveis e não eram mecanizados.

`/backlog-item` declara cinco hard gates e não embarcava um único script. G3
(domínio único), G4 (DoD verificável) e G5 (sem prior-art) são julgamento e
seguem conversacionais, cobertos por eval. G1 (repo resolve) e G2 (a busca de
dedup rodou) não são: o `scripts/route_domain.py` já existia, com 23 testes, e a
skill não o chamava — instruía um `python3 -c` inline e um `grep` que ninguém
verificava. Um gate cuja execução depende de o agente lembrar não é um gate.
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).parent.parent / "scripts" / "check_intake_gates.py"

#: Tabela de roteamento e especialista que existem SÓ para este teste.
#:
#: Os casos abaixo liam a tabela INSTALADA, então mediam a configuração do
#: repositório consumidor em vez do script: no `theo-db` — cuja tabela nomeia
#: outros repos e cujos especialistas são os pilares do produto — três deles
#: falhavam, e no repositório do kit passavam apenas porque `theo-rag` estava
#: listado lá. Um teste unitário que só passa em um consumidor não testa o script.
ROUTING = """# Cycle: BACKLOG

## Domain routing

| Domain | Repos (present on disk) | Specialist |
|---|---|---|
| `data-plane-ts` | `theo-lens`, `theo-rag` | `agents/data-plane-ts.md` |
"""


def _ecosystem(tmp_path: Path) -> Path:
    root = tmp_path / "ecosystem"
    (root / "rules").mkdir(parents=True)
    (root / "agents").mkdir(parents=True)
    (root / "rules" / "cycle-backlog.md").write_text(ROUTING, encoding="utf-8")
    (root / "agents" / "data-plane-ts.md").write_text("# data-plane-ts\n", encoding="utf-8")
    return root

BACKLOG = """# Backlog

## Index

## B-007 — Suspeita de N+1 no ingest do theo-lens   [ ]

domain: data-plane-ts
repo: theo-lens
status: raw
why_now: o ingest ficou lento depois do último deploy

## B-008 — Explorer de traces com p95 alto   [x]

domain: data-plane-ts
repo: theo-lens
status: shipped

## B-009 — Cache de sessão que ninguém mediu   [ ]

domain: data-plane-ts
repo: theo-lens
status: killed
"""


def _run(backlog: Path, repo: str, terms: list[str]) -> tuple[int, dict]:
    args = [
        sys.executable, str(SCRIPT),
        "--backlog", str(backlog),
        "--repo", repo,
        "--project-root", str(_ecosystem(backlog.parent)),
    ]
    for term in terms:
        args.extend(["--term", term])
    result = subprocess.run(args, capture_output=True, text=True)
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        data = {"raw": result.stdout, "stderr": result.stderr}
    return result.returncode, data


def _backlog(tmp_path: Path) -> Path:
    path = tmp_path / "BACKLOG.md"
    path.write_text(BACKLOG, encoding="utf-8")
    return path


def test_unknown_repo_is_refused_by_g1(tmp_path: Path) -> None:
    rc, data = _run(_backlog(tmp_path), "repo-que-nao-existe", ["cache"])
    assert rc == 1
    assert data["verdict"] == "ITEM_REJECTED"
    assert data["g1"]["routed"] is False


def test_known_repo_routes_and_names_the_specialist(tmp_path: Path) -> None:
    rc, data = _run(_backlog(tmp_path), "theo-rag", ["nada-casa-aqui"])
    assert data["g1"]["routed"] is True
    assert data["g1"]["domain"]
    assert data["g1"]["agent"]


def test_no_dedup_hit_passes_both_gates(tmp_path: Path) -> None:
    """Repo sem item algum no registro: G1 roteia, G2 buscou e não achou nada."""
    rc, data = _run(_backlog(tmp_path), "theo-rag", ["nada-casa-aqui"])
    assert rc == 0
    assert data["verdict"] == "GATES_PASS"
    assert data["g2"]["searched"] is True
    assert data["g2"]["candidates"] == []


def test_open_item_hit_recommends_merge(tmp_path: Path) -> None:
    rc, data = _run(_backlog(tmp_path), "theo-lens", ["ingest"])
    assert rc == 3
    assert data["verdict"] == "DEDUP_CANDIDATES"
    candidate = next(c for c in data["g2"]["candidates"] if c["id"] == "B-007")
    assert candidate["status"] == "raw"
    assert candidate["recommended_action"] == "ITEM_MERGED"


def test_shipped_item_hit_recommends_regression_link(tmp_path: Path) -> None:
    rc, data = _run(_backlog(tmp_path), "theo-lens", ["traces"])
    candidate = next(c for c in data["g2"]["candidates"] if c["id"] == "B-008")
    assert candidate["recommended_action"] == "regression_of"


def test_killed_item_hit_recommends_supersedes(tmp_path: Path) -> None:
    rc, data = _run(_backlog(tmp_path), "theo-lens", ["cache"])
    candidate = next(c for c in data["g2"]["candidates"] if c["id"] == "B-009")
    assert candidate["recommended_action"] == "supersedes"


def test_the_repo_name_itself_is_always_a_search_term(tmp_path: Path) -> None:
    """A skill manda buscar os substantivos MAIS o repo; deixar isso a cargo de
    quem chama é como o repo saía da busca sem ninguém notar."""
    rc, data = _run(_backlog(tmp_path), "theo-lens", [])
    assert "theo-lens" in data["g2"]["terms"]
    assert data["g2"]["candidates"], data


def test_missing_backlog_fails_loudly(tmp_path: Path) -> None:
    rc, data = _run(tmp_path / "nao-existe.md", "theo-lens", ["x"])
    assert rc == 2
