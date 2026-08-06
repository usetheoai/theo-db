"""Plan-completeness checker for /discover-plan measurement plans (M2 deterministic).

Bundles 4 sub-checks for orchestrator simplicity:

  1. mandatory_sections  — all required headers present (caps verdict at 70 if any missing)
  2. question_budget     — 3 <= N <= 10, per-corner <= 3, per-corner >= 1 OR DEFER-CORNER
  3. method_per_question — Tool AND Target columns non-empty for every question
  4. falsification       — the `## Falsification` section states what would kill the hypothesis

Returns one combined report; the orchestrator decides which caps fire:
  - missing_mandatory non-empty    -> mandatory_section_missing cap (70)
  - falsification_missing True     -> no_falsification_criterion cap (70)
  - budget_violations non-empty    -> question_budget_violated cap (70)
  - methodless_questions non-empty -> method_missing cap (70)

Two deliberate departures from the ancestor:

**ADRs are gone; falsification replaces them.** The ancestor demanded >=2 ADRs in every
discovery plan. In a MEASUREMENT plan an ADR is premature — nothing has been measured, so
there is nothing to decide yet. What makes a measurement plan honest is stating IN ADVANCE
what result would kill the hypothesis. Without it, any observation can be reinterpreted
after the fact as confirming whatever was already believed, and the measurement becomes
a ritual that cannot fail.

**The question floor drops from 5 to 3.** Five questions was calibrated for a prior-art
survey across several projects. A maintenance item is smaller, and a floor that forces
padding produces questions written to satisfy a counter.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from check_corner_coverage import CORNERS, _has_defer_corner_marker


MIN_QUESTIONS = 3
MAX_QUESTIONS = 10
MAX_PER_CORNER = 3
MIN_FALSIFICATION_CHARS = 40

MANDATORY_SECTIONS = [
    ("Header", r"^#\s+Measurement\s+Plan:"),
    ("Item", r"^\*\*Item:\*\*\s*B-\d+"),
    ("Repo", r"^\*\*Repo:\*\*\s*\S+"),
    ("Mode", r"^\*\*Mode:\*\*\s*(?:review|live-test|bug|evolve)\b"),
    ("Context", r"^##\s+Context"),
    ("Hypothesis", r"^##\s+Hypothesis"),
    ("Falsification", r"^##\s+Falsification"),
    ("Measurement Questions", r"^##\s+Measurement\s+Questions"),
    ("Halt-loop Checkpoints", r"^##\s+Halt[- ]loop\s+Checkpoints"),
    ("Acceptance Criteria", r"^##\s+Acceptance\s+Criteria"),
]

QUESTIONS_HEADER_RE = re.compile(
    r"^##\s+Measurement\s+Questions\s*$", re.MULTILINE | re.IGNORECASE
)
FALSIFICATION_HEADER_RE = re.compile(r"^##\s+Falsification\b", re.MULTILINE | re.IGNORECASE)
NEXT_H2_RE = re.compile(r"^##\s+\S", re.MULTILINE)
TABLE_ROW_RE = re.compile(r"^\|.*\|\s*$", re.MULTILINE)
QID_RE = re.compile(r"^Q\d+$")
PLACEHOLDER_RE = re.compile(r"<!--\s*TBD[\s:].*?-->", re.IGNORECASE | re.DOTALL)
HEADER_LINE_RE = re.compile(r"^#{1,6}\s+.*$", re.MULTILINE)
TOOL_HEADER_RE = re.compile(r"^\s*Tool\b", re.IGNORECASE)
TARGET_HEADER_RE = re.compile(r"^\s*Target\b", re.IGNORECASE)


def _check_mandatory_sections(content: str) -> tuple[list[str], list[str]]:
    present: list[str] = []
    missing: list[str] = []
    for name, pattern in MANDATORY_SECTIONS:
        (present if re.search(pattern, content, re.MULTILINE | re.IGNORECASE) else missing).append(name)
    return present, missing


def _section_body(content: str, header_re: re.Pattern[str]) -> str:
    header = header_re.search(content)
    if not header:
        return ""
    start = header.end()
    next_h2 = NEXT_H2_RE.search(content, pos=start)
    return content[start : next_h2.start()] if next_h2 else content[start:]


def _split_row(row: str) -> list[str]:
    return [p.strip() for p in row.split("|")]


def _find_method_column_indices(questions_section: str) -> tuple[int | None, int | None]:
    """Locate the Tool and Target columns by HEADER TEXT, not by position.

    Position-based lookup breaks the moment someone inserts a column, and it breaks
    silently — every question reads as methodless, or worse, as fine.
    """
    for row_match in TABLE_ROW_RE.finditer(questions_section):
        cells = _split_row(row_match.group(0))
        if not any(TOOL_HEADER_RE.match(c) or TARGET_HEADER_RE.match(c) for c in cells):
            continue
        idx_tool = next((i for i, c in enumerate(cells) if TOOL_HEADER_RE.match(c)), None)
        idx_target = next((i for i, c in enumerate(cells) if TARGET_HEADER_RE.match(c)), None)
        return idx_tool, idx_target
    return None, None


def _parse_question_rows(questions_section: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for row_match in TABLE_ROW_RE.finditer(questions_section):
        cells = _split_row(row_match.group(0))
        if len(cells) < 4 or not QID_RE.match(cells[1]):
            continue
        rows.append({"q_id": cells[1], "corner": cells[3].strip().strip("`").lower(), "cells": cells})
    return rows


def _check_question_budget(content: str, q_rows: list[dict[str, Any]]) -> list[str]:
    violations: list[str] = []
    total = len(q_rows)
    if total < MIN_QUESTIONS:
        violations.append(f"too_few_questions ({total} < {MIN_QUESTIONS})")
    if total > MAX_QUESTIONS:
        violations.append(f"too_many_questions ({total} > {MAX_QUESTIONS})")

    counts = {c: 0 for c in CORNERS}
    for row in q_rows:
        if row["corner"] in counts:
            counts[row["corner"]] += 1

    for corner, n in counts.items():
        if n > MAX_PER_CORNER:
            violations.append(f"corner_overflow_{corner} ({n} > {MAX_PER_CORNER})")
        elif n == 0 and not _has_defer_corner_marker(content, corner):
            violations.append(f"corner_uncovered_{corner}")
    return violations


def _check_methods(
    q_rows: list[dict[str, Any]], idx_tool: int | None, idx_target: int | None
) -> list[str]:
    """Q-IDs missing a Tool or a Target.

    A question with no tool is a wish; one with no target is a tool pointed at nothing.
    Either way the measurement cannot be run as written.
    """
    if idx_tool is None and idx_target is None:
        return ["__header_not_found__"]

    violations: list[str] = []
    for row in q_rows:
        cells = row["cells"]
        tool = cells[idx_tool].strip() if idx_tool is not None and idx_tool < len(cells) else ""
        target = cells[idx_target].strip() if idx_target is not None and idx_target < len(cells) else ""
        if not tool or not target:
            violations.append(row["q_id"])
    return violations


def _check_falsification(content: str) -> tuple[bool, int]:
    """Return (missing, char_count) for the `## Falsification` section body."""
    body = _section_body(content, FALSIFICATION_HEADER_RE)
    stripped = HEADER_LINE_RE.sub("", PLACEHOLDER_RE.sub("", body)).strip()
    return len(stripped) < MIN_FALSIFICATION_CHARS, len(stripped)


def check_plan_completeness(plan_path: Path) -> dict[str, Any]:
    content = plan_path.read_text(encoding="utf-8-sig")

    present, missing = _check_mandatory_sections(content)
    q_section = _section_body(content, QUESTIONS_HEADER_RE)
    idx_tool, idx_target = _find_method_column_indices(q_section)
    q_rows = _parse_question_rows(q_section)
    budget_violations = _check_question_budget(content, q_rows)
    methodless = _check_methods(q_rows, idx_tool, idx_target)
    falsification_missing, falsification_chars = _check_falsification(content)

    contributors: list[str] = [f"{len(present)}/{len(MANDATORY_SECTIONS)} mandatory sections present"]
    if not falsification_missing:
        contributors.append("Falsification criterion stated")
    if not budget_violations:
        contributors.append(f"Question budget OK ({len(q_rows)} Qs)")
    if not methodless:
        contributors.append("Every Q names a Tool and a Target")

    detractors: list[str] = [f"Missing section: {m}" for m in missing[:3]]
    if falsification_missing:
        detractors.append(
            "No falsification criterion — nothing stated in advance would kill the hypothesis"
        )
    detractors.extend(f"Budget: {v}" for v in budget_violations[:3])
    if methodless:
        detractors.append(f"Questions without Tool/Target: {methodless[:3]}")

    return {
        "total_required": len(MANDATORY_SECTIONS),
        "found": len(present),
        "present": present,
        "missing_mandatory": missing,
        "falsification_missing": falsification_missing,
        "falsification_chars": falsification_chars,
        "budget_violations": budget_violations,
        "methodless_questions": methodless,
        "question_count": len(q_rows),
        "contributors": contributors[:3],
        "detractors": detractors[:3],
    }
