#!/usr/bin/env python3
"""Build an isolated sandbox for one Squad skill eval.

Every eval needs a workspace that looks like an adopted umbrella: a populated
`BACKLOG.md`, the rules the skill reads, the agents it routes to, and the scripts its
gates call. Without one, each subagent improvises a different environment and the runs
stop being comparable — a with-skill run that had a registry and a baseline that did not
differ by the fixture, not by the skill.

The sandbox is a COPY. Runs must never touch the real repo: a skill under test writes to
BACKLOG.md, and an eval that mutates the source would poison every subsequent run in the
same batch, silently and in run order.

Usage:
    python3 setup_squad_eval_sandbox.py <dest-dir> [--with-plan SLUG]
"""
from __future__ import annotations

import argparse
import shutil
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parents[3]
FIXTURE_BACKLOG = REPO / "skills" / "backlog-item" / "evals" / "fixtures" / "BACKLOG.md"
# A governed repo with REAL code, so measurement evals have something to open, count and
# cite. An eval whose target does not exist tests the agent's imagination, not the skill.
FIXTURE_REPO = REPO / "skills" / "discover-execute" / "evals" / "fixtures" / "theo-lens"

# What a Squad skill reads at runtime. Copied wholesale rather than cherry-picked: a
# missing rule makes a skill fail in a way that looks like a skill defect.
COPY_DIRS = ["rules", "agents", "scripts"]
COPY_SKILLS = [
    "backlog-item", "backlog-init", "backlog-review",
    "discover-plan", "discover-edge-cases", "discover-plan-confidence",
    "discover-execute", "discover-confidence", "discover-improve",
]


def build(dest: Path, with_plan: str | None = None, baseline: bool = False) -> None:
    """Build the sandbox. `baseline=True` omits the Squad system entirely.

    This distinction is the whole validity of the comparison. A first run copied `rules/`
    and `skills/` into BOTH configurations, and the without-skill agents read
    `rules/cycle-backlog.md`, cited G2 and G5 by name, and produced near-identical
    behaviour. That is not a baseline — it measures "was told to read the SKILL.md" against
    "found the contract anyway", and both arms had the system.

    A baseline sandbox therefore carries only the registry and the CHANGELOG: what someone
    inherits when they open a workspace with no Squad installed. The schema is still
    inferable from the existing items, which is realistic and fair.
    """
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)

    if not baseline:
        for d in COPY_DIRS:
            src = REPO / d
            if src.is_dir():
                shutil.copytree(src, dest / d, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))

        skills_dest = dest / "skills"
        skills_dest.mkdir()
        for s in COPY_SKILLS:
            src = REPO / "skills" / s
            if src.is_dir():
                shutil.copytree(src, skills_dest / s, ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "tests"))

    shutil.copy2(FIXTURE_BACKLOG, dest / "BACKLOG.md")

    # CHANGELOG.md is a pre-flight requirement in several skills (Unbreakable Rule 6).
    (dest / "CHANGELOG.md").write_text(
        "# Changelog\n\n## [Unreleased]\n\n### Added\n\n### Fixed\n", encoding="utf-8"
    )

    for sub in ("discoveries/plans", "discoveries/opportunities", "backlog", "reviews"):
        (dest / "knowledge-base" / sub).mkdir(parents=True, exist_ok=True)

    # Make it a git repo: skills walk up looking for .git or .claude to find the root, and
    # without a marker they resolve to somewhere outside the sandbox.
    (dest / ".git").mkdir()

    if FIXTURE_REPO.is_dir():
        shutil.copytree(FIXTURE_REPO, dest / "theo-lens",
                        ignore=shutil.ignore_patterns("__pycache__", "node_modules"))

    if with_plan:
        _write_plan(dest, with_plan)

    print(f"sandbox: {dest}")
    print(f"  BACKLOG.md items: {(dest / 'BACKLOG.md').read_text(encoding='utf-8').count(chr(10) + '## B-')}")
    if baseline:
        print("  baseline: no rules/, no skills/ — registry only")
    else:
        print(f"  rules: {len(list((dest / 'rules').glob('*')))} · agents: {len(list((dest / 'agents').glob('*.md')))}")
    if (dest / "theo-lens").is_dir():
        print("  governed repo: theo-lens (real N+1 in src/api/traces.ts)")
    if with_plan:
        print(f"  plan: knowledge-base/discoveries/plans/{with_plan}-plan.md")


def _write_plan(dest: Path, slug: str) -> None:
    """A scored measurement plan, for evals that start at /discover-execute."""
    plan = dest / "knowledge-base" / "discoveries" / "plans" / f"{slug}-plan.md"
    plan.write_text(
        "# Measurement Plan: round-trips in the trace listing\n\n"
        "**Item:** B-014\n"
        "**Repo:** theo-lens\n"
        "**Mode:** review\n"
        f"**Slug:** `{slug}`\n"
        "**Created:** 2026-08-05\n\n"
        "## Context\n\n"
        "The dashboard started loading 30 days by default and the listing got slower.\n\n"
        "## Hypothesis\n\n"
        "The listing endpoint issues one query per span, so a 200-span trace costs 200\n"
        "round-trips to Postgres.\n\n"
        "## Falsification\n\n"
        "If the query count is CONSTANT with respect to span count, the hypothesis is dead\n"
        "and B-014 closes with that result as its `kill_reason`.\n\n"
        "Written as constancy, not as 'a single query': the item DoD asks for a count\n"
        "independent of span count, and the batched form issues TWO queries — it satisfies\n"
        "the DoD and would fail a criterion written as 'a single one'. An eval caught that\n"
        "contradiction between criterion and DoD.\n\n"
        "A partial result does not rescue it: the claim is about the access pattern, not\n"
        "about finding some slow query.\n\n"
        "## Measurement Questions\n\n"
        "| # | Question | Corner | Tool | Target | Expected answer shape |\n"
        "|---|---|---|---|---|---|\n"
        "| Q1 | How many queries does `listTraces` issue per request? | evidence | Read | `theo-lens/src/api/traces.ts:31-57` | count + file:line |\n"
        "| Q2 | What else consumes this handler? | blast_radius | Grep | `theo-lens/src/` | caller list |\n"
        "| Q3 | How will we know the fix worked? | verification | Read | `theo-lens/src/api/traces.ts:65-89` | pass/fail criterion |\n""| Q4 | Does `listTraces` have a caller, or is it dead code? | evidence | Grep | `theo-lens/src/` | caller list, or a declared absence |\n\n"
        "<!-- DEFER-CORNER: constraint | current-constraint.md is undeclared -->\n\n"
        "## Halt-loop Checkpoints\n\n"
        "| Checkpoint | Assertion | Action if fails |\n"
        "|---|---|---|\n"
        "| Before answering Qx | the cited target opens and the line exists | mark Qx BLOCKED |\n"
        "| Every iteration | re-read `## Falsification` — already satisfied? | stop and kill the item |\n\n"
        "## Acceptance Criteria\n\n"
        "- [ ] Every question answered or BLOCKED with a reason\n"
        "- [ ] The falsification criterion was evaluated explicitly\n",
        encoding="utf-8",
    )


def main() -> int:
    ap = argparse.ArgumentParser(description="Build a Squad eval sandbox.")
    ap.add_argument("dest", type=Path)
    ap.add_argument("--with-plan", default=None, help="also write a scored measurement plan with this slug")
    ap.add_argument("--baseline", action="store_true", help="omit rules/ and skills/ — the without-skill arm")
    args = ap.parse_args()

    if not FIXTURE_BACKLOG.is_file():
        print(f"FATAL: fixture missing at {FIXTURE_BACKLOG}", file=sys.stderr)
        return 2

    build(args.dest.resolve(), args.with_plan, baseline=args.baseline)
    return 0


if __name__ == "__main__":
    sys.exit(main())
