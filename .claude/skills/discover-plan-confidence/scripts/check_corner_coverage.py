"""Corner-coverage checker for /discover-plan measurement plans (M2 deterministic).

Verifies that all 4 corners (evidence, constraint, blast_radius, verification) are
covered EITHER by at least one Measurement Question OR by an explicit DEFER-CORNER
marker.

An uncovered corner (no Q mapped AND no DEFER-CORNER marker) triggers an
empty_corner_{name} hard cap (<=49) in the orchestrator.

Sibling of `skills/discover-confidence/scripts/check_corner_coverage.py`, but adapted
for plans: it inspects the Measurement Questions TABLE rather than prose sections. The
plan declares how each corner WILL be filled; the opportunity checker later verifies
that it was.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any


CORNERS = ("evidence", "constraint", "blast_radius", "verification")

QUESTIONS_HEADER_RE = re.compile(
    r"^##\s+Measurement\s+Questions\s*$", re.MULTILINE | re.IGNORECASE
)
NEXT_H2_RE = re.compile(r"^##\s+\S", re.MULTILINE)
TABLE_ROW_RE = re.compile(r"^\|.*\|\s*$", re.MULTILINE)
QID_RE = re.compile(r"^Q\d+$")
DEFER_CORNER_RE = re.compile(
    r"<!--\s*DEFER-CORNER:\s*(evidence|constraint|blast_radius|verification)\b",
    re.IGNORECASE | re.DOTALL,
)
TBD_MARKER_RE = re.compile(r"<!--\s*TBD\b", re.IGNORECASE)


def _has_defer_corner_marker(content: str, corner: str) -> bool:
    """True iff a `<!-- DEFER-CORNER: {corner} | reason -->` marker exists in content."""
    return any(m.group(1).lower() == corner for m in DEFER_CORNER_RE.finditer(content))


def _extract_questions_section(content: str) -> str:
    """Return the body of the `## Measurement Questions` H2 (header excluded)."""
    header_match = QUESTIONS_HEADER_RE.search(content)
    if not header_match:
        return ""
    start = header_match.end()
    next_h2 = NEXT_H2_RE.search(content, pos=start)
    return content[start : next_h2.start()] if next_h2 else content[start:]


def _parse_question_corners(questions_section: str) -> list[str]:
    """Corner value (col 3) of every row whose col 1 matches Q\\d+."""
    corners: list[str] = []
    for row_match in TABLE_ROW_RE.finditer(questions_section):
        cells = [p.strip() for p in row_match.group(0).split("|")]
        if len(cells) < 4 or not QID_RE.match(cells[1]):
            continue
        corners.append(cells[3].strip().strip("`").lower())
    return corners


def check_corner_coverage(plan_path: Path) -> dict[str, Any]:
    content = plan_path.read_text(encoding="utf-8-sig")
    mapped = _parse_question_corners(_extract_questions_section(content))

    corners_status: list[dict[str, Any]] = []
    empty_corners: list[str] = []
    covered_count = 0

    for corner in CORNERS:
        question_count = mapped.count(corner)
        deferred = _has_defer_corner_marker(content, corner)
        covered = question_count > 0 or deferred
        corners_status.append(
            {
                "corner": corner,
                "questions": question_count,
                "deferred": deferred,
                "covered": covered,
            }
        )
        if covered:
            covered_count += 1
        else:
            empty_corners.append(corner)

    contributors = [
        (
            f"Corner '{c['corner']}' explicitly deferred"
            if c["deferred"]
            else f"Corner '{c['corner']}' covered by {c['questions']} question(s)"
        )
        for c in corners_status
        if c["covered"]
    ][:3]
    detractors = [
        f"Corner '{c['corner']}' has no question and no DEFER-CORNER marker"
        for c in corners_status
        if not c["covered"]
    ][:3]

    return {
        "corners_populated": covered_count,
        "corners_total": len(CORNERS),
        "corners_status": corners_status,
        "empty_corners": empty_corners,
        "contributors": contributors,
        "detractors": detractors,
    }
