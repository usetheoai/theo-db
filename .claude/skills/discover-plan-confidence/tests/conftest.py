"""Shared pytest fixtures for discover-plan-confidence tests."""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

SKILL_ROOT = Path(__file__).parent.parent
SCRIPTS_DIR = SKILL_ROOT / "scripts"
FIXTURES_DIR = SKILL_ROOT / "fixtures"
TEMPLATES_DIR = SKILL_ROOT / "templates"

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
    return TEMPLATES_DIR / "rubric-measurement-plan.md"


@pytest.fixture
def good_measurement_plan(fixtures_dir: Path) -> Path:
    return fixtures_dir / "good-measurement-plan.md"


@pytest.fixture
def missing_corner_measurement_plan(fixtures_dir: Path) -> Path:
    return fixtures_dir / "missing-corner-measurement-plan.md"


@pytest.fixture
def synthetic_measurement_plan(tmp_path: Path) -> Path:
    """Minimal valid measurement plan: every mandatory section, all 4 corners covered.

    Baseline for negative-path tests that break it in exactly one way. Path targets are
    intentionally absent — target resolution is exercised against the real repo in the
    fixture-backed tests, not here.
    """
    body = (
        "# Measurement Plan: Synthetic\n\n"
        "**Item:** B-001\n"
        "**Repo:** squad\n"
        "**Mode:** review\n"
        "**Slug:** `synthetic`\n\n"
        "## Context\n\nTest context for the synthetic measurement plan used in unit tests.\n\n"
        "## Hypothesis\n\nA stated belief about our system that measurement can refute.\n\n"
        "## Falsification\n\nIf every branch is present and refuses, the hypothesis is dead "
        "and the item is killed with that result recorded.\n\n"
        "## Measurement Questions\n\n"
        "| # | Question | Corner | Tool | Target | Expected answer shape |\n"
        "|---|---|---|---|---|---|\n"
        "| Q1 | What does the rule declare? | evidence | Read | rules-file | Enumerated list |\n"
        "| Q2 | Is the constraint touched? | constraint | Read | constraint-file | Relation statement |\n"
        "| Q3 | What else depends on it? | blast_radius | Grep | settings-file | Caller list |\n"
        "| Q4 | Does the gap reproduce? | verification | Bash | hooks-dir | Exit code table |\n\n"
        "## Halt-loop Checkpoints\n\n"
        "| Checkpoint | Assertion | Action if fails |\n"
        "|---|---|---|\n"
        "| Before Qx | cited path opens | mark BLOCKED |\n\n"
        "## Acceptance Criteria\n\n"
        "- [ ] Every question answered or BLOCKED\n"
        "- [ ] Falsification criterion evaluated explicitly\n"
    )
    plan = tmp_path / "synthetic-plan.md"
    plan.write_text(body, encoding="utf-8")
    return plan
