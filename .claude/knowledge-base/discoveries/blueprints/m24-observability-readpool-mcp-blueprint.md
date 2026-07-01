---
slug: m24-observability-readpool-mcp
version: 1.0
owner: paulo
created_at: 2026-06-30
milestone_id: M24
sources:
  - references/cloudnative-pg (Apache-2.0)
  - references/mcp-go-sdk (MIT→Apache-2.0)
---

# Blueprint: M24 — Observability + read-scale + MCP (Go)

## Objective

Give `/to-plan` a SOTA-anchored, citation-backed design for the three M24 capabilities — domain Prometheus
metrics, a read-routing Service, and an MCP server — each reusing permissive Go pieces (Rule 9) and deployable
today without streaming replication. Success: every M24 implementation decision cites a pattern below.

## Context

M24 layers three own-Go capabilities onto the M23 operator (`operator/`): domain **Prometheus metrics**,
a **read-routing Service** (read-scale infra), and an **MCP server** exposing TheoDB operations to AI agents.
Scope locked deployable-today (streaming replication → M25). Two references: **cloudnative-pg** (metrics +
pooler, Apache-2.0) and the **official Go MCP SDK** (`modelcontextprotocol/go-sdk`, MIT→Apache-2.0). Every
performance statement here is methodology-bound or marked `UNBENCHMARKED` (phd-rigor R3 / `rules/public-copy.md`).

## Coverage Corner 1 — Integration Tests

**Q1 — cnpg metrics test pattern.** cnpg's collector is a `prometheus.Collector` (`Describe`/`Collect`); its
tests build the collector, register it, and assert exported samples. The canonical Go-ecosystem way to assert a
registry's output is `prometheus/client_golang/prometheus/testutil.CollectAndCompare` /
`GatherAndCount`. Evidence: `references/cloudnative-pg/pkg/management/postgres/metrics/collector_test.go`
(suite over the `QueriesCollector`), with the collector defined at
`references/cloudnative-pg/pkg/management/postgres/metrics/collector.go:318` (`Describe`) and gauge/counter
construction at `:351-371`. **TheoDB pattern:** register the domain collector to the controller-runtime
registry in a test, drive a reconcile, then `testutil.CollectAndCompare` the expected gauge/counter values.

**Q2 — MCP SDK server tool-call test.** The SDK tests connect an in-memory client↔server transport pair and
exercise `AddTool` → `CallTool`. Evidence: `references/mcp-go-sdk/mcp/server_test.go:523` (`TestAddTool`
registers `func(ctx, *CallToolRequest, In) (*CallToolResult, Out, error)` handlers) and `:548`
(`TestAddToolNameValidation`). **TheoDB pattern:** a std `testing` test that creates the MCP server, registers
the TheoDB tools, connects an in-memory transport, calls a tool, and asserts the structured result — no network.

## Coverage Corner 2 — Dependencies

**Q3 — Prometheus library (no new dep).** cnpg instruments with
`github.com/prometheus/client_golang/prometheus` (`references/cloudnative-pg/pkg/management/postgres/metrics/collector.go:37`).
controller-runtime exposes a shared registry at `sigs.k8s.io/controller-runtime/pkg/metrics` (`metrics.Registry`),
already pulled in by the M23 operator (controller-runtime v0.23.1). `prometheus/client_golang` is already in
`operator/go.sum`. **→ domain metrics add ZERO new dependencies** (parsimony ladder rung 4).

**Q4 — MCP SDK deps + license + transports.** `references/mcp-go-sdk/go.mod`: `module
github.com/modelcontextprotocol/go-sdk`, `go 1.25.0`; deps = `google/jsonschema-go`, `golang-jwt/jwt`,
`segmentio/encoding`, `yosida95/uritemplate`, `golang.org/x/{oauth2,time,tools}`, `google/go-cmp` — all
permissive (BSD/MIT/Apache — D1 satisfied; no AGPL). Transports: **stdio** (`mcp.NewStdioTransport`) and
**streamable HTTP** (`mcp/streamable_server.go`). go 1.25.0 ≤ operator's 1.25.3 → compatible.

## Coverage Corner 3 — Tools

**Q5 — metrics scrape endpoint.** The M23 operator already scaffolds a metrics Service + a `/metrics` endpoint
guarded by controller-runtime's metrics server (`operator/config/default/metrics_service.yaml`). cnpg exposes
its own metrics webserver at `references/cloudnative-pg/pkg/management/postgres/webserver/metricserver`.
**TheoDB:** reuse the EXISTING controller-runtime `/metrics` endpoint — domain collectors registered to
`metrics.Registry` are scraped through the already-deployed Service; no new endpoint, only a `ServiceMonitor`
note for Prometheus-Operator users.

**Q6 — MCP transport wiring.** The SDK example flips between stdio and streamable HTTP by a flag
(`references/mcp-go-sdk/examples/server/everything/main.go:30` `-http` addr → streamable HTTP, else stdin/stdout)
and registers tools with `mcp.AddTool(server, &mcp.Tool{Name, Description}, handler)` (`:63-71`), then
`server.Run(ctx, transport)`. **TheoDB:** a `cmd/theodb-mcp` entrypoint defaulting to stdio (the agent-spawn
contract) with an optional `-http` for the platform edge.

## Coverage Corner 4 — Techniques

**Q7 — domain collectors + cardinality.** cnpg constructs typed metrics with
`prometheus.NewGauge/NewCounterVec/NewGauge` (`collector.go:351-371`) and implements
`Describe(ch chan<- *prometheus.Desc)` / `Collect` (`:318`). Label discipline: counters carry a small fixed
label set (e.g., per-query name), never unbounded user values. **TheoDB metric set (own Go, bounded cardinality):**
`theodb_reconcile_total{result}` (counter), `theodb_reconcile_duration_seconds` (histogram),
`theodb_cluster_phase{phase}` (gauge 0/1), `theodb_cluster_ready_instances` / `_desired_instances` (gauges) —
labels are closed enumerations (result∈{success,error}, phase∈{Initializing,Healthy,Error}), so cardinality
is constant per cluster.

**Q8 — Pooler rw/ro + read-Service.** cnpg models a `Pooler` with `PoolerType` ∈ `rw` (primary), `ro`
(replicas), `r` (all) — `references/cloudnative-pg/api/v1/pooler_types.go:56-66`, `PoolerSpec.Type` at `:97`.
Read routing is realized as dedicated Services: `CreateClusterReadService` (all ready pods) /
`CreateClusterReadOnlyService` (replicas only) — `references/cloudnative-pg/pkg/specs/services.go:75-104`.
**TheoDB deployable-today design (honest, no replication yet):** the operator provisions a third Service —
a **read Service** `<name>-ro` selecting all ready pods (the `CreateClusterReadService` shape). Today, with no
streaming replication, every pod is independent so the read Service is a read-*scale-out endpoint* the
application can target; when M25 adds replication, the same Service narrows to replica-ordinal pods. The
"pool" is the K8s Service's L4 load-balancing across ready endpoints (KISS — no PgBouncer process in M24).

**Q9 — MCP typed tool + Run.** `mcp.NewServer(&mcp.Implementation{Name,...}, opts)`
(`references/mcp-go-sdk/mcp/server.go:181`); `mcp.AddTool[In, Out](server, &mcp.Tool{Name, Description}, handler)`
where `handler func(ctx, *CallToolRequest, In) (*CallToolResult, Out, error)` — the SDK auto-derives the JSON
schema from `In`/`Out` (`server.go:560`); `server.Run(ctx, transport)` (`server.go:1255`). **TheoDB tools
(read-only, safe):** `list_clusters` (→ the operator's client `List`), `get_cluster{name,namespace}` (→ `Get`).
Typed In/Out structs give automatic schema + validation; the handler reuses the same controller-runtime client
the CLI builds.

## ADRs

### D1 — Reuse controller-runtime's metrics registry; do NOT run a separate Prometheus server

Register domain collectors to `sigs.k8s.io/controller-runtime/pkg/metrics.Registry`. Alternative rejected:
a bespoke `http.Server` on a second port — rejected because the operator already exposes a guarded `/metrics`
(M23 scaffold), so a second server is redundant indirection (KISS + parsimony rung 4). Zero new dependency.

### D2 — Read "pool" = a K8s read Service, not a PgBouncer process (in M24)

The read-scale primitive is a `<name>-ro` Service load-balancing ready pods. Alternative rejected: deploying
PgBouncer (cnpg's Pooler runs a pgbouncer Deployment) — rejected for M24 because (a) it adds an external
process + image to manage, and (b) connection-pool value is marginal until streaming replication exists (M25).
The Service is the smallest deployable read endpoint today; PgBouncer is reconsidered in M25 with replication.

### D3 — MCP server is read-only over stdio by default

The TheoDB MCP server exposes only read tools (`list_clusters`, `get_cluster`) over stdio by default, optional
streamable HTTP. Alternative rejected: write tools (apply/delete) — rejected because an AI-agent surface that
can mutate cluster state is a security escalation that needs the M23 auth/RBAC story extended first (out of
M24 scope; honest deferral). stdio is the canonical agent-spawn transport; HTTP is opt-in for the platform edge.

## Recommendations

1. **Metrics** — own `internal/metrics` package: typed collectors registered to controller-runtime's registry;
   instrument the reconciler (counter+histogram+gauges). Benchmark = `testutil.CollectAndCompare` after a real
   envtest reconcile (the value is asserted, not eyeballed).
2. **Read Service** — extend `resources.go` with `buildReadService` (`<name>-ro`, selects ready pods) + ensure
   it; envtest asserts creation + owner ref. Honest doc: read-scale is endpoint-level until M25 replication.
3. **MCP server** — own `cmd/theodb-mcp` + `internal/mcpserver` package using the official Go SDK; two read
   tools; std `testing` in-memory transport test (handshake + tool call). Benchmark = tool-call latency over the
   in-memory transport (p50/p95 across N calls) — a real, reproducible number.

## Cross-cutting comparison

| Capability | SOTA reference | Permissive reuse | TheoDB deployable-today | Deferred |
|---|---|---|---|---|
| Metrics | cnpg collector + client_golang | controller-runtime registry (no new dep) | domain collectors on `/metrics` | per-query SQL metrics (needs instance manager) |
| Read-pool | cnpg Pooler rw/ro + read Service | K8s Service L4 LB | `<name>-ro` read Service | PgBouncer + replica-only routing (M25) |
| MCP | official Go SDK | the SDK itself (permissive) | read-only stdio MCP server | write tools + auth (post-M23 auth story) |

## Cross-references

- Discovery plan: `.claude/knowledge-base/discoveries/plans/m24-observability-readpool-mcp-plan.md`
- Rigor: `rules/discover-phd-rigor.md` · Cycle: `rules/cycle-discover.md`
- Downstream: `rules/cycle-plan.md` (this blueprint feeds `/to-plan`)
