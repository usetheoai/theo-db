"""Tests for check_evidence_pointers.py — verifies the fabricated-evidence hard cap."""
from __future__ import annotations

from pathlib import Path

import pytest

import check_evidence_pointers as cep
from check_evidence_pointers import check_evidence_pointers  # noqa: E402


@pytest.fixture
def rooted(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Anchor pointer resolution to a hermetic project root.

    The checker resolves pointers against the walked-up project root. Tests must not
    depend on files that happen to exist in the real repo.
    """
    root = tmp_path / "project"
    (root / ".claude").mkdir(parents=True)
    monkeypatch.setattr(cep, "_find_project_root", lambda _start: root)
    return root


def _opportunity(root: Path, body: str) -> Path:
    path = root / "opportunity.md"
    path.write_text(f"# Opportunity: Test\n\n{body}\n", encoding="utf-8")
    return path


def test_resolving_pointer_is_verified(rooted: Path) -> None:
    target = rooted / "src" / "handler.py"
    target.parent.mkdir(parents=True)
    target.write_text("\n".join(f"line {i}" for i in range(1, 51)), encoding="utf-8")

    report = check_evidence_pointers(_opportunity(rooted, "See `src/handler.py:42` for the duplication."))
    assert report["verified"] == 1
    assert report["fabricated"] == 0


def test_missing_file_is_fabricated(rooted: Path) -> None:
    report = check_evidence_pointers(_opportunity(rooted, "See `src/nope.py:42` for the bug."))
    assert report["fabricated"] == 1
    assert report["fabricated_pointers"]["src/nope.py:42"] == "missing_file"


def test_line_past_end_of_file_is_fabricated(rooted: Path) -> None:
    """The capability the ancestor lacked.

    check_reference_citations stripped the `:LINE` suffix before testing the path, so a
    pointer at line 400 of a 30-line file passed as verified. Evidence that points past
    the end of a file is evidence that moved, and downstream trusts it as measured fact.
    """
    target = rooted / "src" / "short.py"
    target.parent.mkdir(parents=True)
    target.write_text("one\ntwo\nthree\n", encoding="utf-8")

    report = check_evidence_pointers(_opportunity(rooted, "See `src/short.py:400`."))
    assert report["verified"] == 0
    assert report["fabricated"] == 1
    assert "line_out_of_range" in report["fabricated_pointers"]["src/short.py:400"]
    assert "3 lines" in report["fabricated_pointers"]["src/short.py:400"]


def test_line_zero_is_rejected(rooted: Path) -> None:
    target = rooted / "src" / "x.py"
    target.parent.mkdir(parents=True)
    target.write_text("one\ntwo\n", encoding="utf-8")

    report = check_evidence_pointers(_opportunity(rooted, "See `src/x.py:0`."))
    assert report["fabricated"] == 1


def test_blocked_pointer_not_counted_as_fabricated(rooted: Path) -> None:
    """An explicitly BLOCKED pointer is a documented gap, not a fabrication."""
    report = check_evidence_pointers(
        _opportunity(rooted, "See `src/gone.py:42` <!-- BLOCKED: file removed in the 2026-07 refactor -->")
    )
    assert report["fabricated"] == 0
    assert report["explicitly_blocked"] == 1


def test_runtime_observations_counted_separately(rooted: Path) -> None:
    """HTTP observations are recorded, never 'verified'.

    They are transient: re-running the same request against a dev environment can
    legitimately differ. Reporting them as verified would assert rather than measure.
    """
    report = check_evidence_pointers(
        _opportunity(
            rooted,
            "Observed `GET https://app-dev.usetheo.dev/api/traces -> 500` twice in a row.\n"
            "Then `POST https://app-dev.usetheo.dev/api/login -> 200`.",
        )
    )
    assert report["runtime_observations"] == 2
    assert report["total"] == 0  # no code pointers
    assert report["verified"] == 0
    assert report["evidence_total"] == 2


def test_prose_ratios_are_not_mistaken_for_pointers(rooted: Path) -> None:
    """`4:1` and `step 3:12` are not evidence pointers.

    The pattern requires a slash and a file extension precisely so that ordinary prose
    does not inflate the evidence count — or, worse, get reported as fabricated.
    """
    report = check_evidence_pointers(
        _opportunity(rooted, "The ratio is 4:1 and step 3:12 of the runbook covers it.")
    )
    assert report["total"] == 0
    assert report["fabricated"] == 0


def test_no_evidence_at_all(rooted: Path) -> None:
    report = check_evidence_pointers(_opportunity(rooted, "Purely narrative, no pointers."))
    assert report["total"] == 0
    assert report["fabricated"] == 0
    assert report["evidence_total"] == 0


# ---------------------------------------------------------------------------
# B-081 — resolução de caminho ciente de layout, numa implementação só.
# ---------------------------------------------------------------------------


def test_a_pointer_into_the_ecosystem_resolves_from_a_plugin_layout(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """O defeito, reproduzido: num consumidor o ecossistema vive sob `.claude/`.

    Medido em 2026-08-20 no `theo-db`: a fixture `good-opportunity.md` **da própria skill**
    pontuou `INVALID` com `fabricated_evidence` e os quatro pointers `missing_file`. No kit
    passa, porque lá `rules/` está na raiz — de modo que o defeito é invisível exatamente onde
    o código é mantido, e universal onde ele é usado.

    Um falso positivo de "evidência fabricada" é o pior tipo: ele acusa de desonestidade quem
    escreveu uma oportunidade legítima, e ensina a ignorar o portão.
    """
    root = tmp_path / "consumidor"
    eco = root / ".claude"
    for d in ("skills", "rules", "hooks"):
        (eco / d).mkdir(parents=True)
    (eco / "rules" / "cycle-discover.md").write_text("\n".join(f"linha {i}" for i in range(1, 60)))
    monkeypatch.setattr(cep, "_find_project_root", lambda _start: root)

    ok, motivo = cep._resolve_code_pointer(root, "rules/cycle-discover.md", 10)
    assert ok, f"pointer do ecossistema deveria resolver em layout de plugin, deu {motivo!r}"


def test_the_same_pointer_resolves_in_the_standalone_layout(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """E o layout do kit continua funcionando — a correção não troca um por outro."""
    root = tmp_path / "kit"
    for d in ("skills", "rules", "hooks"):
        (root / d).mkdir(parents=True)
    (root / "rules" / "cycle-discover.md").write_text("\n".join(f"linha {i}" for i in range(1, 60)))
    monkeypatch.setattr(cep, "_find_project_root", lambda _start: root)

    ok, motivo = cep._resolve_code_pointer(root, "rules/cycle-discover.md", 10)
    assert ok, f"layout standalone deveria continuar resolvendo, deu {motivo!r}"


def test_a_genuinely_absent_file_is_still_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A correção não pode transformar o portão em carimbo.

    Um pointer para arquivo que não existe em NENHUM dos layouts continua `missing_file` — que
    é a única coisa que faz o hard cap valer alguma coisa.
    """
    root = tmp_path / "consumidor"
    (root / ".claude" / "rules").mkdir(parents=True)
    monkeypatch.setattr(cep, "_find_project_root", lambda _start: root)

    ok, motivo = cep._resolve_code_pointer(root, "rules/nao-existe.md", 1)
    assert not ok
    assert motivo == "missing_file"


def test_the_layout_resolution_is_the_shared_one_not_a_local_copy() -> None:
    """Bullet 1 do B-081: UMA implementação, não uma cópia por checador.

    `scripts/ecosystem_utils.py` já declara no próprio docstring que todo script que precisa
    localizar o ecossistema deve importar dali "em vez de duplicar a lógica de detecção". As
    cópias inline eram a violação de um contrato que já existia — e é assim que duas delas
    divergiram: o irmão `check_measurement_targets.py` carregava as duas candidatas para o
    `live-target.txt` e não para os targets do plano, no mesmo arquivo.
    """
    import ecosystem_utils

    assert hasattr(ecosystem_utils, "resolve_ecosystem_path")
    assert cep._resolve_code_pointer.__module__ == "check_evidence_pointers"
    fonte = Path(cep.__file__).read_text(encoding="utf-8")
    assert "resolve_ecosystem_path" in fonte, (
        "o checador deve delegar a resolução ao módulo compartilhado, não reimplementá-la"
    )
