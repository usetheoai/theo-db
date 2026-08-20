"""T4.1 — check_coverage_matrix.py tests (v1.1 EC-4 fix: orphan exclusion)."""
from __future__ import annotations

from pathlib import Path

import pytest


from check_coverage_matrix import (  # noqa: E402
    CoverageReport,
    check_coverage_matrix,
)

FIXTURES = Path(__file__).parent.parent / "fixtures"


def test_coverage_matrix_full_returns_1_0() -> None:
    report = check_coverage_matrix(FIXTURES / "good-plan.md")
    assert isinstance(report, CoverageReport)
    assert report.coverage_ratio == 1.0
    assert report.is_complete is True
    assert report.total_gaps == 3
    assert report.mapped_gaps == 3


def test_coverage_matrix_partial(tmp_path: Path) -> None:
    plan = tmp_path / "partial-plan.md"
    plan.write_text(
        "# Plan: Partial\n\n"
        "## ADRs\n### D1 — toy\n\n## Phase 1\n### T1.1 — Title\n### T1.2 — Title\n\n"
        "## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | gap a | T1.1 | done |\n"
        "| 2 | gap b | T1.2 | done |\n"
        "| 3 | gap c |  | NOT MAPPED |\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.total_gaps == 3
    assert report.mapped_gaps == 2
    assert abs(report.coverage_ratio - 2 / 3) < 1e-9
    assert report.is_complete is False


def test_coverage_matrix_missing_coverage_fixture() -> None:
    report = check_coverage_matrix(FIXTURES / "missing-coverage-plan.md")
    assert report.is_complete is False
    assert report.coverage_ratio < 1.0


def test_coverage_matrix_empty_table_no_orphans(tmp_path: Path) -> None:
    plan = tmp_path / "empty.md"
    plan.write_text(
        "# Plan\n\n## ADRs\n### D1\n\n## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.total_gaps == 0
    # No orphans -> coverage 1.0 (semantic decision per plan algorithm).
    assert report.coverage_ratio == 1.0
    assert report.is_complete is True


def test_coverage_matrix_no_section_reports_instead_of_raising(tmp_path: Path) -> None:
    """Este teste PINAVA a excecao como se fosse contrato, e era o que impedia o conserto.

    A ausencia da secao e a condicao exata que o `plan-confidence-golden-rule.md` cobre com o
    hard cap `coverage_lt_100` — ou seja, e um plano INVALIDO, resultado normal do portao, nao
    uma falha da ferramenta. Pinar a excecao transformava um veredito previsto em crash, e quem
    chama passava a nao distinguir "o plano esta errado" de "a skill quebrou" (B-081).

    Um `FileNotFoundError` continua sendo excecao, e a diferenca e o ponto: um plano que nao
    existe nao pode ser reprovado, so relatado como ausente.
    """
    plan = tmp_path / "no-section.md"
    plan.write_text("# Plan\n\nJust prose, no coverage section.\n", encoding="utf-8")
    report = check_coverage_matrix(plan)
    assert report.is_complete is False
    assert any("Coverage Matrix" in e for e in report.parse_errors)


def test_coverage_matrix_file_not_found() -> None:
    with pytest.raises(FileNotFoundError):
        check_coverage_matrix(Path("/tmp/__definitely_not_a_plan__.md"))


def test_coverage_matrix_task_header_not_counted_as_orphan(tmp_path: Path) -> None:
    """v1.1 EC-4 fix: '### T1.1 — Title' is a definition, not a reference."""
    plan = tmp_path / "headers.md"
    plan.write_text(
        "# Plan\n\n"
        "## Phase 1\n\n"
        "### T1.1 — First task\nbody\n\n"
        "### T1.2 — Second task\nbody\n\n"
        "## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | gap a | T1.1 | done |\n"
        "| 2 | gap b | T1.2 | done |\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    # T1.1 and T1.2 appear ONLY as definitions in body, no inline references.
    # Both are in the matrix, so they're NOT orphan. Without EC-4 fix, they'd be marked orphan.
    assert list(report.orphan_tasks) == []
    assert report.is_complete is True


def test_coverage_matrix_orphan_task_detection(tmp_path: Path) -> None:
    plan = tmp_path / "orphan.md"
    plan.write_text(
        "# Plan\n\n"
        "## Phase 1\n\n"
        "### T1.1 — Task\nThis depends on T2.5 indirectly.\n\n"
        "## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | gap a | T1.1 | done |\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    # T2.5 mentioned inline in body but NOT in matrix -> orphan.
    # T1.1 is a header AND in matrix -> not orphan.
    assert "T2.5" in report.orphan_tasks
    assert "T1.1" not in report.orphan_tasks


def test_coverage_matrix_multi_task_per_row(tmp_path: Path) -> None:
    plan = tmp_path / "multi.md"
    plan.write_text(
        "# Plan\n\n## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | gap a | T1.1, T1.2 | done |\n"
        "| 2 | gap b | T2.1 | done |\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.total_gaps == 2
    assert report.mapped_gaps == 2  # both rows have task refs


def test_coverage_matrix_utf8_bom(tmp_path: Path) -> None:
    """v1.1 EC-8 fix: handle UTF-8 BOM (use encoding='utf-8-sig' or strip BOM)."""
    plan = tmp_path / "bom-plan.md"
    # Write with BOM
    content = (
        "# Plan\n\n## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | gap a | T1.1 | done |\n"
    )
    plan.write_bytes(b"\xef\xbb\xbf" + content.encode("utf-8"))
    report = check_coverage_matrix(plan)
    assert report.total_gaps == 1
    assert report.is_complete is True


def test_coverage_matrix_is_deterministic(tmp_path: Path) -> None:
    """Hashable input -> same output. Run twice, expect identical result."""
    plan = FIXTURES / "good-plan.md"
    r1 = check_coverage_matrix(plan)
    r2 = check_coverage_matrix(plan)
    assert r1 == r2


# v1.1+ Out-of-scope detection (#2 fix)

def test_coverage_matrix_out_of_scope_not_counted_as_unmapped(tmp_path: Path) -> None:
    """'N/A — D9 out-of-scope' is deliberately deferred, not missed."""
    plan = tmp_path / "deferred.md"
    plan.write_text(
        "# Plan\n\n## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | gap a | T1.1 | done |\n"
        "| 2 | gap b | N/A — D9 out-of-scope | deferred to follow-up |\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.deferred_gaps == 1
    assert report.mapped_gaps == 1
    # is_complete: mapped + deferred == total
    assert report.is_complete is True
    assert report.coverage_ratio == 1.0


def test_coverage_matrix_out_of_scope_variant_phrases(tmp_path: Path) -> None:
    """Multiple ways to mark out-of-scope."""
    plan = tmp_path / "variants.md"
    plan.write_text(
        "# Plan\n\n## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | a | (out-of-scope D5) | deferred |\n"
        "| 2 | b | out of scope | deferred |\n"
        "| 3 | c | OUT-OF-SCOPE | deferred |\n"
        "| 4 | d | DEFERRED to v2 | later |\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.deferred_gaps == 4
    assert report.mapped_gaps == 0
    assert report.is_complete is True


def test_coverage_matrix_real_plan_example() -> None:
    """End-to-end: a real project plan should pass coverage-matrix completeness.

    Gracefully skips when no real plan file is available in the host project."""
    project_root = Path(__file__).parent.parent.parent.parent.parent  # repo root
    plans_dir = project_root / "knowledge-base" / "plans"
    if not plans_dir.is_dir():
        pytest.skip(f"plans dir not found: {plans_dir}")
    candidates = list(plans_dir.glob("*-plan.md"))
    if not candidates:
        pytest.skip(f"no real plans under {plans_dir}")
    plan_path = candidates[0]
    report = check_coverage_matrix(plan_path)
    # F-CODE-01 is explicitly out-of-scope via D9 — should be deferred, not unmapped.
    assert report.deferred_gaps >= 1
    assert report.is_complete is True, (
        f"theo-cli plan should be complete after out-of-scope fix; "
        f"got mapped={report.mapped_gaps}, deferred={report.deferred_gaps}, total={report.total_gaps}"
    )


def test_coverage_matrix_truly_unmapped_still_fails(tmp_path: Path) -> None:
    """Empty task column (no out-of-scope mark) is STILL unmapped failure."""
    plan = tmp_path / "unmapped.md"
    plan.write_text(
        "# Plan\n\n## Coverage Matrix\n\n"
        "| # | Gap | Task(s) | Resolution |\n"
        "|---|-----|---------|------------|\n"
        "| 1 | a | T1.1 | done |\n"
        "| 2 | b |  | forgot to map |\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.deferred_gaps == 0
    assert report.mapped_gaps == 1
    assert report.is_complete is False


# ---------------------------------------------------------------------------
# B-081 — um portao que QUEBRA nao e um portao que reprova.
# ---------------------------------------------------------------------------


def test_a_plan_without_a_coverage_matrix_is_invalid_not_a_crash(tmp_path: Path) -> None:
    """Medido em 2026-08-20: `run_structural` levantava
    `ValueError: No '## Coverage Matrix' section found in plan` para
    `b015-b018-b019-plan.md`, em vez de emitir o hard cap que o
    `plan-confidence-golden-rule.md` define para EXATAMENTE essa condicao.

    Quem chama nao consegue distinguir "plano invalido" de "ferramenta quebrada", e as duas
    coisas exigem acoes opostas — a primeira e para o autor do plano consertar, a segunda para
    o dono da skill. `rules/error-handling.md` § 2: erro explicito e tipado, nunca excecao crua
    atravessando a fronteira.

    O contrato ja existia no proprio dataclass: `is_complete` e `parse_errors` estao la desde
    sempre. Faltava usa-los.
    """
    plan = tmp_path / "sem-matriz-plan.md"
    plan.write_text(
        "---\ntype: plan\nslug: sem-matriz\n---\n\n## Goal\n\nFazer algo.\n\n"
        "## Tasks\n\n### T1.1 — passo\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.is_complete is False
    assert report.coverage_ratio == 0.0
    assert any("Coverage Matrix" in e for e in report.parse_errors), (
        f"o motivo deve estar legivel em parse_errors, veio {report.parse_errors!r}"
    )


def test_a_plan_with_a_coverage_matrix_is_unaffected(tmp_path: Path) -> None:
    """A correcao nao pode transformar o portao em carimbo."""
    plan = tmp_path / "com-matriz-plan.md"
    plan.write_text(
        "---\ntype: plan\nslug: com-matriz\n---\n\n## Coverage Matrix\n\n"
        "| Claim | Where | Task |\n|---|---|---|\n| faz X | Goal | T1.1 |\n\n"
        "## Tasks\n\n### T1.1 — passo\n",
        encoding="utf-8",
    )
    report = check_coverage_matrix(plan)
    assert report.parse_errors == ()
