"""Regression tests for _resolve_thresholds — the project's own bands must win.

Surfaced by running the scorer against the positive fixture: the resolver looked only
under `.claude/rules/`, so in standalone layout (rules at the repo root) the project's
`discover-plan-thresholds.txt` was never found and the bundled example silently won.

Silent is the operative word. A scorer that grades against the wrong bands still prints a
confident verdict, and the decision taken on top of it has already been taken. Same shape
as the `check_xrefs` defect recorded in the CHANGELOG, where the validator audited the
`cwd` project and printed the other project's name.
"""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).parent.parent / "scripts"
sys.path.insert(0, str(SCRIPTS))

from run_measurement_plan_score import _resolve_thresholds  # noqa: E402


BANDS = "SHIPPABLE|95|2027-01-31|ADR\nINVALID|0|2027-01-31|ADR\n"


def _plan_in(root: Path) -> Path:
    plan = root / "plan.md"
    plan.write_text("# Measurement Plan: X\n", encoding="utf-8")
    return plan


def test_standalone_layout_rules_dir_wins(tmp_path: Path) -> None:
    """rules/ at the repo root — the standalone layout this repo actually uses."""
    root = tmp_path / "proj"
    (root / ".git").mkdir(parents=True)
    (root / "rules").mkdir()
    project_file = root / "rules" / "discover-plan-thresholds.txt"
    project_file.write_text(BANDS, encoding="utf-8")

    assert _resolve_thresholds(None, _plan_in(root)) == project_file


def test_plugin_layout_dot_claude_rules_wins(tmp_path: Path) -> None:
    """.claude/rules/ — the plugin layout. Both must resolve, not one or the other."""
    root = tmp_path / "proj"
    (root / ".claude" / "rules").mkdir(parents=True)
    project_file = root / ".claude" / "rules" / "discover-plan-thresholds.txt"
    project_file.write_text(BANDS, encoding="utf-8")

    assert _resolve_thresholds(None, _plan_in(root)) == project_file


def test_falls_back_to_bundled_example_when_project_declares_none(tmp_path: Path) -> None:
    root = tmp_path / "proj"
    (root / ".git").mkdir(parents=True)

    resolved = _resolve_thresholds(None, _plan_in(root))
    assert resolved.name == "discover-plan-thresholds.example.txt"
    assert resolved.is_file(), "the bundled fallback must exist, or the scorer crashes"


def test_explicit_argument_beats_everything(tmp_path: Path) -> None:
    root = tmp_path / "proj"
    (root / "rules").mkdir(parents=True)
    (root / "rules" / "discover-plan-thresholds.txt").write_text(BANDS, encoding="utf-8")

    explicit = tmp_path / "explicit.txt"
    explicit.write_text(BANDS, encoding="utf-8")

    assert _resolve_thresholds(explicit, _plan_in(root)) == explicit


def test_project_thresholds_file_actually_parses(project_root: Path) -> None:
    """The guard that was missing when the two defects hid each other.

    Resolution finding a file proves nothing if the file is in a shape the parser cannot
    read. This repo's `discover-plan-thresholds.txt` used `key = value` while
    `_parse_thresholds` splits on `|`, so it yielded zero bands — and with no bands every
    score, including 100, fell through to INVALID. Assert the real file the project ships
    produces usable bands, not merely that a path resolves.
    """
    from run_measurement_plan_score import _parse_thresholds

    for rel in ("rules", ".claude/rules"):
        candidate = project_root / rel / "discover-plan-thresholds.txt"
        if candidate.is_file():
            bands = _parse_thresholds(candidate)
            assert bands, f"{candidate} parsed to zero bands — every score would read INVALID"
            assert "SHIPPABLE" in bands
            assert bands["SHIPPABLE"] > bands.get("INVALID", -1)
            return
    pytest.skip("project ships no discover-plan-thresholds.txt")
