#!/usr/bin/env python3
"""Arm or clear the cycle-goal Stop hook — the part `/goal` could not automate.

Writes two things and nothing else:

  - `.claude/cycle-goal.json`  — the goal state (which milestones, block counter)
  - a Stop hook in `.claude/settings.local.json` invoking check_goal_met.py

`settings.local.json` on purpose: personal and gitignored, so arming a goal never
lands in a teammate's checkout. Existing settings are merged, never replaced — a
hook that clobbers a permissions block would be a worse bug than the one it fixes.

Usage:
    python3 install_goal_hook.py --milestones M2 M3 [--project-root .] [--max-blocks 40]
    python3 install_goal_hook.py --milestones M27 --roadmap ../outro-repo/ROADMAP.md \\
                                 --acceptance-dir ../outro-repo/knowledge-base/acceptance
    python3 install_goal_hook.py --clear [--project-root .]

Exit codes:
    0 — armed or cleared
    2 — bad argument, unreadable settings file, or a path that does not resolve
        (roadmap / acceptance dir) — see --force
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HOOK_MARKER = "cycle-goal/scripts/check_goal_met.py"
DEFAULT_MAX_BLOCKS = 40
#: Canonico: o knowledge-base mora DENTRO de .claude/ (plugin install). O layout
#: standalone -- o proprio repo do kit -- e o unico onde ele fica na raiz. Escolher
#: errado nao quebra ruidosamente: cria um segundo knowledge-base vazio ao lado do
#: real, e os artefatos passam a se perder entre os dois.
PLUGIN_ACCEPTANCE_DIR = ".claude/knowledge-base/acceptance"
STANDALONE_ACCEPTANCE_DIR = "knowledge-base/acceptance"


def default_acceptance_dir(root: Path) -> str:
    """Resolve o knowledge-base canonico para o layout deste projeto."""
    if (root / ".claude" / "knowledge-base").exists():
        return PLUGIN_ACCEPTANCE_DIR
    if (root / ".claude").exists():
        return PLUGIN_ACCEPTANCE_DIR  # plugin install; o scaffold ainda vai nascer
    return STANDALONE_ACCEPTANCE_DIR


def _load_settings(path: Path) -> dict:
    if not path.exists():
        return {}
    raw = path.read_text(encoding="utf-8").strip()
    if not raw:
        return {}
    return json.loads(raw)


def _strip_our_hook(settings: dict) -> dict:
    """Remove only OUR Stop hook, leaving every other hook untouched."""
    stop_entries = settings.get("hooks", {}).get("Stop")
    if not stop_entries:
        return settings

    kept_entries = []
    for entry in stop_entries:
        kept_hooks = [
            h for h in entry.get("hooks", [])
            if HOOK_MARKER not in str(h.get("command", ""))
        ]
        if kept_hooks:
            kept_entries.append({**entry, "hooks": kept_hooks})

    if kept_entries:
        settings["hooks"]["Stop"] = kept_entries
    else:
        settings["hooks"].pop("Stop", None)
        if not settings["hooks"]:
            settings.pop("hooks", None)
    return settings


def _acceptance_path_declared(root: Path) -> tuple[bool, str]:
    """A viable route to ACCEPTED must exist BEFORE a goal is armed.

    Arming a gate whose condition nobody can satisfy is a trap: it blocks every
    stop attempt until the ceiling, and each block looks like a legitimate verdict.
    The project therefore declares how its released delivery is reached; without
    that declaration the goal is refused rather than armed into a dead end.
    """
    declaration = root / ".claude" / "rules" / "acceptance-target.txt"
    if not declaration.exists():
        return False, f"{declaration} não existe — declare como a entrega publicada é alcançada."

    keys = {}
    for line in declaration.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if "=" in line:
            k, _, v = line.partition("=")
            keys[k.strip()] = v.strip()

    missing = [k for k in ("kind", "target") if not keys.get(k)]
    if missing:
        return False, (
            f"{declaration} não declara {' e '.join(missing)}. Sem isso /acceptance não tem "
            "como alcançar a entrega, e a meta seria insatisfazível."
        )
    return True, f"{keys['kind']} → {keys['target']}"


def _milestones_have_dod(roadmap_path: Path, milestones: list[str]) -> list[str]:
    """Milestones whose Definition of done is missing — acceptance has no criteria."""
    import re

    text = roadmap_path.read_text(encoding="utf-8")
    headers = list(re.finditer(r"^###\s+(M\d+)\s+[—\-]{1,2}\s+\[[ x]\]", text, re.MULTILINE))
    sem = []
    for wanted in milestones:
        for index, match in enumerate(headers):
            if match.group(1) != wanted:
                continue
            end = headers[index + 1].start() if index + 1 < len(headers) else len(text)
            block = text[match.end():end]
            if not re.search(r"^\*\*Definition of done[^*]*:\*\*", block, re.MULTILINE) or \
               not re.search(r"^-\s+\[[ x]\]", block, re.MULTILINE):
                sem.append(wanted)
            break
        else:
            sem.append(wanted)
    return sem


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--milestones", nargs="*", default=[])
    parser.add_argument("--clear", action="store_true")
    parser.add_argument("--project-root", type=Path, default=Path("."))
    parser.add_argument("--roadmap", default="ROADMAP.md")
    parser.add_argument(
        "--acceptance-dir",
        default=None,
        help=(
            "Where cycle-acceptance writes its records, relative to --project-root. "
            "Defaults to the canonical .claude/knowledge-base/acceptance. Must stay INSIDE "
            "the project — consumers are autonomous and do not share a knowledge-base."
        ),
    )
    parser.add_argument("--max-blocks", type=int, default=DEFAULT_MAX_BLOCKS)
    parser.add_argument(
        "--force",
        action="store_true",
        help="Arm even when the roadmap or acceptance directory does not resolve.",
    )
    args = parser.parse_args()

    root = args.project_root.resolve()
    claude_dir = root / ".claude"
    settings_path = claude_dir / "settings.local.json"
    state_path = claude_dir / "cycle-goal.json"

    try:
        settings = _load_settings(settings_path)
    except json.JSONDecodeError as exc:
        print(f"{settings_path} is not valid JSON ({exc}) — refusing to touch it.", file=sys.stderr)
        return 2

    if args.clear:
        settings = _strip_our_hook(settings)
        claude_dir.mkdir(parents=True, exist_ok=True)
        settings_path.write_text(json.dumps(settings, indent=2) + "\n", encoding="utf-8")
        existed = state_path.exists()
        state_path.unlink(missing_ok=True)
        print(f"cleared: Stop hook removed; goal state {'deleted' if existed else 'was absent'}.")
        return 0

    if not args.milestones:
        print("nothing to arm — pass --milestones M<N> [...] or --clear.", file=sys.stderr)
        return 2

    # Resolver ANTES de armar. Um gate apontando para um roadmap ou um diretorio de
    # aceitacao que nao existe bloqueia para sempre por um motivo falso ("acceptance
    # never ran") que parece um veredito legitimo. Descobrir isso depois custa uma
    # sessao inteira; descobrir agora custa uma linha.
    acceptance_rel = args.acceptance_dir or default_acceptance_dir(root)
    roadmap_path = (root / args.roadmap).resolve()
    acceptance_path = (root / acceptance_rel).resolve()

    problems = []

    # Autonomia: cada projeto tem o SEU knowledge-base e o SEU roadmap. Um gate que
    # aponta para fora acopla dois repos autonomos e faz o milestone de um depender
    # do estado do outro -- exatamente o que a arquitetura proibe.
    for label, path in (("roadmap", roadmap_path), ("diretório de aceitação", acceptance_path)):
        if root not in path.parents and path != root:
            problems.append(
                f"{label} está FORA do projeto: {path}. Os consumidores são autônomos — "
                "cada um tem o próprio ROADMAP.md e o próprio .claude/knowledge-base/."
            )
    if not roadmap_path.exists():
        problems.append(f"roadmap não existe: {roadmap_path}")
    if not acceptance_path.exists():
        problems.append(
            f"diretório de aceitação não existe: {acceptance_path} "
            "(use --acceptance-dir quando os artefatos do ciclo moram em outro repo)"
        )

    # Uma meta so pode ser armada se houver rota ate ACCEPTED. Duas condicoes
    # mecanicamente verificaveis: a entrega tem caminho declarado, e cada milestone
    # tem Definition of done (que e de onde /acceptance tira os criterios).
    if roadmap_path.exists():
        sem_dod = _milestones_have_dod(roadmap_path, args.milestones)
        if sem_dod:
            problems.append(
                f"sem Definition of done: {', '.join(sem_dod)}. /acceptance lê esses bullets COMO "
                "critérios de aceite — sem eles o milestone nunca pode ser aceito."
            )

    declarado, detalhe = _acceptance_path_declared(root)
    if not declarado:
        problems.append(f"sem rota até ACCEPTED — {detalhe}")

    if problems and not args.force:
        for problem in problems:
            print(f"BLOCKED cycle-goal: {problem}", file=sys.stderr)
        print(
            "Nada foi armado. Uma meta sem rota até ACCEPTED é uma armadilha: ela bloqueia "
            "toda tentativa de parada até o teto, e cada bloqueio parece um veredito legítimo. "
            "Corrija o que está acima, ou passe --force assumindo esse risco conscientemente.",
            file=sys.stderr,
        )
        return 2

    gate = Path(__file__).resolve()
    claude_dir.mkdir(parents=True, exist_ok=True)

    state_path.write_text(json.dumps({
        "milestones": args.milestones,
        "roadmap": args.roadmap,
        "acceptance_dir": acceptance_rel,
        "blocks": 0,
        "max_blocks": args.max_blocks,
    }, indent=2) + "\n", encoding="utf-8")

    settings = _strip_our_hook(settings)  # idempotent: never stack duplicates
    command = (
        f'python3 "{gate.parent / "check_goal_met.py"}" '
        f'--state "{state_path}" --project-root "{root}"'
    )
    settings.setdefault("hooks", {}).setdefault("Stop", []).append({
        "hooks": [{
            "type": "command",
            "command": command,
            "timeout": 30,
            "statusMessage": "cycle-goal: checking acceptance evidence",
        }]
    })
    settings_path.write_text(json.dumps(settings, indent=2) + "\n", encoding="utf-8")

    print(f"armed: {' '.join(args.milestones)}")
    print(f"  roadmap    : {roadmap_path}{'' if roadmap_path.exists() else '   <== NÃO EXISTE (--force)'}")
    print(f"  acceptance : {acceptance_path}{'' if acceptance_path.exists() else '   <== NÃO EXISTE (--force)'}")
    print(f"  state      : {state_path}")
    print(f"  hook       : {settings_path}")
    print(f"  alvo       : {detalhe}")
    print(f"  ceiling    : {args.max_blocks} blocks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
