#!/usr/bin/env python3
"""Programmatic grading for the backlog-item eval battery.

The skill-creator's guidance is to script what can be scripted: a script is faster,
reproducible across iterations, and cannot talk itself into a pass. Assertions that need
judgement (did the skill ASK for evidence? was the reason explained?) are left to a
reading pass and marked `needs_review` here rather than guessed.

Emits grading.json per run, in the exact shape the viewer expects: an `expectations`
array of {text, passed, evidence}.

Usage:
    python3 grade_squad_backlog_item.py <iteration-dir>
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

BASE_IDS = {"B-007", "B-009", "B-014", "B-018", "B-021", "B-022", "B-025", "B-030", "B-031"}
ID_RE = re.compile(r"^## (B-\d+) — (.+?)\s*\[", re.MULTILINE)


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8", errors="replace") if p.is_file() else ""


def _blocks(text: str) -> dict[str, str]:
    """{id: block body} for every B-NNN block."""
    out: dict[str, str] = {}
    matches = list(ID_RE.finditer(text))
    for i, m in enumerate(matches):
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        out[m.group(1)] = text[m.start():end]
    return out


def _field(block: str, name: str) -> str:
    m = re.search(rf"^{name}:\s*(.*)$", block, re.MULTILINE)
    return m.group(1).strip() if m else ""


def grade(run_dir: Path, eval_id: int) -> list[dict]:
    outputs = run_dir / "outputs"
    backlog = _read(outputs / "BACKLOG.md")
    transcript = _read(outputs / "transcript.md")
    blocks = _blocks(backlog)
    new_ids = sorted(set(blocks) - BASE_IDS)
    new_block = blocks[new_ids[0]] if new_ids else ""
    combined = (backlog + "\n" + transcript).lower()

    def r(text: str, passed: bool | None, evidence: str) -> dict:
        return {
            "text": text,
            "passed": bool(passed) if passed is not None else False,
            "evidence": evidence if passed is not None else f"NEEDS REVIEW — {evidence}",
        }

    if eval_id == 0:  # accepts an unmeasured hunch
        ev = _field(new_block, "evidence")
        return [
            r("A new B-NNN block was appended to BACKLOG.md", bool(new_ids),
              f"new ids: {new_ids or 'none'}"),
            r("The block carries evidence: none-yet", ev == "none-yet", f"evidence={ev!r}"),
            r("The block carries status: raw", _field(new_block, "status") == "raw",
              f"status={_field(new_block, 'status')!r}"),
            r("domain is data-plane-ts and repo is theo-promptly",
              _field(new_block, "domain") == "data-plane-ts" and _field(new_block, "repo") == "theo-promptly",
              f"domain={_field(new_block,'domain')!r} repo={_field(new_block,'repo')!r}"),
            r("why_now records the observation without inventing a trigger",
              None, "judgement: the user gave no trigger — check nothing was fabricated"),
            r("The skill did NOT ask the user to provide evidence before registering",
              None, "read transcript.md for a request for evidence/measurement"),
            r("dod has at least one bullet that could fail",
              len(re.findall(r"^\s+- \S", new_block, re.MULTILINE)) >= 1,
              f"{len(re.findall(r'^  - ', new_block, re.MULTILINE))} bullets"),
        ]

    if eval_id == 1:  # G5 — prior art must be refused
        langsmith_in_registry = "langsmith" in backlog.lower()
        return [
            r("The skill flagged that the justification rests on another project",
              "langsmith" in transcript.lower() and bool(re.search(r"g5|prior[- ]art|outro projeto|another project", transcript, re.I)),
              f"transcript mentions G5/prior-art: {bool(re.search(r'g5|prior.art', transcript, re.I))}"),
            r("The user was offered a choice rather than a flat refusal",
              bool(re.search(r"reformul|false positive|falso positivo|cancel", transcript, re.I)),
              "looked for reformulate/false-positive/cancel options"),
            r("No B-NNN block carries the LangSmith comparison as why_now",
              not langsmith_in_registry,
              f"'langsmith' present in BACKLOG.md: {langsmith_in_registry}"),
            r("Explains that knowing another project's solution is fine — it just cannot be the reason",
              None, "judgement: read the transcript's explanation"),
            r("The refusal states what WOULD make the item acceptable",
              None, "judgement: read the transcript"),
        ]

    if eval_id == 2:  # G3 — two domains must split
        fd = [b for b in blocks.values() if _field(b, "domain") == "frontend-dashboard" and b not in BASE_IDS]
        new_blocks = [blocks[i] for i in new_ids]
        domains = {_field(b, "domain") for b in new_blocks}
        repos = {_field(b, "repo") for b in new_blocks}
        return [
            r("The skill identified that the request spans two domains",
              bool(re.search(r"dois dom|two domain|G3|split", transcript, re.I)),
              f"new items: {len(new_ids)}"),
            r("The split was proposed as two items, not registered as one",
              len(new_ids) == 2, f"new ids: {new_ids}"),
            r("The UI half routes to frontend-dashboard with repo theo-cloud/dashboard",
              "frontend-dashboard" in domains and "theo-cloud/dashboard" in repos,
              f"domains={sorted(domains)} repos={sorted(repos)}"),
            r("The API half routes to control-plane with repo theo-cloud",
              "control-plane" in domains and "theo-cloud" in repos,
              f"domains={sorted(domains)} repos={sorted(repos)}"),
            r("The reason given is one item maps to one specialist",
              None, "judgement: read the transcript"),
        ]

    if eval_id == 3:  # dedup — B-014 already covers it
        return [
            r("The skill searched BACKLOG.md before allocating an id",
              bool(re.search(r"grep|search|dedup|busqu|busca|procur", transcript, re.I)),
              "looked for a search step in the transcript"),
            r("The existing open item about the same problem was found",
              "B-014" in transcript, f"'B-014' in transcript: {'B-014' in transcript}"),
            r("No new B-NNN id was allocated", not new_ids, f"new ids: {new_ids or 'none'}"),
            r("The report names which existing id absorbed the request",
              "B-014" in transcript, "expects B-014 named as the absorbing item"),
            r("The existing block was not renumbered or reordered",
              list(blocks) == sorted(blocks, key=lambda x: int(x.split("-")[1])),
              f"order: {list(blocks)}"),
        ]

    return []


def main() -> int:
    it = Path(sys.argv[1]).resolve()
    summary = []
    for eval_dir in sorted(it.glob("eval-*")):
        eid = int(eval_dir.name.split("-")[1])
        for cfg in ("with_skill", "without_skill"):
            run = eval_dir / cfg
            if not run.is_dir():
                continue
            exps = grade(run, eid)
            if not exps:
                continue
            passed = sum(1 for e in exps if e["passed"])
            (run / "grading.json").write_text(
                json.dumps({"expectations": exps, "pass_rate": passed / len(exps)}, ensure_ascii=False, indent=2),
                encoding="utf-8")
            needs = sum(1 for e in exps if e["evidence"].startswith("NEEDS REVIEW"))
            summary.append((eval_dir.name, cfg, passed, len(exps), needs))

    print(f"{'eval':10} {'config':16} {'auto-pass':>10}  needs-review")
    for name, cfg, p, t, n in summary:
        print(f"{name:10} {cfg:16} {p:5}/{t:<4} {n:>8}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
