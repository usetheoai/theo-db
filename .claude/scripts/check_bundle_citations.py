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


def _clone_raso(raiz: Path) -> bool:
    r = subprocess.run(
        ["git", "-C", str(raiz), "rev-parse", "--is-shallow-repository"],
        capture_output=True,
        text=True,
    )
    return r.stdout.strip() == "true"


def _existe_no_git(raiz: Path, sha: str, caminho: str) -> bool | None:
    """`True` existe, `False` nao existe, **`None` nao deu para perguntar**.

    A distincao e a que este projeto ja pagou para aprender duas vezes (B-051, B-088), e ela mordeu
    de novo aqui: no CI o `actions/checkout` clona RASO, entao um `git cat-file` sobre um commit
    antigo responde "nao existe" quando a verdade e "esta copia nao tem essa historia". Colapsar as
    duas faz o gate acusar de citacao morta um ponteiro perfeitamente valido — e um gate que acusa
    trabalho correto e desligado antes de pegar o primeiro defeito real.
    """
    r = subprocess.run(
        ["git", "-C", str(raiz), "cat-file", "-e", f"{sha}:{caminho}"],
        capture_output=True,
    )
    if r.returncode == 0:
        return True
    return None if _clone_raso(raiz) else False


def verificar(raiz: Path, alvo: Path) -> tuple[list[tuple[Path, str, str]], int]:
    """`(quebradas, nao_verificaveis)` — a segunda e o que o clone raso impediu de checar."""
    quebradas: list[tuple[Path, str, str]] = []
    nao_verificaveis = 0
    for f in sorted(alvo.rglob("*.md")):
        texto = f.read_text(encoding="utf-8")
        for m in NO_GIT.finditer(texto):
            veredito = _existe_no_git(raiz, m.group(1), m.group(2))
            if veredito is None:
                nao_verificaveis += 1
            elif veredito is False:
                quebradas.append((f, m.group(0), "nao resolve no git"))
        # Só depois de tirar as formas `git:` é que sobra o que de fato aponta para disco.
        sem_git = NO_GIT.sub("", texto)
        for m in DISCO.finditer(sem_git):
            caminho = m.group(0).rstrip(".,;:)`'\"")
            if not (raiz / caminho).exists():
                quebradas.append((f, caminho, "nao existe em disco"))
    return quebradas, nao_verificaveis


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raiz", type=Path, default=Path.cwd())
    p.add_argument("--alvo", type=Path, default=None, help="default: <raiz>/wiki")
    args = p.parse_args()
    alvo = args.alvo or (args.raiz / "wiki")
    if not alvo.exists():
        print(f"OK: {alvo} nao existe — nada a verificar")
        return 0

    quebradas, nao_verificaveis = verificar(args.raiz, alvo)
    # Dito SEMPRE, e nao so quando convem: um relatorio que omite o que nao checou reporta uma
    # cobertura que nao teve. Mesmo principio do `--max-zone-files` do detector de vazamento, que
    # declara PARTIAL em vez de alegar completude.
    if nao_verificaveis:
        print(
            f"NOTA: {nao_verificaveis} citacao(oes) `git:<sha>:` NAO foram verificadas — este clone "
            f"e raso e nao tem essa historia. Nao e um veredito sobre elas."
        )
    if not quebradas:
        print("OK: toda citacao de bundle verificavel resolve (em disco ou por `git show`)")
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
