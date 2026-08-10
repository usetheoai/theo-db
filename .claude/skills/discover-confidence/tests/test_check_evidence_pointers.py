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
