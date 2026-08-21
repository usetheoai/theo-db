"""B-069 bullet 3 — a alegacao e o artefato nao podem divergir.

MEDIDO em 2026-08-21: 170 documentos em `wiki/benchmarks/`, 13 citam bundle, e **26 citacoes estavam
quebradas em 9 arquivos** — todas residuo de UMA remocao deliberada (`7cd157d`, "remove benchmarks/ e
registra a especificacao de reconstrucao"). Nao eram fabricacoes: eram ponteiros que a limpeza deixou
para tras.

A nota do item dizia que um gate reprovaria "168 de 171" e por isso seria desligado. A medicao nao
sustenta: o gate nao exige que todo documento cite bundle — exige que QUEM CITA cite algo que
resolve. Nao ter prova e uma coisa; alegar uma prova inexistente e outra, e pior.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[3]
SCRIPT = RAIZ / ".claude" / "scripts" / "check_bundle_citations.py"


def _roda(raiz: Path, alvo: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--raiz", str(raiz), "--alvo", str(alvo)],
        capture_output=True,
        text=True,
    )


def _repo(tmp: Path) -> Path:
    subprocess.run(["git", "init", "-q", str(tmp)], check=True)
    subprocess.run(["git", "-C", str(tmp), "config", "user.email", "t@t"], check=True)
    subprocess.run(["git", "-C", str(tmp), "config", "user.name", "t"], check=True)
    return tmp


def test_a_citation_that_resolves_on_disk_passes(tmp_path: Path) -> None:
    raiz = _repo(tmp_path)
    (raiz / "benchmarks" / "artifacts" / "b1").mkdir(parents=True)
    (raiz / "w").mkdir()
    (raiz / "w" / "d.md").write_text("ver benchmarks/artifacts/b1\n", encoding="utf-8")
    r = _roda(raiz, raiz / "w")
    assert r.returncode == 0, r.stdout


def test_a_citation_that_does_not_resolve_blocks(tmp_path: Path) -> None:
    """O caso que motivou o gate: o documento aponta para um artefato que nao existe."""
    raiz = _repo(tmp_path)
    (raiz / "w").mkdir()
    (raiz / "w" / "d.md").write_text("ver benchmarks/artifacts/sumiu/x.json\n", encoding="utf-8")
    r = _roda(raiz, raiz / "w")
    assert r.returncode == 1
    assert "nao existe em disco" in r.stdout


def test_a_removed_artifact_pointed_at_by_sha_passes(tmp_path: Path) -> None:
    """A saida honesta para o que foi removido de proposito: `git:<sha>:<caminho>`.

    E a convencao que o resto do acervo ja usa para apontar a arvore `docs/` removida. Recuperar
    com `git show` e reproducao real — diferente de um caminho morto, que so parece uma.
    """
    raiz = _repo(tmp_path)
    alvo = raiz / "benchmarks" / "artifacts" / "b1"
    alvo.mkdir(parents=True)
    (alvo / "x.json").write_text("{}", encoding="utf-8")
    subprocess.run(["git", "-C", str(raiz), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(raiz), "commit", "-qm", "com o artefato"], check=True)
    sha = subprocess.run(
        ["git", "-C", str(raiz), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()[:8]
    import shutil

    shutil.rmtree(raiz / "benchmarks")
    (raiz / "w").mkdir()
    (raiz / "w" / "d.md").write_text(
        f"ver git:{sha}:benchmarks/artifacts/b1/x.json\n", encoding="utf-8"
    )
    r = _roda(raiz, raiz / "w")
    assert r.returncode == 0, r.stdout


def test_a_sha_pointer_that_does_not_resolve_blocks(tmp_path: Path) -> None:
    """Apontar para um sha onde o arquivo nao esta e o mesmo defeito com outra roupa."""
    raiz = _repo(tmp_path)
    (raiz / "w").mkdir()
    (raiz / "w" / "x.md").write_text("a", encoding="utf-8")
    subprocess.run(["git", "-C", str(raiz), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(raiz), "commit", "-qm", "vazio"], check=True)
    sha = subprocess.run(
        ["git", "-C", str(raiz), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()[:8]
    (raiz / "w" / "d.md").write_text(
        f"ver git:{sha}:benchmarks/artifacts/nunca/existiu.json\n", encoding="utf-8"
    )
    r = _roda(raiz, raiz / "w")
    assert r.returncode == 1
    assert "nao resolve no git" in r.stdout


def test_a_converted_citation_is_not_counted_twice(tmp_path: Path) -> None:
    """A primeira versao deste script contava cada citacao convertida DUAS vezes.

    Em `git:7cd157d^:benchmarks/...` o que precede `benchmarks/` e `7cd157d^:`, e nao `git:` — um
    lookbehind por `git:` nao pega, e a forma valida era reportada como caminho de disco quebrado.
    O contador subiu de 28 para 51 depois de um conserto que estava CERTO, e foi isso que revelou
    o erro no instrumento.
    """
    raiz = _repo(tmp_path)
    alvo = raiz / "benchmarks" / "artifacts" / "b1"
    alvo.mkdir(parents=True)
    (alvo / "x.json").write_text("{}", encoding="utf-8")
    subprocess.run(["git", "-C", str(raiz), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(raiz), "commit", "-qm", "c"], check=True)
    sha = subprocess.run(
        ["git", "-C", str(raiz), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()[:8]
    (raiz / "w").mkdir()
    (raiz / "w" / "d.md").write_text(
        f"git:{sha}:benchmarks/artifacts/b1/x.json\n", encoding="utf-8"
    )
    r = _roda(raiz, raiz / "w")
    assert r.returncode == 0, r.stdout
    assert "BLOQUEADO" not in r.stdout


def test_the_real_wiki_passes(tmp_path: Path) -> None:
    """O acervo de verdade. Um gate so vale enquanto o que ele fiscaliza esta limpo."""
    del tmp_path
    r = _roda(RAIZ, RAIZ / "wiki")
    assert r.returncode == 0, r.stdout

def test_a_shallow_clone_reports_what_it_could_not_check(tmp_path: Path) -> None:
    """"Nao deu para perguntar" NAO e "a resposta e nao".

    O CI clona RASO (`actions/checkout` com fetch-depth 1), entao `git cat-file` sobre um commit
    antigo responde ausencia quando a verdade e que esta copia nao tem a historia. A primeira versao
    deste gate colapsou as duas e **reprovou o proprio PR que o introduziu**, acusando 557 ponteiros
    validos de mortos.

    E a MESMA distincao que o B-051 construiu (`[]` = perguntei e nao ha, `None` = nao deu para
    perguntar) e que o B-088 violou. Terceira vez. Fica pinada aqui.
    """
    origem = _repo(tmp_path / "origem")
    alvo = origem / "benchmarks" / "artifacts" / "b1"
    alvo.mkdir(parents=True)
    (alvo / "x.json").write_text("{}", encoding="utf-8")
    (origem / "w").mkdir()
    subprocess.run(["git", "-C", str(origem), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(origem), "commit", "-qm", "com o artefato"], check=True)
    sha = subprocess.run(
        ["git", "-C", str(origem), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()[:8]

    import shutil

    shutil.rmtree(origem / "benchmarks")
    (origem / "w" / "d.md").write_text(
        f"ver git:{sha}:benchmarks/artifacts/b1/x.json\n", encoding="utf-8"
    )
    subprocess.run(["git", "-C", str(origem), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(origem), "commit", "-qm", "remove e cita por sha"], check=True)

    raso = tmp_path / "raso"
    subprocess.run(
        ["git", "clone", "-q", "--depth", "1", f"file://{origem}", str(raso)], check=True
    )
    assert (
        subprocess.run(
            ["git", "-C", str(raso), "rev-parse", "--is-shallow-repository"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        == "true"
    ), "o clone precisa ser raso para o teste medir o que se propoe"

    r = _roda(raso, raso / "w")
    assert r.returncode == 0, r.stdout
    assert "NAO foram verificadas" in r.stdout, "o que nao foi checado tem de ser DITO"
    assert "BLOQUEADO" not in r.stdout
