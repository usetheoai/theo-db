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
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from check_phase_completeness import PHASE_HEADER_RE, _plan_task_ids_in_phase


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


def _has_report(review_dirs: list[Path], slug: str, phase: str) -> bool:
    """The report `mini_review.py` writes: `{slug}-phase{N}-review-{date}.md`."""
    pattern = f"{slug}-phase{phase}-review-*.md"
    return any(directory.exists() and any(directory.glob(pattern)) for directory in review_dirs)


def check_phase_review(
    plan_path: Path,
    progress: Any,
    slug: str,
    review_dirs: list[Path],
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
        if _has_report(review_dirs, slug, phase):
            reviewed.append(phase)
        else:
            findings.append(Finding(
                "HIGH",
                "phase_review_missing",
                f"Phase {phase} is fully committed but no mini-review report "
                f"({slug}-phase{phase}-review-*.md) exists. Step 4.7 was skipped, "
                "which cycle-implement.md documents as an anti-pattern.",
            ))

    status = "FAIL" if findings else "PASS"
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

    review_dirs = args.review_dir or [Path("knowledge-base") / "mini-reviews"]
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
