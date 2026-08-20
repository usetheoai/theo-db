"""Tests for the phase-review gate.

`rules/cycle-implement.md` calls skipping the Step 4.7 mini review a documented
anti-pattern, and `SKILL.md` says the skill NEVER skips it — but nothing
downstream ever checked. The mini review is invoked by prose, writes a report
nobody looks for, and a run that silently skipped every phase boundary was
indistinguishable from one that reviewed all of them.
"""
from __future__ import annotations

import json
from pathlib import Path

from check_phase_review import check_phase_review  # noqa: E402 — conftest puts scripts/ on path


PLAN = """# Plan

## Phase 1 — foundation

### T1.1 — first
#### TDD
assert add(1, 2) == 3

### T1.2 — second
#### TDD
assert sub(2, 1) == 1

## Phase 2 — wiring

### T2.1 — third
#### TDD
assert wire() == "ok"
"""


def _plan(tmp_path: Path) -> Path:
    path = tmp_path / "demo-plan.md"
    path.write_text(PLAN, encoding="utf-8")
    return path


def _progress(committed: list[str]) -> dict:
    return {
        "slug": "demo",
        "tasks": [
            {"id": tid, "phase": tid[1], "status": "committed", "commit_sha": "abc"}
            for tid in committed
        ],
    }


def test_closed_phase_without_a_mini_review_report_fails(tmp_path: Path) -> None:
    """Phase 1 is fully committed and no report exists — the boundary was skipped."""
    reviews = tmp_path / "mini-reviews"
    reviews.mkdir()
    report = check_phase_review(_plan(tmp_path), _progress(["T1.1", "T1.2"]), "demo", [reviews])
    assert report.status == "FAIL"
    assert report.phases_closed == ["1"]
    assert report.phases_reviewed == []
    assert any(f.code == "phase_review_missing" for f in report.findings)


def test_closed_phase_with_its_report_passes(tmp_path: Path) -> None:
    reviews = tmp_path / "mini-reviews"
    reviews.mkdir()
    (reviews / "demo-phase1-review-2026-08-18.md").write_text("PHASE_REVIEW_PASS", encoding="utf-8")
    report = check_phase_review(_plan(tmp_path), _progress(["T1.1", "T1.2"]), "demo", [reviews])
    assert report.status == "PASS"
    assert report.phases_reviewed == ["1"]


def test_partially_committed_phase_is_not_yet_a_boundary(tmp_path: Path) -> None:
    """A phase still in flight has no boundary to review — no finding."""
    reviews = tmp_path / "mini-reviews"
    reviews.mkdir()
    report = check_phase_review(_plan(tmp_path), _progress(["T1.1"]), "demo", [reviews])
    assert report.status == "PASS"
    assert report.phases_closed == []


def test_plan_without_phases_skips(tmp_path: Path) -> None:
    path = tmp_path / "flat-plan.md"
    path.write_text("# Plan\n\n### T1.1 — only\n#### TDD\nassert x == 1\n", encoding="utf-8")
    report = check_phase_review(path, _progress(["T1.1"]), "flat", [tmp_path])
    assert report.status == "SKIP"


def test_report_for_another_slug_does_not_count(tmp_path: Path) -> None:
    """A neighbouring plan's report must not launder this plan's skipped boundary."""
    reviews = tmp_path / "mini-reviews"
    reviews.mkdir()
    (reviews / "other-phase1-review-2026-08-18.md").write_text("x", encoding="utf-8")
    report = check_phase_review(_plan(tmp_path), _progress(["T1.1", "T1.2"]), "demo", [reviews])
    assert report.status == "FAIL"


def test_progress_as_json_string_is_tolerated(tmp_path: Path) -> None:
    """The caller may hand over the raw checkpoint; malformed input must not crash."""
    reviews = tmp_path / "mini-reviews"
    reviews.mkdir()
    report = check_phase_review(_plan(tmp_path), json.loads("{}"), "demo", [reviews])
    assert report.status == "PASS"
    assert report.phases_closed == []
