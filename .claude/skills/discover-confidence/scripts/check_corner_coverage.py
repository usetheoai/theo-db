"""Corner-coverage checker for /discover-execute opportunities (M2 deterministic).

Verifies that all 4 corners are populated:
  1. Evidence            — the measurement, per the mode's contract
  2. Constraint Relation — explore / subordinate / elevate / local optimisation
  3. Blast Radius        — what else across the ecosystem this touches
  4. Verification        — how we will know the fix worked; where the limit moves

An empty corner triggers an empty_corner_{name} hard cap (<=49).

Replaces the ancestor `check_research_coverage.py`, whose corners (Integration
Tests / Dependencies / Tools / Techniques) were corners of PRIOR-ART RESEARCH.
They asked what other projects do. These ask what is true about ours.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any


CORNERS = [
    ("evidence", r"##\s+Corner\s+1\s*(?:—|-)\s*Evidence"),
    ("constraint", r"##\s+Corner\s+2\s*(?:—|-)\s*Constraint\s+Relation"),
    ("blast_radius", r"##\s+Corner\s+3\s*(?:—|-)\s*Blast\s+Radius"),
    ("verification", r"##\s+Corner\s+4\s*(?:—|-)\s*Verification"),
]

# `unknown` is a complete answer for the constraint corner ONLY.
#
# rules/current-constraint.md declares the constraint a LENS, not a gate: we do not
# instrument flow across the ecosystem, so demanding a constraint claim would be
# answered by assertion -- the exact defect gate G5 refuses at intake.
#
# The marker is deliberately NOT honoured in the other three corners. An opportunity
# whose Evidence corner is `unknown` has measured nothing, and one whose Blast Radius
# is `unknown` is a change nobody can scope. Allowing a global escape hatch would let
# the cheapest corner to fake become the one every run reaches for.
UNKNOWN_CORNERS = {"constraint"}
UNKNOWN_RE = re.compile(r"<!--\s*UNKNOWN[\s:]+(.+?)-->", re.IGNORECASE | re.DOTALL)

PLACEHOLDER_RE = re.compile(r"<!--\s*TBD[\s:].*?-->", re.IGNORECASE | re.DOTALL)
HEADER_RE = re.compile(r"^#{1,6}\s+.*$", re.MULTILINE)

MIN_CONTENT_CHARS = 50  # Rough threshold: corner is populated if >=50 non-trivial chars


def _extract_section(content: str, pattern: str) -> str | None:
    """Extract H2 section content (from header to next H2 or EOF)."""
    section_re = re.compile(f"^{pattern}\\s*$", re.MULTILINE | re.IGNORECASE)
    match = section_re.search(content)
    if not match:
        return None
    start = match.end()
    next_h2 = re.search(r"^##\s+", content[start:], re.MULTILINE)
    end = start + next_h2.start() if next_h2 else len(content)
    return content[start:end]


def _is_populated(section_content: str, corner: str) -> bool:
    """Section is populated when it has real content beyond placeholders and headers.

    For the constraint corner only, an `<!-- UNKNOWN: reason -->` marker carrying a
    non-empty reason counts as populated.
    """
    if corner in UNKNOWN_CORNERS:
        unknown = UNKNOWN_RE.search(section_content)
        if unknown and unknown.group(1).strip():
            return True

    stripped = PLACEHOLDER_RE.sub("", section_content)
    no_headers = HEADER_RE.sub("", stripped)
    no_fences = re.sub(r"^```[^\n]*$", "", no_headers, flags=re.MULTILINE)
    return len(no_fences.strip()) >= MIN_CONTENT_CHARS


def check_corner_coverage(opportunity_path: Path) -> dict[str, Any]:
    content = opportunity_path.read_text(encoding="utf-8-sig")
    corners_status: list[dict[str, Any]] = []
    empty_corners: list[str] = []
    populated_count = 0

    for name, pattern in CORNERS:
        section = _extract_section(content, pattern)
        if section is None:
            corners_status.append({"corner": name, "present": False, "populated": False})
            empty_corners.append(name)
            continue

        populated = _is_populated(section, name)
        corners_status.append({"corner": name, "present": True, "populated": populated})
        if populated:
            populated_count += 1
        else:
            empty_corners.append(name)

    contributors = [
        f"Corner '{c['corner']}' populated" for c in corners_status if c["populated"]
    ][:3]
    detractors = [
        f"Corner '{c['corner']}' empty or missing" for c in corners_status if not c["populated"]
    ][:3]

    return {
        "corners_populated": populated_count,
        "corners_total": 4,
        "corners_status": corners_status,
        "empty_corners": empty_corners,
        "contributors": contributors,
        "detractors": detractors,
    }
