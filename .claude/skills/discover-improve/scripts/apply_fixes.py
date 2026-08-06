#!/usr/bin/env python3
"""Apply deterministic discover-improve fixes to an opportunity.

Idempotent. Same input always produces same output. Cost: $0.

WHAT IT FIXES — prose smells, and only inside `## Recommendation`:
  1. Weak imperatives: should/could -> must/can
  2. Loopholes stripped: "if possible", "as appropriate", "when applicable",
     "where feasible"

WHAT IT REPORTS BUT NEVER TOUCHES — unresolvable evidence pointers.

Two deliberate restrictions, both narrower than the ancestor. They are the point of
this module, so they are documented here rather than in a commit nobody re-reads.

## 1. Only `## Recommendation` is rewritten

An opportunity is part RECORD and part PROSE. `## Corner 1 — Evidence` records what was
measured; `## Recommendation` argues what to do about it. Rewriting the recommendation is
editing an argument. Rewriting the evidence is editing the record of a measurement, and a
regex cannot tell a weak imperative from a factual one:

    "the endpoint may return 500 under load"   (a measured fact)
    "the endpoint must return 500 under load"  (a different, false claim)

The ancestor rewrote the whole document. Against a prior-art blueprint that was merely
sloppy; against a measurement record it silently falsifies findings. `may` and `might` are
therefore no longer substituted at all — in descriptive prose they are correct, and no
regex distinguishes description from prescription.

## 2. The automatic BLOCKED marker is REMOVED, permanently

The ancestor marked every unresolvable citation with `<!-- BLOCKED: ... -->`. Measured
against the current checker (2026-08-05): a marked pointer moves out of `fabricated` and
into `explicitly_blocked`, and `fabricated_evidence` stops firing.

That makes the marker an automated bypass of the cycle's most important hard cap. A script
would turn an INVALID opportunity into a passing one without anyone measuring anything —
and `fabricated_evidence` exists precisely because everything downstream treats a pointer
as measured fact.

A pointer that does not resolve means someone invented it, or the code moved. Both need a
human or a re-measurement, never a marker applied in bulk. So this script REPORTS them and
changes nothing. `test_apply_fixes.py::test_never_writes_a_blocked_marker` locks it shut.
"""
from __future__ import annotations

import argparse
import json as _json
import re
import sys
from pathlib import Path
from typing import Any


# `dir/file.ext:LINE` — same shape the confidence checker resolves.
CODE_POINTER_RE = re.compile(
    r"\b((?:[A-Za-z0-9_.\-]+/)+[A-Za-z0-9_.\-]+\.[A-Za-z0-9]{1,10}):(\d+)(?::\d+)?\b"
)
RECOMMENDATION_RE = re.compile(r"^##\s+Recommendation\b.*?(?=^##\s|\Z)", re.MULTILINE | re.DOTALL)
# `may`/`might` deliberately absent — see module docstring.
WEAK_IMPERATIVES_RE = re.compile(r"\b(should|could)\b", re.IGNORECASE | re.UNICODE)
LOOPHOLES_RE = re.compile(
    r"\b(if possible|as appropriate|when applicable|where feasible)\b",
    re.IGNORECASE | re.UNICODE,
)
FENCED_CODE_RE = re.compile(r"^```[^\n]*\n.*?^```", re.MULTILINE | re.DOTALL)
BLOCKED_MARKER_RE = re.compile(r"<!--\s*BLOCKED:.*?-->", re.IGNORECASE | re.DOTALL)

REPLACEMENT_MAP = {"should": "must", "could": "can"}


def _find_project_root(start: Path) -> Path:
    current = start.resolve().parent if start.is_file() else start.resolve()
    while current != current.parent:
        if (current / ".claude").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return start.resolve().parent if start.is_file() else start.resolve()


def _split_code_and_prose(content: str) -> list[tuple[bool, str]]:
    """Split into alternating (is_code, chunk) tuples so fences are never rewritten."""
    chunks: list[tuple[bool, str]] = []
    last = 0
    for match in FENCED_CODE_RE.finditer(content):
        if match.start() > last:
            chunks.append((False, content[last : match.start()]))
        chunks.append((True, match.group(0)))
        last = match.end()
    if last < len(content):
        chunks.append((False, content[last:]))
    return chunks


def _fix_weak_imperatives(prose: str) -> tuple[str, int]:
    counter = 0

    def replacer(m: re.Match[str]) -> str:
        nonlocal counter
        repl = REPLACEMENT_MAP.get(m.group(0).lower())
        if repl is None:
            return m.group(0)
        counter += 1
        return repl

    fixed = WEAK_IMPERATIVES_RE.sub(replacer, prose)
    return re.sub(r"  +", " ", fixed), counter


def _fix_loopholes(prose: str) -> tuple[str, int]:
    counter = 0

    def replacer(_m: re.Match[str]) -> str:
        nonlocal counter
        counter += 1
        return ""

    fixed = LOOPHOLES_RE.sub(replacer, prose)
    fixed = re.sub(r"  +", " ", fixed)
    return re.sub(r",\s*,", ",", fixed), counter


def _rewrite_recommendation(content: str) -> tuple[str, int, int]:
    """Apply prose fixes to `## Recommendation` only. Everything else is untouched."""
    match = RECOMMENDATION_RE.search(content)
    if not match:
        return content, 0, 0

    weak_total = 0
    loop_total = 0
    new_chunks: list[str] = []
    for is_code, chunk in _split_code_and_prose(match.group(0)):
        if is_code:
            new_chunks.append(chunk)
            continue
        fixed, wc = _fix_weak_imperatives(chunk)
        fixed, lc = _fix_loopholes(fixed)
        new_chunks.append(fixed)
        weak_total += wc
        loop_total += lc

    section = "".join(new_chunks)
    return content[: match.start()] + section + content[match.end() :], weak_total, loop_total


def _report_unresolvable_pointers(content: str, opportunity_path: Path) -> list[dict[str, str]]:
    """Find pointers that do not resolve. REPORT ONLY — never annotate, never rewrite."""
    project_root = _find_project_root(opportunity_path)
    findings: list[dict[str, str]] = []
    seen: set[str] = set()

    for match in CODE_POINTER_RE.finditer(content):
        pointer = match.group(0)
        if pointer in seen:
            continue
        seen.add(pointer)
        # An already-BLOCKED pointer was marked by a human or by the measurement; it is a
        # documented gap, not a finding for this script to re-raise.
        if BLOCKED_MARKER_RE.search(content[match.end() : match.end() + 80]):
            continue

        path = project_root / match.group(1)
        line = int(match.group(2))
        if not path.is_file():
            findings.append({"pointer": pointer, "reason": "missing_file"})
            continue
        try:
            total = sum(1 for _ in path.open("r", encoding="utf-8", errors="replace"))
        except OSError as e:
            findings.append({"pointer": pointer, "reason": f"unreadable: {e.strerror or type(e).__name__}"})
            continue
        if line < 1 or line > total:
            findings.append(
                {"pointer": pointer, "reason": f"line_out_of_range (file has {total} lines)"}
            )
    return findings


def apply_fixes(opportunity_path: Path, dry_run: bool = False) -> dict[str, Any]:
    original = opportunity_path.read_text(encoding="utf-8-sig")

    new_content, weak_count, loop_count = _rewrite_recommendation(original)
    unresolvable = _report_unresolvable_pointers(new_content, opportunity_path)

    changed = new_content != original
    if not dry_run and changed:
        opportunity_path.write_text(new_content, encoding="utf-8")

    return {
        "opportunity": str(opportunity_path),
        "weak_imperatives_fixed": weak_count,
        "loopholes_stripped": loop_count,
        "unresolvable_pointers": unresolvable,
        "unresolvable_pointer_count": len(unresolvable),
        "changed": changed,
        "dry_run": dry_run,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Apply deterministic discover-improve fixes.")
    parser.add_argument("opportunity", type=Path, help="path to opportunity .md")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if not args.opportunity.exists():
        print(f"Opportunity not found: {args.opportunity}", file=sys.stderr)
        return 2

    result = apply_fixes(args.opportunity, dry_run=args.dry_run)

    if args.json:
        print(_json.dumps(result, indent=2))
    else:
        print(f"Opportunity: {result['opportunity']}")
        print(f"Weak imperatives fixed (## Recommendation only): {result['weak_imperatives_fixed']}")
        print(f"Loopholes stripped (## Recommendation only): {result['loopholes_stripped']}")
        print(f"Unresolvable pointers: {result['unresolvable_pointer_count']}")
        for finding in result["unresolvable_pointers"]:
            print(f"  - {finding['pointer']} ({finding['reason']})")
        if result["unresolvable_pointers"]:
            print(
                "\nNOT auto-fixed, by design. A pointer that does not resolve means someone\n"
                "invented it or the code moved. Both need a human or a re-measurement —\n"
                "annotating them in bulk would disarm the fabricated_evidence hard cap."
            )
        print(f"Changed: {result['changed']}")
        if result["dry_run"]:
            print("(dry-run: no changes written)")

    # Exit 3 signals "cannot be auto-fixed" so a halt-loop stops instead of iterating.
    return 3 if result["unresolvable_pointers"] else 0


if __name__ == "__main__":
    sys.exit(main())
