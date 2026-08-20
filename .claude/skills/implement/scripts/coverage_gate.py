#!/usr/bin/env python3
"""The coverage gate — a number that was measured, or an honest WARN.

WHY this module exists
----------------------
The previous `check_coverage` ran `npm run test:coverage` and returned `PASS` on
exit 0. Its own docstring admitted it never parsed lcov or json-summary, so the
"≥ 90% on changed files" promised by SKILL.md was enforced by nothing: a project
whose runner had no threshold configured passed the gate at any coverage at all.
A gate named after a number it never reads is worse than no gate — it launders a
command's exit code into a measurement.

Now the report is read. When it cannot be read, the check says so (`WARN`)
instead of claiming a pass.

Honest scope: this reads TOTAL line coverage. The per-changed-file and
critical-path thresholds in SKILL.md remain unenforced here — they need the
plan's file list and a per-file report, and claiming them from a total would be
the same laundering in a new place.
"""
from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

#: Floor used when neither the CLI nor the project's rules file names one.
DEFAULT_MIN_PERCENT = 80

#: Where a project may raise the floor, in both supported layouts.
_THRESHOLD_FILES = (
    Path("rules") / "code-quality-thresholds.txt",
    Path(".claude") / "rules" / "code-quality-thresholds.txt",
)
_THRESHOLD_KEY = "coverage.min_percent"

_LINE_RATE_RE = re.compile(r'line-rate="([0-9.]+)"')


def resolve_threshold(project_root: Path, cli_value: float | None = None) -> tuple[float, str]:
    """Return (threshold, source). Source is reported so the number is traceable."""
    if cli_value is not None:
        return cli_value, "cli"
    for relative in _THRESHOLD_FILES:
        path = project_root / relative
        if not path.exists():
            continue
        for raw_line in path.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, value = line.partition("=")
            if key.strip() == _THRESHOLD_KEY:
                try:
                    number = float(value.strip())
                except ValueError:
                    continue
                return (int(number) if number.is_integer() else number), "project"
    return DEFAULT_MIN_PERCENT, "default"


def _from_json_summary(path: Path) -> float | None:
    """Istanbul `json-summary` reporter — the npm ecosystem's default artifact."""
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        return float(data["total"]["lines"]["pct"])
    except (json.JSONDecodeError, KeyError, TypeError, ValueError, OSError):
        return None


def _from_cobertura(path: Path) -> float | None:
    """Cobertura XML — what coverage.py, gocover-cobertura and tarpaulin emit."""
    try:
        match = _LINE_RATE_RE.search(path.read_text(encoding="utf-8"))
    except OSError:
        return None
    if not match:
        return None
    try:
        return round(float(match.group(1)) * 100, 2)
    except ValueError:
        return None


def _from_coverage_py_json(path: Path) -> float | None:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        return float(data["totals"]["percent_covered"])
    except (json.JSONDecodeError, KeyError, TypeError, ValueError, OSError):
        return None


#: Ordered so the most specific artifact wins; each entry is (relative path, parser).
_ARTIFACTS = (
    (Path("coverage") / "coverage-summary.json", _from_json_summary),
    (Path("coverage.xml"), _from_cobertura),
    (Path("coverage") / "cobertura-coverage.xml", _from_cobertura),
    (Path("coverage.json"), _from_coverage_py_json),
)


def read_coverage_percent(project_root: Path) -> tuple[float | None, str | None]:
    """Return (percent, artifact_path) for the first report that parses."""
    for relative, parser in _ARTIFACTS:
        path = project_root / relative
        if not path.exists():
            continue
        percent = parser(path)
        if percent is not None:
            return percent, str(relative)
    return None, None


def evaluate(project_root: Path, *, command_ran: bool, command_failed: bool,
             cli_threshold: float | None = None) -> dict[str, Any]:
    """Build the coverage check from what is actually on disk.

    `command_ran` / `command_failed` describe the coverage command the caller
    invoked (if any); this function owns the verdict.
    """
    threshold, source = resolve_threshold(project_root, cli_threshold)
    percent, artifact = read_coverage_percent(project_root)

    if command_failed:
        return {
            "name": "coverage",
            "status": "FAIL",
            "reason": "the coverage command exited non-zero",
            "threshold": threshold,
            "threshold_source": source,
            "coverage_pct": percent,
        }

    if percent is None:
        if not command_ran:
            return {
                "name": "coverage",
                "status": "SKIP",
                "reason": "no coverage command and no coverage report on disk — pre-code phase",
                "threshold": threshold,
                "threshold_source": source,
            }
        return {
            "name": "coverage",
            "status": "WARN",
            "reason": (
                "the coverage command exited 0 but no parseable report was found "
                f"(looked for {', '.join(str(p) for p, _ in _ARTIFACTS)}) — "
                "the threshold was NOT verified. Enable a json-summary or cobertura reporter."
            ),
            "threshold": threshold,
            "threshold_source": source,
        }

    status = "PASS" if percent >= threshold else "FAIL"
    check: dict[str, Any] = {
        "name": "coverage",
        "status": status,
        "coverage_pct": percent,
        "threshold": threshold,
        "threshold_source": source,
        "artifact": artifact,
    }
    if status == "FAIL":
        check["reason"] = f"total line coverage {percent}% is below the {threshold}% floor"
    return check
