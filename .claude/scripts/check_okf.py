#!/usr/bin/env python3
"""Structural validator for the OKF knowledge bundle (rules/okf-knowledge-base.md).

Deterministic and side-effect free. It answers ONE question: is the bundle
structurally honest? It deliberately does NOT judge content quality — a checker
that pretends to grade prose would be exactly the "cobertura alegada sem
execucao" failure mode the bundle documents.

Five checks. C1-C4 have zero false-positive surface. C5 compares a normalized
string against a closed set — it strips an inline YAML comment and surrounding
quotes first, because `type: "Failure Mode"` and `type: Failure Mode  # note`
are both legal YAML that a naive match would reject:

  C1  every concept file declares `type` in its YAML frontmatter
      (OKF v0.1 requires exactly one field, and this is it)
  C2  every internal markdown link resolves on disk
      (the bundle preaches "citacao que nao resolve nao entra" — it must comply)
  C3  each directory's index.md lists EXACTLY the concepts present
      (an index that drifts is worse than no index: it hides concepts)
  C4  the required root files exist (index.md, log.md)
  C6  every `resource:` in the frontmatter resolves on disk when it is a repo path
      (this is the hole the 2026-07-30 review found: C2 validates markdown links only,
      so a bad path in `resource:` — where `rules/reference-provenance.md` and a
      truncated `docs/adr/0035` both hid — was checked by nothing)
  C5  `type` is one of the taxonomy values declared in rules/okf-knowledge-base.md § 2
      (C1 only checks PRESENCE; a value outside the closed set is a sixth type, which
      the LOCKED clause requires an ADR for — and the front door had exactly that bug)

Exit codes:
  0  bundle is structurally valid
  1  at least one check failed (findings printed)
  2  invocation error (bundle missing, unreadable)

Usage:
  python3 .claude/scripts/check_okf.py [--bundle PATH] [--json]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

DEFAULT_BUNDLE = ".claude/knowledge-base/okf"
RESERVED = {"index.md", "log.md"}
# Closed set from rules/okf-knowledge-base.md § 2 (LOCKED). Adding a value here requires an ADR.
VALID_TYPES = {"Failure Mode", "Technique", "Invariant", "Measurement", "Honest Negative", "Index", "Log"}

TYPE_RE = re.compile(r"^type:\s*(\S.*)$", re.M)
RESOURCE_RE = re.compile(r"^resource:\s*(\S.*)$", re.M)
# `resource:` values that are NOT repo paths and must not be probed.
_NON_PATH = ("http://", "https://", "mailto:")
# Study material is gitignored by design (CLAUDE.md) — absence is not a defect.
_GITIGNORED_PREFIXES = ("references/", "knowledge-base/references/")
LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
# An index row is a markdown table row whose first cell is a link: | [Title](file.md) | ... |
INDEX_ROW_RE = re.compile(r"^\|\s*\[[^\]]*\]\(([^)]+\.md)\)\s*\|", re.M)


def _normalize_type(raw: str) -> str:
    """Normalize a frontmatter `type` value the way a YAML parser would.

    `type: Failure Mode  # note` and `type: "Failure Mode"` are both legal YAML that
    resolve to `Failure Mode`. Comparing the raw capture against the closed set would
    reject them — a false positive on a legal file, which C5 must not have.
    """
    t = raw.split(" #", 1)[0].strip()
    if len(t) >= 2 and t[0] == t[-1] and t[0] in "\"'":
        t = t[1:-1].strip()
    return t


def frontmatter(text: str) -> str | None:
    """Return the YAML frontmatter block, or None when the file has none."""
    if not text.startswith("---"):
        return None
    parts = text.split("---", 2)
    return parts[1] if len(parts) >= 3 else None


def concept_files(directory: Path) -> list[Path]:
    return sorted(p for p in directory.glob("*.md") if p.name not in RESERVED)


def check(bundle: Path) -> tuple[list[str], dict]:
    findings: list[str] = []
    stats = {"concepts": 0, "links_ok": 0, "indexes": 0, "types": {}}

    if not bundle.is_dir():
        raise FileNotFoundError(f"OKF bundle not found at {bundle}")

    all_md = sorted(bundle.rglob("*.md"))

    # C4 — required root files
    for required in ("index.md", "log.md"):
        if not (bundle / required).is_file():
            findings.append(f"C4 missing-root-file: {required} is required by the bundle contract")

    for path in all_md:
        rel = path.relative_to(bundle)
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:  # unreadable file is a structural problem, not a content one
            findings.append(f"C0 unreadable: {rel} ({exc})")
            continue

        # C1 — `type` is OKF's single mandatory field
        fm = frontmatter(text)
        if fm is None:
            findings.append(f"C1 no-frontmatter: {rel} has no YAML frontmatter block")
        else:
            m = TYPE_RE.search(fm)
            if not m:
                findings.append(f"C1 no-type: {rel} frontmatter declares no `type`")
            else:
                t = _normalize_type(m.group(1))
                stats["types"][t] = stats["types"].get(t, 0) + 1
                # C5 — the value must be in the closed taxonomy, not merely present
                if t not in VALID_TYPES:
                    findings.append(
                        f"C5 type-outside-taxonomy: {rel} declares `type: {t}`, which is not one of "
                        f"{sorted(VALID_TYPES)} (rules/okf-knowledge-base.md § 2 is LOCKED; a new type needs an ADR)"
                    )

            # C6 — a `resource:` that points at a repo path must resolve
            mr = RESOURCE_RE.search(fm)
            if mr:
                res = _normalize_type(mr.group(1))          # same quote/comment stripping
                res = res.split(" (", 1)[0].strip()          # trailing gloss, e.g. "path (umbrella)"
                res = res.split("#", 1)[0].strip()           # section anchor, as C2 already strips for links
                if res and not res.startswith(_NON_PATH) and "/" in res:
                    if not any(res.startswith(g) or f"/{g}" in res for g in _GITIGNORED_PREFIXES):
                        # Duas bases DECLARADAS, e só elas: a raiz do repo (docs/, theodb_rs/, benchmarks/) e
                        # `.claude/` (rules/, knowledge-base/). Uma terceira base de fallback tolerava caminho
                        # que nenhum leitor resolve — foi assim que `../../../.claude/rules/...` passou.
                        candidates = [Path(res), bundle.parent.parent / res]
                        if not any(c.exists() for c in candidates):
                            findings.append(
                                f"C6 resource-not-found: {rel} declares `resource: {res}`, which does not "
                                "resolve against the repo root or .claude/ (a citation that does not resolve "
                                "does not belong in this bundle — the bundle's own rule)"
                            )

        if path.name not in RESERVED:
            stats["concepts"] += 1

        # C2 — every internal link resolves
        for link in LINK_RE.findall(text):
            target = link.split("#", 1)[0].strip()
            if not target or target.startswith(("http://", "https://", "mailto:")):
                continue
            if (path.parent / target).resolve().exists():
                stats["links_ok"] += 1
            else:
                findings.append(f"C2 broken-link: {rel} -> {link}")

    # C3 — every index lists exactly the concepts beside it
    for index_path in sorted(bundle.rglob("index.md")):
        directory = index_path.parent
        if directory == bundle:
            continue  # the root index is prose + directory links, not a concept table
        stats["indexes"] += 1
        listed = set(INDEX_ROW_RE.findall(index_path.read_text(encoding="utf-8")))
        present = {p.name for p in concept_files(directory)}
        rel_index = index_path.relative_to(bundle)
        for missing in sorted(present - listed):
            findings.append(
                f"C3 concept-not-indexed: {rel_index} does not list {missing} "
                "(an unlisted concept is invisible to a reader walking the bundle)"
            )
        for phantom in sorted(listed - present):
            findings.append(f"C3 index-phantom: {rel_index} lists {phantom}, which does not exist")

    return findings, stats


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bundle", default=DEFAULT_BUNDLE, help=f"bundle root (default: {DEFAULT_BUNDLE})")
    ap.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    args = ap.parse_args()

    try:
        findings, stats = check(Path(args.bundle))
    except FileNotFoundError as exc:
        if args.json:
            print(json.dumps({"ok": False, "error": str(exc)}))
        else:
            print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    ok = not findings
    if args.json:
        print(json.dumps({"ok": ok, "findings": findings, "stats": stats}, ensure_ascii=False, indent=1))
    else:
        by_type = " · ".join(f"{k}: {v}" for k, v in sorted(stats["types"].items()))
        print(f"OKF bundle: {stats['concepts']} concepts, {stats['links_ok']} internal links resolved, "
              f"{stats['indexes']} category indexes")
        print(f"  types: {by_type}")
        if ok:
            print("  GATE OK — structurally valid.")
        else:
            print(f"\n  {len(findings)} FINDING(S):")
            for f in findings:
                print(f"    {f}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
