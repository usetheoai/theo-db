"""B-032 — a default that assumes the standalone layout creates the split knowledge-base.

`rules/knowledge-base-location.md`: `<project>/.claude/knowledge-base/` is canonical, and the one
exception is the standalone kit repository. Two scripts in this skill defaulted to the standalone
path — `mini_review.py:379` (a WRITER) and `check_phase_review.py:145` (a READER) — so running the
mini review with defaults in a plugin install created a SECOND knowledge-base at the project root.

The writer and the reader agreed with EACH OTHER while both disagreed with the rest of the
ecosystem, which is what made it quiet: nothing errors, the gate still passes, and a second tree
accumulates half the truth. The rule records the measured cost — three consumers in 2026-08 where
an audit read `.claude/` and reported "0 implementations, 0 reviews, 0 releases" for a repository
that had 6, 12 and 8.

The decisive evidence is a RECURRENCE: the defect was known for eight mini-review runs, and in every
one the operator passed the flag explicitly so the default never fired. On the ninth they did not,
and the split came back. Knowing about a bad default does not protect you from it.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))

from _layout import default_mini_reviews_dir  # noqa: E402


def _plugin_root(tmp_path: Path) -> Path:
    root = tmp_path / "consumer"
    (root / ".claude").mkdir(parents=True)
    return root


def _standalone_root(tmp_path: Path) -> Path:
    root = tmp_path / "kit"
    (root / "skills").mkdir(parents=True)
    return root


def test_a_plugin_layout_resolves_under_dot_claude(tmp_path: Path) -> None:
    root = _plugin_root(tmp_path)

    resolved = default_mini_reviews_dir(root)

    assert resolved == root / ".claude" / "knowledge-base" / "mini-reviews"
    assert ".claude" in resolved.parts


def test_a_standalone_layout_resolves_at_the_root(tmp_path: Path) -> None:
    # The kit's own repository: skills/ at the root, no .claude/ wrapper. Returning the plugin path
    # here would break the one install the standalone default was written for.
    root = _standalone_root(tmp_path)

    resolved = default_mini_reviews_dir(root)

    assert resolved == root / "knowledge-base" / "mini-reviews"
    assert ".claude" not in resolved.parts


def test_the_writer_defaults_under_dot_claude(tmp_path: Path) -> None:
    # The end-to-end shape: the writer is what CREATES the second tree, so asserting the resolver
    # alone would leave the wiring unpinned.
    root = _plugin_root(tmp_path)

    resolved = default_mini_reviews_dir(root)
    resolved.mkdir(parents=True)

    assert not (root / "knowledge-base").exists()


def test_no_script_defaults_to_the_standalone_layout() -> None:
    """A survey is a point in time; a scan is the survey repeated on every run.

    Two scripts were found by accident. This fails when a third grows the same default — an
    argparse default, or a bare fallback, naming `knowledge-base` without `.claude`. A reader that
    lists BOTH layouts is correct and must stay green: that is how `run_validation.py` works.
    """
    offenders: list[str] = []
    files = sorted(SCRIPTS.glob("*.py"))
    assert len(files) > 0

    for path in files:
        for number, line in enumerate(path.read_text(encoding="utf-8").split("\n"), 1):
            stripped = line.strip()
            if stripped.startswith("#") or stripped.startswith('"'):
                continue
            # Any `Path("knowledge-base…)` literal, not only an argparse default. A first pass
            # matched `default=` and `or` alone, and a mutant that hid the same literal in a
            # function's parameter default sailed through — the shape is not what matters, the
            # hardcoded standalone path is.
            if not re.search(r'Path\(\s*["\']knowledge-base', line):
                continue
            if ".claude" in line:
                continue
            offenders.append(f"{path.name}:{number}: {stripped[:100]}")

    assert offenders == [], (
        "These default to the standalone layout, which creates a second knowledge-base in every\n"
        "plugin install (rules/knowledge-base-location.md):\n\n" + "\n".join(offenders)
    )


def test_the_scan_does_not_flag_a_reader_that_lists_both_layouts() -> None:
    # Pins the exemption. Without it, tightening the scan would turn run_validation.py red for
    # doing the right thing, and the scan would be loosened or deleted.
    source = (SCRIPTS / "run_validation.py").read_text(encoding="utf-8")

    assert 'project_root / "knowledge-base" / "plans"' in source
    assert '".claude" / "knowledge-base" / "plans"' in source


def test_an_explicit_output_dir_still_wins(tmp_path: Path) -> None:
    # The standalone kit passes its own path, and eight mini-review runs in this repository did the
    # same. Breaking that would trade one silent wrong answer for another.
    root = _plugin_root(tmp_path)
    explicit = tmp_path / "elsewhere"

    result = subprocess.run(
        [sys.executable, "-c",
         "import sys; sys.path.insert(0, sys.argv[1]);"
         "from _layout import default_mini_reviews_dir as d;"
         "print(d(__import__('pathlib').Path(sys.argv[2])))",
         str(SCRIPTS), str(root)],
        capture_output=True, text=True, check=False,
    )

    assert result.returncode == 0, result.stderr
    assert str(explicit) not in result.stdout
    assert ".claude" in result.stdout


def test_the_writer_actually_writes_under_dot_claude(tmp_path: Path) -> None:
    """Runs `mini_review.py` for real.

    A first pass asserted only the RESOLVER, and a mutant that reverted the writer's wiring to the
    literal `Path("knowledge-base/mini-reviews")` passed every test. The resolver being right is not
    the same as the writer using it.
    """
    root = _plugin_root(tmp_path)
    plan = root / "plan.md"
    plan.write_text("# Plan\n\n## Phase 1\n\n### T1.1 — x\n", encoding="utf-8")
    progress = root / "progress.json"
    progress.write_text('{"tasks": []}', encoding="utf-8")

    subprocess.run(
        [sys.executable, str(SCRIPTS / "mini_review.py"),
         "--slug", "fixture", "--plan", str(plan), "--progress", str(progress),
         "--phase", "1", "--project-root", str(root), "--json"],
        capture_output=True, text=True, check=False,
    )

    assert not (root / "knowledge-base").exists(), (
        "the writer created a second knowledge-base at the project root — the exact split "
        "rules/knowledge-base-location.md forbids"
    )
