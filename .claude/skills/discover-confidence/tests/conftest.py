"""Shared pytest fixtures for discover-confidence tests."""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

SKILL_ROOT = Path(__file__).parent.parent
SCRIPTS_DIR = SKILL_ROOT / "scripts"
FIXTURES_DIR = SKILL_ROOT / "fixtures"
TEMPLATES_DIR = SKILL_ROOT / "templates"

# Make scripts/ importable
sys.path.insert(0, str(SCRIPTS_DIR))


def _find_project_root(start: Path) -> Path:
    current = start.resolve()
    while current != current.parent:
        if (current / ".claude").is_dir() or (current / ".git").exists():
            return current
        current = current.parent
    return start.parent.parent.parent


PROJECT_ROOT = _find_project_root(SKILL_ROOT)


@pytest.fixture(scope="session")
def skill_root() -> Path:
    return SKILL_ROOT


@pytest.fixture(scope="session")
def fixtures_dir() -> Path:
    return FIXTURES_DIR


@pytest.fixture(scope="session")
def templates_dir() -> Path:
    return TEMPLATES_DIR


@pytest.fixture(scope="session")
def project_root() -> Path:
    return PROJECT_ROOT


@pytest.fixture(scope="session")
def rubric_path() -> Path:
    return TEMPLATES_DIR / "rubric-opportunity.md"


@pytest.fixture
def good_opportunity(fixtures_dir: Path) -> Path:
    return fixtures_dir / "good-opportunity.md"


@pytest.fixture
def synthetic_opportunity(tmp_path: Path) -> Path:
    """Minimal valid opportunity for negative-path tests.

    Each corner carries >50 chars to clear MIN_CONTENT_CHARS in check_corner_coverage.
    Deliberately repo-local in its blast radius, so no ADR is required — the ADR
    conditional is exercised explicitly in test_check_opportunity_completeness.
    """
    body = (
        "# Opportunity: Test\n\n"
        "**Item:** B-001\n"
        "**Repo:** squad\n"
        "**Mode:** review\n"
        "**Slug:** `test`\n\n"
        "## Context\n\nTest context for the synthetic fixture used in unit tests.\n\n"
        "## Corner 1 — Evidence\n\n"
        "The measurement is recorded here with enough substantive detail to clear the "
        "minimum content threshold enforced by the corner checker.\n\n"
        "## Corner 2 — Constraint Relation\n\n"
        "Local optimisation. The declared constraint is untouched by this change, and "
        "that is stated plainly rather than left blank.\n\n"
        "## Corner 3 — Blast Radius\n\n"
        "Repo-local. Nothing outside this repository consumes the affected surface, so "
        "no consumer has to migrate.\n\n"
        "## Corner 4 — Verification\n\n"
        "A regression test asserts the corrected behaviour and fails against the current "
        "state. The limit plausibly moves to the next stage afterwards.\n\n"
        "## Recommendation\n\n- Do X for reason Y as explained above\n"
    )
    path = tmp_path / "test-opportunity.md"
    path.write_text(body, encoding="utf-8")
    return path
