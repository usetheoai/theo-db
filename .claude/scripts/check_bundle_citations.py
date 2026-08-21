#!/usr/bin/env python3
"""Um número publicado e o artefato que o sustenta não podem divergir.

B-069, bullet 3. Um documento de `wiki/benchmarks/` que cita um bundle tem de citar um bundle que
EXISTE — em disco, ou no git sob a forma `git:<sha>:<caminho>` que o resto do acervo já usa para
apontar o que foi removido (`CLAUDE.md`, seção da wiki).

POR QUE ESTE GATE, E POR QUE AGORA. A nota do item dizia que ele reprovaria "168 de 171" e que um
portão que nunca passa alguém desliga. **A medição não sustenta isso.** Medido em 2026-08-21: 170
documentos, 13 citam bundle, e as citações quebradas são **26, concentradas em 9 arquivos** — todas
resíduo de UMA remoção deliberada (`7cd157d`, "remove benchmarks/ e registra a especificação de
reconstrução"). Não é uma decisão pendente sobre 168 documentos; é uma limpeza que ficou pela metade.

O QUE ESTE GATE NÃO FAZ: não exige que todo documento cite bundle. Exigir isso hoje seria a proposta
que a nota corretamente recusou. Ele exige apenas que **quem cita, cite algo que resolve** — que é a
diferença entre não ter prova e alegar uma prova inexistente. A segunda é pior: ela convida o leitor
a confiar num artefato que ninguém pode abrir.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

#: `benchmarks/artifacts/...` em disco, ou `git:<sha>:<caminho>` recuperável por `git show`.
#:
#: As formas `git:` são retiradas do texto ANTES de procurar as de disco. Um lookbehind não serve:
#: em `git:7cd157d^:benchmarks/...` o que precede `benchmarks/` é `7cd157d^:`, e não `git:` — a
#: primeira versão deste script errou exatamente aí e contou cada citação convertida DUAS vezes.
DISCO = re.compile(r"\bbenchmarks/artifacts/[A-Za-z0-9._/\-]+")
NO_GIT = re.compile(r"\bgit:([0-9a-f]{7,40}\^?):([A-Za-z0-9._/\-]+)")


def _existe_no_git(raiz: Path, sha: str, caminho: str) -> bool:
    r = subprocess.run(
        ["git", "-C", str(raiz), "cat-file", "-e", f"{sha}:{caminho}"],
        capture_output=True,
    )
    return r.returncode == 0


def verificar(raiz: Path, alvo: Path) -> list[tuple[Path, str, str]]:
    """Devolve `(arquivo, citacao, motivo)` para cada citação que não resolve."""
    quebradas: list[tuple[Path, str, str]] = []
    for f in sorted(alvo.rglob("*.md")):
        texto = f.read_text(encoding="utf-8")
        for m in NO_GIT.finditer(texto):
            if not _existe_no_git(raiz, m.group(1), m.group(2)):
                quebradas.append((f, m.group(0), "nao resolve no git"))
        # Só depois de tirar as formas `git:` é que sobra o que de fato aponta para disco.
        sem_git = NO_GIT.sub("", texto)
        for m in DISCO.finditer(sem_git):
            caminho = m.group(0).rstrip(".,;:)`'\"")
            if not (raiz / caminho).exists():
                quebradas.append((f, caminho, "nao existe em disco"))
    return quebradas


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raiz", type=Path, default=Path.cwd())
    p.add_argument("--alvo", type=Path, default=None, help="default: <raiz>/wiki")
    args = p.parse_args()
    alvo = args.alvo or (args.raiz / "wiki")
    if not alvo.exists():
        print(f"OK: {alvo} nao existe — nada a verificar")
        return 0

    quebradas = verificar(args.raiz, alvo)
    if not quebradas:
        print("OK: toda citacao de bundle resolve (em disco ou por `git show`)")
        return 0

    print(f"BLOQUEADO: {len(quebradas)} citacao(oes) de bundle nao resolve(m)\n")
    for f, cit, motivo in quebradas:
        print(f"  {f.relative_to(args.raiz)}")
        print(f"    {cit}  ({motivo})")
    print(
        "\nUm numero publicado e o artefato que o sustenta nao podem divergir. Conserte apontando\n"
        "para um bundle que existe, ou — se ele foi removido de proposito — para a forma\n"
        "`git:<sha>:<caminho>`, que o `git show` recupera."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
