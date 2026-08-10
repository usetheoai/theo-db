"""Shared pytest fixtures for discover-improve tests."""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

SKILL_ROOT = Path(__file__).parent.parent
SCRIPTS_DIR = SKILL_ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))


def _opportunity(evidence: str, recommendation: str) -> str:
    return (
        "# Opportunity: Test\n\n"
        "**Item:** B-001\n"
        "**Repo:** squad\n"
        "**Mode:** review\n\n"
        "## Context\n\nBackground prose.\n\n"
        f"## Corner 1 — Evidence\n\n{evidence}\n\n"
        "## Corner 2 — Constraint Relation\n\n<!-- UNKNOWN: undeclared -->\n\n"
        "## Corner 3 — Blast Radius\n\nRepo-local.\n\n"
        "## Corner 4 — Verification\n\nA regression test asserts the fix.\n\n"
        f"## Recommendation\n\n{recommendation}\n"
    )


@pytest.fixture
def smelly_opportunity(tmp_path: Path) -> Path:
    """Weak imperatives and loopholes in BOTH Evidence and Recommendation.

    The Evidence text is phrased the way a real measurement is — descriptive, using
    "may" for something genuinely intermittent. Only the Recommendation may be rewritten.
    """
    path = tmp_path / "smelly.md"
    path.write_text(
        _opportunity(
            evidence=(
                "The endpoint may return 500 under load, and the retry could fire twice.\n\n"
                "```typescript\n"
                "// This should stay as is - a code fence is never rewritten\n"
                "if (something) { return; }\n"
                "```\n"
            ),
            recommendation=(
                "We should add a guard if possible. The handler could be split when applicable."
            ),
        ),
        encoding="utf-8",
    )
    return path


@pytest.fixture
def unresolvable_pointer_opportunity(tmp_path: Path) -> Path:
    """Carries a pointer that cannot resolve — the case that must NEVER be auto-annotated."""
    path = tmp_path / "unresolvable.md"
    path.write_text(
        _opportunity(
            evidence="See `src/never-exists-zzz.py:99` for the duplication.",
            recommendation="Fix it.",
        ),
        encoding="utf-8",
    )
    return path
