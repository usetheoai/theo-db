"""Tests for check_phase_completeness."""
from __future__ import annotations

import json
from pathlib import Path


from check_phase_completeness import check_phase_completeness


def _write_progress(tmp_path: Path, tasks: list[dict]) -> Path:
    p = tmp_path / ".progress-foo.json"
    p.write_text(json.dumps({"slug": "foo", "tasks": tasks}), encoding="utf-8")
    return p


def _write_plan(tmp_path: Path, body: str) -> Path:
    p = tmp_path / "foo-plan.md"
    p.write_text(body, encoding="utf-8")
    return p


def test_phase_all_committed_pass(tmp_path: Path) -> None:
    progress = _write_progress(tmp_path, [
        {"id": "T1.1", "phase": "1", "status": "committed"},
        {"id": "T1.2", "phase": "1", "status": "committed"},
        {"id": "T2.1", "phase": "2", "status": "pending"},
    ])
    plan = _write_plan(tmp_path, "## Phase 1\n### T1.1\n### T1.2\n## Phase 2\n### T2.1\n")
    report = check_phase_completeness(plan, progress, "1")
    assert report.total_tasks_in_phase == 2
    assert report.committed_count == 2
    assert report.blocked_count == 0
    assert report.has_high_or_blocker is False


def test_phase_with_blocked_task_high(tmp_path: Path) -> None:
    progress = _write_progress(tmp_path, [
        {"id": "T1.1", "phase": "1", "status": "committed"},
        {"id": "T1.2", "phase": "1", "status": "blocked"},
    ])
    plan = _write_plan(tmp_path, "## Phase 1\n### T1.1\n### T1.2\n")
    report = check_phase_completeness(plan, progress, "1")
    assert report.blocked_count == 1
    assert report.has_high_or_blocker is True
    high_codes = [f.code for f in report.findings if f.severity == "HIGH"]
    assert "phase_has_blocked_tasks" in high_codes


def test_phase_with_pending_task_high(tmp_path: Path) -> None:
    progress = _write_progress(tmp_path, [
        {"id": "T1.1", "phase": "1", "status": "committed"},
        {"id": "T1.2", "phase": "1", "status": "pending"},
    ])
    plan = _write_plan(tmp_path, "## Phase 1\n### T1.1\n### T1.2\n")
    report = check_phase_completeness(plan, progress, "1")
    assert report.has_high_or_blocker is True
    high_codes = [f.code for f in report.findings if f.severity == "HIGH"]
    assert "phase_has_pending_tasks" in high_codes


def test_missing_phase_in_progress(tmp_path: Path) -> None:
    progress = _write_progress(tmp_path, [{"id": "T1.1", "phase": "1", "status": "committed"}])
    plan = _write_plan(tmp_path, "## Phase 1\n### T1.1\n## Phase 99\n")
    report = check_phase_completeness(plan, progress, "99")
    assert report.has_high_or_blocker is True
    codes = [f.code for f in report.findings]
    assert "phase_not_found_in_progress" in codes


def test_phase_dod_declared_and_populated(tmp_path: Path) -> None:
    progress = _write_progress(tmp_path, [{"id": "T1.1", "phase": "1", "status": "committed"}])
    plan_body = (
        "## Phase 1\n"
        "### T1.1\nBuild it.\n"
        "### Phase 1 — Definition of Done\n"
        "- All tasks committed\n"
        "- Integration test green\n"
    )
    plan = _write_plan(tmp_path, plan_body)
    report = check_phase_completeness(plan, progress, "1")
    assert report.phase_dod_present is True
    assert report.phase_dod_lines >= 2
    assert report.has_high_or_blocker is False
    # No MEDIUM "empty" finding
    assert not any(f.code == "phase_dod_empty" for f in report.findings)


def test_phase_dod_declared_but_empty(tmp_path: Path) -> None:
    progress = _write_progress(tmp_path, [{"id": "T1.1", "phase": "1", "status": "committed"}])
    plan_body = (
        "## Phase 1\n"
        "### T1.1\nBuild it.\n"
        "### Phase 1 — Definition of Done\n\n"
        "## Phase 2\n### T2.1\n"
    )
    plan = _write_plan(tmp_path, plan_body)
    report = check_phase_completeness(plan, progress, "1")
    assert report.phase_dod_present is True
    assert report.phase_dod_lines == 0
    medium_codes = [f.code for f in report.findings if f.severity == "MEDIUM"]
    assert "phase_dod_empty" in medium_codes
    # MEDIUM does not trigger HIGH/BLOCKER gate
    assert report.has_high_or_blocker is False


def test_phase_dod_absent_only_info(tmp_path: Path) -> None:
    progress = _write_progress(tmp_path, [{"id": "T1.1", "phase": "1", "status": "committed"}])
    plan = _write_plan(tmp_path, "## Phase 1\n### T1.1\nBuild it.\n")
    report = check_phase_completeness(plan, progress, "1")
    assert report.phase_dod_present is False
    info_codes = [f.code for f in report.findings if f.severity == "INFO"]
    assert "phase_dod_absent" in info_codes
    assert report.has_high_or_blocker is False


def test_a_plan_task_absent_from_the_checkpoint_is_caught(tmp_path: Path) -> None:
    """The phase's inventory comes from the PLAN, not from the checkpoint.

    Reading it from the checkpoint made the checkpoint judge itself: a task declared in the
    plan and never written there was not `pending`, it did not exist. Measured before the fix:
    T1.1/T1.2/T1.3 with T1.3 absent reported `total_tasks_in_phase: 2` and exit 0, while the
    same task recorded `pending` was caught HIGH.
    """
    plan = tmp_path / "plan.md"
    plan.write_text(
        "## Phase 1 — core\n\n"
        "### T1.1 — first\n### T1.2 — second\n### T1.3 — the skipped one\n\n"
        "### Phase 1 — Definition of Done\n- [x] all three covered\n",
        encoding="utf-8",
    )
    progress = tmp_path / ".progress-x.json"
    progress.write_text(
        json.dumps({"tasks": [
            {"id": "T1.1", "phase": 1, "status": "committed", "commit_sha": "a" * 7},
            {"id": "T1.2", "phase": 1, "status": "committed", "commit_sha": "b" * 7},
        ]}),
        encoding="utf-8",
    )

    report = check_phase_completeness(plan, progress, "1")
    codes = [f.code for f in report.findings]
    assert "plan_task_absent_from_progress" in codes
    assert report.has_high_or_blocker
    assert "T1.3" in next(f for f in report.findings if f.code == "plan_task_absent_from_progress").message


def test_a_phase_whose_plan_tasks_are_all_recorded_still_passes(tmp_path: Path) -> None:
    """The new check must not fire when the checkpoint accounts for every declared task."""
    plan = tmp_path / "plan.md"
    plan.write_text(
        "## Phase 1 — core\n\n### T1.1 — first\n### T1.2 — second\n\n"
        "### Phase 1 — Definition of Done\n- [x] both covered\n",
        encoding="utf-8",
    )
    progress = tmp_path / ".progress-x.json"
    progress.write_text(
        json.dumps({"tasks": [
            {"id": "T1.1", "phase": 1, "status": "committed", "commit_sha": "a" * 7},
            {"id": "T1.2", "phase": 1, "status": "committed", "commit_sha": "b" * 7},
        ]}),
        encoding="utf-8",
    )
    assert not check_phase_completeness(plan, progress, "1").has_high_or_blocker


def test_tasks_of_a_later_phase_do_not_fail_this_boundary(tmp_path: Path) -> None:
    """A phase-1 boundary must not fail over phase-2 work that has not started.

    The scoping is the `T{phase}.{n}` id convention; without it the check would make every
    boundary red until the very last task landed, which is the fastest way to get a gate
    disabled.
    """
    plan = tmp_path / "plan.md"
    plan.write_text(
        "## Phase 1 — core\n\n### T1.1 — first\n\n"
        "### Phase 1 — Definition of Done\n- [x] covered\n\n"
        "## Phase 2 — later\n\n### T2.1 — not started\n",
        encoding="utf-8",
    )
    progress = tmp_path / ".progress-x.json"
    progress.write_text(
        json.dumps({"tasks": [
            {"id": "T1.1", "phase": 1, "status": "committed", "commit_sha": "a" * 7},
        ]}),
        encoding="utf-8",
    )
    assert not check_phase_completeness(plan, progress, "1").has_high_or_blocker
