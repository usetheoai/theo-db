#!/usr/bin/env python3
"""O commit acrescentou uma ENTRADA ao `[Unreleased]`, ou so tocou o arquivo? (B-088)

O gate anterior perguntava `echo "$ALL_FILES" | grep -qE '^CHANGELOG\\.md$'` — se o arquivo estava
no diff. Medido em 2026-08-20 sobre o proprio trabalho desta sessao: o commit `c52dfda` entregou o
B-081, tocou o `CHANGELOG.md` por OUTRAS razoes, e nao acrescentou nenhuma entrada. O gate passou.

A omissao so apareceu no corte da release, uma sessao depois e por acaso. O CHANGELOG e o contrato
com quem consome (Regra Inquebravel 6); descobrir a falta no release e descobrir tarde.

Uso:
    python3 check_changelog_entry.py [--rev HEAD]

Saida: 0 quando ha entrada ou quando nao havia obrigacao; 1 quando codigo de producao mudou e
nenhuma entrada foi acrescentada; 2 em erro de invocacao.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys

#: Extensoes que carregam comportamento visivel para quem consome o produto.
FONTES = (".go", ".py", ".ts", ".tsx", ".js", ".jsx", ".rs", ".java", ".kt", ".rb", ".cs", ".sql")

#: Caminhos que NAO sao codigo de producao.
#:
#: Esta lista ESPELHA a que o `stop-validation.sh` ja usava, deliberadamente. Meu primeiro rascunho
#: inventou uma definicao propria — excluindo `.claude/`, `wiki/` e `docs/` — e o resultado foi que
#: o gate absolveu o proprio commit que motivou o item: `c52dfda` mexe em `.claude/**/*.py`, que o
#: gate existente CONTA como producao e cujo trabalho o CHANGELOG deste projeto documenta.
#:
#: Duas definicoes de "codigo de producao" no mesmo repositorio divergem, e a segunda teria mudado
#: em silencio o que o portao significa. Uma definicao, e este arquivo torna o gate mais ESTRITO
#: sem redefinir o que ele olha.
#:
#: Documentacao, workflow e markdown nao precisam de exclusao: nenhuma dessas extensoes esta em
#: `FONTES`, entao ja nao contam.
NAO_PRODUCAO = re.compile(
    r"(^|/)(tests?|__tests__|testdata|fixtures)/"
    r"|(_test|\.test|\.spec)\.[a-z]+$"
    r"|(^|/)test_[^/]+\.[a-z]+$"
    r"|(^|/)(node_modules|vendor|dist|build|target|\.venv|__pycache__)/"
)


def touches_production_source(arquivos: list[str]) -> bool:
    """Algum dos arquivos e codigo de producao?"""
    return any(
        f.endswith(FONTES) and not NAO_PRODUCAO.search(f) for f in arquivos if f
    )


def added_unreleased_bullets(diff: str) -> int:
    """Quantas linhas de bullet FORAM ACRESCENTADAS dentro da secao `[Unreleased]`.

    Conta linhas `+` que comecam um bullet, e apenas enquanto o cursor esta dentro do
    `[Unreleased]` — uma edicao numa versao ja lancada nao acrescenta nada ao contrato pendente.

    Uma reescrita (`-` seguido de `+`) conta como UMA entrada, que e o que ela e. O gate pergunta
    "ha entrada nova?", nao "quantas linhas mudaram?".
    """
    dentro = False
    total = 0
    for linha in diff.splitlines():
        if linha.startswith(("+++", "---", "@@")):
            continue
        conteudo = linha[1:] if linha[:1] in "+- " else linha
        cabecalho = conteudo.strip()
        if cabecalho.startswith("## ["):
            dentro = cabecalho.startswith("## [Unreleased]")
            continue
        if dentro and linha.startswith("+") and conteudo.lstrip().startswith("- "):
            total += 1
    return total


def _git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=False
    ).stdout


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rev", default="HEAD", help="revisao a inspecionar (padrao: HEAD)")
    args = p.parse_args()

    # Um MERGE COMMIT nao introduz trabalho proprio: ele integra commits que ja passaram por este
    # portao. `git show --name-only` num merge limpo devolve VAZIO, e tratar isso como "nao pude
    # inspecionar" fazia o hook bloquear o encerramento logo apos um `git merge origin/develop`.
    #
    # A distincao e a mesma que o portao do B-051 faz entre `[]` e `None` nas tags semver — "nao ha
    # o que perguntar" nao e "perguntei e a resposta e nao". Eu a construi la e nao a apliquei aqui.
    pais = _git("rev-list", "--parents", "-n", "1", args.rev).split()
    if len(pais) > 2:
        print(f"OK: {args.rev} e um merge — integra trabalho ja registrado")
        return 0

    arquivos = [f for f in _git("show", "--name-only", "--format=", args.rev).splitlines() if f]
    if not arquivos:
        print(f"check_changelog_entry: {args.rev} nao lista arquivos", file=sys.stderr)
        return 2
    if not touches_production_source(arquivos):
        print(f"OK: {args.rev} nao toca codigo de producao")
        return 0

    bullets = added_unreleased_bullets(_git("show", args.rev, "--", "CHANGELOG.md"))
    if bullets:
        print(f"OK: {args.rev} acrescentou {bullets} entrada(s) ao [Unreleased]")
        return 0

    print(
        f"{args.rev} muda codigo de producao e NAO acrescenta entrada ao [Unreleased].\n"
        "  Tocar o arquivo nao e acrescentar entrada — foi assim que as entradas do B-081\n"
        "  ficaram de fora e so apareceram no corte da release, uma sessao depois (B-088).",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
