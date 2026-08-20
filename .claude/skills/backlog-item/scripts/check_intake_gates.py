#!/usr/bin/env python3
"""Gates G1 e G2 do intake, executados em vez de lembrados.

POR QUE ESTE SCRIPT EXISTE
--------------------------
`rules/cycle-backlog.md` declara cinco hard gates e a skill não embarcava um
único script. G3 (domínio único), G4 (DoD verificável) e G5 (sem justificativa
por prior art) são julgamento e seguem conversacionais — é o desenho certo, e a
bateria de evals cobre exatamente isso. G1 e G2 não são julgamento:

  G1 — o `repo` resolve para um domínio com especialista em disco.
       `scripts/route_domain.py` já fazia isso, com 23 testes, e a skill não o
       chamava: instruía um `python3 -c` inline.
  G2 — a busca de dedup RODOU. A skill instruía um `grep` cuja execução ninguém
       verificava depois. Um gate que depende de o agente lembrar não é um gate;
       é uma intenção.

O que este script NÃO faz: decidir. Ele roteia, busca, e devolve os candidatos
com a ação que a regra prescreve para cada status. A escolha entre `ITEM_MERGED`,
`supersedes` e `regression_of` continua sendo do humano no grill — um keyword hit
é candidato, não veredito, e automatizar essa decisão inventaria fusões erradas.

Uso:
    python3 check_intake_gates.py --backlog BACKLOG.md --repo theo-lens \\
        --term ingest --term latencia

Exit codes:
    0 — G1 e G2 passaram, nenhum candidato de dedup
    1 — G1 recusou o item (repo fora da tabela de roteamento)
    2 — erro de execução (BACKLOG.md ausente, tabela ilegível)
    3 — G2 encontrou candidatos: leia cada bloco antes de alocar um id novo
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

#: Uma definição do formato do bloco, importada de quem já a mantém. Um segundo
#: regex aqui divergiria em silêncio, e os dois discordariam sobre o que o
#: registro contém — o defeito exato que o índice do backlog existe para expor.
def _load_block_re() -> Any:
    for base in (
        Path(__file__).resolve().parents[2] / "backlog-review" / "scripts",
        Path(__file__).resolve().parents[4] / "skills" / "backlog-review" / "scripts",
    ):
        if (base / "check_backlog_structure.py").is_file():
            sys.path.insert(0, str(base))
            from check_backlog_structure import BLOCK_RE  # noqa: PLC0415

            return BLOCK_RE
    raise FileNotFoundError(
        "check_backlog_structure.py não encontrado — o parser de blocos do BACKLOG "
        "é mantido lá e não é duplicado aqui"
    )


STATUS_RE = re.compile(r"^status:\s*`?([a-z_]+)`?", re.MULTILINE)

#: O que a regra manda fazer para cada status atingido pela busca
#: (`rules/cycle-backlog.md § Chain`, Step 2 do SKILL).
ACTION_BY_STATUS = {
    "raw": "ITEM_MERGED",
    "triaged": "ITEM_MERGED",
    "planned": "ITEM_MERGED",
    "killed": "supersedes",
    "shipped": "regression_of",
}


def _route(repo: str, project_root: Path) -> dict[str, Any]:
    """G1 — delega à tabela de roteamento, que é a fonte única."""
    for candidate in (project_root / "scripts" / "route_domain.py",
                      project_root / ".claude" / "scripts" / "route_domain.py",
                      Path(__file__).resolve().parents[3] / "scripts" / "route_domain.py"):
        if candidate.is_file():
            script = candidate
            break
    else:
        return {"routed": False, "error": "route_domain.py não encontrado"}

    # `--project-root` escolhia QUAL script e não QUAL tabela: o `route_domain.py`
    # resolve a tabela pela própria localização, então apontar o flag para outro
    # projeto continuava roteando pela tabela instalada. O nome do flag prometia o
    # escopo inteiro e entregava metade — e um teste que passasse esse flag estaria
    # medindo a configuração do consumidor em vez do script.
    command = [sys.executable, str(script), repo, "--json"]
    for table in (project_root / "rules" / "cycle-backlog.md",
                  project_root / ".claude" / "rules" / "cycle-backlog.md"):
        if table.is_file():
            command.extend(["--rule", str(table)])
            break

    result = subprocess.run(command, capture_output=True, text=True)
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        return {"routed": False, "error": result.stderr.strip() or "saída ilegível"}
    payload.setdefault("routed", False)
    return payload


def _dedup(backlog_text: str, terms: list[str]) -> list[dict[str, Any]]:
    """G2 — todo bloco cujo título ou corpo casa qualquer termo (case-insensitive)."""
    block_re = _load_block_re()
    matches = list(block_re.finditer(backlog_text))
    lowered = [t.lower() for t in terms if t.strip()]
    candidates: list[dict[str, Any]] = []

    for i, match in enumerate(matches):
        start = match.end()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(backlog_text)
        body = backlog_text[start:end]
        haystack = f"{match.group(2)}\n{body}".lower()

        hits = [term for term in lowered if term in haystack]
        if not hits:
            continue
        status_match = STATUS_RE.search(body)
        status = status_match.group(1) if status_match else "unknown"
        candidates.append({
            "id": match.group(1),
            "title": match.group(2).strip(),
            "status": status,
            "matched_terms": hits,
            "recommended_action": ACTION_BY_STATUS.get(status, "read the block before deciding"),
        })
    return candidates


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backlog", type=Path, required=True)
    parser.add_argument("--repo", required=True, help="o campo `repo:` do item (gate G1)")
    parser.add_argument("--term", action="append", default=[],
                        help="substantivo significativo da descrição (repetível)")
    parser.add_argument("--project-root", type=Path, default=None)
    args = parser.parse_args(argv)

    if not args.backlog.is_file():
        print(json.dumps({
            "verdict": "ERROR",
            "message": f"BACKLOG.md não encontrado em {args.backlog} — rode /backlog-init primeiro",
        }, indent=2, ensure_ascii=False))
        return 2

    project_root = args.project_root or Path.cwd()
    g1 = _route(args.repo, project_root)

    # O nome do repo é SEMPRE um termo de busca. Deixar isso a cargo de quem chama
    # é como o repo saía da busca sem ninguém notar — e o repo é o termo que mais
    # colide num registro que cobre 21 deles.
    terms = list(dict.fromkeys([*args.term, args.repo]))
    try:
        candidates = _dedup(args.backlog.read_text(encoding="utf-8-sig"), terms)
    except FileNotFoundError as exc:
        print(json.dumps({"verdict": "ERROR", "message": str(exc)}, indent=2, ensure_ascii=False))
        return 2

    if not g1.get("routed"):
        verdict, code = "ITEM_REJECTED", 1
    elif candidates:
        verdict, code = "DEDUP_CANDIDATES", 3
    else:
        verdict, code = "GATES_PASS", 0

    print(json.dumps({
        "verdict": verdict,
        "g1": g1,
        "g2": {"searched": True, "terms": terms, "candidates": candidates},
    }, indent=2, ensure_ascii=False))
    return code


if __name__ == "__main__":
    sys.exit(main())
