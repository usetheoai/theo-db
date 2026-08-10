"""Evidence-pointer checker for /discover-execute opportunities (M2 deterministic).

Replaces the ancestor `check_reference_citations.py`, which verified citations into
`knowledge-base/references/` -- the prior-art study zone this cycle retired. Evidence
now points at OUR system, so the checker resolves two different pointer classes:

  1. Code pointers      `path/to/file.ext:LINE` -- deterministically verifiable.
  2. Runtime observations  `METHOD URL -> STATUS` -- NOT re-verifiable on disk.

The distinction is the honest part. A code pointer either resolves or it does not, and
saying so is a measurement. An HTTP observation against a dev environment is transient:
re-running it later can legitimately differ, and a checker that pretended otherwise
would be asserting rather than verifying. Runtime observations are therefore checked
STRUCTURALLY (is it recorded in a re-readable form?) and counted separately -- never
reported as "verified".

One capability the ancestor lacked: it stripped the `:LINE` suffix before testing the
path, so a pointer to line 400 of a 30-line file passed as verified. Evidence that
points past the end of a file is evidence that moved, and it is now caught.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any


# `dir/file.ext:LINE` (optionally `:COL`). Requires a slash and an extension so that
# prose like "step 3:12" or "Ratio 4:1" is not mistaken for a pointer.
CODE_POINTER_RE = re.compile(
    r"\b((?:[A-Za-z0-9_.\-]+/)+[A-Za-z0-9_.\-]+\.[A-Za-z0-9]{1,10}):(\d+)(?::\d+)?\b"
)
# `GET https://host/path -> 200`
RUNTIME_OBS_RE = re.compile(
    r"\b(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\s+(\S+)\s*(?:->|→)\s*(\d{3})\b"
)
BLOCKED_MARKER_RE = re.compile(r"<!--\s*BLOCKED:.*?-->", re.IGNORECASE | re.DOTALL)
WORD_RE = re.compile(r"\b\w+\b")


def _find_project_root(start: Path) -> Path:
    """Walk up from start looking for .claude/ or .git/."""
    current = start.resolve().parent if start.is_file() else start.resolve()
    while current != current.parent:
        if (current / ".claude").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return start.resolve().parent if start.is_file() else start.resolve()


def _word_count(content: str) -> int:
    return len(WORD_RE.findall(content))


def _is_explicitly_blocked(raw: str, match_end: int) -> bool:
    """Return True when a BLOCKED marker follows the pointer within ~80 chars.

    Pointers explicitly marked `<!-- BLOCKED: ... -->` are documented gaps, not
    fabrications. They belong to the honest blocked-questions audit trail.
    """
    return bool(BLOCKED_MARKER_RE.search(raw[match_end : match_end + 80]))


def _resolve_code_pointer(project_root: Path, rel_path: str, line: int) -> tuple[bool, str]:
    """Return (ok, reason). Reason is '' when ok."""
    path = project_root / rel_path
    if not path.is_file():
        return False, "missing_file"
    try:
        total_lines = sum(1 for _ in path.open("r", encoding="utf-8", errors="replace"))
    except OSError as e:
        # An unreadable file is a real, reportable condition -- not silently "fine".
        return False, f"unreadable: {e.strerror or e.__class__.__name__}"
    if line < 1 or line > total_lines:
        return False, f"line_out_of_range (file has {total_lines} lines)"
    return True, ""


def check_evidence_pointers(opportunity_path: Path) -> dict[str, Any]:
    raw = opportunity_path.read_text(encoding="utf-8-sig")
    project_root = _find_project_root(opportunity_path)

    verified: set[str] = set()
    fabricated: dict[str, str] = {}
    blocked: set[str] = set()

    for match in CODE_POINTER_RE.finditer(raw):
        pointer = match.group(0)
        if _is_explicitly_blocked(raw, match.end()):
            blocked.add(pointer)
            continue
        ok, reason = _resolve_code_pointer(project_root, match.group(1), int(match.group(2)))
        if ok:
            verified.add(pointer)
        else:
            fabricated[pointer] = reason

    runtime_observations = [
        f"{m.group(1)} {m.group(2)} -> {m.group(3)}" for m in RUNTIME_OBS_RE.finditer(raw)
    ]

    total = len(verified) + len(fabricated)
    word_count = _word_count(raw)
    evidence_total = total + len(runtime_observations)
    density = (evidence_total * 200 / word_count) if word_count > 0 else 0.0

    contributors: list[str] = []
    if verified:
        contributors.append(f"{len(verified)} verified code pointer(s)")
    if runtime_observations:
        contributors.append(
            f"{len(runtime_observations)} recorded runtime observation(s) (not re-verifiable)"
        )
    if blocked:
        contributors.append(f"{len(blocked)} explicitly BLOCKED pointer(s) (honest gaps)")

    detractors = [
        f"Unresolvable pointer: {p} ({reason})" for p, reason in sorted(fabricated.items())[:3]
    ]

    return {
        "total": total,
        "verified": len(verified),
        "fabricated": len(fabricated),
        "fabricated_pointers": dict(sorted(fabricated.items())[:10]),
        "explicitly_blocked": len(blocked),
        "blocked_pointers": sorted(blocked)[:10],
        "runtime_observations": len(runtime_observations),
        "runtime_observation_samples": runtime_observations[:5],
        "evidence_total": evidence_total,
        "word_count": word_count,
        "evidence_density_per_200w": round(density, 2),
        "contributors": contributors,
        "detractors": detractors,
    }
