"""Shared pytest fixtures for backlog-review tests."""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

SKILL_ROOT = Path(__file__).parent.parent
sys.path.insert(0, str(SKILL_ROOT / "scripts"))
sys.path.insert(0, str(Path(__file__).parent))

from helpers import item_block, write_backlog  # noqa: E402


@pytest.fixture
def clean_backlog(tmp_path: Path) -> Path:
    """A registry with nothing wrong — the baseline negative tests break in one way."""
    return write_backlog(
        tmp_path,
        item_block("B-001"),
        item_block(
            "B-002",
            "Corrigir exit code do deploy parcial",
            domain="platform-cli",
            repo="theo-cli",
            suggested_mode="bug",
            status="triaged",
            evidence="src/deploy.ts:88",
            dod=["`theo deploy` retorna exit != 0 quando um passo falha"],
        ),
    )
