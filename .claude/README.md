<div align="center">

# Squad

**Maintain a running ecosystem on measurements, not hunches.**

[![Status](https://img.shields.io/badge/status-alpha-orange)](CHANGELOG.md)
[![Version](https://img.shields.io/badge/version-0.1.0-blue)](plugin.json)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Python](https://img.shields.io/badge/python-3.10%2B-3776AB)](pyproject.toml)
[![Claude Code](https://img.shields.io/badge/Claude%20Code-plugin-blueviolet)](https://code.claude.com/docs/en/)

A development squad that keeps the **Theo ecosystem** healthy: eight domain specialists and a pipeline that carries a maintenance item from **hunch → measurement → plan → code → merge**. Every item starts as a hypothesis. Nothing reaches a plan until somebody measured it — and finding nothing is a successful outcome.

[Quick start](#quick-start) · [How it works](#how-it-works) · [The specialists](#the-eight-specialists) · [Contributing](CONTRIBUTING.md)

</div>

---

## Table of contents

- [Why this exists](#why-this-exists)
- [What you get](#what-you-get)
- [How it works](#how-it-works)
- [The eight specialists](#the-eight-specialists)
- [Quick start](#quick-start)
- [The four discover modes](#the-four-discover-modes)
- [Project structure](#project-structure)
- [Advisory skills](#advisory-skills)
- [Unbreakable principles](#unbreakable-principles)
- [Relationship to Cycle](#relationship-to-cycle)
- [Status](#status) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [License](#license)

---

## Why this exists

Maintaining a live multi-repo ecosystem fails in ways that building a new one does not:

1. **Work justified by hunches.** "The trace explorer feels slow" becomes a refactor nobody sized, because nobody measured what was slow.
2. **Fabricated evidence.** A `file:line` nobody opened, a status code nobody requested, a test asserted to fail but never run. Everything downstream treats it as fact.
3. **Findings that die orphaned.** A review notices six real problems; they live in a report, get read once, and never become work.
4. **Local optimisation.** Ten well-evidenced improvements shipped into a stage that was never the limit, mistaken for throughput.
5. **Generic agents.** A reviewer that does not know a root `go build ./...` covers almost nothing in `theo` reports "builds clean" and has measured nothing.

Squad addresses each with a phase, a gate, or a specialist who knows the difference.

## What you get

- **A hunch is registerable, and cheap.** `/backlog-item` takes an unmeasured hypothesis — no evidence required, on purpose. Demanding proof at intake silences the cheapest signal a maintenance team has.
- **Measurement decides, not conviction.** `/discover` runs against *our* code and runtime in one of four modes, and has the authority to **kill** the item. A run that finds nothing protected the plan cycle from a hunch.
- **Prior art can never be evidence.** Gate G5 rejects "project X does it this way" as a justification. Knowing how others solved it is fine; it is simply not a measurement of our system.
- **Pointers are verified, line included.** A cited `file:line` that does not resolve — missing file, or a line past the end of one — caps the artifact at INVALID.
- **One registry, two producers.** `BACKLOG.md` is the single answer to "what is pending?". Humans file items; sweeps register findings with evidence attached. Orphaned findings have nowhere to hide.
- **Eight specialists who know the terrain.** Each carries build commands verified on disk, the domain's invariants, and the false positives that domain generates.
- **Guardrails at runtime.** Claude Code hooks enforce git safety (no `--force`, no direct-to-`main`), TDD discipline, CHANGELOG hygiene and honest public copy while you work.

## How it works

```
        ┌──────────────────────────────────────────────┐
        │  BACKLOG · /backlog-item          (phase 0)  │
        │  a hypothesis. evidence: none-yet            │
        └────────────────────┬─────────────────────────┘
                             │ B-NNN · status: raw
                             ▼
        ┌──────────────────────────────────────────────┐
        │  DISCOVER · /discover --mode {…}             │
        │  measures OUR code / OUR runtime             │
        ├──────────────────────┬───────────────────────┤
        │  evidence found      │  nothing found        │
        │  → status: triaged   │  → status: killed     │
        └──────────┬───────────┴───────────────────────┘
                   │                    ✔ a successful outcome
                   ▼
        ┌──────────────────────────────────────────────┐
        │  PLAN → IMPLEMENT → CODE-QUALITY → REVIEW    │
        │  → RELEASE          (TDD, gates, jury)       │
        └────────────────────┬─────────────────────────┘
                             │ RELEASED
                             ▼
                   status: shipped ──→ back to SELECT
```

The macro loop (`cycle-maintenance`) selects the next item — measured before unmeasured, then oldest first — routes it to a specialist, and delegates. **It never reports "complete".** A backlog is not a scope; an empty one means nobody has looked recently, so the empty state is a prompt to sweep.

## The eight specialists

| Specialist | Repos | Knows |
|---|---|---|
| `engine-go` | `theo` | A root `go build ./...` covers almost nothing — it is multi-module |
| `control-plane` | `theo-cloud`, `theo-traefik-mcp` | Cross-tenant leakage; metering that mis-counts money |
| `data-plane-ts` | `theo-memory`, `theo-rag`, `theo-lens`, `theo-trust`, `theo-skills`, `theo-promptly` | Tenant isolation; drift between SDK, REST and MCP |
| `theo-db` | `theo-db` | A defect crashes the database; the AGPL licence gate |
| `infra-terraform` | `theo-infra-modules`, `theo-infra-live` | Terraform (not OpenTofu); RDS is a protected unit; Pulumi is legacy |
| `contracts-auth` | `theo-contracts` | Everything imports it — assume cross-repo by default |
| `frontend-dashboard` | `theo-cloud/dashboard` | Environment vs product — the only domain with a live target |
| `platform-cli` | `theo-cli`, `theo-storage` | `npm`, not `pnpm`; consumers are scripts, not importers |

Routing is deterministic (`scripts/route_domain.py`) and reads its table from `rules/cycle-backlog.md` — one table, one truth. See [`agents/README.md`](agents/README.md).

## Quick start

**Requirements:** Python 3.10+, `git`, Claude Code, and the `ralph-loop` plugin for halt-loop phases.

```bash
# 1. Create the registry, once (inventories repos FROM DISK, never from a table)
/backlog-init

# 2. Register something worth looking at — a hunch is enough
/backlog-item theo-lens-trace-latency

# 3. Measure it. This may kill the item, and that is a good day
/discover --mode live-test B-014

# 4. If it survived, run the chain
/auto-plan B-014
```

Sweep a whole domain instead of filing by hand:

```bash
/discover --sweep data-plane-ts        # findings land in BACKLOG.md with evidence attached
/backlog-review                        # what has rotted in the registry
```

## The four discover modes

Each mode defines what counts as a measurement. Evidence from one does not satisfy another.

| Mode | Finds | Evidence required |
|---|---|---|
| `review` | A defect visible in our code | `file:line` + the rule violated + why it matters **here** |
| `live-test` | Behaviour wrong in the running system | `METHOD URL -> status`, console, trace id, timing, screenshot |
| `bug` | A reproduced defect | Numbered repro **plus a test that fails on the current state, executed** |
| `evolve` | Measured cost of the status quo | A number: N round-trips, N duplicated call sites, N ms |

`bug` has a hard floor: **no failing test, no bug.** A defect nobody can express as a failing test is not understood well enough to fix. `live-test` refuses on a domain with no declared target — six of eight have none, by design, because a Go library and a Terraform module have no surface a browser can probe.

## Project structure

```
squad/
├── agents/          ← the 8 domain specialists + README
├── rules/           ← contracts. cycle-*.md are the source of truth
│   ├── cycle-backlog.md      ← the registry, intake, domain routing
│   ├── cycle-discover.md     ← the four modes, evidence contracts, gates
│   ├── cycle-maintenance.md  ← the macro loop
│   ├── current-constraint.md ← the constraint lens (advisory, never a gate)
│   └── live-target.txt       ← declared live environments
├── skills/          ← one directory per phase, each with its SKILL.md
├── scripts/         ← route_domain.py, check_xrefs.py, validators
├── hooks/           ← runtime guardrails
└── tests/           ← root suite; per-slice suites live in skills/*/tests
```

Rules are the contract; a SKILL.md carries only phase-specific detail and points back at its rule.

## Advisory skills

Beyond the pipeline phases, the bundle ships skills that answer architecture questions rather than driving a cycle. They are auxiliary — bound to no `cycle-*.md`, invoked on demand:

| Skill | Answers |
|---|---|
| `cap-theorem-specialist` | Consistency vs availability during a network partition; CP/AP classification of an operation |
| `backpressure-specialist` | A producer outrunning a consumer: buffers, drop policies, flow control |
| `resilience-specialist` | Timeouts, retries, circuit breakers, bulkheads, load shedding, degradation, recovery |

Each refuses the shortcut its field is prone to — classifying a product as CP or AP without its configuration, recommending an unbounded buffer, or retrying a non-idempotent operation without protection.

## Unbreakable principles

- **Evidence is ours or it is not evidence.** Gate G5 at intake, and the Evidence corner downstream.
- **A pointer resolves, line included.** Otherwise the artifact is INVALID.
- **Killing an item is success.** The cycle can say no, with a `kill_reason` naming what was measured.
- **`unknown` is a complete answer** — for the constraint corner, and only there. We do not instrument flow, so demanding a constraint claim would be answered by assertion.
- **Ids are never reused or renumbered.** A killed `B-007` stays `B-007` forever; the number is the audit trail.
- **Measuring is reading.** Discover produces a document, never a patch.
- **Verdicts are derived from findings**, never asserted.

## Relationship to Cycle

Squad is derived from Cycle (MIT) and inverts its centre. Cycle is greenfield and stack-agnostic: its DISCOVER studies **how other projects solved a problem** and explicitly forbids looking at your own code. That is the right question when building something new and the wrong one when maintaining something that runs — it produces imitation, not maintenance.

| | Cycle | Squad |
|---|---|---|
| Driver | a milestone in `ROADMAP.md` | an item in `BACKLOG.md` |
| Discover asks | how did project X solve this? | what is true about *our* system? |
| Terminal artifact | blueprint (a design to copy) | opportunity (a measured gap) |
| Agents | generic, stack-agnostic | 8 specialists with verified build commands |
| Ends when | every milestone is `[x]` | never — maintenance is continuous |

What Squad keeps: TDD halt-loops, the wiring triad, hard gates with derived verdicts, the orthogonal Codex jury, git-safety hooks, and an auditable `knowledge-base/`.

## Status

Alpha. The pipeline and its gates are implemented and covered by tests; no item has yet run end to end through this version. Everything above describes what the code does, not accumulated production evidence.

## License

MIT — see [LICENSE](LICENSE).
