"""Tests for check_backlog_structure.py — the ways a maintenance registry rots."""
from __future__ import annotations

from datetime import date
from pathlib import Path

import pytest

from check_backlog_structure import check_backlog
from helpers import item_block, write_backlog


def _checks(report: dict) -> set[str]:
    return {f["check"] for f in report["findings"]}


def _find(report: dict, check: str) -> dict:
    return next(f for f in report["findings"] if f["check"] == check)


def test_clean_backlog_is_shippable(clean_backlog: Path) -> None:
    report = check_backlog(clean_backlog)
    assert report["verdict"] == "SHIPPABLE", report["findings"]
    assert report["items_total"] == 2


def test_status_counts(clean_backlog: Path) -> None:
    report = check_backlog(clean_backlog)
    assert report["items_by_status"]["raw"] == 1
    assert report["items_by_status"]["triaged"] == 1


def test_duplicate_id_is_a_blocker(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block("B-001"), item_block("B-001", "Outro")))
    assert "duplicate_id" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_non_monotonic_ids_are_a_blocker(tmp_path: Path) -> None:
    """A reused or reordered id makes every earlier reference ambiguous."""
    report = check_backlog(write_backlog(tmp_path, item_block("B-005"), item_block("B-002", "Outro")))
    assert "renumbered" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_triaged_without_evidence_is_a_blocker(tmp_path: Path) -> None:
    """Triaged means measured. Without evidence the status is a claim nobody made."""
    report = check_backlog(write_backlog(tmp_path, item_block(status="triaged", evidence="none-yet")))
    assert "triaged_without_evidence" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_raw_carrying_evidence_is_flagged(tmp_path: Path) -> None:
    """Measurement happened and nobody advanced the status — the rot this loop prevents."""
    report = check_backlog(write_backlog(tmp_path, item_block(status="raw", evidence="src/x.ts:12")))
    assert "raw_with_evidence" in _checks(report)


def test_killed_without_reason_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(status="killed")))
    assert "killed_without_reason" in _checks(report)


def test_killed_with_reason_is_clean(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="killed", extra="kill_reason: medido, 1 query por request\n"))
    )
    assert "killed_without_reason" not in _checks(report)


def test_illegal_status_is_a_blocker(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(status="in-progress")))
    assert "illegal_status" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_invalid_mode_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(suggested_mode="vibes")))
    assert "invalid_mode" in _checks(report)


def test_missing_field_is_flagged(tmp_path: Path) -> None:
    block = item_block().replace("why_now: o dashboard passou a carregar 30d por padrão\n", "")
    report = check_backlog(write_backlog(tmp_path, block))
    assert "missing_field" in _checks(report)


ROUTING_TABLE = """# Cycle: BACKLOG

## Domain routing

| Domain | Repos | Specialist |
|---|---|---|
| `data-plane-ts` | `theo-lens`, `theo-memory` | `agents/data-plane-ts.md` |
| `platform-cli` | `theo-cli` | `agents/platform-cli.md` |

## Next
"""


def _with_routing_table(tmp_path: Path) -> Path:
    """Plant a real routing table next to the backlog so the G1 check actually runs."""
    rules = tmp_path / "rules"
    rules.mkdir(exist_ok=True)
    (rules / "cycle-backlog.md").write_text(ROUTING_TABLE, encoding="utf-8")
    return tmp_path


def test_unroutable_repo_is_a_blocker(tmp_path: Path) -> None:
    """A repo in no domain routes to nobody — gate G1.

    Plants a routing table so the check is genuinely exercised. Without one the checker
    correctly declines to judge, and the assertion would pass for the wrong reason: the
    check never ran.
    """
    _with_routing_table(tmp_path)
    report = check_backlog(write_backlog(tmp_path, item_block(repo="theo-gateway")))
    assert report["routing_table_read"] is True
    assert "unroutable_repo" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_routable_repo_produces_no_finding(tmp_path: Path) -> None:
    _with_routing_table(tmp_path)
    report = check_backlog(write_backlog(tmp_path, item_block(repo="theo-lens")))
    assert report["routing_table_read"] is True
    assert "unroutable_repo" not in _checks(report)


def test_unreadable_routing_table_does_not_assert_violations(tmp_path: Path) -> None:
    """Missing data must not become a reported violation.

    With no routing table, every repo would look unroutable. Reporting that would assert
    a violation the evidence does not support — the same defect the thresholds resolver
    had when it silently used the wrong bands.
    """
    report = check_backlog(write_backlog(tmp_path, item_block(repo="anything-at-all")))
    assert report["routing_table_read"] is False
    assert "unroutable_repo" not in _checks(report)


def test_thin_dod_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(dod=[])))
    assert "thin_dod" in _checks(report)


def test_vague_dod_is_heuristic_not_deterministic(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(dod=["melhorar a performance"])))
    assert "vague_dod" in _checks(report)
    assert _find(report, "vague_dod")["kind"] == "heuristic"


def test_dod_with_a_number_is_not_vague(tmp_path: Path) -> None:
    """A criterion that happens to mention speed is still falsifiable.

    Flagging "p95 below 800ms" because it contains "fast"-adjacent wording would train
    people to ignore the check, which costs more than the false negatives it prevents.
    """
    report = check_backlog(write_backlog(tmp_path, item_block(dod=["p95 abaixo de 800ms, mais rápido que hoje"])))
    assert "vague_dod" not in _checks(report)


def test_stale_raw_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="raw", registered="2026-01-01")),
        today=date(2026, 8, 5),
    )
    assert "stale_raw" in _checks(report)
    assert _find(report, "stale_raw")["kind"] == "heuristic"


def test_recent_raw_is_not_stale(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="raw", registered="2026-08-01")),
        today=date(2026, 8, 5),
    )
    assert "stale_raw" not in _checks(report)


def test_possible_duplicate_between_open_items(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(
            tmp_path,
            item_block("B-001", "Reduzir round-trips do listing de traces"),
            item_block("B-002", "Reduzir round-trips no listing de traces do explorer"),
        )
    )
    assert "possible_duplicate" in _checks(report)


def test_closed_items_are_not_duplicate_candidates(tmp_path: Path) -> None:
    """A shipped item and a new one about the same area is normal — that is a follow-up.

    Only OPEN items compete for the same work; flagging closed ones would make every
    recurring area look duplicated forever.
    """
    report = check_backlog(
        write_backlog(
            tmp_path,
            item_block("B-001", "Reduzir round-trips do listing de traces", status="shipped"),
            item_block("B-002", "Reduzir round-trips no listing de traces do explorer"),
        )
    )
    assert "possible_duplicate" not in _checks(report)


def test_verdict_is_derived_from_findings(tmp_path: Path) -> None:
    """The verdict is computed, never asserted — same discipline as the scorers."""
    blocker = check_backlog(write_backlog(tmp_path, item_block(status="bogus")))
    assert blocker["severity_counts"]["blocker"] >= 1 and blocker["verdict"] == "INVALID"

    major = check_backlog(write_backlog(tmp_path, item_block(dod=[])))
    assert major["severity_counts"]["blocker"] == 0 and major["verdict"] == "NEEDS_REVISION"

    minor = check_backlog(write_backlog(tmp_path, item_block(dod=["melhorar a performance"])))
    assert minor["severity_counts"]["major"] == 0 and minor["verdict"] == "SHIPPABLE_WITH_CAVEATS"


def test_every_finding_declares_its_kind(tmp_path: Path) -> None:
    """A reader must be able to tell "the machine is sure" from "a human should look"."""
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="bogus", dod=["melhorar tudo"], suggested_mode="vibes"))
    )
    assert report["findings"]
    for f in report["findings"]:
        assert f["kind"] in ("deterministic", "heuristic"), f
