#!/usr/bin/env python3
"""Deterministic structural review of BACKLOG.md.

Sibling of the retired `check_roadmap_structure.py`. Written fresh rather than ported:
a roadmap is a finite, ordered, dependency-linked scope, and a backlog is none of those.
Cycle detection, the M0-M8 cap and ordering checks have no meaning over independent
items, so carrying them across would have produced checks that always pass.

What it checks instead — the ways a maintenance registry actually rots:

  DETERMINISTIC (a machine can be sure)
    duplicate_id            two blocks share a B-NNN — the audit trail is broken
    malformed_block         a header the loop cannot parse
    missing_field           a required field absent
    illegal_status          a status outside the declared set
    killed_without_reason   killed with no kill_reason (gate G-K, after the fact)
    triaged_without_evidence  triaged but evidence is still none-yet
    raw_with_evidence       raw but carrying evidence — status never advanced
    unroutable_repo         repo in no domain (gate G1)
    invalid_mode            suggested_mode outside the four
    renumbered              ids not monotonic — a reused id destroys traceability

  HEURISTIC (a reader decides; labelled as such in every finding)
    vague_dod               a DoD bullet nothing could falsify
    thin_dod                zero DoD bullets
    stale_raw               raw for longer than the staleness window
    possible_duplicate      two open items whose titles overlap heavily

Every finding declares `kind` (deterministic | heuristic). The verdict is DERIVED from
the findings, never asserted — the same discipline the confidence scorers follow.

Exit codes: 0 SHIPPABLE / SHIPPABLE_WITH_CAVEATS · 1 INVALID · 3 NEEDS_REVISION
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from datetime import date, datetime
from pathlib import Path
from typing import Any


BLOCK_RE = re.compile(r"^##\s+(B-\d+)\s+—\s+(.+?)\s*(?:\[( |x)\])?\s*$", re.MULTILINE)
FIELD_RE = re.compile(r"^([a-z_]+):\s*(.*)$", re.MULTILINE)
DOD_BULLET_RE = re.compile(r"^\s*-\s+(.+)$", re.MULTILINE)
REGISTERED_RE = re.compile(r"Registrado\s+(\d{4}-\d{2}-\d{2})|registered\s+(\d{4}-\d{2}-\d{2})", re.I)

REQUIRED_FIELDS = ("domain", "repo", "suggested_mode", "source", "evidence", "why_now", "status")
LEGAL_STATUS = {"raw", "triaged", "planned", "shipped", "killed"}
LEGAL_MODES = {"review", "live-test", "bug", "evolve"}
OPEN_STATUS = {"raw", "triaged", "planned"}

# A DoD bullet built only from these cannot fail, so it cannot close an item.
VAGUE_TERMS = {
    "improve", "improved", "better", "faster", "fast", "scalable", "robust", "clean",
    "cleanup", "clean up", "optimize", "optimise", "enhance", "polish", "reliable",
    "performant", "best practices", "as needed", "etc", "and so on", "properly",
    "correctly", "appropriately", "melhorar", "melhor", "mais rápido", "adequado",
}
STALE_RAW_DAYS = 90


@dataclass
class Finding:
    check: str
    kind: str  # deterministic | heuristic
    severity: str  # blocker | major | minor
    item: str
    message: str


@dataclass
class Item:
    item_id: str
    title: str
    fields: dict[str, str] = field(default_factory=dict)
    dod: list[str] = field(default_factory=list)
    registered_on: date | None = None
    line: int = 0


def _parse_items(content: str) -> list[Item]:
    items: list[Item] = []
    matches = list(BLOCK_RE.finditer(content))
    for i, match in enumerate(matches):
        start = match.end()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(content)
        body = content[start:end]

        item = Item(
            item_id=match.group(1),
            title=match.group(2).strip(),
            line=content[: match.start()].count("\n") + 1,
        )
        for fmatch in FIELD_RE.finditer(body):
            item.fields[fmatch.group(1)] = fmatch.group(2).strip()

        dod_match = re.search(r"^dod:\s*$(.*?)(?=^[a-z_]+:|\Z)", body, re.MULTILINE | re.DOTALL)
        if dod_match:
            item.dod = [b.strip() for b in DOD_BULLET_RE.findall(dod_match.group(1)) if b.strip()]

        reg = REGISTERED_RE.search(body)
        if reg:
            raw = reg.group(1) or reg.group(2)
            try:
                item.registered_on = datetime.strptime(raw, "%Y-%m-%d").date()
            except ValueError:
                pass
        items.append(item)
    return items


def _is_vague(bullet: str) -> bool:
    """True when a bullet carries no falsifiable content.

    A bullet with a number, a comparison or a concrete artifact is treated as
    falsifiable even if it also contains a vague word — "p95 below 800ms" is a criterion
    that happens to mention speed, and flagging it would train people to ignore the check.
    """
    lowered = bullet.lower()
    if re.search(r"\d", lowered) or "`" in bullet:
        return False
    return any(term in lowered for term in VAGUE_TERMS)


def _title_overlap(a: str, b: str) -> float:
    wa = {w for w in re.findall(r"\w+", a.lower()) if len(w) > 3}
    wb = {w for w in re.findall(r"\w+", b.lower()) if len(w) > 3}
    if not wa or not wb:
        return 0.0
    return len(wa & wb) / min(len(wa), len(wb))


def _known_repos(backlog_dir: Path) -> set[str] | None:
    """Repos the routing table knows. None when the table cannot be read.

    None is not an empty set: an unreadable table means we cannot judge routing, and
    reporting every repo as unroutable from missing data would assert a violation the
    evidence does not support.

    `route_domain` is located relative to THIS FILE, not to the backlog. In real use the
    registry sits at the umbrella root while the tooling lives under `.claude/scripts/`,
    so resolving the importer against the backlog's directory finds nothing — and a bare
    `except Exception` around the import would swallow that into a silent None. The check
    would then never run while the report looked healthy.
    """
    tooling = Path(__file__).resolve().parents[3] / "scripts"
    if str(tooling) not in sys.path:
        sys.path.insert(0, str(tooling))
    try:
        from route_domain import _routing_table_path, parse_routing_table  # noqa: PLC0415
    except ImportError:
        # The routing tool is genuinely unavailable — report inability, never a violation.
        return None

    rule = _routing_table_path(backlog_dir)
    if rule is None:
        return None
    try:
        return {r for entry in parse_routing_table(rule).values() for r in entry["repos"]}
    except ValueError:
        # A malformed table is a real problem, but it is `backlog-review`'s job to review
        # items, not the rule. Decline to judge routing rather than blame every item.
        return None


def check_backlog(backlog_path: Path, today: date | None = None) -> dict[str, Any]:
    today = today or date.today()
    content = backlog_path.read_text(encoding="utf-8-sig")
    items = _parse_items(content)
    findings: list[Finding] = []

    project_root = backlog_path.resolve().parent
    known_repos = _known_repos(project_root)

    seen_ids: dict[str, Item] = {}
    numeric_ids: list[int] = []

    for item in items:
        iid = item.item_id

        if iid in seen_ids:
            findings.append(Finding("duplicate_id", "deterministic", "blocker", iid,
                f"`{iid}` appears twice (lines {seen_ids[iid].line} and {item.line}). "
                "Ids are the audit trail; two blocks sharing one destroys it."))
        seen_ids[iid] = item
        numeric_ids.append(int(iid.split("-")[1]))

        for required in REQUIRED_FIELDS:
            if required not in item.fields:
                findings.append(Finding("missing_field", "deterministic", "major", iid,
                    f"`{required}` is absent"))

        status = item.fields.get("status", "")
        if status and status not in LEGAL_STATUS:
            findings.append(Finding("illegal_status", "deterministic", "blocker", iid,
                f"status `{status}` is outside {sorted(LEGAL_STATUS)}"))

        mode = item.fields.get("suggested_mode", "")
        if mode and mode not in LEGAL_MODES:
            findings.append(Finding("invalid_mode", "deterministic", "major", iid,
                f"suggested_mode `{mode}` is outside {sorted(LEGAL_MODES)}"))

        evidence = item.fields.get("evidence", "")
        if status == "killed" and not item.fields.get("kill_reason"):
            findings.append(Finding("killed_without_reason", "deterministic", "major", iid,
                "killed with no kill_reason — indistinguishable from an abandoned run (gate G-K)"))
        if status == "triaged" and evidence in ("", "none-yet"):
            findings.append(Finding("triaged_without_evidence", "deterministic", "blocker", iid,
                "triaged but evidence is still `none-yet`. Triaged means measured; "
                "without evidence the status is a claim nobody made."))
        if status == "raw" and evidence not in ("", "none-yet"):
            findings.append(Finding("raw_with_evidence", "deterministic", "major", iid,
                f"raw but carries evidence (`{evidence}`) — measurement happened and the "
                "status was never advanced"))

        repo = item.fields.get("repo", "")
        if repo and known_repos is not None and repo not in known_repos:
            findings.append(Finding("unroutable_repo", "deterministic", "blocker", iid,
                f"`{repo}` is in no domain — the item routes to nobody (gate G1)"))

        if not item.dod:
            findings.append(Finding("thin_dod", "heuristic", "major", iid,
                "no DoD bullet — nothing states when this item is done, so it never closes"))
        else:
            for bullet in item.dod:
                if _is_vague(bullet):
                    findings.append(Finding("vague_dod", "heuristic", "minor", iid,
                        f"DoD bullet has nothing falsifiable: \"{bullet}\""))

        if status == "raw" and item.registered_on:
            age = (today - item.registered_on).days
            if age > STALE_RAW_DAYS:
                findings.append(Finding("stale_raw", "heuristic", "minor", iid,
                    f"raw for {age} days. Either it matters and nobody measured it, or it "
                    "does not and it should be killed with that as the reason."))

    if numeric_ids and numeric_ids != sorted(numeric_ids):
        findings.append(Finding("renumbered", "deterministic", "blocker", "-",
            "ids are not monotonic. Ids are never reused and never reordered — a reused "
            "id makes every earlier reference ambiguous."))

    open_items = [i for i in items if i.fields.get("status") in OPEN_STATUS]
    for idx, a in enumerate(open_items):
        for b in open_items[idx + 1 :]:
            if _title_overlap(a.title, b.title) >= 0.6:
                findings.append(Finding("possible_duplicate", "heuristic", "minor", a.item_id,
                    f"title overlaps heavily with {b.item_id} (\"{b.title}\") — the intake "
                    "dedup may have missed it"))

    counts = {"blocker": 0, "major": 0, "minor": 0}
    for f in findings:
        counts[f.severity] += 1

    if counts["blocker"]:
        verdict = "INVALID"
    elif counts["major"]:
        verdict = "NEEDS_REVISION"
    elif counts["minor"]:
        verdict = "SHIPPABLE_WITH_CAVEATS"
    else:
        verdict = "SHIPPABLE"

    return {
        "backlog": str(backlog_path),
        "items_total": len(items),
        "items_by_status": {
            s: sum(1 for i in items if i.fields.get("status") == s) for s in sorted(LEGAL_STATUS)
        },
        "routing_table_read": known_repos is not None,
        "findings": [f.__dict__ for f in findings],
        "severity_counts": counts,
        "verdict": verdict,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Deterministic structural review of BACKLOG.md.")
    parser.add_argument("backlog", type=Path, nargs="?", default=Path("BACKLOG.md"))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if not args.backlog.exists():
        print(f"BACKLOG.md not found at {args.backlog} — run /backlog-init first", file=sys.stderr)
        return 2

    report = check_backlog(args.backlog)

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"Backlog : {report['backlog']}")
        print(f"Items   : {report['items_total']}  {report['items_by_status']}")
        if not report["routing_table_read"]:
            print("WARN    : routing table unreadable — repo routing was NOT checked")
        for f in report["findings"]:
            print(f"  [{f['severity'].upper()}/{f['kind'][:4]}] {f['item']} {f['check']}: {f['message']}")
        print(f"\nVerdict : {report['verdict']}  {report['severity_counts']}")

    return {"INVALID": 1, "NEEDS_REVISION": 3}.get(report["verdict"], 0)


if __name__ == "__main__":
    sys.exit(main())
