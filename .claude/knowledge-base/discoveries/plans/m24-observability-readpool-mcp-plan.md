---
slug: m24-observability-readpool-mcp
version: 1.1
owner: paulo
created_at: 2026-06-30
milestone_id: M24
---

# Discovery Plan: M24 — Observability + read-scale + MCP (Go)

## Context

M24 (`ROADMAP-v2.md:151`) requires, in **own Go**, three capabilities layered onto the M23 operator
(`operator/`): (1) **runtime metrics** exposed via Prometheus/OTel, (2) **read pools** for read-scale,
(3) an **MCP server** exposing TheoDB operations to AI agents. Scope locked (2026-06-30, deployable-today
depth): domain Prometheus metrics on the operator + a read-routing Service/Pooler the operator provisions
(streaming replication is explicitly deferred to M25) + a Go MCP server. Each piece needs envtest/benchmark
evidence (project rule 5 — performance is a claim, not opinion; `rules/public-copy.md`).

The reference for metrics + pooler is **cloudnative-pg** (Apache-2.0, the M23 architectural model); the
reference for MCP is the **official Go SDK** (`modelcontextprotocol/go-sdk`, MIT→Apache-2.0 — D1 license gate
satisfied). Project rigor: `rules/discover-phd-rigor.md`.

## Objective

Produce a blueprint that, for each of metrics / read-pool / MCP, states the SOTA approach (cnpg or the MCP
SDK), the permissive Go pieces TheoDB reuses (Rule 9), the integration-test pattern, and the deployable-today
design that does NOT depend on unbuilt streaming replication. Success = every M24 design decision in the
subsequent `/to-plan` cites a blueprint pattern.

## In-scope / Out-of-scope

| Reference | In scope | Out of scope |
|---|---|---|
| `references/cloudnative-pg/` | `pkg/management/postgres/metrics/`, `api/v1/pooler_types.go`, `pkg/specs/services.go` | instance-manager, backup, replication internals |
| `references/mcp-go-sdk/` | `mcp/{server,tool,transport,streamable_server}.go`, `examples/{server,http}` | client-only paths, auth providers beyond a note |

Out-of-scope is explicit: streaming-replication read routing (M25), PgBouncer-as-external-process depth, MCP auth.

## ADRs

### D1 — One authoritative reference per capability

Metrics + pooler are studied in **cloudnative-pg** (the M23 model, Apache-2.0); MCP is studied in the
**official Go SDK** (`modelcontextprotocol/go-sdk`). Alternative rejected: a community MCP lib
(`mark3labs/mcp-go`) — rejected because the official SDK is co-maintained with the protocol authors and is
the canonical, longest-lived choice (Rule 9 — pick the battle-tested option). phd-rigor R2 (≥2 sources per
technique) is satisfied by SDK-code + SDK-example and cnpg-code + cnpg-test.

### D2 — Time budget + per-question stop

cnpg 3h, mcp-go-sdk 2h. Per-question stop = the answer shape is filled with a real `file:line` citation OR the
question is marked `blocked` with a reason. MCP perf statements carry methodology or the literal `UNBENCHMARKED`
marker (phd-rigor R3). Alternative rejected: unbounded exploration — rejected because it is investigation
theatre (cycle-discover anti-pattern).

## Research Questions

| Q | Question | Corner | Fase A (static read) | Fase B (interpret) |
|---|---|---|---|---|
| Q1 | How does cnpg test its Prometheus metrics collector (registry assertion, expected sample)? | tests | Read `references/cloudnative-pg/pkg/management/postgres/metrics/collector_test.go` | Extract the registry-assertion test skeleton TheoDB will mirror |
| Q2 | How does the MCP SDK test a server tool-call end-to-end (in-memory transport)? | tests | Read `references/mcp-go-sdk/mcp/server_test.go` | Derive the handshake+tool-call test pattern for our MCP server |
| Q3 | Which Prometheus library does controller-runtime expose for custom metrics, and is it already in the operator's dep tree (parsimony rung 4)? | deps | Grep `references/cloudnative-pg/pkg/management/postgres/metrics/collector.go` for `client_golang`; cross-check `operator/go.sum` | Confirm no new dependency is needed for domain metrics |
| Q4 | What are the Go MCP SDK's module deps, Go version, license, and transport options? | deps | Read `references/mcp-go-sdk/go.mod` + `references/mcp-go-sdk/mcp/transport.go` | Confirm permissive license (D1) + list usable transports |
| Q5 | How does cnpg expose + let Prometheus scrape metrics, and how does the M23 operator already expose `/metrics`? | tools | Read `references/cloudnative-pg/pkg/management/postgres/webserver/metricserver` + `operator/config/default/metrics_service.yaml` | Map the scrape endpoint TheoDB reuses |
| Q6 | How does the MCP SDK serve over HTTP (streamable) vs stdio, and how is a server wired in main? | tools | Read `references/mcp-go-sdk/mcp/streamable_server.go` + `references/mcp-go-sdk/examples/server` | Pick the transport for the TheoDB MCP server entrypoint |
| Q7 | How does cnpg register domain collectors (Gauge/Counter/Histogram), name+label them, and control cardinality? | techniques | Read `references/cloudnative-pg/pkg/management/postgres/metrics/collector.go` | Distil the collector + label-cardinality discipline |
| Q8 | How does cnpg model a Pooler (rw vs ro routing), and what read-only-Service primitive fits TheoDB today given NO streaming replication? | techniques | Read `references/cloudnative-pg/api/v1/pooler_types.go` + `references/cloudnative-pg/pkg/specs/services.go` | Design the deployable-today read-Service, honest about the no-replication caveat |
| Q9 | How does the Go MCP SDK define a typed tool (`AddTool` generics, `ToolHandlerFor`) and run a server? | techniques | Read `references/mcp-go-sdk/mcp/server.go` + `references/mcp-go-sdk/mcp/tool.go` | Distil the tool-registration + `Run` pattern for TheoDB ops |

Budget: 9 questions (5–10); per corner tests=2, deps=2, tools=2, techniques=3 (≥1 each, ≤3 each; techniques ≥2 per phd-rigor R4).

## Coverage Matrix

| # | Question | Corner | Method | Reference (pre-validated) |
|---|---|---|---|---|
| 1 | Q1 metrics test pattern | tests | Read | `references/cloudnative-pg/pkg/management/postgres/metrics/collector_test.go` |
| 2 | Q2 MCP server test | tests | Read | `references/mcp-go-sdk/mcp/server_test.go` |
| 3 | Q3 Prometheus dep reuse | deps | Grep | `references/cloudnative-pg/pkg/management/postgres/metrics/collector.go` |
| 4 | Q4 MCP SDK deps/license | deps | Read | `references/mcp-go-sdk/go.mod` |
| 5 | Q5 metrics scrape endpoint | tools | Read | `references/cloudnative-pg/pkg/management/postgres/webserver/metricserver` |
| 6 | Q6 MCP transports | tools | Read | `references/mcp-go-sdk/mcp/streamable_server.go` |
| 7 | Q7 domain collectors | techniques | Read | `references/cloudnative-pg/pkg/management/postgres/metrics/collector.go` |
| 8 | Q8 Pooler + read-Service | techniques | Read | `references/cloudnative-pg/api/v1/pooler_types.go` + `references/cloudnative-pg/pkg/specs/services.go` |
| 9 | Q9 MCP typed tool + Run | techniques | Read | `references/mcp-go-sdk/mcp/server.go` + `references/mcp-go-sdk/mcp/tool.go` |

All 4 corners ≥1 question; techniques has 3 (≥2 per phd-rigor R4); total 9 (within 5–10, ≤3 per corner). No corner deferred.

## Halt-loop checkpoints

Before marking a question DONE: the cited path resolved on disk AND the answer shape is filled with a real
`file:line` citation. MCP perf/throughput statements carry methodology or `UNBENCHMARKED` (phd-rigor R3).

## Acceptance Criteria

- Every question answered with ≥1 real `references/` citation OR honestly `blocked`.
- All 4 coverage corners populated; ≥1 ADR in the blueprint.
- For each of metrics/read-pool/MCP: SOTA-anchored pattern + the permissive Go reuse + the deployable-today design.

## Global Definition of Done

Blueprint scores ≥ SHIPPABLE_WITH_CAVEATS via `/discover-confidence` (no fabricated citation, all corners populated).

## Cross-references

- Cycle: `rules/cycle-discover.md` · Rigor: `rules/discover-phd-rigor.md` · Allowlist: `rules/discover-web-allowlist.txt`
- Downstream: `rules/cycle-plan.md` (the blueprint feeds `/to-plan`)
