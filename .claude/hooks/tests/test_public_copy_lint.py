"""B-091 — o lint de copy publica tinha falso positivo E falso negativo ao mesmo tempo.

Medido em 2026-08-20 contra o `README.md` real:

  · FALSO POSITIVO — avisava de `production-ready` nas quatro ocorrencias do arquivo, e as quatro
    sao NEGACOES: *"Sem afirmacao de 'production-ready' ate haver evidencia"*, *"**Nao e
    production-ready**"*, *"o gate para sequer comecar a alegar production-ready"*. Ele avisava
    exatamente sobre o texto que existe para CUMPRIR a regra.

  · FALSO NEGATIVO — `public-copy.md § 6` bane `lock-in free`/`lock-in proof` como claim absoluta, e
    o README dizia *"sem lock-in"*. Nao avisado. O lint cobria 3 das 4 familias do § 6.

Um lint com as duas falhas ao mesmo tempo e pior que nenhum: treina o leitor a ignorar o aviso,
porque o aviso que ele da e sobre o texto certo, enquanto deixa passar o que deveria pegar.

E o hook e um PostToolUse que le o caminho e o conteudo do STDIN como JSON. Invoca-lo sem isso nao
verifica NADA e sai 0 — foi o meu primeiro erro ao investigar, e e a razao de o bullet 3 do item
exigir um teste que use a entrada real.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

HOOK = Path(__file__).resolve().parent.parent / "public-copy-lint.sh"
RAIZ = Path(__file__).resolve().parents[3]


def _lint(texto: str, nome: str = "README.md") -> str:
    """Roda o hook com a entrada JSON que ele espera e devolve a saida."""
    entrada = json.dumps(
        {"tool_input": {"file_path": str(RAIZ / nome), "content": texto}}
    )
    r = subprocess.run(
        ["bash", str(HOOK)], input=entrada, capture_output=True, text=True, cwd=RAIZ
    )
    return r.stdout + r.stderr


# --------------------------------------------------------------- falso positivo


@pytest.mark.parametrize(
    "linha",
    [
        "> Sem afirmação de \"production-ready\" até haver evidência de uso sustentado.",
        "**Não é production-ready**: falta a única coisa que benchmark não dá.",
        "o gate para sequer começar a alegar production-ready",
        "Nao e battle-tested, e dizer isso e o ponto.",
    ],
)
def test_a_negated_term_is_not_a_claim(linha: str) -> None:
    """O texto que DECLARA nao ter a propriedade e o texto que cumpre a regra.

    Avisar sobre ele e o defeito: ensina que o aviso nao vale a pena ler.
    """
    saida = _lint(f"# Projeto\n\n{linha}\n")
    assert "[WARN]" not in saida, f"nao devia avisar sobre negacao: {saida!r}"


def test_an_actual_claim_still_warns() -> None:
    """A correcao nao pode transformar o lint em carimbo."""
    saida = _lint("# Projeto\n\nEste banco e production-ready e battle-tested.\n")
    assert "production-ready" in saida
    assert "battle-tested" in saida


# --------------------------------------------------------------- falso negativo


@pytest.mark.parametrize(
    "linha",
    [
        "Aberto, portavel e sem lock-in.",
        "A truly lock-in free database.",
        "lock-in proof by design",
        "O AlloyDB killer que faltava.",
        "Um drop-in replacement para o pgvector.",
        "Upgrades com zero downtime.",
    ],
)
def test_the_four_banned_framings_of_section_6_are_checked(linha: str) -> None:
    """`public-copy.md § 6` bane quatro familias, e o lint cobria tres."""
    saida = _lint(f"# Projeto\n\n{linha}\n")
    assert "[WARN]" in saida, f"devia avisar sobre: {linha!r}"


def test_a_scoped_exit_affordance_is_what_the_rule_asks_for() -> None:
    """O § 6 nao pede silencio sobre o tema — pede a afordancia CONCRETA no lugar do absoluto."""
    saida = _lint(
        "# Projeto\n\nOs dados saem por `pg_dump` e por Parquet, e a extensao e Apache 2.0.\n"
    )
    assert "[WARN]" not in saida


# --------------------------------------------------------------- o arquivo real


def test_the_projects_own_readme_is_clean() -> None:
    """O README deste repositorio tem de passar no proprio lint.

    Hoje ele dispara quatro avisos, todos sobre negacoes — que e o bullet 1 do B-091 por medicao,
    e nao por argumento.
    """
    readme = (RAIZ / "README.md").read_text(encoding="utf-8")
    saida = _lint(readme)
    assert "[WARN]" not in saida, f"o README do projeto nao passa no proprio lint:\n{saida}"


# --------------------------------------------------------------- limites ditos


def test_a_line_that_negates_one_term_and_asserts_another_loses_both() -> None:
    """LIMITACAO CONHECIDA do filtro por linha, fixada aqui em vez de descoberta depois.

    O filtro remove a LINHA quando ela carrega um marcador de negacao, entao uma linha que negue um
    termo e AFIRME outro perde os dois. Encontrado no proprio README: a linha que diz "nunca
    afirmacoes sem evidencia" tambem dizia "sem lock-in", e o segundo passava por causa do primeiro.

    A frase do README foi reescrita para nao depender dessa sorte. Este teste existe para que a
    limitacao seja um fato conhecido e nao uma surpresa — se alguem tornar o filtro mais preciso,
    ele falha e e removido, que e o desfecho desejado.
    """
    saida = _lint("# Projeto\n\nNunca afirmações sem evidência, e somos lock-in free.\n")
    assert "[WARN]" not in saida, (
        "se este teste FALHOU, o filtro ficou mais preciso — remova o teste e a limitacao do "
        "comentario em `public-copy-lint.sh`"
    )


def test_a_claim_split_across_two_lines_is_not_seen() -> None:
    """Segunda limitacao do mesmo desenho: o grep e por linha, e uma claim quebrada nao casa.

    Era o caso real do README — `sem\\nlock-in` atravessava a quebra e passava por acidente, nao por
    projeto. A frase foi reescrita; a limitacao continua.
    """
    saida = _lint("# Projeto\n\nAberto, portavel e sem\nlock-in.\n")
    assert "[WARN]" not in saida
