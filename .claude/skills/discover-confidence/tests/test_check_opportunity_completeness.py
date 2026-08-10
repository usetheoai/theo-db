"""Tests for check_opportunity_completeness.py — mandatory sections + conditional ADR."""
from __future__ import annotations

from pathlib import Path

import pytest

from check_opportunity_completeness import check_opportunity_completeness  # noqa: E402


def _opportunity(tmp_path: Path, name: str, *, repo: str = "theo-lens", blast: str, adrs: str = "") -> Path:
    path = tmp_path / name
    path.write_text(
        "# Opportunity: Test\n\n"
        "**Item:** B-014\n"
        f"**Repo:** {repo}\n"
        "**Mode:** review\n\n"
        "## Context\n\nText.\n\n"
        "## Corner 1 — Evidence\n\nText.\n\n"
        "## Corner 2 — Constraint Relation\n\nText.\n\n"
        f"## Corner 3 — Blast Radius\n\n{blast}\n\n"
        "## Corner 4 — Verification\n\nText.\n\n"
        f"{adrs}"
        "## Recommendation\n\n- Do X\n",
        encoding="utf-8",
    )
    return path


def test_good_opportunity_all_sections(good_opportunity: Path) -> None:
    report = check_opportunity_completeness(good_opportunity)
    assert report["found"] == report["total_required"]
    assert report["missing_mandatory"] == []


def test_synthetic_opportunity_complete(synthetic_opportunity: Path) -> None:
    report = check_opportunity_completeness(synthetic_opportunity)
    assert report["missing_mandatory"] == []


def test_missing_section_detected(tmp_path: Path) -> None:
    path = tmp_path / "missing.md"
    path.write_text(
        "# Opportunity: Test\n\n"
        "**Item:** B-014\n"
        "**Repo:** theo-lens\n"
        "**Mode:** review\n\n"
        "## Context\n\nText.\n\n"
        "## Corner 1 — Evidence\n\nText.\n\n"
        # Corners 2-4 and Recommendation missing
        ,
        encoding="utf-8",
    )
    report = check_opportunity_completeness(path)
    assert "Corner 3 — Blast Radius" in report["missing_mandatory"]
    assert "Recommendation" in report["missing_mandatory"]


def test_invalid_mode_is_a_missing_section(tmp_path: Path) -> None:
    """`Mode` must name one of the four real modes, not any word."""
    path = _opportunity(tmp_path, "bad-mode.md", blast="Repo-local.")
    path.write_text(path.read_text(encoding="utf-8").replace("**Mode:** review", "**Mode:** vibes"), encoding="utf-8")
    report = check_opportunity_completeness(path)
    assert "Mode" in report["missing_mandatory"]


def test_repo_local_change_requires_no_adr(tmp_path: Path) -> None:
    """A one-line fix in a leaf repo should not have to carry an ADR.

    The ancestor demanded >=1 ADR from every blueprint. For maintenance work that is
    ceremony: most items touch one repo and decide nothing architectural.
    """
    path = _opportunity(
        tmp_path,
        "local.md",
        repo="theo-lens",
        blast="Repo-local. Only theo-lens consumes the affected surface.",
    )
    report = check_opportunity_completeness(path)
    assert report["cross_repo"] is False
    assert report["adr_required"] is False
    assert report["adr_missing"] is False


def test_cross_repo_change_without_adr_is_flagged(tmp_path: Path) -> None:
    """The other half of the conditional — the one that actually protects anything.

    A change whose blast radius reaches other repos decides something for their
    maintainers. Shipping that without a recorded decision is how a breaking change
    arrives unannounced in a repo nobody warned.
    """
    path = _opportunity(
        tmp_path,
        "cross.md",
        repo="theo-contracts",
        blast="Changes the jwt claim shape consumed by theo-cloud and theo-cli.",
    )
    report = check_opportunity_completeness(path)
    assert report["cross_repo"] is True
    assert report["foreign_repos"] == ["theo-cli", "theo-cloud"]
    assert report["adr_required"] is True
    assert report["adr_missing"] is True


def test_cross_repo_change_with_adr_passes(tmp_path: Path) -> None:
    path = _opportunity(
        tmp_path,
        "cross-with-adr.md",
        repo="theo-contracts",
        blast="Changes the jwt claim shape consumed by theo-cloud.",
        adrs="## ADRs\n\n### D1 — Version the claim instead of renaming it\n\nRationale.\n\n",
    )
    report = check_opportunity_completeness(path)
    assert report["cross_repo"] is True
    assert report["adr_count"] == 1
    assert report["adr_missing"] is False


def test_own_repo_mention_does_not_make_it_cross_repo(tmp_path: Path) -> None:
    """Naming your own repo in the blast radius is normal and must not trigger the cap."""
    path = _opportunity(
        tmp_path,
        "self-mention.md",
        repo="theo-lens",
        blast="Confined to theo-lens; the theo-lens SDK surface is untouched.",
    )
    report = check_opportunity_completeness(path)
    assert report["own_repo"] == "theo-lens"
    assert report["foreign_repos"] == []
    assert report["cross_repo"] is False


def test_repos_named_outside_blast_radius_are_ignored(tmp_path: Path) -> None:
    """Only the Blast Radius corner decides cross-repo, not incidental prose elsewhere.

    Context routinely mentions sibling repos while explaining background. Scanning the
    whole document would make nearly every opportunity cross-repo and turn the ADR
    requirement back into the blanket demand it replaced.
    """
    path = tmp_path / "mention-in-context.md"
    path.write_text(
        "# Opportunity: Test\n\n"
        "**Item:** B-014\n"
        "**Repo:** theo-lens\n"
        "**Mode:** review\n\n"
        "## Context\n\nSimilar to something theo-rag and theo-memory already solved.\n\n"
        "## Corner 1 — Evidence\n\nText.\n\n"
        "## Corner 2 — Constraint Relation\n\nText.\n\n"
        "## Corner 3 — Blast Radius\n\nRepo-local; nothing downstream consumes it.\n\n"
        "## Corner 4 — Verification\n\nText.\n\n"
        "## Recommendation\n\n- Do X\n",
        encoding="utf-8",
    )
    report = check_opportunity_completeness(path)
    assert report["cross_repo"] is False
