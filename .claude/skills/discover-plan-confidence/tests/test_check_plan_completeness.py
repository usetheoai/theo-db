"""Tests for check_plan_completeness.py — sections, budget, method, falsification."""
from __future__ import annotations

from pathlib import Path

import pytest

from check_plan_completeness import MIN_QUESTIONS, check_plan_completeness


def test_good_plan_is_complete(good_measurement_plan: Path) -> None:
    report = check_plan_completeness(good_measurement_plan)
    assert report["missing_mandatory"] == []
    assert report["budget_violations"] == []
    assert report["methodless_questions"] == []
    assert report["falsification_missing"] is False


def test_synthetic_plan_is_complete(synthetic_measurement_plan: Path) -> None:
    report = check_plan_completeness(synthetic_measurement_plan)
    assert report["missing_mandatory"] == []
    assert report["falsification_missing"] is False


def test_missing_section_detected(tmp_path: Path) -> None:
    plan = tmp_path / "missing.md"
    plan.write_text(
        "# Measurement Plan: X\n\n**Item:** B-001\n**Repo:** squad\n**Mode:** review\n\n"
        "## Context\n\nText.\n",
        encoding="utf-8",
    )
    report = check_plan_completeness(plan)
    assert "Hypothesis" in report["missing_mandatory"]
    assert "Falsification" in report["missing_mandatory"]
    assert "Measurement Questions" in report["missing_mandatory"]


def test_invalid_mode_is_a_missing_section(synthetic_measurement_plan: Path) -> None:
    content = synthetic_measurement_plan.read_text(encoding="utf-8")
    synthetic_measurement_plan.write_text(
        content.replace("**Mode:** review", "**Mode:** whatever"), encoding="utf-8"
    )
    report = check_plan_completeness(synthetic_measurement_plan)
    assert "Mode" in report["missing_mandatory"]


def test_empty_falsification_is_flagged(fixtures_dir: Path) -> None:
    """The check that replaced the >=2 ADR requirement.

    An ADR in a measurement plan is premature — nothing has been measured, so there is
    nothing to decide. What makes the plan honest is stating in advance what result would
    kill the hypothesis; without it, any observation can be reinterpreted afterwards as
    confirming what was already believed, and the measurement cannot fail.
    """
    report = check_plan_completeness(fixtures_dir / "no-falsification-measurement-plan.md")
    assert report["falsification_missing"] is True


def test_placeholder_falsification_does_not_count(
    tmp_path: Path, synthetic_measurement_plan: Path
) -> None:
    content = synthetic_measurement_plan.read_text(encoding="utf-8")
    stubbed = content.replace(
        "If every branch is present and refuses, the hypothesis is dead "
        "and the item is killed with that result recorded.",
        "<!-- TBD: decide later -->",
    )
    plan = tmp_path / "stub.md"
    plan.write_text(stubbed, encoding="utf-8")
    report = check_plan_completeness(plan)
    assert report["falsification_missing"] is True


def test_under_budget_detected(fixtures_dir: Path) -> None:
    report = check_plan_completeness(fixtures_dir / "under-budget-measurement-plan.md")
    assert any("too_few_questions" in v for v in report["budget_violations"])
    assert report["question_count"] < MIN_QUESTIONS


def test_question_floor_is_three_not_five(good_measurement_plan: Path) -> None:
    """Recalibrated for maintenance work.

    Five was sized for a prior-art survey across several projects. A maintenance item is
    smaller, and a floor that forces padding produces questions written to satisfy a
    counter rather than to measure anything.
    """
    assert MIN_QUESTIONS == 3
    report = check_plan_completeness(good_measurement_plan)
    assert report["question_count"] == 4
    assert report["budget_violations"] == []


def test_methodless_question_detected(fixtures_dir: Path) -> None:
    report = check_plan_completeness(fixtures_dir / "method-missing-measurement-plan.md")
    assert "Q2" in report["methodless_questions"]


def test_missing_method_headers_reported(tmp_path: Path) -> None:
    """Renaming the columns must not silently pass every question as fine."""
    plan = tmp_path / "no-headers.md"
    plan.write_text(
        "# Measurement Plan: X\n\n## Measurement Questions\n\n"
        "| # | Question | Corner | Thing | Place |\n"
        "|---|---|---|---|---|\n"
        "| Q1 | Something? | evidence | Read | somewhere |\n",
        encoding="utf-8",
    )
    report = check_plan_completeness(plan)
    assert report["methodless_questions"] == ["__header_not_found__"]


def test_target_without_tool_is_methodless(tmp_path: Path) -> None:
    """A target with no tool is a place nobody said how to look at."""
    plan = tmp_path / "half.md"
    plan.write_text(
        "# Measurement Plan: X\n\n## Measurement Questions\n\n"
        "| # | Question | Corner | Tool | Target |\n"
        "|---|---|---|---|---|\n"
        "| Q1 | Something? | evidence |  | somewhere |\n",
        encoding="utf-8",
    )
    report = check_plan_completeness(plan)
    assert report["methodless_questions"] == ["Q1"]
