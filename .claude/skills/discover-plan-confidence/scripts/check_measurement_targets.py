"""Measurement-target checker for /discover-plan measurement plans (M2 deterministic).

Replaces the ancestor `check_reference_citations.py`, which verified citations into
`knowledge-base/references/` -- the prior-art study zone this cycle retired.

A measurement plan names WHAT IT WILL MEASURE, and that is the thing to verify before
anyone spends time measuring. Two target classes:

  1. Path targets    `dir/` or `dir/file.ext` -- resolved on disk.
  2. Live targets    an https:// URL -- checked against `rules/live-target.txt`.

The distinction from the opportunity-side checker is deliberate. There, evidence is
`file:line` and the line must exist, because a measurement already happened. Here the
plan points at what it INTENDS to open, so a directory is a legitimate target and no
line number is expected yet.

A plan that names a path nobody can open is a plan that will produce fabricated
evidence -- caught before the measurement rather than after.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any


TARGETS_HEADER_RE = re.compile(r"^##\s+Measurement\s+Questions\s*$", re.MULTILINE | re.IGNORECASE)
# Backticked path: `theo-lens/src/` or `theo-lens/src/trace.ts`. Requires a slash so
# that prose words in backticks are not mistaken for targets.
PATH_TARGET_RE = re.compile(r"`((?:\.?[A-Za-z0-9_.\-]+/)+[A-Za-z0-9_.\-]*)`")
URL_TARGET_RE = re.compile(r"https?://[A-Za-z0-9_.\-]+(?:/[A-Za-z0-9_.\-/]*)?")
BLOCKED_MARKER_RE = re.compile(r"<!--\s*BLOCKED:.*?-->", re.IGNORECASE | re.DOTALL)
WORD_RE = re.compile(r"\b\w+\b")


def _find_project_root(start: Path) -> Path:
    current = start.resolve().parent if start.is_file() else start.resolve()
    while current != current.parent:
        if (current / ".claude").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return start.resolve().parent if start.is_file() else start.resolve()


def _declared_live_targets(project_root: Path) -> set[str]:
    """Hosts declared in rules/live-target.txt.

    A plan naming a live URL that no domain declares is planning a probe the cycle
    refuses to run (`cycle-discover.md`, gate G-L). Catching it here means the refusal
    lands while the plan is cheap to change.
    """
    for candidate in (
        project_root / "rules" / "live-target.txt",
        project_root / ".claude" / "rules" / "live-target.txt",
    ):
        if candidate.is_file():
            return {
                m.group(1)
                for m in re.finditer(
                    r"^\s*target\s*=\s*https?://([A-Za-z0-9_.\-]+)",
                    candidate.read_text(encoding="utf-8-sig"),
                    re.MULTILINE,
                )
            }
    return set()


def _is_explicitly_blocked(raw: str, match_end: int) -> bool:
    return bool(BLOCKED_MARKER_RE.search(raw[match_end : match_end + 80]))


def check_measurement_targets(plan_path: Path) -> dict[str, Any]:
    raw = plan_path.read_text(encoding="utf-8-sig")
    project_root = _find_project_root(plan_path)
    declared_hosts = _declared_live_targets(project_root)

    verified: set[str] = set()
    fabricated: dict[str, str] = {}
    blocked: set[str] = set()

    for match in PATH_TARGET_RE.finditer(raw):
        target = match.group(1)
        if _is_explicitly_blocked(raw, match.end()):
            blocked.add(target)
            continue
        if (project_root / target).exists():
            verified.add(target)
        else:
            fabricated[target] = "path_not_found"

    undeclared_hosts: list[str] = []
    live_targets: set[str] = set()
    for match in URL_TARGET_RE.finditer(raw):
        url = match.group(0)
        host = re.sub(r"^https?://", "", url).split("/")[0]
        live_targets.add(url)
        if declared_hosts and host not in declared_hosts:
            undeclared_hosts.append(host)

    total = len(verified) + len(fabricated)
    word_count = len(WORD_RE.findall(raw))
    density = ((total + len(live_targets)) * 200 / word_count) if word_count else 0.0

    contributors: list[str] = []
    if verified:
        contributors.append(f"{len(verified)} resolvable path target(s)")
    if live_targets and not undeclared_hosts:
        contributors.append(f"{len(live_targets)} live target(s), all declared")
    if blocked:
        contributors.append(f"{len(blocked)} explicitly BLOCKED target(s) (honest gaps)")

    detractors = [f"Unresolvable target: {t} ({why})" for t, why in sorted(fabricated.items())[:3]]
    detractors.extend(
        f"Live host not declared in rules/live-target.txt: {h}"
        for h in sorted(set(undeclared_hosts))[:2]
    )

    return {
        "total": total,
        "verified": len(verified),
        "fabricated": len(fabricated),
        "fabricated_targets": dict(sorted(fabricated.items())[:10]),
        "explicitly_blocked": len(blocked),
        "blocked_targets": sorted(blocked)[:10],
        "live_targets": sorted(live_targets)[:10],
        "undeclared_live_hosts": sorted(set(undeclared_hosts)),
        "word_count": word_count,
        "target_density_per_200w": round(density, 2),
        "contributors": contributors[:3],
        "detractors": detractors[:3],
    }
