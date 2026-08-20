"""Shared ecosystem layout detection utilities.

Provides canonical functions for locating the Cycle ecosystem directory
across the three supported layouts:
  1. Standalone — the plan/ repo itself (skills/ + rules/ + hooks/ at root)
  2. User config — installed under <project>/.claude/
  3. Plugin install — installed under <project>/.claude/plugins/cycle/

Every script and test that needs to find the ecosystem directory should
import from here instead of duplicating the detection logic.
"""
from __future__ import annotations

from pathlib import Path


def is_ecosystem_layout(d: Path) -> bool:
    """A directory is an ecosystem layout if it has skills/ + rules/ + hooks/ directly."""
    return (d / "skills").is_dir() and (d / "rules").is_dir() and (d / "hooks").is_dir()


def find_ecosystem_dir(start: Path | None = None, *, require: bool = True) -> Path | None:
    """Locate the ecosystem directory by probing upward from *start*.

    At each level (from *start* upward, max 20 levels):
      1. Standalone — ``current/`` itself has ``skills/ + rules/ + hooks/``.
      2. User config — ``current/.claude/`` has them.
      3. Plugin install — ``current/.claude/plugins/cycle/`` has them.

    Parameters
    ----------
    start : Path or None
        Starting directory for the search.  Defaults to ``Path.cwd()``.
    require : bool
        If True (default), raises ``FileNotFoundError`` when no layout is found.
        If False, returns ``None`` instead.

    Returns
    -------
    Path or None
        The ecosystem directory, or None if *require* is False and not found.
    """
    current = (start or Path.cwd()).resolve()
    if current.is_file():
        current = current.parent

    for _ in range(20):
        if is_ecosystem_layout(current):
            return current
        claude_sub = current / ".claude"
        if is_ecosystem_layout(claude_sub):
            return claude_sub
        plugin_sub = current / ".claude" / "plugins" / "cycle"
        if is_ecosystem_layout(plugin_sub):
            return plugin_sub
        if current == current.parent:
            break
        current = current.parent

    if require:
        raise FileNotFoundError(
            "Ecosystem directory not found. Expected one of: "
            "<cwd>/{skills,rules,hooks}, <cwd>/.claude/{skills,rules,hooks}, "
            "or <cwd>/.claude/plugins/cycle/{skills,rules,hooks}."
        )
    return None


def resolve_ecosystem_dir(project_dir: Path) -> Path | None:
    """Find ecosystem directory anchored at *project_dir* (no upward walk).

    Probes three candidate locations under *project_dir* only:
      1. ``project_dir/`` itself
      2. ``project_dir/.claude/``
      3. ``project_dir/.claude/plugins/cycle/``

    Returns the first match with ``knowledge-base/`` present, falling back
    to any layout with ``skills/ + rules/ + hooks/``.  Returns None if
    nothing matches.
    """
    candidates = [
        project_dir,
        project_dir / ".claude",
        project_dir / ".claude" / "plugins" / "cycle",
    ]
    # Prefer candidate with knowledge-base/
    for c in candidates:
        if c.is_dir() and (c / "knowledge-base").is_dir():
            return c
    # Fallback: any layout with skills/+rules/+hooks/
    for c in candidates:
        if c.is_dir() and is_ecosystem_layout(c):
            return c
    return None


def resolve_ecosystem_path(
    project_root: Path, rel_path: str | Path, *, files_only: bool = True
) -> Path | None:
    """O arquivo do ecossistema que ``rel_path`` nomeia, em QUALQUER layout suportado.

    ``rel_path`` é escrito como o ecossistema o vê — ``rules/cycle-discover.md``,
    ``skills/implement/SKILL.md`` — porque é assim que uma regra, um plano e uma oportunidade
    o citam. Onde esse caminho aterrissa depende do layout, e é exatamente isso que quem cita
    não deveria precisar saber.

    Devolve ``None`` quando não resolve em nenhum layout, e a distinção importa: "não resolve
    em lugar nenhum" é um ponteiro fabricado, enquanto "não resolve no layout que eu
    presumi" era o falso positivo (B-081).

    ``files_only=False`` aceita diretório. Não é conveniência: um *target de medição* pode ser
    ``knowledge-base/references/{projeto}``, que É um diretório, enquanto um *ponteiro de
    evidência* (``arquivo.md:42``) só faz sentido sobre arquivo. Quem chama sabe de que tipo
    precisa; esta função só sabe onde procurar.

    POR QUE ESTA FUNÇÃO EXISTE. O docstring no topo deste módulo já dizia que todo script
    deveria importar daqui em vez de duplicar a detecção. Três checadores duplicavam mesmo
    assim, e a duplicação divergiu do jeito previsível: `check_measurement_targets.py`
    carregava as duas candidatas para o `live-target.txt` (linhas 53-54) e **não** para os
    targets do plano (linha 86), no mesmo arquivo. Uma cópia consertada, a irmã não — que é a
    forma que a duplicação sempre toma antes de virar defeito.
    """
    exists = (lambda p: p.is_file()) if files_only else (lambda p: p.exists())
    rel = Path(rel_path)
    if rel.is_absolute():
        return rel if exists(rel) else None
    # Ordem: standalone primeiro (é onde o kit é mantido), depois plugin. As duas são
    # tentadas SEMPRE — decidir pelo layout detectado tornaria a resolução dependente de um
    # palpite sobre a raiz, e a raiz é justamente o que estava errado.
    for candidate in (project_root / rel, project_root / ".claude" / rel):
        if exists(candidate):
            return candidate
    return None
