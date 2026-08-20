#!/usr/bin/env python3
"""Phase-review gate — was the Step 4.7 mini review actually run?

WHY this gate exists
--------------------
`rules/cycle-implement.md` calls skipping the phase-boundary mini review a
documented anti-pattern, and `skills/implement/SKILL.md` says the skill NEVER
skips it and NEVER emits `PHASE_REVIEW_PASS` without `mini_review.py` having
written the report. Both sentences addressed the agent; neither was checked by
anything. `mini_review.py` is invoked from prose, writes a report into
`knowledge-base/mini-reviews/`, and nobody downstream ever looked for it — so a
run that skipped every boundary was indistinguishable from one that reviewed
them all.

That is the same shape the kit already fixed for wiring: the final gate does not
trust the self-report, it re-checks the artifact. This does the cheap half of
that — it verifies the review happened at each boundary the plan actually closed.

Honest scope: this asserts the report EXISTS for every closed phase. It does not
re-run the mini review's four checks; `run_validation` re-runs their aggregate
over the whole implementation anyway.

Usage:
    python3 check_phase_review.py --plan PLAN --progress PROGRESS --slug SLUG

Exit codes:
    0 — every closed phase has its mini-review report (or the plan has no phases)
    1 — at least one closed phase has no report
    2 — file not found / parse error
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from check_phase_completeness import PHASE_HEADER_RE, _plan_task_ids_in_phase

from _layout import default_mini_reviews_dir


@dataclass(frozen=True)
class Finding:
    severity: str
    code: str
    message: str


@dataclass(frozen=True)
class PhaseReviewReport:
    status: str
    phases_declared: list[str] = field(default_factory=list)
    phases_closed: list[str] = field(default_factory=list)
    phases_reviewed: list[str] = field(default_factory=list)
    findings: list[Finding] = field(default_factory=list)


def _declared_phases(plan_path: Path) -> list[str]:
    content = plan_path.read_text(encoding="utf-8-sig")
    return [match.group(1) for match in PHASE_HEADER_RE.finditer(content)]


def _committed_ids(progress: Any) -> set[str]:
    if not isinstance(progress, dict):
        return set()
    return {
        task.get("id")
        for task in progress.get("tasks", [])
        if isinstance(task, dict) and task.get("status") == "committed"
    }


def _find_report(review_dirs: list[Path], slug: str, phase: str) -> Path | None:
    """The report `mini_review.py` writes: `{slug}-phase{N}-review-{date}.md`."""
    pattern = f"{slug}-phase{phase}-review-*.md"
    for directory in review_dirs:
        if not directory.exists():
            continue
        for match in sorted(directory.glob(pattern)):
            return match
    return None


def _has_report(review_dirs: list[Path], slug: str, phase: str) -> bool:
    return _find_report(review_dirs, slug, phase) is not None


_HEAD_RE = re.compile(r"Reviewed at head:\*{0,2}\s*`?([0-9a-f]{7,40})`?", re.IGNORECASE)


def _recorded_head(report: Path) -> str | None:
    """The git HEAD the review ran against, as the report itself recorded it.

    B-039 — a TIMESTAMP is whatever the writer says it is. Three reports in this repository were
    written 41 minutes after the phase they grade had already closed, and their recorded date said
    nothing about it. A sha is checkable against the repository instead of against a clock.
    """
    try:
        match = _HEAD_RE.search(report.read_text(encoding="utf-8-sig"))
    except OSError:
        return None
    return match.group(1) if match else None


def _phase_last_commit(progress: Any, phase: str) -> str | None:
    """The last commit of the phase, from the checkpoint the halt-loop writes."""
    if not isinstance(progress, dict):
        return None
    shas = [
        task.get("commit_sha") for task in progress.get("tasks", [])
        if isinstance(task, dict) and str(task.get("phase")) == str(phase)
        and isinstance(task.get("commit_sha"), str)
    ]
    return shas[-1] if shas else None


def _is_ancestor(repo_root: Path, ancestor: str, descendant: str) -> bool | None:
    """True/False, or None when git cannot decide — never guessed either way."""
    try:
        proc = subprocess.run(
            ["git", "-C", str(repo_root), "merge-base", "--is-ancestor", ancestor, descendant],
            capture_output=True, text=True, check=False, timeout=30,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if proc.returncode == 0:
        return True
    if proc.returncode == 1:
        return False
    return None


def _check_ordering(
    report: Path,
    progress: Any,
    phase: str,
    repo_root: Path | None,
) -> list[Finding]:
    """Did this review run at or before the phase closed?

    PASS when the recorded head IS the phase's last commit, or an ANCESTOR of it — a review running
    mid-phase is early, not late, and failing it would push people to re-run reviews for no reason.

    FAIL only when the phase's last commit is an ancestor of the recorded head: later commits had
    already landed, so the review graded a tree the phase had moved past. That is the measured case.
    """
    head = _recorded_head(report)
    if head is None:
        # Every report written before B-039 lacks the field. Failing them would turn the whole
        # existing audit trail red in one step, which is how a gate gets disabled. The three known
        # retroactive ones are annotated by hand instead. Backfilling a sha to make a record pass
        # would be manufacturing exactly the evidence this check exists to detect.
        return [Finding("INFO", "no_recorded_head",
                        f"phase {phase}'s report records no `Reviewed at head` — written before "
                        "B-039; its ordering cannot be checked")]
    last = _phase_last_commit(progress, phase)
    if last is None or repo_root is None:
        return [Finding("INFO", "ordering_not_checkable",
                        f"phase {phase}: no commit sha in the checkpoint, or no repo root")]
    verdict = _is_ancestor(repo_root, last, head)
    if verdict is None:
        return [Finding("INFO", "ordering_not_checkable",
                        f"phase {phase}: git could not compare {last[:9]} and {head[:9]}")]
    if verdict and last != head:
        return [Finding("HIGH", "retroactive_review",
                        f"phase {phase}'s mini review recorded head {head[:9]}, a DESCENDANT of the "
                        f"phase's last commit {last[:9]} — it ran after the phase closed and "
                        "therefore gated nothing")]
    return []


def check_phase_review(
    plan_path: Path,
    progress: Any,
    slug: str,
    review_dirs: list[Path],
    repo_root: Path | None = None,
) -> PhaseReviewReport:
    declared = _declared_phases(plan_path)
    if not declared:
        return PhaseReviewReport(
            status="SKIP",
            findings=[Finding("INFO", "no_phases_declared",
                              "the plan declares no `## Phase N` headers — no boundary to review")],
        )

    committed = _committed_ids(progress)
    closed: list[str] = []
    reviewed: list[str] = []
    findings: list[Finding] = []

    for phase in declared:
        task_ids = _plan_task_ids_in_phase(plan_path, phase)
        # A phase with no declared tasks never closes — phase_completeness owns that case.
        if not task_ids or not all(tid in committed for tid in task_ids):
            continue
        closed.append(phase)
        report_path = _find_report(review_dirs, slug, phase)
        if report_path is not None:
            reviewed.append(phase)
            findings.extend(_check_ordering(report_path, progress, phase, repo_root))
        else:
            findings.append(Finding(
                "HIGH",
                "phase_review_missing",
                f"Phase {phase} is fully committed but no mini-review report "
                f"({slug}-phase{phase}-review-*.md) exists. Step 4.7 was skipped, "
                "which cycle-implement.md documents as an anti-pattern.",
            ))

    # B-039 — FAIL on severity, not on the mere presence of a finding. Before this the rule was
    # `"FAIL" if findings else "PASS"`, which was fine while every finding was a missing report;
    # the ordering check adds INFO findings (a report predating B-039, or a comparison git cannot
    # make) and those must NOT fail the phase. An INFO that blocks is the same defect as a HIGH that
    # passes — B-038, one gate over.
    blocking = [f for f in findings if f.severity in ("HIGH", "BLOCKER")]
    status = "FAIL" if blocking else "PASS"
    return PhaseReviewReport(
        status=status,
        phases_declared=declared,
        phases_closed=closed,
        phases_reviewed=reviewed,
        findings=findings,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--progress", type=Path, required=True)
    parser.add_argument("--slug", required=True)
    parser.add_argument("--review-dir", type=Path, action="append", default=None)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    for path in (args.plan, args.progress):
        if not path.exists():
            print(f"file not found: {path}", file=sys.stderr)
            return 2
    try:
        progress = json.loads(args.progress.read_text(encoding="utf-8-sig"))
    except json.JSONDecodeError as exc:
        print(f"malformed checkpoint: {exc}", file=sys.stderr)
        return 2

    # B-032 — the reader must agree with the writer BY CONSTRUCTION, from the same resolver.
    # Both layouts are tried so a tree written before this fix is still found.
    root = args.plan.resolve().parent
    while root != root.parent and not (root / ".claude").exists() and not (root / ".git").exists():
        root = root.parent
    review_dirs = args.review_dir or [
        default_mini_reviews_dir(root),
        root / "knowledge-base" / "mini-reviews",
    ]
    report = check_phase_review(args.plan, progress, args.slug, review_dirs)

    if args.json:
        print(json.dumps({
            "status": report.status,
            "phases_declared": report.phases_declared,
            "phases_closed": report.phases_closed,
            "phases_reviewed": report.phases_reviewed,
            "findings": [{"severity": f.severity, "code": f.code, "message": f.message}
                         for f in report.findings],
        }, indent=2))
    else:
        print(f"{report.status}: {len(report.phases_reviewed)}/{len(report.phases_closed)} "
              "closed phases reviewed")
        for finding in report.findings:
            print(f"  [{finding.severity}] {finding.code}: {finding.message}")

    return 1 if report.status == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
