#!/usr/bin/env python3
"""Route a repo (or a B-NNN item) to its domain specialist.

Deterministic, not heuristic. `/review`'s `detect_domain.py` guesses a technical domain
from keywords because it runs against an arbitrary plan. Here the item already declares
`repo:`, and a repo belongs to exactly one domain — so guessing would only add a way to
be wrong.

The routing table is PARSED from `rules/cycle-backlog.md § Domain routing` rather than
duplicated here. One table, one truth: a copy in code drifts from the rule the moment
someone edits one of them, and the drift is silent — work routes to a specialist who
cannot open the repo, and nothing errors.

Exit codes:
  0 — routed
  1 — repo not in the routing table (gate G1 refuses the item)
  2 — the routing table could not be read or parsed
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


ROW_RE = re.compile(r"^\|\s*`([a-z0-9-]+)`\s*\|(.+?)\|(.+?)\|\s*$", re.MULTILINE)
# `/` is allowed so a repo split across domains can be addressed by path
# (`theo-cloud/dashboard`). Without it that row parsed to an EMPTY repo list and the
# domain became silently unreachable — every other check still passed.
REPO_RE = re.compile(r"`([A-Za-z0-9_./-]+)`")
AGENT_RE = re.compile(r"`(agents/[a-z0-9-]+\.md)`")
ITEM_REPO_RE = re.compile(r"^repo:\s*`?([A-Za-z0-9_.-]+)`?", re.MULTILINE)


def _find_project_root(start: Path) -> Path:
    current = start.resolve() if start.is_dir() else start.resolve().parent
    while current != current.parent:
        if (current / "rules").is_dir() or (current / ".git").exists():
            return current
        current = current.parent
    return start.resolve()


def _routing_table_path(project_root: Path) -> Path | None:
    for candidate in (
        project_root / "rules" / "cycle-backlog.md",
        project_root / ".claude" / "rules" / "cycle-backlog.md",
    ):
        if candidate.is_file():
            return candidate
    return None


def parse_routing_table(rule_path: Path) -> dict[str, dict[str, Any]]:
    """Return {domain: {"repos": [...], "agent": "agents/x.md"}} from the rule's table."""
    content = rule_path.read_text(encoding="utf-8-sig")

    section = re.search(
        r"^##\s+Domain routing\b(.*?)(?=^##\s|\Z)", content, re.MULTILINE | re.DOTALL
    )
    if not section:
        raise ValueError(f"{rule_path}: no '## Domain routing' section")

    table: dict[str, dict[str, Any]] = {}
    for match in ROW_RE.finditer(section.group(1)):
        domain, repos_cell, agent_cell = match.groups()
        if domain in {"domain"}:  # header row, if it ever gets backticked
            continue
        agent = AGENT_RE.search(agent_cell)
        table[domain] = {
            "repos": REPO_RE.findall(repos_cell),
            "agent": agent.group(1) if agent else None,
        }

    if not table:
        raise ValueError(f"{rule_path}: '## Domain routing' parsed to zero rows")
    return table


def route(repo: str, table: dict[str, dict[str, Any]]) -> tuple[str, str | None] | None:
    for domain, entry in table.items():
        if repo in entry["repos"]:
            return domain, entry["agent"]
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Route a repo to its domain specialist.")
    parser.add_argument("target", help="repo name, or a path to a B-NNN item file")
    parser.add_argument("--rule", type=Path, default=None, help="override the routing table path")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    project_root = _find_project_root(Path(__file__))
    rule_path = args.rule or _routing_table_path(project_root)
    if rule_path is None or not rule_path.is_file():
        print("FATAL: rules/cycle-backlog.md not found — cannot route", file=sys.stderr)
        return 2

    try:
        table = parse_routing_table(rule_path)
    except ValueError as e:
        print(f"FATAL: {e}", file=sys.stderr)
        return 2

    repo = args.target
    candidate = Path(args.target)
    if candidate.is_file():
        found = ITEM_REPO_RE.search(candidate.read_text(encoding="utf-8-sig"))
        if not found:
            print(f"FATAL: no `repo:` field in {candidate}", file=sys.stderr)
            return 2
        repo = found.group(1)

    result = route(repo, table)
    if result is None:
        known = sorted(r for entry in table.values() for r in entry["repos"])
        payload = {"repo": repo, "routed": False, "known_repos": known}
        if args.json:
            print(json.dumps(payload, indent=2))
        else:
            print(f"UNROUTED: `{repo}` is not in the routing table.")
            print("An item nobody owns is an item nobody does — gate G1 refuses it.")
            print(f"Known repos: {', '.join(known)}")
        return 1

    domain, agent = result
    payload = {"repo": repo, "routed": True, "domain": domain, "agent": agent}
    if args.json:
        print(json.dumps(payload, indent=2))
    else:
        print(f"repo   : {repo}")
        print(f"domain : {domain}")
        print(f"agent  : {agent or '(none declared)'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
