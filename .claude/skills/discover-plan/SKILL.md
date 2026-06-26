---
name: discover-plan
version: 0.1.0
requires: []
description: Turn the current context + any reference projects under knowledge-base/references/ + allowlisted external sources into a discovery plan saved at knowledge-base/discoveries/plans/{slug}-plan.md. Use when you want to plan a deep-research investigation into a technology or pattern before producing a blueprint.
user-invocable: true
allowed-tools: Read Glob Grep Bash Write
argument-hint: "{topic-slug}"
---

# Discover-Plan — Plan a Deep-Research Investigation

Generates a structured **discovery plan** for investigating the reference implementations under `knowledge-base/references/`. The output is the input for `/discover-execute`.

Sibling of `/to-plan` — same backbone, different output. `to-plan` produces implementation plans; `discover-plan` produces research plans whose deliverable is a **technical blueprint** (a structured report under `docs/exploration-reports/` per investigation).

## When to Trigger

User explicitly invokes `/discover-plan {topic-slug}` when they want to:

- Investigate how a reference project solves a specific problem (e.g., failover, replication, multi-tenancy).
- Compile a side-by-side blueprint across two or more cloned reference projects under `knowledge-base/references/`.
- Audit integration tests / dependencies / tools in those references before locking an architectural decision.

## Cycle contract

This skill is **phase 1** of [`cycle-discover`](../../rules/cycle-discover.md). The cycle rule is the **source of truth** for:

- Chain order (this skill → `/discover-edge-cases` → `/discover-execute` → `/discover-confidence` → optional `/discover-improve`; the blueprint is terminal — optional out-of-cycle skill distillation via the standalone `/skill-creator`)
- Hard gates, soft gates, stop conditions
- Anti-patterns at the cycle level
- Rollback procedures
- Cross-references to companion cycles (`cycle-plan.md`)

**Read `cycle-discover.md` before invoking this skill.** This SKILL.md retains only phase-specific detail (the protocol below for generating the discovery plan).

## Process

### Step 0 — Read project rules (MANDATORY)

Before exploring `knowledge-base/references/`, internalize the project rules:

```bash
ls rules/
```

Read each `.md` file. The discovery plan SHALL cite at least one project rule (e.g., `architecture.md` for boundaries that any borrowed pattern must respect, `testing.md` for test pyramid expectations that any borrowed test technique must align with).

**MANDATORY for TheoDB:** read `rules/discover-phd-rigor.md` and apply its bar. TheoDB is a frontier DB anchored on the AlloyDB SOTA — discovery is applied-PhD-grade: SOTA-anchored, ≥ 2 primary sources per technique, benchmark evidence or an honest `UNBENCHMARKED` flag for every performance claim.

### Step 1 — Inventory what is already known

Read in order:

1. Any foundation docs under `docs/` (landscape surveys, position papers) — if present
2. Prior reports under `docs/exploration-reports/*.md` — if present
3. `CLAUDE.md § Architectural Decisions Locked` (or equivalent) — what is already locked
4. The `knowledge-base/references/<project>/` top-level structure (`ls`, `tree -L 2`)

Identify what is **already documented** vs **still open**. The discovery plan exists to close the open gaps, NOT to re-document what is already covered.

### Step 2 — Define the investigation targets

For each reference project in scope, declare:

- **Project**: `knowledge-base/references/<project-slug>/`
- **In scope**: subdirectories the investigation will touch (e.g., `src/core/`, `internal/services/`)
- **Out of scope**: subdirectories explicitly excluded (e.g., `docs/`, build artifacts, vendor trees)

Out-of-scope MUST be explicit. Vague "everything else" violates Coverage Matrix completeness.

### Step 3 — Define the four-corner research coverage (MANDATORY — explicit decision step)

This is its own decision step, NOT a check that happens at the end. Pause here and declare the four-corner coverage BEFORE drafting Research Questions in Step 4.

Every discovery plan MUST cover the four corners or explicitly justify deferral with an ADR:

| Corner | What to find | Example questions |
|---|---|---|
| **Integration tests** | How does each ref project test the boundary the blueprint targets? | "How does <project A> test the persistence layer against a real DB?" |
| **Dependencies** | What runtime + dev deps do they pull in? Versions? Justifications? | "Does <project B> depend on driver X or Y? Which version?" |
| **Tools** | Build/test/lint tooling. CI shape. Local dev story. | "Does <project A> ship docker-compose for dev? What's the test command?" |
| **Techniques** | The algorithm / pattern / data structure being borrowed. | "How does <project B> implement <specific behavior>?" |

#### Question budget (mandatory — FRONTIER PROFILE)

Per `rules/discover-phd-rigor.md § 2`. Discovery on a frontier DB earns deeper interrogation than the generic default.

- **Total: 6-14 questions across all corners.** Below 6 → too thin for a frontier bet; above 14 → halt-loop will likely exhaust budget. Sweet spot 8-10. (Ceiling enforced at 14 by `check_plan_completeness.py`; locked hard cap stays ≤ 15.)
- **Max 5 questions per corner** — to let the `techniques` corner go deep on the algorithm/SOTA axis.
- **Techniques corner: ≥ 2 questions (R4).** The SOTA axis is where TheoDB's bets live (e.g., the pgvector/pgvectorscale fork trigger — PRD D3). One technique question is not frontier discovery.
- **Min 1 question per other corner.** If a corner has zero, you MUST add an ADR justifying the deferral.
- **Each question maps to exactly one corner.** Questions that legitimately span corners should be split.

#### SOTA-rigor on the techniques corner (mandatory — R1/R2/R3)

For every `techniques` question on a performance- or algorithm-bearing pillar (P2 vector/AI, P3 columnar, P4 HA, P7 auto-tuning):

- **R1 — anchor on SOTA.** Name how AlloyDB/ScaNN (or the field) solves the same problem; state the gap TheoDB must close with a permissive piece.
- **R2 — ≥ 2 primary sources.** Plan to cite ≥ 2 independent primary sources per technique (a peer-reviewed paper via the allowlist + an official doc/repo under `knowledge-base/references/`). One source is insufficient.
- **R3 — benchmark or `UNBENCHMARKED`.** Any performance claim the blueprint will make must plan to carry methodology + numbers + source, or be flagged `UNBENCHMARKED` (an honest gap that seeds the next discovery). No bare "X is faster" prose (`public-copy.md`, PRD D3).

#### Pre-validate cited paths (mandatory)

Before adding a `knowledge-base/references/{project}/{path}` to ANY question's column, verify the path exists. The `discover-confidence` hard cap on fabricated citations will fire if even one fake path slips through.

The Coverage Matrix in the discovery plan MUST map each question to a method (read which file, grep which symbol, run which command) and a target answer format.

### Step 4 — Write the discovery plan

Use the template at `skills/discover-plan/templates/discovery-plan-template.md`. Save to:

```
knowledge-base/discoveries/plans/{slug}-plan.md
```

Where `{slug}` is kebab-case derived from the topic. Examples: `<framework>-failover-internals`, `<library>-schema-design`, `<pattern>-implementation-comparison`.

## Plan Template Outline

The full template is in `templates/discovery-plan-template.md`. Required sections:

1. **Header** — version, slug, owner, dates.
2. **Context** — what motivates this discovery NOW, what evidence triggered it.
3. **Objective** — one sentence + measurable success criteria for the resulting blueprint.
4. **In-scope / Out-of-scope** — by reference project + by subdirectory.
5. **ADRs** — decisions about HOW to investigate (depth, time-budget, what to skip).
6. **Research questions** — numbered list. Each question → coverage corner (tests/deps/tools/techniques) → planned method → expected answer shape.
7. **Coverage Matrix** — 100% mapping: every question → at least one method. Gaps marked explicitly as ADR-deferred.
8. **Halt-loop checkpoints** — for `/discover-execute`: what intermediate state must hold before the loop can mark a sub-task DONE.
9. **Acceptance Criteria** — observable conditions for "this discovery is done": every question answered, every citation backed by a `knowledge-base/references/` path, blueprint sections complete.
10. **Global Definition of Done** — links to `/discover-confidence` thresholds + golden rule.

## Quality Rules

These rules are NON-NEGOTIABLE for every discovery plan:

1. **Every research question maps to a method.** No "we'll figure it out". Method = `Read path/to/file`, `Grep 'pattern' in dir/`, `find -name`, `git log --grep`, etc.
2. **Every citation in the plan points to a real path in `knowledge-base/references/`.** Fabricated paths are a `discover-confidence` hard cap (INVALID).
3. **Out-of-scope is explicit.** Vague "rest" is rejected.
4. **Question budget respected (FRONTIER PROFILE).** Total 6-14 questions, max 5 per corner, techniques ≥ 2, min 1 per other corner (or ADR-deferred). See Step 3 + `rules/discover-phd-rigor.md`.
5. **Time-budget per project.** Each reference project gets a budget (e.g., "<project A>: 4h, <project B>: 2h"). Halt-loop respects it. Per-question stop condition mandatory.
6. **Coverage Matrix is complete.** Every research question maps to at least one method. Deferred questions need an ADR.
7. **ADRs justify investigation depth.** Why dig into one subdirectory but skip another? Document the rationale.
8. **No premature conclusions.** A discovery plan ASKS questions; it doesn't answer them. Answers come from `/discover-execute`.

## What this skill does NOT do

- Execute the research itself — that's `/discover-execute`.
- Write the blueprint — that's `/discover-execute` output.
- Modify anything inside `knowledge-base/references/` — read-only zone (enforced by `hooks/boundary-check.sh`).
- Score the result — that's `/discover-confidence`.

## Anti-patterns

1. **"Let's just explore the codebase first and see what we find."** — no plan = no Coverage Matrix = no quality gate. Always plan first.
2. **Citing files without verifying they exist.** Open a Read tool and check before adding a path to the plan.
3. **Skipping a coverage corner.** "We don't need integration tests for this" — that's an ADR, not a free pass.
4. **Asking unanswerable questions.** "Is <project>'s architecture good?" — make it concrete: "Does <project>'s `Foo.bar()` accept argument X? Where?"

## Related

- Sibling skill: `/discover-edge-cases` (next step after this skill)
- Sibling skill: `/discover-execute` (consumes the plan)
- Sibling skill: `/discover-confidence` (scores the blueprint produced by execute)
- Rigor contract (TheoDB): `rules/discover-phd-rigor.md`
- Allowlist for external sources: `rules/discover-web-allowlist.txt`
- Template: `templates/discovery-plan-template.md`
