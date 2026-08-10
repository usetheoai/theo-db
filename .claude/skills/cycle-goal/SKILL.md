---
name: cycle-goal
version: 0.1.0
requires: [auto-plan, to-plan, implement, code-quality, review, release, acceptance]
description: 'Turn one or more ROADMAP.md milestones into an active session goal. Validates the requested M<N> ids against ROADMAP.md (exists, still open, dependency order honoured), composes a termination condition whose single stop criterion is a green /acceptance run (ACCEPTED or ACCEPTED_WITH_CAVEATS) and which names the artifact each cycle phase must produce on the way there, and hands it to Claude Code''s built-in /goal — which registers a session-scoped Stop hook so the agent cannot stop until the condition is evaluated as met. Milestones run sequentially, one in flight at a time, per the single-flip invariant. Use AFTER /backlog-init or /backlog-item have created the milestones and BEFORE driving them with /auto-plan. Refuses on unknown, already-released, or dependency-blocked milestones, and on a condition over /goal''s 4000-char cap.'
user-invocable: true
allowed-tools: Read Glob Grep Bash Write Edit
argument-hint: "M<N> [M<N> ...]"
---

# `/cycle-goal` — Bind a session to one or more roadmap milestones

`/backlog-init` writes the milestones. `/auto-plan` executes one. Between them sits a gap: nothing holds the session to the process. The agent can stop early, declare a phase done without its artifact, or drift into a second milestone before the first shipped.

This skill closes that gap. It hands Claude Code's built-in `/goal` a condition that only a genuinely finished milestone can satisfy.

## Cycle contract

This skill is the entry gate of [`cycle-maintenance`](../../rules/cycle-maintenance.md) — the macro super-loop. That rule is the **source of truth** for milestone selection, the single-flip invariant, dependency respect, the roadmap-run audit file, and the verdicts (`MILESTONE_RELEASED`, `MILESTONE_IN_FLIGHT`, `ROADMAP_COMPLETE`, `ROADMAP_BLOCKED`, `MILESTONE_BLOCKED`). **Read it before invoking.** This skill sets the termination condition; [`cycle-auto-plan`](../../rules/cycle-auto-plan.md) does the executing.

## When to invoke

`/cycle-goal M2` or `/cycle-goal M2 M3 M4` when:

- `ROADMAP.md` exists at the repo root and the named milestones are still `[ ]`.
- You are on the `workspace` branch (per `rules/git-safety.md`).
- You want the session bound to the full CYCLE process rather than to a single command.

Do NOT invoke when:

- `ROADMAP.md` is missing — run `/backlog-init` first.
- The milestone is already `[x]` — a goal over finished work is met before any work happens.
- A dependency of the requested milestone is neither released nor part of the same call — resolve the wall first.
- A goal is already active — clear it first (`install_goal_hook.py --clear`).

## Unbreakable rules of the goal

These are stated inside the composed condition itself, not merely here — the Stop-hook evaluator reads the condition, so a rule that lives only in this file would not bind anything.

1. **The stop criterion IS the acceptance run.** The goal is met for a milestone **if and only if** `/acceptance M<N>` emitted `ACCEPTED` or `ACCEPTED_WITH_CAVEATS`.
2. **Nothing else ends it.** Not a green test suite, not `READY_TO_MERGE`, not `RELEASED`, not a published tag, not the agent's own sense that the work looks finished. `RELEASED` means it shipped; only the acceptance verdict means it works. That gap is exactly why `cycle-acceptance` exists, so a goal that stopped at `RELEASED` would re-open it.
3. **`/acceptance` not run ⇒ goal not met.** Silence is not a pass.
4. **`REJECTED` and `NOT_VALIDATED` never satisfy the goal.** Both keep it open, and both keep the checkbox `[ ]`.
5. **Three exits are closed by name:** re-running `/acceptance` without fixing what it found; editing the milestone's Definition of done so the run can pass; reporting a verdict `compute_acceptance_verdict.py` did not print. Each is a violation, not a completion.

The other six phases are in the condition as the honest path to that verdict — an acceptance run reached by skipping `/review` is not the same fact — but the terminator is one and only one thing.

## Why this does not use `/goal`

The first version of this skill printed a `/goal` command for the user to paste. That was a dead end, and the reason is worth stating so nobody re-litigates it:

- **A skill cannot invoke `/goal`.** It is a built-in command; the `SlashCommand` tool does not expose built-ins, and in many environments the tool is absent entirely.
- **The settings route is closed.** `type: "prompt"` hooks — what `/goal` registers internally — are only valid on `PreToolUse`, `PostToolUse` and `PermissionRequest`. Never on `Stop`.
- **And `/goal` judges the transcript.** A small model reads what was *said*. A confident sentence can satisfy it, which is precisely the failure mode this cycle exists to prevent.

So the skill arms a **`Stop` hook of type `command`** instead, and that is an upgrade rather than a consolation prize: the gate reads the **filesystem** — the acceptance record's `verdict:` line and the `ROADMAP.md` checkbox. An assertion cannot forge either. No 4000-char cap applies, and no human paste is needed.

The composed condition text is still produced: it goes into the session as the operating contract, where the persona and the process discipline belong.

## Process

### 1. Resolve and validate the milestones

```bash
python3 skills/cycle-goal/scripts/compose_goal_condition.py --roadmap ROADMAP.md M2 M3
```

The script is the deterministic gate. It parses `ROADMAP.md` with the same header shape `cycle-release` flips (`### M<N> — [ ] Name`), then refuses on: unknown id, already-`[x]` milestone, duplicate id, invalid id format, dependency wall, duplicated header, wrong header level, and condition over the cap. On success it prints the condition to stdout and exits 0; every refusal exits 1 with a `BLOCKED cycle-goal:` line naming the cause.

Milestones are normalized to ascending order — never parallel. If the caller's order differed, the script says so on stderr rather than reordering silently.

### 2. Read the operating contract into the session

Restate the contract below to the user, naming the specific milestones. This is the persona-and-discipline half, which `/goal` cannot carry.

### 3. Arm the gate

```bash
python3 skills/cycle-goal/scripts/install_goal_hook.py --milestones M2 M3
```

Writes `.claude/cycle-goal.json` (the goal state) and a `Stop` hook in `.claude/settings.local.json` — personal and gitignored, so arming a goal never lands in a teammate's checkout. Existing settings are merged, never replaced, and re-arming replaces our hook instead of stacking duplicates.

From then on, every attempt to end the session runs `check_goal_met.py`, which blocks with a per-milestone reason until each one has a green `verdict:` in its acceptance record **and** an `[x]` in `ROADMAP.md`.

Two safety properties, both deliberate:

- **Fail-open.** Any error in the gate allows the stop. A gate that bricks the session is worse than one that misses once.
- **Bounded, twice over.** Each block increments a counter; past `max_blocks` (default 40) the gate releases with a warning saying plainly the milestone is **not** done. But the ceiling that actually binds is the CLI's: Claude Code overrides a Stop hook after **9 consecutive blocks** (`CLAUDE_CODE_STOP_HOOK_BLOCK_CAP`) and ends the turn regardless of what we set. Observed in the field. So `max_blocks` only governs blocks spread across turns — raising it is not a tighter grip, it is theatre. The gate cannot trap a session, and that is a property to rely on rather than a gap to close.

To cancel: `python3 skills/cycle-goal/scripts/install_goal_hook.py --clear`.

### 4. Report

State: the ordered milestone chain, that the Stop hook is armed and where, the block ceiling, and how to clear it. The hook may need `/hooks` opened once (or a restart) if the settings watcher was not already watching `.claude/` this session — say so rather than assuming it took effect.

## The operating contract

Read into the session at step 2, adapted to the milestones in play.

**Role.** Senior engineer accountable for system design, architecture, design patterns, SOLID, DRY, KISS, DDD, automated testing, observability, performance, scalability, security. The priority is a solution that is robust, simple to maintain and ready to evolve — not one that merely passes.

**The process is the seven-phase cycle, in order, no phase implicit:**

| Phase | Rule | Produces |
|---|---|---|
| discover | [`cycle-discover`](../../rules/cycle-discover.md) | requirements, dependencies, risks, bottlenecks, edge cases, architectural impact |
| plan | [`cycle-plan`](../../rules/cycle-plan.md) | `knowledge-base/plans/{slug}-plan.md` with `milestone_id`, acceptance criteria, test strategy, rollback |
| implement | [`cycle-implement`](../../rules/cycle-implement.md) | `knowledge-base/implementations/{slug}-implementation.md`, one commit per task |
| code-quality | [`cycle-code-quality`](../../rules/cycle-code-quality.md) | quality verdict with no BLOCKER open |
| review | [`cycle-review`](../../rules/cycle-review.md) | `READY_TO_MERGE` verdict |
| release | [`cycle-release`](../../rules/cycle-release.md) | PR `workspace → develop`, then `develop → main` + semver tag → `RELEASED` |
| **acceptance** | [`cycle-acceptance`](../../rules/cycle-acceptance.md) | the released delivery exercised against the milestone DoD with evidence → `ACCEPTED`; **this is the terminator**, and only it flips the ROADMAP checkbox |

The fourth phase is the one most process descriptions omit. It is not optional here.

**Done is `cycle-acceptance`, not `cycle-release`.** The tag being cut is the second-to-last step; the milestone is finished when the released delivery was exercised and accepted.

**Integration is `cycle-release`, not a manual merge.** `workspace` originates the work, `develop` only integrates via promotion PR, `main` receives release merges. Direct commits to `develop` or `main` are forbidden by `rules/git-safety.md` — no exception for "just this once".

**Absolute rules.** Never bypass a phase. Never mask a problem with a workaround. Never invent results, evidence or data. Never claim something was executed, validated, tested or integrated without the artifact that proves it. Never omit a known risk. Never ignore an architectural failure.

**Always.** Think about bottlenecks, edge cases, integration failures, scalability, security, observability, systemic impact. Explain architectural decisions when they are not obvious. When uncertain, say so explicitly and propose an objective way to settle it — per the 95% confidence rule, stopping to ask beats redoing.

**Scope.** Only the named milestones, one in flight at a time, in ascending order.

## Hard gates

- **One milestone in flight.** Parallel milestones collide on shared modules and corrupt the audit trail (`cycle-maintenance` § Anti-patterns). The composed condition states the order and the script enforces ascending.
- **No goal over finished work.** An already-`[x]` milestone makes the condition true before anything happens, which reads as success and is not.
- **Dependency respect.** A dependency must be released already or earlier in the same call.
- **The terminator is the acceptance verdict, and only that.** A condition that would let `RELEASED` (or any earlier verdict) end the goal is malformed — it re-opens the shipped-vs-works gap `cycle-acceptance` closes.
- **Cap is a block, not a trim.** A condition over 4000 chars is refused with its measured length. Silently truncating would drop phases from the condition — the exact bypass this skill exists to prevent.
- **No unconfirmed activation.** Reporting "goal set" without the CLI's acknowledgement is the same class of unevidenced claim the contract forbids.

## Anti-patterns

- **Stuffing the persona into `/goal`.** It is a Stop-hook condition evaluated by a small model, and it would blow the cap. The persona belongs in the session; the condition stays verifiable.
- **Vague conditions.** "M2 is done" lets the evaluator accept an assertion. Name the artifact per phase.
- **Setting a goal to force work through a block.** A blocked phase means the condition is not met; the goal must not become a reason to invent a way around it.
- **Treating `RELEASED` as the finish line.** It is the second-to-last phase. A goal that stops there certifies that something shipped, not that it works.
- **Using this instead of `/auto-plan`.** This binds the session; it does not execute the cycle.
- **Leaving a goal active after the milestones ship.** `/goal clear` — a stale goal re-prompts against work nobody is doing.

## What this skill does NOT do

- Does not execute any cycle phase — `/auto-plan M<N>` does that.
- Does not edit `ROADMAP.md`. The checkbox flip belongs to `cycle-release`.
- Does not create plans, branches, commits or PRs.
- Does not clear an existing goal for you — that is `/goal clear`, a deliberate human act.
- Does not evaluate whether the condition was met. The CLI's evaluator owns that.

## Related

- [`skills/backlog-init/SKILL.md`](../backlog-init/SKILL.md) — creates `ROADMAP.md` and its milestones
- [`skills/backlog-item/SKILL.md`](../backlog-item/SKILL.md) — appends a milestone to an existing roadmap
- [`skills/auto-plan/SKILL.md`](../auto-plan/SKILL.md) — executes one milestone end-to-end
- `commands/plan-goal.md` — the sibling bridge that derives a `/goal` condition from an active plan instead of a milestone
- `commands/plan-loop.md` — cadence primitive that pairs with a goal
- `rules/git-safety.md` — the `workspace → develop → main` flow the release phase obeys
