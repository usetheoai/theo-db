"""Tests for apply_fixes.py — deterministic fixes, and the two things it must never do."""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).parent.parent / "scripts" / "apply_fixes.py"


def _run(opportunity: Path, dry_run: bool = False) -> tuple[int, dict]:
    args = [sys.executable, str(SCRIPT), str(opportunity), "--json"]
    if dry_run:
        args.append("--dry-run")
    result = subprocess.run(args, capture_output=True, text=True)
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        data = {"raw": result.stdout, "stderr": result.stderr}
    return result.returncode, data


def test_recommendation_prose_is_fixed(smelly_opportunity: Path) -> None:
    rc, data = _run(smelly_opportunity)
    assert rc == 0
    assert data["weak_imperatives_fixed"] >= 2
    assert data["loopholes_stripped"] >= 2
    assert data["changed"] is True

    content = smelly_opportunity.read_text(encoding="utf-8-sig")
    recommendation = content.split("## Recommendation")[1]
    assert "must add a guard" in recommendation
    assert "if possible" not in recommendation
    assert "when applicable" not in recommendation


def test_evidence_section_is_never_rewritten(smelly_opportunity: Path) -> None:
    """The load-bearing restriction.

    Evidence is the RECORD of a measurement. "the endpoint may return 500 under load" is a
    measured fact about something intermittent; rewriting it to "must return 500" states a
    different and false claim. A regex cannot tell description from prescription, so it is
    not allowed to try.
    """
    _run(smelly_opportunity)
    content = smelly_opportunity.read_text(encoding="utf-8-sig")
    evidence = content.split("## Corner 1 — Evidence")[1].split("## Corner 2")[0]

    assert "may return 500 under load" in evidence
    assert "could fire twice" in evidence
    assert "must return 500" not in evidence


def test_code_fences_are_never_rewritten(smelly_opportunity: Path) -> None:
    _run(smelly_opportunity)
    content = smelly_opportunity.read_text(encoding="utf-8-sig")
    assert "// This should stay as is" in content


def test_never_writes_a_blocked_marker(unresolvable_pointer_opportunity: Path) -> None:
    """Locks the removed bypass shut.

    The ancestor annotated every unresolvable citation with `<!-- BLOCKED: ... -->`.
    Measured against the current checker: a marked pointer leaves `fabricated` and enters
    `explicitly_blocked`, so `fabricated_evidence` stops firing. That made the fixer an
    automated bypass of the cycle's most important hard cap — a script turning an INVALID
    opportunity into a passing one with nothing measured.

    If this test fails, someone reintroduced the marker. It is not a regression in
    formatting; it is a hole in the gate.
    """
    before = unresolvable_pointer_opportunity.read_text(encoding="utf-8-sig")
    _run(unresolvable_pointer_opportunity)
    after = unresolvable_pointer_opportunity.read_text(encoding="utf-8-sig")

    assert "<!-- BLOCKED:" not in after
    assert "never-exists-zzz.py:99" in after
    assert before == after, "a file whose only defect is an unresolvable pointer must not be touched"


def test_unresolvable_pointer_is_reported_not_fixed(unresolvable_pointer_opportunity: Path) -> None:
    rc, data = _run(unresolvable_pointer_opportunity)
    assert data["unresolvable_pointer_count"] == 1
    assert data["unresolvable_pointers"][0]["pointer"] == "src/never-exists-zzz.py:99"
    assert data["unresolvable_pointers"][0]["reason"] == "missing_file"
    assert data["changed"] is False
    assert rc == 3, "exit 3 tells a halt-loop to stop rather than iterate on something it cannot fix"


def test_line_past_end_of_file_is_reported(tmp_path: Path) -> None:
    """A resolving path with a stale line is still unresolvable evidence."""
    root = tmp_path / "proj"
    (root / ".git").mkdir(parents=True)
    target = root / "src" / "short.py"
    target.parent.mkdir()
    target.write_text("one\ntwo\nthree\n", encoding="utf-8")

    # A directory is required in the path: the pattern demands a slash so that prose like
    # "ratio 4:1" or "step 3:12" is never mistaken for an evidence pointer.
    path = root / "opp.md"
    path.write_text(
        "# Opportunity: Test\n\n## Corner 1 — Evidence\n\nSee `src/short.py:400`.\n\n"
        "## Recommendation\n\nFix it.\n",
        encoding="utf-8",
    )

    rc, data = _run(path)
    assert data["unresolvable_pointer_count"] == 1
    assert "line_out_of_range" in data["unresolvable_pointers"][0]["reason"]
    assert rc == 3


def test_already_blocked_pointer_is_not_re_reported(tmp_path: Path) -> None:
    """A human-marked gap is documented, not a finding to raise again."""
    path = tmp_path / "opp.md"
    path.write_text(
        "# Opportunity: Test\n\n## Corner 1 — Evidence\n\n"
        "See `src/gone.py:12` <!-- BLOCKED: removed in the 2026-07 refactor -->\n\n"
        "## Recommendation\n\nFix it.\n",
        encoding="utf-8",
    )
    rc, data = _run(path)
    assert data["unresolvable_pointer_count"] == 0
    assert rc == 0


def test_resolvable_pointer_produces_no_finding(tmp_path: Path) -> None:
    root = tmp_path / "proj"
    (root / ".git").mkdir(parents=True)
    (root / "src").mkdir()
    (root / "src" / "real.py").write_text("\n".join(f"line {i}" for i in range(1, 51)), encoding="utf-8")

    path = root / "opp.md"
    path.write_text(
        "# Opportunity: Test\n\n## Corner 1 — Evidence\n\nSee `src/real.py:42`.\n\n"
        "## Recommendation\n\nFix it.\n",
        encoding="utf-8",
    )
    rc, data = _run(path)
    assert data["unresolvable_pointer_count"] == 0
    assert rc == 0


def test_dry_run_writes_nothing(smelly_opportunity: Path) -> None:
    before = smelly_opportunity.read_text(encoding="utf-8-sig")
    rc, data = _run(smelly_opportunity, dry_run=True)
    assert data["dry_run"] is True
    assert data["weak_imperatives_fixed"] >= 2
    assert smelly_opportunity.read_text(encoding="utf-8-sig") == before


def test_idempotent(smelly_opportunity: Path) -> None:
    _run(smelly_opportunity)
    first = smelly_opportunity.read_text(encoding="utf-8-sig")
    rc, data = _run(smelly_opportunity)
    assert smelly_opportunity.read_text(encoding="utf-8-sig") == first
    assert data["weak_imperatives_fixed"] == 0
    assert data["loopholes_stripped"] == 0
    assert data["changed"] is False


def test_no_recommendation_section_is_harmless(tmp_path: Path) -> None:
    path = tmp_path / "opp.md"
    path.write_text("# Opportunity: Test\n\n## Context\n\nWe should do things if possible.\n", encoding="utf-8")
    before = path.read_text(encoding="utf-8")
    rc, data = _run(path)
    assert data["weak_imperatives_fixed"] == 0
    assert path.read_text(encoding="utf-8") == before
