#!/usr/bin/env python3
"""Stop-hook gate: refuse to end the session while a milestone goal is unmet.

This is what `/cycle-goal` uses INSTEAD of Claude Code's built-in `/goal`, and
the substitution is an upgrade rather than a workaround:

  - `/goal` cannot be invoked by a skill. It is a built-in command, and the
    `SlashCommand` tool (where it exists at all) does not expose built-ins. The
    settings route is closed too: `type: "prompt"` hooks are only valid on
    PreToolUse / PostToolUse / PermissionRequest — never on Stop.
  - `/goal` judges the TRANSCRIPT with a small model. A confident sentence can
    satisfy it. This gate reads the FILESYSTEM: the acceptance record and the
    ROADMAP checkbox. An assertion cannot forge either.

Contract with `cycle-acceptance`: the run writes
`knowledge-base/acceptance/{milestone}-{date}.md` carrying `verdict: <TOKEN>` in
its frontmatter. Only ACCEPTED / ACCEPTED_WITH_CAVEATS satisfy the goal, and the
milestone's ROADMAP checkbox must read `[x]` — the same pair `cycle-roadmap`
requires before calling a milestone released.

Two safety properties, both deliberate:

  - **Fail-open.** Any unexpected error allows the stop. A gate that bricks the
    session is worse than a gate that misses once; the goal is to hold the
    process honest, not to trap the user.
  - **Bounded, twice over.** Each block increments a counter; past `max_blocks`
    the gate releases with a warning. But the REAL ceiling is the CLI's, not ours:
    Claude Code overrides a Stop hook after 9 consecutive blocks
    (`CLAUDE_CODE_STOP_HOOK_BLOCK_CAP`) and ends the turn regardless. Observed in
    the field. So `max_blocks` only matters for blocks spread across turns — a
    large value is not a tighter grip, it is theatre. The gate cannot trap a
    session, and that is a property to rely on, not a gap to close.

Reads the goal state from `.claude/cycle-goal.json`; absent state = no goal = allow.

Usage (wired automatically by install_goal_hook.py):
    python3 check_goal_met.py [--state PATH] [--project-root PATH]

Exit codes:
    0 — always. The decision travels in the JSON on stdout, per the Stop-hook contract.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

GREEN_VERDICTS = {"ACCEPTED", "ACCEPTED_WITH_CAVEATS"}
DEFAULT_MAX_BLOCKS = 40  # ver nota acima: o teto efetivo é o do CLI (9 consecutivos)

_VERDICT_RE = re.compile(r"^verdict:\s*([A-Z_]+)\s*$", re.MULTILINE)


def _checkbox_state(roadmap_text: str, milestone_id: str) -> str | None:
    """Return ' ' or 'x' for the milestone header, or None when absent."""
    match = re.search(
        rf"^###\s+{re.escape(milestone_id)}\s+[—\-]{{1,2}}\s+\[([ x])\]",
        roadmap_text,
        re.MULTILINE,
    )
    return match.group(1) if match else None


def _acceptance_verdict(acceptance_dir: Path, milestone_id: str) -> str | None:
    """Newest acceptance record's verdict for the milestone, or None if never run."""
    records = sorted(acceptance_dir.glob(f"{milestone_id}-*.md"))
    if not records:
        return None
    for record in reversed(records):
        match = _VERDICT_RE.search(record.read_text(encoding="utf-8"))
        if match:
            return match.group(1)
    return None


def evaluate(milestones: list[str], roadmap_text: str, acceptance_dir: Path) -> list[str]:
    """Return one blocking reason per unmet milestone; empty list means the goal is met."""
    reasons: list[str] = []

    # Um acceptance_dir inexistente e um milestone que nunca foi aceito produzem a
    # MESMA ausência de arquivo, e a mensagem "never ran" soa como veredito legítimo.
    # Separar os dois é o que impede o gate de bloquear para sempre por engano de
    # configuração — a armadilha que apareceu no primeiro uso real, num setup onde o
    # roadmap e os artefatos moravam num repo irmão.
    if not acceptance_dir.exists():
        return [
            f"MISCONFIGURED: {acceptance_dir} does not exist, so no acceptance record can "
            "ever be found there and this gate would block forever for a false reason. "
            "Re-arm with --acceptance-dir pointing at the repo that holds the cycle "
            "artifacts, or clear the goal."
        ]

    for milestone_id in milestones:
        verdict = _acceptance_verdict(acceptance_dir, milestone_id)
        checkbox = _checkbox_state(roadmap_text, milestone_id)

        if verdict is None:
            reasons.append(
                f"{milestone_id}: /acceptance never ran — no record in "
                f"{acceptance_dir}/{milestone_id}-*.md. Silence is not a pass."
            )
            continue
        if verdict not in GREEN_VERDICTS:
            reasons.append(
                f"{milestone_id}: acceptance verdict is {verdict}, which never satisfies the goal. "
                "Fix what the run found and re-run /acceptance — re-running without fixing, or "
                "editing the Definition of done so it can pass, is a violation, not a completion."
            )
            continue
        if checkbox != "x":
            state = "missing from ROADMAP.md" if checkbox is None else "still [ ]"
            reasons.append(
                f"{milestone_id}: acceptance is {verdict} but the ROADMAP checkbox is {state}. "
                "The flip is the last phase — run it."
            )

    return reasons


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state", type=Path, default=Path(".claude/cycle-goal.json"))
    parser.add_argument("--project-root", type=Path, default=Path("."))
    args = parser.parse_args()

    try:
        if not args.state.exists():
            return 0  # no active goal

        state = json.loads(args.state.read_text(encoding="utf-8"))
        milestones = state.get("milestones") or []
        if not milestones:
            return 0

        blocks = int(state.get("blocks", 0))
        max_blocks = int(state.get("max_blocks", DEFAULT_MAX_BLOCKS))

        roadmap_path = args.project_root / state.get("roadmap", "ROADMAP.md")
        if not roadmap_path.exists():
            print(json.dumps({
                "systemMessage": f"cycle-goal: {roadmap_path} not found — goal gate stood down.",
            }))
            return 0

        acceptance_dir = args.project_root / state.get(
            "acceptance_dir", "knowledge-base/acceptance"
        )
        reasons = evaluate(
            milestones, roadmap_path.read_text(encoding="utf-8"), acceptance_dir
        )

        if not reasons:
            args.state.unlink(missing_ok=True)
            print(json.dumps({
                "systemMessage": "cycle-goal: every milestone accepted and flipped — goal met, gate cleared.",
            }))
            return 0

        if blocks >= max_blocks:
            print(json.dumps({
                "systemMessage": (
                    f"cycle-goal: released after {blocks} blocks without the goal being met. "
                    "The gate stops holding rather than loop forever — the milestone is NOT done. "
                    + " | ".join(reasons)
                ),
            }))
            return 0

        state["blocks"] = blocks + 1
        args.state.write_text(json.dumps(state, indent=2), encoding="utf-8")

        print(json.dumps({
            "decision": "block",
            "reason": (
                "The active cycle-goal is not met. The stop criterion is the acceptance run, "
                "and nothing else ends it — not a green test suite, not READY_TO_MERGE, not "
                "RELEASED, not a published tag, not your own judgement that the work looks "
                "finished.\n\n" + "\n".join(f"- {r}" for r in reasons) +
                "\n\nContinue the cycle. If the GOAL itself is wrong, clear it with:\n"
                "  python3 .claude/skills/cycle-goal/scripts/install_goal_hook.py --clear"
            ),
        }))
        return 0

    except Exception as exc:  # noqa: BLE001 — fail-open is the deliberate policy here
        print(json.dumps({
            "systemMessage": f"cycle-goal gate errored ({type(exc).__name__}: {exc}) — allowing stop.",
        }))
        return 0


if __name__ == "__main__":
    sys.exit(main())
