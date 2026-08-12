---
name: arch-check
version: 0.1.0
requires: []
description: Check a repo's architecture boundaries, or set them up when it has none. Verifies declared rules via dependency-cruiser (TypeScript), go-arch-lint (Go), layered-crate (Rust) or import-linter (Python), and asserts the rules can still fire — a rule naming a directory that moved passes GREEN in every one of these tools. When no rules exist, measures the real import graph and proposes only boundaries the repo ALREADY obeys, each with the count that proves it. Use when adopting architecture gates in a repo, after a restructure that moved directories, when `npm run boundaries` or `go-arch-lint` passes and you want to know whether it verified anything, or before writing a `.dependency-cruiser.cjs` / `.go-arch-lint.yml` by hand.
user-invocable: true
allowed-tools: Read Glob Grep Bash Write Edit AskUserQuestion
argument-hint: "[repo-path] (defaults to the current repo)"
---

# `/arch-check` — boundaries a repo already has, or the ones it declared

Two jobs, decided by what is on disk:

| State | What happens |
|---|---|
| A config exists | **Verify.** Run the linter, plus the meta-gate that its rules can still fail |
| No config | **Propose.** Measure the import graph and offer only rules the repo already obeys |

Both halves refuse rather than guess. This skill never writes a boundary the code does not
already respect, and never reports a clean run it could not perform.

---

## Why proposing is not the same as deciding

An architecture rule is knowledge of the project. Writing one for a repo you did not design means
deciding what its architecture *is* — and a gate built from taste reports violations that are
evidence of nothing, the same defect gate G5 rejects when prior art is offered as justification.

So the proposal half has exactly one criterion, and it is measurable:

> **An edge that is already one-way is a measured invariant, not an opinion.**

If `tui/` imports `agents/` thirty-three times and `agents/` imports `tui/` zero times, then
"agents must not import tui" is *already true*. Writing it down changes no code and freezes a
property the repo has. If both directions carry traffic there is nothing to freeze, and nothing
is proposed.

The same criterion, stated from the adoption side by `usetheo-labs/agent-builder`:

> *"Nenhuma destas regras foi escrita contra violação existente: as cinco saíram de 0 violações no
> commit que as introduziu, o que significa que elas CONGELAM um estado bom em vez de anunciar
> dívida."*

A candidate that would fail on day one is therefore **not** proposed as a gate. Someone has to
decide whether that crossing is a defect or the architecture, and that decision belongs in
`BACKLOG.md`, not in a config file. The proposal names it under `not_proposed`, with the reason.

**You ratify; the skill never writes a config on its own.** What it hands you is a set of
candidates, each carrying the count that justifies it, ready to paste with its evidence intact.

---

## What it proposes, and from what measurement

| Kind | Proposed when | The measurement |
|---|---|---|
| `no-circular` | zero cycles today | cycle count across all units |
| `one-way` | A→B has traffic, B→A has none | both counts, both directions |
| `siblings` | two units in the graph exchange nothing | zero in either direction |
| `independence` | units exist, no imports between any | unit count, zero edges |

Cycles are the one finding never up for architectural taste — the Acyclic Dependencies Principle
has no "unless you meant it" clause. When cycles already exist, `no-circular` is withheld and
reported as a finding instead.

Run it directly:

```bash
python3 skills/arch-check/scripts/propose_rules.py <repo> [--language go|typescript]
```

Go builds its graph from `go list -json ./...` — the toolchain resolves imports exactly, so it
cannot silently under-read. TypeScript counts relative imports, plus bare specifiers that name a
package the repo's own `workspaces` declares: in a monorepo those **are** the cross-unit edges.
Everything else bare is somebody else's code, and treating `react` as a unit would invent
architecture out of the dependency list.

Skipping bare specifiers wholesale was the first version, and it inverted the answer. Measured on
`TheoCode`: 4 packages exchanging 80 imports read as 0 edges, and the proposer then offered
`independence` — a rule forbidding all 80. `import('...')` is read too; 20 of those 80 exist only
in that form, invisible to a reader of import *statements*.

---

## The refusal that matters

`extract_imports_and_calls` returns an empty list when tree-sitter is unavailable — it degrades
instead of failing. An empty graph would make **every** pair look one-way, and the skill would
emit a complete rule set built on having parsed nothing.

So an empty scan is refused, and the refusal distinguishes two states that look identical from
the edge count alone:

- **units seen, zero edges between them** → real independence, and a stronger rule than any
  direction (measured on `theo-contracts`: `jwt`, `plan` and `serviceauth` import none of each
  other)
- **fewer than two units seen** → the parser did not run; nothing is proposed until the graph
  is real
- **units seen, zero edges, but a specifier named a workspace package and did not resolve** →
  edges exist and went unmeasured. This is the monorepo case above: 0 edges there means the
  resolver failed, and reporting the catalogue's strongest rule off a graph with known holes in
  it is the vacuous gate D5 exists to catch

---

## Verifying an already-configured repo

Verification is `/code-quality`'s D5 detector — this skill does not duplicate it. What D5 adds
over running the linter yourself is the meta-gate:

> **A rule that cannot fail is worse than no rule.** It transfers confidence to a mechanism that
> stopped existing, and nothing else reports it.

Measured in this ecosystem on 2026-08-06:

- `theo-contracts` — a `.go-arch-lint.yml` naming a directory that does not exist answers
  `ArchHasWarnings: false`. Green. The diagnosis goes to `ExecutionWarnings`, a field nothing read.
- `usetheo-labs/agent-builder` — the global `depcruise` binary cruised **0 modules** against a
  config the local one cruised 279 with. Zero violations over zero modules is not a pass.

D5 reports four shapes of this: a rule whose `from` names a directory that moved, a cruise that
reached no modules, `allow.ignoreNotFoundComponents: true`, and `tsarch` declared as a dependency
that no file imports.

---

## Adopting a proposal

1. Run the proposer and read the evidence on each candidate — the count is the argument.
2. Drop the ones you disagree with. A boundary nobody believes gets deleted at the first
   inconvenience, so a rule you kept only because a tool suggested it is worse than no rule.
3. Write the config in the linter's own format, carrying the evidence into the rule's comment.
   Squad invents no fourth format.
4. Install the linter **locally**, never as a global binary — see the measured reason above.
5. Run `/code-quality` and confirm zero violations. A config that is red on arrival was adopted
   against the criterion.

For Go, `.go-arch-lint.yml` needs `components` plus `deps`; leave `allow.ignoreNotFoundComponents`
alone, since it disables the tool's own guard against exactly the failure D5 exists to catch.

---

## Refuses when

- The repo root carries no supported manifest — there is no graph to build.
- The scan reaches fewer than two units — see the refusal above.
- You ask it to write a config: it proposes, you ratify.
