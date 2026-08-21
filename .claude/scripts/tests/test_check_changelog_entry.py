"""B-088 — o gate de CHANGELOG checava presenca de ARQUIVO, nao de ENTRADA.

Medido em 2026-08-20 sobre o proprio trabalho desta sessao: o commit `c52dfda` entregou o B-081 e
nao acrescentou nenhuma entrada ao `[Unreleased]`. O gate passou porque `CHANGELOG.md` ESTAVA no
diff — por causa de outras edicoes da mesma sessao.

A omissao so apareceu no corte da release, uma sessao depois e por acaso, quando a lista de itens
entregues foi comparada com a lista de itens citados. O CHANGELOG e o contrato com quem consome;
descobrir a falta no release e descobrir tarde.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from check_changelog_entry import added_unreleased_bullets, touches_production_source


def test_a_commit_that_only_reorders_the_file_added_no_bullet() -> None:
    diff = """--- a/CHANGELOG.md
+++ b/CHANGELOG.md
@@
 ## [Unreleased]
 
 ### Fixed
-- entrada antiga (#B-001)
+- entrada antiga reescrita (#B-001)
"""
    # Uma linha `-` seguida de uma `+` e reescrita, e continua sendo UMA entrada — nao duas.
    assert added_unreleased_bullets(diff) == 1


def test_an_edit_below_the_unreleased_section_does_not_count() -> None:
    """Editar uma versao JA LANCADA nao e acrescentar entrada ao contrato pendente."""
    diff = """--- a/CHANGELOG.md
+++ b/CHANGELOG.md
@@
 ## [0.160.0] - 2026-08-19
 
 ### Fixed
+- corrigindo uma redacao de versao ja lancada (#B-002)
"""
    assert added_unreleased_bullets(diff) == 0


def test_a_new_bullet_in_unreleased_counts() -> None:
    diff = """--- a/CHANGELOG.md
+++ b/CHANGELOG.md
@@
 ## [Unreleased]
 
 ### Added
+- **Algo novo e visivel para quem consome.** (#B-003)
 
 ## [0.160.0] - 2026-08-19
"""
    assert added_unreleased_bullets(diff) == 1


def test_production_source_is_detected() -> None:
    assert touches_production_source(["theodb_rs/src/am/scan.rs"])
    assert touches_production_source(["src/adapters/postgres.py"])


def test_documentation_and_tests_are_not_production_source() -> None:
    """O caso legitimo tem de continuar passando: uma sessao que so mexe em documentacao, em
    teste ou no proprio CHANGELOG nao pode ser reprovada por nao acrescentar entrada."""
    assert not touches_production_source(["CHANGELOG.md", "BACKLOG.md"])
    assert not touches_production_source(["wiki/decisions/0062-x.md"])
    assert not touches_production_source(["theodb_rs/src/am/scan_test.rs", "tests/test_x.py"])
    assert not touches_production_source([".github/workflows/ci.yml"])


# ---------------------------------------------------------------------------
# Bullet 2 do B-088 — o gate tem de reprovar contra o COMMIT REAL em que a
# omissao passou. Um gate provado so em diff sintetico e um gate cujo autor
# escolheu o proprio exame.
# ---------------------------------------------------------------------------

import subprocess


def _rc(rev: str) -> int:
    script = Path(__file__).resolve().parent.parent / "check_changelog_entry.py"
    raiz = Path(__file__).resolve().parents[3]
    return subprocess.run(
        [sys.executable, str(script), "--rev", rev],
        cwd=raiz, capture_output=True, text=True, check=False,
    ).returncode


def _existe(rev: str) -> bool:
    raiz = Path(__file__).resolve().parents[3]
    return subprocess.run(
        ["git", "cat-file", "-e", f"{rev}^{{commit}}"],
        cwd=raiz, capture_output=True, check=False,
    ).returncode == 0


def test_it_fails_against_c52dfda_the_commit_that_omitted() -> None:
    """`c52dfda` entregou o B-081, mexeu em `.claude/**/*.py` e nao acrescentou entrada.

    O gate antigo passou porque `CHANGELOG.md` estava no diff — por outras edicoes da mesma
    sessao. A omissao so apareceu no corte da release, uma sessao depois e por acaso.
    """
    if not _existe("c52dfda"):
        import pytest as _pytest

        _pytest.skip("c52dfda nao esta neste checkout")
    assert _rc("c52dfda") == 1


def test_it_passes_against_cf7633a_which_carried_its_entry() -> None:
    """O contrapositivo importa tanto quanto: um gate que reprova tudo nao distingue nada."""
    if not _existe("cf7633a"):
        import pytest as _pytest

        _pytest.skip("cf7633a nao esta neste checkout")
    assert _rc("cf7633a") == 0


# ---------------------------------------------------------------------------
# Um MERGE COMMIT nao introduz trabalho proprio, e nao lista arquivos.
#
# Medido em 2026-08-20: o hook bloqueou o encerramento da sessao logo apos um
# `git merge origin/develop`. `git show --name-only` num merge limpo devolve VAZIO, o script saiu 2
# ("nao pude inspecionar") e o hook tratou qualquer nao-zero como violacao — colapsando "nao pude
# perguntar" com "perguntei e a resposta e nao".
#
# E a mesma distincao que o portao do B-051 faz entre `[]` e `None` para as tags semver, e que eu
# construi la e nao apliquei aqui.
# ---------------------------------------------------------------------------


def test_a_merge_commit_is_not_a_violation() -> None:
    """Um merge integra trabalho ja registrado; exigir entrada nova dele e exigir entrada em dobro."""
    raiz = Path(__file__).resolve().parents[3]
    merge = subprocess.run(
        ["git", "log", "--merges", "--format=%H", "-1"],
        cwd=raiz, capture_output=True, text=True, check=False,
    ).stdout.strip()
    if not merge:
        import pytest as _pytest

        _pytest.skip("sem merge commit neste checkout")
    assert _rc(merge) == 0
