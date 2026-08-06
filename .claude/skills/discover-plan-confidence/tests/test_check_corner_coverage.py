"""Tests for check_corner_coverage.py (plan variant) — question-to-corner mapping."""
from __future__ import annotations

from pathlib import Path

import pytest

from check_corner_coverage import CORNERS, check_corner_coverage, _has_defer_corner_marker


def test_good_plan_covers_all_corners(good_measurement_plan: Path) -> None:
    report = check_corner_coverage(good_measurement_plan)
    assert report["corners_populated"] == 4
    assert report["empty_corners"] == []


def test_synthetic_plan_covers_all_corners(synthetic_measurement_plan: Path) -> None:
    report = check_corner_coverage(synthetic_measurement_plan)
    assert report["corners_populated"] == 4
    assert report["empty_corners"] == []


def test_missing_corner_detected(missing_corner_measurement_plan: Path) -> None:
    report = check_corner_coverage(missing_corner_measurement_plan)
    assert "verification" in report["empty_corners"]
    assert report["corners_populated"] == 3


def test_defer_marker_covers_a_corner(good_measurement_plan: Path) -> None:
    """The good fixture defers `constraint` — the corner counts as covered.

    Deferring the constraint corner is the expected path, not an exception: the
    constraint is a lens rather than a gate, and a plan that invented a constraint
    question to fill the slot would be padding.
    """
    report = check_corner_coverage(good_measurement_plan)
    constraint = next(c for c in report["corners_status"] if c["corner"] == "constraint")
    assert constraint["deferred"] is True
    assert constraint["questions"] == 0
    assert constraint["covered"] is True


def test_defer_marker_is_corner_specific(tmp_path: Path) -> None:
    """A marker for one corner must not silently cover another."""
    plan = tmp_path / "p.md"
    plan.write_text(
        "# Measurement Plan: X\n\n<!-- DEFER-CORNER: constraint | not measurable yet -->\n",
        encoding="utf-8",
    )
    content = plan.read_text(encoding="utf-8")
    assert _has_defer_corner_marker(content, "constraint") is True
    assert _has_defer_corner_marker(content, "evidence") is False
    assert _has_defer_corner_marker(content, "blast_radius") is False


def test_corners_are_the_maintenance_four(good_measurement_plan: Path) -> None:
    """Guards the rename: the prior-art corners must not come back by accident."""
    assert CORNERS == ("evidence", "constraint", "blast_radius", "verification")
    report = check_corner_coverage(good_measurement_plan)
    assert [c["corner"] for c in report["corners_status"]] == list(CORNERS)


def test_plan_without_questions_section_covers_nothing(tmp_path: Path) -> None:
    plan = tmp_path / "empty.md"
    plan.write_text("# Measurement Plan: X\n\n## Context\n\nNothing here.\n", encoding="utf-8")
    report = check_corner_coverage(plan)
    assert report["corners_populated"] == 0
    assert sorted(report["empty_corners"]) == sorted(CORNERS)
