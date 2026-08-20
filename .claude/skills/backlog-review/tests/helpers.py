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
    # Adopted from `theo`, which grew it for its own `render_open_index` tests. Propagating the
    # kit's copy over theirs dropped the parameter and broke two of their tests — a consumer that
    # extended a shared fixture is ahead of the kit, not behind it, and the fix is to carry the
    # extension upstream rather than to overwrite it back out. Default keeps every existing
    # caller's output byte-identical.
    checkbox: str = " ",
) -> str:
    bullets = dod if dod is not None else ["p95 do endpoint abaixo de 800ms com janela de 30d"]
    dod_block = "dod:\n" + "".join(f"  - {b}\n" for b in bullets) if bullets else "dod:\n"
    reg = f"\n> Registrado {registered} por `/backlog-item`.\n" if registered else ""
    return (
        f"## {item_id} — {title}   [{checkbox}]\n"
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


def write_backlog(tmp_path: Path, *blocks: str, index: bool = True) -> Path:
    """A well-formed registry: the blocks, plus the index that summarises them.

    The index is generated here rather than left out because a backlog WITHOUT one is now a
    `index_stale` finding — so omitting it would add that finding to every fixture, and each
    defect test would be asserting on its own defect plus one it never asked for. Tests that
    want a stale or absent index pass `index=False` and say so.
    """
    path = tmp_path / "BACKLOG.md"
    content = "# Backlog\n\n## Itens\n\n" + "".join(blocks)
    if index:
        from backlog_index import apply_index, render_index  # noqa: PLC0415
        from check_backlog_structure import _parse_items  # noqa: PLC0415

        content = apply_index(content, render_index(content, _parse_items(content)))
    path.write_text(content, encoding="utf-8")
    return path
