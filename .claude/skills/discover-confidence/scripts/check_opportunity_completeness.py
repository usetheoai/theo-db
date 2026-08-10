"""Mandatory-section + conditional-ADR checker for opportunities (M2 deterministic).

Verifies that the opportunity contains every mandatory section, and that an ADR is
present WHEN the change reaches beyond its own repo.

Replaces the ancestor `check_blueprint_completeness.py`. Two sections were dropped
and one requirement was made conditional:

  - `## Objective` folded into `## Context`. A maintenance opportunity's objective is
    the backlog item's DoD; restating it is duplication, not rigour.
  - `## Cross-cutting Comparison` removed outright. It compared REFERENCE PROJECTS to
    one another -- the prior-art practice this cycle no longer performs.
  - `## ADRs` made conditional on blast radius. Demanding an architectural decision
    record for a one-line fix in a leaf repo is ceremony; omitting one for a change
    that reaches other repos is how a breaking decision ships unrecorded.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any


# Each entry: (display name, regex matching the header)
MANDATORY_SECTIONS = [
    ("Header", r"^#\s+Opportunity:"),
    ("Item", r"^\*\*Item:\*\*\s*B-\d+"),
    ("Repo", r"^\*\*Repo:\*\*\s*\S+"),
    ("Mode", r"^\*\*Mode:\*\*\s*(?:review|live-test|bug|evolve)\b"),
    ("Context", r"^##\s+Context"),
    ("Corner 1 — Evidence", r"^##\s+Corner\s+1\s*(?:—|-)\s*Evidence"),
    ("Corner 2 — Constraint Relation", r"^##\s+Corner\s+2\s*(?:—|-)\s*Constraint\s+Relation"),
    ("Corner 3 — Blast Radius", r"^##\s+Corner\s+3\s*(?:—|-)\s*Blast\s+Radius"),
    ("Corner 4 — Verification", r"^##\s+Corner\s+4\s*(?:—|-)\s*Verification"),
    ("Recommendation", r"^##\s+Recommendation"),
]

ADR_HEADER_RE = re.compile(r"^###\s+D\d+\s*(?:—|-)", re.MULTILINE)
REPO_DECL_RE = re.compile(r"^\*\*Repo:\*\*\s*`?([A-Za-z0-9_.\-]+)`?", re.MULTILINE)

# Repos of the governed ecosystem, as they appear in prose. Deliberately a NAME SHAPE
# rather than a hardcoded inventory: the inventory lives in BACKLOG.md and drifts, and
# a checker that carries a stale copy of it silently stops recognising new repos.
ECOSYSTEM_REPO_RE = re.compile(r"\b(theo(?:kit)?(?:-[a-z0-9]+)*)\b", re.IGNORECASE)


def _section_body(content: str, header_pattern: str) -> str:
    match = re.search(header_pattern, content, re.MULTILINE | re.IGNORECASE)
    if not match:
        return ""
    start = match.end()
    next_h2 = re.search(r"^##\s+", content[start:], re.MULTILINE)
    return content[start : start + next_h2.start()] if next_h2 else content[start:]


def check_opportunity_completeness(opportunity_path: Path) -> dict[str, Any]:
    content = opportunity_path.read_text(encoding="utf-8-sig")

    present: list[str] = []
    missing: list[str] = []

    for name, pattern in MANDATORY_SECTIONS:
        if re.search(pattern, content, re.MULTILINE | re.IGNORECASE):
            present.append(name)
        else:
            missing.append(name)

    adrs_body = _section_body(content, r"^##\s+ADRs\b")
    adr_count = len(ADR_HEADER_RE.findall(adrs_body))

    # Is the change cross-repo? Read the Blast Radius corner and look for any ecosystem
    # repo name other than the one this opportunity declares.
    repo_match = REPO_DECL_RE.search(content)
    own_repo = repo_match.group(1).lower() if repo_match else None

    blast_body = _section_body(content, r"^##\s+Corner\s+3\s*(?:—|-)\s*Blast\s+Radius")
    mentioned = {m.group(1).lower() for m in ECOSYSTEM_REPO_RE.finditer(blast_body)}
    foreign_repos = sorted(r for r in mentioned if r != own_repo)
    cross_repo = bool(foreign_repos)
    adr_required = cross_repo
    adr_missing = adr_required and adr_count == 0

    total_required = len(MANDATORY_SECTIONS)
    found = len(present)

    contributors = [f"{found}/{total_required} mandatory sections present"]
    if adr_count > 0:
        contributors.append(f"{adr_count} ADR(s) found in ADRs section")
    if not adr_required:
        contributors.append("Change is repo-local — no ADR required")

    detractors: list[str] = [f"Missing section: {m}" for m in missing[:3]]
    if adr_missing:
        detractors.append(
            f"Blast radius reaches {', '.join(foreign_repos)} but no ADR is recorded"
        )

    return {
        "total_required": total_required,
        "found": found,
        "present": present,
        "missing_mandatory": missing,
        "adr_count": adr_count,
        "own_repo": own_repo,
        "foreign_repos": foreign_repos,
        "cross_repo": cross_repo,
        "adr_required": adr_required,
        "adr_missing": adr_missing,
        "contributors": contributors,
        "detractors": detractors,
    }
