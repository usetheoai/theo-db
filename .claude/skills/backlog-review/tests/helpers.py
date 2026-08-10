"""Backlog fixture builders, in their own module.

pytest's importlib import mode (pyproject) does not expose `conftest` as an
importable module, so shared builders live here instead of in the conftest.
"""
from __future__ import annotations

from pathlib import Path


def item_block(
    item_id: str = "B-001",
    title: str = "Reduzir round-trips do listing de traces",
    *,
    domain: str = "data-plane-ts",
    repo: str = "theo-lens",
    suggested_mode: str = "review",
    source: str = "human",
    evidence: str = "none-yet",
    why_now: str = "o dashboard passou a carregar 30d por padrão",
    status: str = "raw",
    dod: list[str] | None = None,
    extra: str = "",
    registered: str | None = None,
) -> str:
    bullets = dod if dod is not None else ["p95 do endpoint abaixo de 800ms com janela de 30d"]
    dod_block = "dod:\n" + "".join(f"  - {b}\n" for b in bullets) if bullets else "dod:\n"
    reg = f"\n> Registrado {registered} por `/backlog-item`.\n" if registered else ""
    return (
        f"## {item_id} — {title}   [ ]\n"
        f"{reg}\n"
        f"domain: {domain}\n"
        f"repo: {repo}\n"
        f"suggested_mode: {suggested_mode}\n"
        f"source: {source}\n"
        f"evidence: {evidence}\n"
        f"why_now: {why_now}\n"
        f"status: {status}\n"
        f"{extra}"
        f"{dod_block}\n"
    )


def write_backlog(tmp_path: Path, *blocks: str) -> Path:
    path = tmp_path / "BACKLOG.md"
    path.write_text("# Backlog\n\n## Itens\n\n" + "".join(blocks), encoding="utf-8")
    return path
