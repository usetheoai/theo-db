"""End-to-end tests for run_opportunity_score.py — verifies scorer integration."""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).parent.parent / "scripts" / "run_opportunity_score.py"


def _run(opportunity_path: Path, project_root: Path) -> tuple[int, dict]:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), str(opportunity_path), "--no-warn"],
        capture_output=True,
        text=True,
        cwd=str(project_root),
    )
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        data = {"raw_stdout": result.stdout, "stderr": result.stderr}
    return result.returncode, data


@pytest.fixture
def staged(project_root: Path):
    """Write an opportunity inside the repo so the project root resolves to it.

    Pointer resolution walks up from the artifact, so an artifact parked in /tmp would
    resolve every repo-relative pointer as fabricated.
    """
    written: list[Path] = []

    def _write(name: str, content: str) -> Path:
        directory = project_root / ".claude" / "knowledge-base" / "discoveries" / "opportunities"
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / name
        path.write_text(content, encoding="utf-8")
        written.append(path)
        return path

    yield _write

    for path in written:
        path.unlink(missing_ok=True)


def test_good_opportunity_scores_shippable(good_opportunity: Path, project_root: Path) -> None:
    rc, data = _run(good_opportunity, project_root)
    assert rc == 0, f"Expected exit 0, got {rc}: {data}"
    assert data["verdict"] == "SHIPPABLE"
    assert data["final_score_after_caps"] >= 90
    assert data["hard_caps_triggered"] == []


def test_fabricated_evidence_is_invalid(good_opportunity: Path, staged) -> None:
    corrupted = staged(
        "test-fabricated-opportunity.md",
        good_opportunity.read_text(encoding="utf-8-sig")
        # Deliberately NOT under rules/: check_xrefs scans .py files for `rules/...`
        # references and would flag this intentional non-existent path as a broken xref.
        + "\n\nBogus pointer: `docs/this-file-does-not-exist-xyz.md:99`\n",
    )
    rc, data = _run(corrupted, corrupted.parents[4])
    assert rc == 1, f"Expected exit 1 (INVALID), got {rc}: {data}"
    assert data["verdict"] == "INVALID"
    assert "fabricated_evidence" in data["hard_caps_triggered"]
    assert data["final_score_after_caps"] <= 49.0


def test_pointer_past_end_of_file_is_invalid(good_opportunity: Path, staged) -> None:
    """A real file cited at a line it does not have caps the score just like a fake path."""
    corrupted = staged(
        "test-stale-pointer-opportunity.md",
        good_opportunity.read_text(encoding="utf-8-sig")
        + "\n\nStale pointer: `rules/current-constraint.md:99999`\n",
    )
    rc, data = _run(corrupted, corrupted.parents[4])
    assert rc == 1
    assert "fabricated_evidence" in data["hard_caps_triggered"]


def test_missing_corner_is_invalid(staged) -> None:
    path = staged(
        "test-missing-corner-opportunity.md",
        "# Opportunity: Test\n\n"
        "**Item:** B-002\n"
        "**Repo:** squad\n"
        "**Mode:** review\n\n"
        "## Context\n\nText.\n\n"
        # Corner 1 — Evidence MISSING
        "## Corner 2 — Constraint Relation\n\nReal content here, well past the threshold "
        "for a populated corner section.\n\n"
        "## Corner 3 — Blast Radius\n\nReal content here, well past the threshold for a "
        "populated corner section.\n\n"
        "## Corner 4 — Verification\n\nReal content here, well past the threshold for a "
        "populated corner section.\n\n"
        "## Recommendation\n\n- Do X\n",
    )
    rc, data = _run(path, path.parents[4])
    assert rc == 1, f"Expected exit 1 (INVALID), got {rc}: {data}"
    assert data["verdict"] == "INVALID"
    assert "empty_corner_evidence" in data["hard_caps_triggered"]


def test_cross_repo_without_adr_is_capped(staged) -> None:
    """Exercises the conditional ADR cap end-to-end, not just in the unit."""
    path = staged(
        "test-cross-repo-opportunity.md",
        "# Opportunity: Test\n\n"
        "**Item:** B-003\n"
        "**Repo:** theo-contracts\n"
        "**Mode:** review\n\n"
        "## Context\n\nText.\n\n"
        "## Corner 1 — Evidence\n\nReal content here, well past the threshold for a "
        "populated corner section.\n\n"
        "## Corner 2 — Constraint Relation\n\nReal content here, well past the threshold "
        "for a populated corner section.\n\n"
        "## Corner 3 — Blast Radius\n\nThe jwt claim shape is consumed by theo-cloud and "
        "theo-cli, both of which must migrate.\n\n"
        "## Corner 4 — Verification\n\nReal content here, well past the threshold for a "
        "populated corner section.\n\n"
        "## Recommendation\n\n- Do X\n",
    )
    rc, data = _run(path, path.parents[4])
    assert "no_adr_on_cross_repo_change" in data["hard_caps_triggered"]
    assert data["final_score_after_caps"] <= 70.0
