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


# B-039 — three mini reviews were written AFTER the commits they claim to gate, and nothing could
# tell. Measured 2026-08-18 by mtime against `git log`:
#
#     last commit of the chain   14:30
#     b020 phase-1 review        14:54
#     b033 phase-1 review        14:54     <- 41 minutes after phase 2 was already committed
#     b033 phase-2 review        14:54
#     b034 phase-1 review        14:54
#
# `cycle-implement.md` says the mini review runs BEFORE the halt-loop accepts the next task. The
# contrast proves the ordering is observable when real: b025's three carry 12:32/12:44/12:48,
# interleaved with its commits.
#
# The signal is the git HEAD, not a timestamp. A timestamp is whatever the writer says it is; a sha
# is checkable against the repository, and `git merge-base --is-ancestor` decides whether the review
# ran at or before the phase closed.

import subprocess
import sys

_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@t", "PATH": "/usr/bin:/bin"}


def _repo_with_two_commits(tmp_path: Path) -> tuple[Path, str, str]:
    """A repo whose phase closes at commit A, and which then moves on to commit B."""
    root = tmp_path / "repo"
    root.mkdir()
    env = {**_ENV, "HOME": str(root)}
    run = lambda *a: subprocess.run(["git", "-C", str(root), *a], check=True,
                                    capture_output=True, text=True, env=env)
    run("init", "-q")
    (root / "a.txt").write_text("one\n", encoding="utf-8")
    run("add", "-A"); run("-c", "commit.gpgsign=false", "commit", "-q", "-m", "phase 1 last")
    first = run("rev-parse", "HEAD").stdout.strip()
    (root / "b.txt").write_text("two\n", encoding="utf-8")
    run("add", "-A"); run("-c", "commit.gpgsign=false", "commit", "-q", "-m", "later work")
    second = run("rev-parse", "HEAD").stdout.strip()
    return root, first, second


def _report(root: Path, slug: str, phase: str, head: str | None) -> Path:
    d = root / "reviews"
    d.mkdir(exist_ok=True)
    p = d / f"{slug}-phase{phase}-review-2026-08-19.md"
    body = f"# Mini review — {slug} — Phase {phase}\n\n**Verdict:** `PHASE_REVIEW_PASS`\n"
    if head is not None:
        body += f"\n**Reviewed at head:** `{head}`\n"
    p.write_text(body, encoding="utf-8")
    return d


def _progress_at(sha: str) -> dict:
    return {"slug": "s", "tasks": [
        {"id": "T1.1", "phase": "1", "status": "committed", "commit_sha": sha},
        {"id": "T1.2", "phase": "1", "status": "committed", "commit_sha": sha},
    ]}


PLAN_ONE_PHASE = "# Plan\n\n## Phase 1 — foundation\n\n### T1.1 — first\n#### TDD\nRED\n\n### T1.2 — second\n#### TDD\nRED\n"


def test_a_report_recorded_after_the_phase_closed_fails(tmp_path: Path) -> None:
    root, first, second = _repo_with_two_commits(tmp_path)
    plan = root / "plan.md"; plan.write_text(PLAN_ONE_PHASE, encoding="utf-8")
    reviews = _report(root, "s", "1", second)   # recorded AFTER the phase's last commit

    report = check_phase_review(plan, _progress_at(first), "s", [reviews], repo_root=root)

    assert any(f.code == "retroactive_review" for f in report.findings), report.findings
    assert report.status == "FAIL"


def test_a_report_recorded_at_the_phase_close_passes(tmp_path: Path) -> None:
    root, first, _second = _repo_with_two_commits(tmp_path)
    plan = root / "plan.md"; plan.write_text(PLAN_ONE_PHASE, encoding="utf-8")
    reviews = _report(root, "s", "1", first)

    report = check_phase_review(plan, _progress_at(first), "s", [reviews], repo_root=root)

    assert not any(f.code == "retroactive_review" for f in report.findings)


def test_a_report_recorded_mid_phase_passes(tmp_path: Path) -> None:
    # An ANCESTOR is early, not late. Failing it would push people to re-run reviews for no reason.
    root, first, second = _repo_with_two_commits(tmp_path)
    plan = root / "plan.md"; plan.write_text(PLAN_ONE_PHASE, encoding="utf-8")
    reviews = _report(root, "s", "1", first)

    report = check_phase_review(plan, _progress_at(second), "s", [reviews], repo_root=root)

    assert not any(f.code == "retroactive_review" for f in report.findings)


def test_a_report_without_a_recorded_head_is_info(tmp_path: Path) -> None:
    # Every report written before this change lacks the field. Failing them would turn the whole
    # existing audit trail red in one step, which is how a gate gets disabled.
    root, first, _second = _repo_with_two_commits(tmp_path)
    plan = root / "plan.md"; plan.write_text(PLAN_ONE_PHASE, encoding="utf-8")
    reviews = _report(root, "s", "1", None)

    report = check_phase_review(plan, _progress_at(first), "s", [reviews], repo_root=root)

    finding = next(f for f in report.findings if f.code == "no_recorded_head")
    assert finding.severity == "INFO"
    assert report.status != "FAIL"
