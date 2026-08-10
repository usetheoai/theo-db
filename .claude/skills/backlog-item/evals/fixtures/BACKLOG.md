# Backlog — single maintenance registry for the Theo ecosystem

> **Eval fixture.** Synthetic registry used by the Squad skill eval batteries. Not the real
> umbrella registry. The items are coherent with one another so an eval can exercise dedup,
> routing and status transitions without depending on production state.

Ids are monotonic and **never renumbered** — a killed item keeps its number forever.

## How an item gets here

Two producers, one schema: `/backlog-item` (the human door, a hypothesis with no evidence)
and `/discover --sweep` (a measured finding, evidence already attached). The schema contract
lives in `rules/cycle-backlog.md`; this file is data.

## Domain routing

| Domain | Repos | Specialist |
|---|---|---|
| `engine-go` | `theo` | `agents/engine-go.md` |
| `control-plane` | `theo-cloud`, `theo-traefik-mcp` | `agents/control-plane.md` |
| `data-plane-ts` | `theo-memory`, `theo-rag`, `theo-lens`, `theo-trust`, `theo-skills`, `theo-promptly` | `agents/data-plane-ts.md` |
| `theo-db` | `theo-db` | `agents/theo-db.md` |
| `infra-terraform` | `theo-infra-modules`, `theo-infra-live` | `agents/infra-terraform.md` |
| `contracts-auth` | `theo-contracts` | `agents/contracts-auth.md` |
| `frontend-dashboard` | `theo-cloud/dashboard` | `agents/frontend-dashboard.md` |
| `platform-cli` | `theo-cli`, `theo-storage` | `agents/platform-cli.md` |

## Items

## B-007 — Suspected N+1 in theo-rag ingest   [ ]

> Registered 2026-05-20 by `/backlog-item` (slug: `theo-rag-ingest-n-plus-one`).

domain: data-plane-ts
repo: theo-rag
suggested_mode: evolve
source: human
evidence: none-yet
why_now: large-batch ingest felt slow in May
status: killed
kill_reason: measured 2026-05-28 — ingest issues one batched query regardless of document count (`theo-rag/src/ingest/batch.ts:142`). The hypothesis did not hold.
dod:
  - ingest issues a query count independent of document count
## B-009 — Session cache survives a tenant switch   [x]

> Registered 2026-06-11 by `/backlog-item` (slug: `dashboard-tenant-cache-leak`).

domain: frontend-dashboard
repo: theo-cloud/dashboard
suggested_mode: bug
source: human
evidence: `dashboard/src/state/session.ts:88`
why_now: a user saw data from the previous tenant after switching accounts
status: shipped
dod:
  - switching tenant clears the session cache
  - a regression test covers the switch

## B-014 — Reduce round-trips in the trace listing endpoint   [ ]

> Registered 2026-07-30 by `/backlog-item` (slug: `theo-lens-listing-round-trips`).

domain: data-plane-ts
repo: theo-lens
suggested_mode: review
source: human
evidence: none-yet
why_now: the dashboard started loading 30d by default in 2026-07 and the listing became visibly slower
status: raw
dod:
  - the listing issues a query count independent of span count
  - a regression test fails on the current state

## B-018 — Trace listing returns 500 on the 30-day window   [ ]

> Registered 2026-08-01 by `/backlog-item` (slug: `theo-lens-listing-500`).

domain: frontend-dashboard
repo: theo-cloud/dashboard
suggested_mode: live-test
source: human
evidence: none-yet
why_now: reported by two internal users after the 2026-07-28 deploy
status: raw
dod:
  - the listing responds 200 with a 30d window
  - the cause is attributed to environment or product, with evidence

## B-021 — Auth logic duplicated in three places in theo-cloud   [ ]

> Registered 2026-08-02 by `/backlog-item` (slug: `theo-cloud-auth-duplication`).

domain: control-plane
repo: theo-cloud
suggested_mode: review
source: human
evidence: none-yet
why_now: the three copies diverged once in 2026-06 and the bug took two days to find
status: raw
dod:
  - tenant resolution has a single source of truth
  - a test covers the path that diverged

## B-022 — `theo deploy` returns exit 0 when a step fails   [ ]

> Registered 2026-08-03 by `/backlog-item` (slug: `theo-cli-exit-code`).

domain: platform-cli
repo: theo-cli
suggested_mode: bug
source: human
evidence: none-yet
why_now: a CI pipeline went green on a partial deploy on 2026-08-02
status: raw
dod:
  - `theo deploy` returns exit != 0 when any step fails
  - a regression test fails on the current state

## B-025 — Trace explorer navigation   [ ]

> Registered 2026-08-04 by `/backlog-item` (slug: `theo-lens-explorer-navigation`).
> Deliberately vague — used to exercise unfalsifiable-hypothesis detection.

domain: data-plane-ts
repo: theo-lens
suggested_mode: evolve
source: human
evidence: none-yet
why_now: recurring complaint in team conversations, never measured
status: raw
dod:
  - navigating between nested spans costs fewer steps than today

## B-030 — Dashboard latency under load   [ ]

> Registered 2026-08-04 by `/backlog-item` (slug: `dashboard-latency-under-load`).

domain: frontend-dashboard
repo: theo-cloud/dashboard
suggested_mode: live-test
source: human
evidence: none-yet
why_now: spike in complaints after the 2026-08-03 deploy
status: raw
dod:
  - load p95 measured, with window and conditions declared

## B-031 — Seemingly unused function in theo-contracts   [ ]

> Registered 2026-08-05 by `/backlog-item` (slug: `theo-contracts-unused-helper`).

domain: contracts-auth
repo: theo-contracts
suggested_mode: review
source: human
evidence: none-yet
why_now: turned up in a grep during another investigation; no local caller
status: raw
dod:
  - the function has a confirmed caller, or is removed with consumers verified

