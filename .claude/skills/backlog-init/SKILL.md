---
name: backlog-init
version: 0.1.0
requires: []
description: Create BACKLOG.md once, at the root of the governed scope — the umbrella when repos live below it, or the repository itself when it is autonomous — inventorying what is FROM DISK and deriving the domain routing table from THIS project. Use this the first time anyone tries to register maintenance work and no registry exists yet, when /backlog-item refuses because BACKLOG.md is missing, or when adopting Squad in a new workspace. Refuses if BACKLOG.md already exists.
user-invocable: true
allowed-tools: Read Glob Grep Bash Write Edit AskUserQuestion
argument-hint: "(no arguments)"
---

# `/backlog-init` — Create the ecosystem maintenance registry

Create `BACKLOG.md` at the umbrella root: the one place that answers *"what is pending anywhere in the Theo ecosystem?"*

Run once, at adoption. Every item after that arrives through `/backlog-item` (human) or `/discover --sweep` (measured finding).

## Cycle contract

**Two scopes, both valid.** `detect_scope` answers which one you are in: `umbrella` (the directory is not itself a repo and groups repos that are) or `single-repo` (the directory IS the repo). The previous pre-flight refused the second — *"no umbrella detected: run at the workspace root"* — which, in an autonomous project, means writing the registry into the parent directory, outside the project. Measured on `theokit-framework`: ten independent repos, each with its own cycle, and the kit pushed all ten registries into a directory that is nobody's repository.

The principle the rule defends ("one question, one place to look") never required an umbrella. It requires **one registry per governed scope**, and an autonomous repo is a scope.

This skill bootstraps the artifact that [`cycle-backlog`](../../rules/cycle-backlog.md) governs. The cycle rule is the **source of truth** for the item schema, status transitions, domain routing, verdicts and gates. This skill only creates the empty registry those rules operate on — it never registers an item.

## When NOT to invoke

DO NOT invoke when:

- `BACKLOG.md` exists. This skill refuses — use `/backlog-item` to add to it.
- You want to add an item. That is `/backlog-item`, always.
- You are inside a governed repo. A per-repo backlog is the fragmentation the single registry exists to prevent (`cycle-backlog.md § Output`).

## Process

### Step 0 — Pre-flight (MANDATORY, fail-fast)

```bash
# 0.1  BACKLOG.md must NOT exist (opposite of /backlog-item)
test -f BACKLOG.md && { echo "FATAL: BACKLOG.md already exists — use /backlog-item"; exit 1; }

# 0.2  determine the SCOPE of the registry — never refuse for lack of an umbrella
ECO=$([ -d .claude/skills ] && echo .claude || echo .)   # plugin vs standalone
SCOPE=$(python3 "$ECO/skills/backlog-init/scripts/detect_domains.py" --root . --json \
          | python3 -c 'import json,sys; print(json.load(sys.stdin)["scope"])')
echo "scope: $SCOPE"   # umbrella | single-repo — both are valid registry roots

# 0.3  CHANGELOG.md must exist (Unbreakable Rule 6)
test -f CHANGELOG.md || { echo "FATAL: CHANGELOG.md missing"; exit 1; }
```

### Step 1 — Inventory the repos FROM DISK

Never write the inventory from memory or from an existing `CLAUDE.md` table. Both drift, and a routing table that names a repo which is not checked out sends items to a specialist that cannot open the code.

```bash
for d in */; do
  [ -d "$d/.git" ] && printf '%-24s %s\n' "${d%/}" "$(git -C "$d" rev-list --count HEAD 2>/dev/null || echo 0) commits"
done
```

Then derive the routing table FROM THIS PROJECT:

```bash
ECO=$([ -d .claude/skills ] && echo .claude || echo .)   # plugin vs standalone
python3 "$ECO/skills/backlog-init/scripts/detect_domains.py" --root . --json
```

The rule is the repository as the unit of ownership: an umbrella of checked-out repos gets one domain per repo; a single repo gets ONE domain named after it, with each monorepo package listed as a path-addressed entry (`packages/sdk`) — the form the routing table already supports.

Do NOT classify the target's repos into the 8 domains that ship in `cycle-backlog.md`. Those are the `theo` ecosystem's, and they are there as that project's own instance of this table, not as a set every consumer must fit into. Measured on `theokit-sdk` (2026-08-18): 88 items carrying measured `file:line` evidence, every one of them `BLOCKER/unroutable_repo`, because `packages/sdk` cannot exist in another ecosystem's map.

Two classes get **excluded from routing**, and the registry says so out loud rather than omitting them silently:

- **Zero-commit repos** — nothing to maintain yet.
- **Nested clones of the umbrella itself** — not a product; working in one duplicates the same repository into two checkouts that diverge in silence.

A repo on disk that the detector did not reach is a finding, not a rounding error: surface it with `AskUserQuestion` and let the human either fold it into a derived domain or declare it out of scope. Never quietly drop it — a repo absent from the routing table can never receive an item.

The specialist file is NOT derived. `detect_domains.py` names `agents/<domain>.md`, and writing it is human work: `route_domain.py` exits 3 when the table names an agent that is not on disk, so a generated table with no specialist trades one blocker for another.

### Step 2 — Confirm the routing table

Print the derived table and ask for confirmation before writing. The routing table decides which specialist owns which code for the life of the registry; a wrong mapping here is a wrong mapping in every item that follows.

```
Domain               Repos (verified on disk)
-------------------  ------------------------------------------
engine-go            theo
control-plane        theo-cloud · theo-traefik-mcp
data-plane-ts        theo-memory · theo-rag · theo-lens · …
…
Excluded             theo-itself (0 commits) · theo-workspace (nested clone)
```

### Step 3 — Write `BACKLOG.md`

Structure, in this order:

1. **Header** — what the registry is, and the one-line rule that governs it: *ids are monotonic and never renumbered*.
2. **How an item gets here** — the two producers (`/backlog-item` human, `/discover --sweep` measured), pointing at `cycle-backlog.md` for the schema rather than restating it. The registry is data; the contract lives in the rule.
3. **Domain routing table** — as confirmed in Step 2, with the exclusions and their reasons.
4. **`## Index`** — the three-bucket summary (`cycle-backlog.md § The index that opens the
   registry`). Do **not** hand-write it; run it, even on an empty registry:

   ```bash
   python3 .claude/skills/backlog-review/scripts/backlog_index.py BACKLOG.md --write
   ```

   Generating it now, over zero items, is what makes the section exist before anyone has a reason
   to skip it. `check_backlog_structure.py` treats an absent index as stale, so a registry created
   without one is born non-conformant.
5. **`## Items`** — empty, with the next free id declared as `B-001`.

Seed **no items**. An item nobody filed has no `why_now`, no DoD and no owner — it is a placeholder that will be inherited as though it were a decision.

### Step 4 — CHANGELOG + report

One line under `[Unreleased] § Added`. Then report the table, the exclusions, and the next step:

```
BACKLOG.md created — 8 domains, {n} repos routed, {m} excluded.
Next step:  /backlog-item {slug}   or   /discover --sweep {domain}
```

## Out of scope (deliberately)

**Migrating existing findings.** The `knowledge-base/` of an adopting workspace often holds review reports with real, still-open findings. Importing them here is a **separate, evidence-preserving migration** — each imported item needs its original evidence pointer and its original date, or it arrives as a hunch and loses exactly what made it worth keeping. This skill does not attempt it, and a registry created by it is honestly empty rather than dishonestly populated.

## Anti-patterns

- **Writing the inventory from `CLAUDE.md`.** It is documentation and it drifts. `find`/`git -C` is the source of truth.
- **Seeding "obvious" items.** Every item needs a human `why_now` and a DoD. Pre-filled items have neither and get inherited as decisions nobody made.
- **Silently dropping an unclassifiable repo.** It disappears from routing and becomes unmaintainable through the system. Ask.
- **Creating a per-repo `BACKLOG.md`.** Directly re-creates the orphaned-findings problem the single registry solves.
- **Re-running to "refresh" the routing table.** The skill refuses when the file exists, on purpose. Edit the table in place; the items must survive.

## Cross-references

- Cycle rule (source of truth): [`rules/cycle-backlog.md`](../../rules/cycle-backlog.md)
- Sister skill, opposite pre-condition: [`skills/backlog-item/SKILL.md`](../backlog-item/SKILL.md)
- Live environment declaration used by `/discover --mode live-test`: [`rules/live-target.txt`](../../rules/live-target.txt)
- Branching contract for the registry commit: [`rules/git-safety.md`](../../rules/git-safety.md)
