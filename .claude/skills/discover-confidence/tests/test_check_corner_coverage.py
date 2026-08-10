"""Tests for check_corner_coverage.py — verifies the 4-corner check."""
from __future__ import annotations

from pathlib import Path

import pytest

from check_corner_coverage import check_corner_coverage  # noqa: E402


CORNER_BLOCK = (
    "## Corner 1 — Evidence\n\n{evidence}\n\n"
    "## Corner 2 — Constraint Relation\n\n{constraint}\n\n"
    "## Corner 3 — Blast Radius\n\n{blast}\n\n"
    "## Corner 4 — Verification\n\n{verification}\n"
)

LONG = (
    "Substantive content describing this corner with enough text to clear the "
    "MIN_CONTENT_CHARS threshold enforced by the checker. " * 2
)


def _write(tmp_path: Path, name: str, **corners: str) -> Path:
    filled = {k: corners.get(k, LONG) for k in ("evidence", "constraint", "blast", "verification")}
    path = tmp_path / name
    path.write_text("# Opportunity: Test\n\n" + CORNER_BLOCK.format(**filled), encoding="utf-8")
    return path


def test_good_opportunity_all_corners_populated(good_opportunity: Path) -> None:
    report = check_corner_coverage(good_opportunity)
    assert report["corners_populated"] == 4
    assert report["corners_total"] == 4
    assert report["empty_corners"] == []


def test_synthetic_opportunity_minimal_passes(synthetic_opportunity: Path) -> None:
    report = check_corner_coverage(synthetic_opportunity)
    assert report["corners_populated"] == 4
    assert report["empty_corners"] == []


def test_missing_corner_detected(tmp_path: Path) -> None:
    path = tmp_path / "missing-evidence.md"
    path.write_text(
        "# Opportunity: Test\n\n"
        f"## Corner 2 — Constraint Relation\n\n{LONG}\n\n"
        f"## Corner 3 — Blast Radius\n\n{LONG}\n\n"
        f"## Corner 4 — Verification\n\n{LONG}\n",
        encoding="utf-8",
    )
    report = check_corner_coverage(path)
    assert report["corners_populated"] == 3
    assert "evidence" in report["empty_corners"]


def test_placeholder_corner_not_populated(tmp_path: Path) -> None:
    path = _write(tmp_path, "tbd.md", evidence="<!-- TBD: measure it -->")
    report = check_corner_coverage(path)
    assert "evidence" in report["empty_corners"]


def test_unknown_marker_populates_constraint_corner(tmp_path: Path) -> None:
    """`unknown` with a reason is a complete answer for Constraint Relation.

    rules/current-constraint.md declares the constraint a lens, not a gate: with no
    flow instrumentation, demanding a constraint claim would be answered by assertion.
    """
    path = _write(
        tmp_path,
        "unknown-constraint.md",
        constraint="<!-- UNKNOWN: current-constraint.md is status=undeclared -->",
    )
    report = check_corner_coverage(path)
    assert report["corners_populated"] == 4
    assert report["empty_corners"] == []


def test_unknown_marker_does_NOT_populate_evidence_corner(tmp_path: Path) -> None:
    """The escape hatch is scoped to Constraint Relation and must not leak.

    This is the load-bearing test of the whole checker. An opportunity whose Evidence
    corner is `unknown` has measured nothing — the one thing this cycle exists to
    require. If UNKNOWN were honoured globally it would become the cheapest way to
    pass, and every other gate would be decoration.
    """
    path = _write(
        tmp_path,
        "unknown-evidence.md",
        evidence="<!-- UNKNOWN: did not get around to measuring -->",
    )
    report = check_corner_coverage(path)
    assert "evidence" in report["empty_corners"]
    assert report["corners_populated"] == 3


def test_unknown_marker_without_reason_does_not_populate(tmp_path: Path) -> None:
    """A bare marker carries no information and does not count as an answer."""
    path = _write(tmp_path, "bare-unknown.md", constraint="<!-- UNKNOWN: -->")
    report = check_corner_coverage(path)
    assert "constraint" in report["empty_corners"]


def test_deferred_marker_no_longer_populates(tmp_path: Path) -> None:
    """DEFERRED was honoured by the ancestor checker; it is not honoured here.

    Deferring prior-art research was reasonable. Deferring the measurement, the blast
    radius or the verification of a change to a running system is not — each is the
    substance of the opportunity rather than an optional depth of study.
    """
    path = _write(tmp_path, "deferred.md", verification="<!-- DEFERRED: figure it out later -->")
    report = check_corner_coverage(path)
    assert "verification" in report["empty_corners"]
