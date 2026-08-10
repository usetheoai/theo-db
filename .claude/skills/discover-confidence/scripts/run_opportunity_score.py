#!/usr/bin/env python3
"""Run M2 structural opportunity-confidence scoring.

Sibling of plan-confidence/scripts/run_structural.py — same architecture, different
rubric and checkers (corner_coverage / evidence_pointers / opportunity_completeness /
structural_risk).

Replaces the ancestor `run_blueprint_score.py`.
"""
from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# Allow sibling imports when invoked directly
sys.path.insert(0, str(Path(__file__).parent))

from _rubric_loader import load_rubric  # noqa: E402,F401
from check_corner_coverage import check_corner_coverage  # noqa: E402
from check_evidence_pointers import check_evidence_pointers  # noqa: E402
from check_opportunity_completeness import check_opportunity_completeness  # noqa: E402
from check_spec_smells import check_spec_smells  # noqa: E402


SKILL_ROOT = Path(__file__).parent.parent


def _find_project_root(start: Path) -> Path:
    current = start.resolve().parent if start.is_file() else start.resolve()
    while current != current.parent:
        if (current / ".claude").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return start.resolve().parent if start.is_file() else start.resolve()


def _resolve_opportunity(arg: str) -> Path:
    p = Path(arg)
    if p.exists() and p.suffix == ".md":
        return p.resolve()
    base = Path(".claude/knowledge-base/discoveries/opportunities")
    for c in (base / f"{arg}-opportunity.md", base / f"{arg}.md"):
        if c.exists():
            return c.resolve()
    raise FileNotFoundError(f"Could not resolve opportunity: {arg}")


def _resolve_rubric(arg: Path | None) -> Path:
    if arg and arg.exists():
        return arg
    return SKILL_ROOT / "templates" / "rubric-opportunity.md"


def _resolve_thresholds(arg: Path | None, opportunity_path: Path) -> Path:
    if arg and arg.exists():
        return arg
    project_root = _find_project_root(opportunity_path)
    # Both layouts, deliberately: `rules/` is standalone, `.claude/rules/` is plugin.
    # Checking only one made the project's own bands lose silently in the other, and a
    # scorer grading against the wrong bands still prints a confident verdict.
    for candidate in (
        project_root / "rules" / "discover-opportunity-thresholds.txt",
        project_root / ".claude" / "rules" / "discover-opportunity-thresholds.txt",
    ):
        if candidate.exists():
            return candidate
    return SKILL_ROOT / "templates" / "discover-opportunity-thresholds.example.txt"


def _parse_thresholds(path: Path) -> dict[str, int]:
    bands: dict[str, int] = {}
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        parts = [p.strip() for p in line.split("|")]
        if len(parts) >= 2:
            try:
                bands[parts[0]] = int(parts[1])
            except ValueError:
                continue
    return bands


def _verdict_for(score: float, bands: dict[str, int]) -> str:
    for name, threshold in sorted(bands.items(), key=lambda kv: kv[1], reverse=True):
        if score >= threshold:
            return name
    return "INVALID"


def main() -> int:
    parser = argparse.ArgumentParser(description="Run M2 structural opportunity-confidence scoring.")
    parser.add_argument("opportunity", help="opportunity slug or .md path")
    parser.add_argument("--rubric", type=Path, default=None)
    parser.add_argument("--thresholds", type=Path, default=None)
    parser.add_argument("--no-warn", action="store_true", help="suppress calibration warning")
    args = parser.parse_args()

    try:
        opportunity_path = _resolve_opportunity(args.opportunity)
    except FileNotFoundError as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        return 2

    rubric_path = _resolve_rubric(args.rubric)
    bands = _parse_thresholds(_resolve_thresholds(args.thresholds, opportunity_path))

    coverage = check_corner_coverage(opportunity_path)
    evidence = check_evidence_pointers(opportunity_path)
    completeness = check_opportunity_completeness(opportunity_path)
    smells = check_spec_smells(opportunity_path, rubric_path)

    # Per-dimension scores (0-100)
    cc_score = 100.0 * coverage["corners_populated"] / coverage["corners_total"]

    # An opportunity with zero code pointers is not automatically weak: a `live-test`
    # finding is carried by runtime observations, which are not disk-verifiable. The
    # empty-corner and mode-contract gates are what catch a genuinely evidence-free
    # opportunity, so this dimension does not double-penalise.
    ep_score = 100.0 if evidence["total"] == 0 else 100.0 * evidence["verified"] / evidence["total"]

    oc_score = 100.0 * completeness["found"] / completeness["total_required"]
    sr_score = max(0.0, 100.0 + smells.total_penalty)  # penalty is negative

    weights = {
        "corner_coverage": 0.30,
        "evidence_pointers": 0.30,
        "opportunity_completeness": 0.25,
        "structural_risk": 0.15,
    }
    weighted = (
        weights["corner_coverage"] * cc_score
        + weights["evidence_pointers"] * ep_score
        + weights["opportunity_completeness"] * oc_score
        + weights["structural_risk"] * sr_score
    )

    hard_caps_triggered: list[str] = []
    cap_value: float = 100.0

    for empty in coverage["empty_corners"]:
        hard_caps_triggered.append(f"empty_corner_{empty}")
        cap_value = min(cap_value, 49.0)

    if evidence["fabricated"] > 0:
        hard_caps_triggered.append("fabricated_evidence")
        cap_value = min(cap_value, 49.0)

    if completeness["missing_mandatory"]:
        hard_caps_triggered.append("mandatory_section_missing")
        cap_value = min(cap_value, 70.0)

    # ADR is required only when the blast radius reaches beyond the opportunity's own
    # repo. A repo-local fix carries no cap; a cross-repo change without a recorded
    # decision does.
    if completeness["adr_missing"]:
        hard_caps_triggered.append("no_adr_on_cross_repo_change")
        cap_value = min(cap_value, 70.0)

    if smells.total_hits >= 20:
        hard_caps_triggered.append("soft_floor_smell_density_high")
        cap_value = min(cap_value, 89.0)

    if evidence["evidence_total"] > 0 and evidence["evidence_density_per_200w"] < 1.0:
        hard_caps_triggered.append("soft_floor_evidence_density_low")
        cap_value = min(cap_value, 89.0)

    final_score = min(weighted, cap_value)
    verdict = _verdict_for(final_score, bands)

    reasons = {
        "corner_coverage": {
            "contributors": coverage["contributors"],
            "detractors": coverage["detractors"],
        },
        "evidence_pointers": {
            "contributors": evidence["contributors"],
            "detractors": evidence["detractors"],
        },
        "opportunity_completeness": {
            "contributors": completeness["contributors"],
            "detractors": completeness["detractors"],
        },
        "structural_risk": {
            "contributors": [f"{smells.total_hits} smell hits across categories"]
            if smells.total_hits == 0
            else [],
            "detractors": [
                f"{cat}: {count} hits"
                for cat, count in sorted(smells.by_category.items(), key=lambda x: -x[1])[:3]
            ],
        },
    }

    sub_reports: dict[str, Any] = {
        "corner_coverage": coverage,
        "evidence_pointers": evidence,
        "opportunity_completeness": completeness,
        "structural_risk": {
            "total_hits": smells.total_hits,
            "by_category": smells.by_category,
            "total_penalty": smells.total_penalty,
        },
    }

    out = {
        "opportunity_slug": opportunity_path.stem.replace("-opportunity", ""),
        "opportunity_path": str(opportunity_path),
        "scored_at": datetime.now(timezone.utc).isoformat(),
        "corner_coverage_score": round(cc_score, 1),
        "evidence_pointers_score": round(ep_score, 1),
        "opportunity_completeness_score": round(oc_score, 1),
        "structural_risk_score": round(sr_score, 1),
        "active_dimensions": [
            "corner_coverage",
            "evidence_pointers",
            "opportunity_completeness",
            "structural_risk",
        ],
        "weight_normalization_factor": 1.0,
        "weighted_avg": round(weighted, 1),
        "hard_caps_triggered": hard_caps_triggered,
        "final_score_after_caps": round(final_score, 1),
        "verdict": verdict,
        "calibration": {
            "status": "PROVISIONAL_v1",
            "holdout_count": 0,
            "holdout_target": 30,
            "kappa_measured": False,
        },
        "reasons": reasons,
        "sub_reports": sub_reports,
    }

    print(json.dumps(out, indent=2))

    if not args.no_warn and out["calibration"]["status"] == "PROVISIONAL_v1":
        print(
            "WARN: PROVISIONAL_v1 calibration — score bands are SOTA defaults, not yet "
            "calibrated against project holdout.",
            file=sys.stderr,
        )

    if verdict == "INVALID":
        return 1
    if verdict == "NON_SHIPPABLE":
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
