---
slug: m24-observability-readpool-mcp
milestone_id: M24
created_at: 2026-06-30
goal: Ship own-Go domain Prometheus metrics, a read-routing Service, and a read-only MCP server on the M23 operator, proven by operator envtest tests + an MCP tool-call benchmark, all green via `cd operator && make test`.
---

# Plan: M24 — Observability + read-scale + MCP (Go)

> **Version 1.0** — Layer three deployable-today capabilities onto the M23 operator (`operator/`):
> (1) **domain Prometheus metrics** registered to controller-runtime's registry (no new dep), (2) a
> **read-routing Service** `<name>-ro` the operator provisions, (3) a **read-only MCP server**
> (`cmd/theodb-mcp`) using the official Go SDK. Streaming replication is deferred to M25 (honest scope).

## Goal

Ship own-Go domain Prometheus metrics, a read-routing Service, and a read-only MCP server on the M23 operator,
proven by operator envtest tests + an MCP tool-call benchmark, all green via `cd operator && make test`
(single observable metric: the new test suite is green AND `docs/benchmarks/m24-observability-readpool-mcp.md`
records the MCP tool-call latency + the asserted metric samples).

## Context

Built directly on M23 (`operator/`, v0.23.0). The blueprint
(`.claude/knowledge-base/discoveries/blueprints/m24-observability-readpool-mcp-blueprint.md`, SHIPPABLE 100)
anchors every decision: metrics on cnpg's collector pattern, read-Service on cnpg's `CreateClusterReadService`,
MCP on the official Go SDK. Scope locked deployable-today (2026-06-30): no streaming replication (M25).

## Baseline Context

### Files that will be touched

| File | LoC today | git sha | Role | Change |
|---|---|---|---|---|
| `operator/internal/controller/resources.go` | 127 | `7661e6f` | pure builders (SS + 2 Services) | + `buildReadService` |
| `operator/internal/controller/theodbcluster_controller.go` | 226 | `7661e6f` | reconciler | instrument with metrics; `ensureReadService`; `Owns` already covers Service |
| `operator/cmd/main.go` | 204 | `7661e6f` | manager composition root | register the metrics collector |
| `operator/internal/metrics/metrics.go` | 0 (NEW) | — | own domain collectors | new package |
| `operator/internal/mcpserver/server.go` | 0 (NEW) | — | MCP server + tools | new package |
| `operator/cmd/theodb-mcp/main.go` | 0 (NEW) | — | MCP entrypoint (stdio/http) | new binary |
| `operator/go.mod` | 93 | `9b3a405` | module manifest | + `modelcontextprotocol/go-sdk` |

### Current callers / dependents

- `Reconcile` (`theodbcluster_controller.go:50`) calls `ensureHeadlessService`/`ensureService`/`ensureStatefulSet`/`updateStatus`. The metric instrumentation wraps these; `ensureReadService` is added alongside.
- The MCP server reuses the SAME controller-runtime client builder pattern as `cmd/theodbctl/main.go:newClient` (kubeconfig → typed client). DIP: `internal/mcpserver` depends on `client.Client`, wired at `cmd/theodb-mcp/main.go`.
- controller-runtime's `metrics.Registry` (`sigs.k8s.io/controller-runtime/pkg/metrics`) is the existing scrape target (M23 `cmd/main.go:34-35`). Domain collectors register to it; no existing caller depends on the new packages yet (they are new).

### Architecture boundaries affected

- `internal/metrics` and `internal/mcpserver` are new internal packages; the composition roots (`cmd/main.go`, `cmd/theodb-mcp/main.go`) wire concretes (DIP per `architecture.md`). No inner→outer import is introduced: `internal/mcpserver` depends only on the `client.Client` interface + the API types, never on `cmd`.
- The read Service is added to the existing reconcile boundary (`resources.go` builders + `ensureServiceObject`); no new layer.

### Domain glossary

- **Domain metric** — a Prometheus metric about TheoDBCluster reconciliation (not Go runtime / controller-runtime built-ins).
- **Read Service** — a ClusterIP Service `<name>-ro` selecting ready pods; the read-scale endpoint (L4 LB).
- **MCP tool** — a typed `In→Out` function the Go SDK exposes to AI agents over stdio/HTTP.
- **Cardinality** — the number of label-value combinations of a metric; kept constant (closed enums only).

## Prior Art & Related Work

- Blueprint `m24-observability-readpool-mcp` (SHIPPABLE 100) — the authoritative design source.
- cnpg collector (`references/cloudnative-pg/pkg/management/postgres/metrics/collector.go`), Pooler/read-Service (`references/cloudnative-pg/pkg/specs/services.go:75`), MCP Go SDK (`references/mcp-go-sdk/mcp/server.go:181,560,1255`).

## ADRs

### ADR-1 — Domain metrics on controller-runtime's registry (no separate server)

Register collectors to `sigs.k8s.io/controller-runtime/pkg/metrics.Registry`; scrape via the existing `/metrics`.
**Alternatives:** (a) a bespoke `http.Server` on a second port — rejected: redundant indirection, the operator
already exposes a guarded `/metrics` (blueprint metrics ADR, KISS); (b) OTel SDK push exporter — rejected: adds a
collector dependency + a push pipeline nobody asked for (YAGNI); Prometheus pull is the K8s-native default.

### ADR-2 — Read "pool" is a K8s read Service, not PgBouncer (M24)

Provision a `<name>-ro` Service load-balancing ready pods. **Alternatives:** (a) a PgBouncer Deployment
(cnpg's Pooler) — rejected: external process + image to manage, marginal value before replication exists
(blueprint read-pool ADR); (b) a Go connection-pool library (pgxpool) — rejected: reinvents/duplicates a client-side
concern that belongs in the application, not the control plane (Rule 9). The Service is the smallest deployable
read endpoint; PgBouncer is reconsidered in M25.

### ADR-3 — MCP server is read-only over stdio by default

Expose only `list_clusters` + `get_cluster` over stdio (optional `-http`). **Alternatives:** (a) write tools
(apply/delete) — rejected: an AI-mutable control surface is a security escalation needing the M23 auth story
extended first (blueprint MCP ADR, out of M24 scope); (b) HTTP-only — rejected: stdio is the canonical
agent-spawn transport, HTTP is opt-in for the platform edge.

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `github.com/prometheus/client_golang` | (transitive, in go.sum) | go | domain collectors (Gauge/Counter/Histogram) — already pulled by controller-runtime |
| `sigs.k8s.io/controller-runtime` | v0.23.1 | go | the metrics registry + client (M23) |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale (libs evaluated) | Why this one |
|---|---|---|---|---|
| `github.com/modelcontextprotocol/go-sdk` (NEW) | v1.6.1 | go | Evaluated: `mark3labs/mcp-go` (community, popular — rejected: the official SDK is co-maintained with the protocol authors, longest-lived); hand-rolling JSON-RPC (rejected — reinvents the MCP spec, Rule 9) | the canonical Go MCP server SDK; permissive (MIT→Apache-2.0; Apache-only license rule); latest stable (v1.7.0-pre.1 is a pre-release) |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency Graph

```
Phase 1 (metrics) ─┐
Phase 2 (read svc) ─┼─ independent; can land in any order
Phase 3 (MCP)      ─┘
        ↓ all three
Final Phase (integration validation: make test + MCP benchmark + kind smoke)
```

## Phase 1: Domain Prometheus metrics

### T1.1 — Own `internal/metrics` package + reconciler instrumentation

#### Why this step
The operator exposes only Go-runtime + controller-runtime built-in metrics today; ops cannot see TheoDB
reconcile health. This adds bounded-cardinality domain metrics (blueprint Q7/ADR-1; cnpg `collector.go:351-371`).

#### Files to edit
```
operator/internal/metrics/metrics.go (NEW) — collectors + Register()
operator/internal/metrics/metrics_test.go (NEW) — registry assertion
operator/internal/controller/theodbcluster_controller.go — instrument Reconcile (counter/histogram/gauges)
operator/cmd/main.go — call metrics.Register() once at startup
```

#### Deep file dependency analysis
`metrics.go` depends only on `prometheus/client_golang/prometheus` + controller-runtime's `metrics.Registry`.
The reconciler records into the package-level collectors; `cmd/main.go` calls `metrics.Register()` before `mgr.Start`.

#### TDD
```
RED: TestMetrics_Register_Idempotent — metrics.Register() twice does not panic / double-register (uses a fresh registry)
RED: TestMetrics_ReconcileRecordsSamples — after recording, testutil.CollectAndCompare yields the expected counter+gauge values
RED (envtest): TestReconcile_EmitsDomainMetrics — after a reconcile, theodb_cluster_ready_instances gauge for the cluster equals 0 (no kubelet) and reconcile_total{result=success} incremented
GREEN: implement collectors + instrumentation
REFACTOR: extract a record helper; tests stay green
VERIFY: cd operator && go test ./internal/metrics/... ./internal/controller/... -run Metrics
```

#### Concurrency tests
(none — single-threaded). The collectors are registered once at startup; Prometheus' registry is internally synchronized, and the reconciler is serialized per-object by controller-runtime (no shared mutable state introduced).

#### Acceptance Criteria
- [ ] `theodb_reconcile_total{result}`, `theodb_reconcile_duration_seconds`, `theodb_cluster_phase{phase}`, `theodb_cluster_ready_instances`, `theodb_cluster_desired_instances` are registered with closed-enum labels only.
- [ ] `testutil.CollectAndCompare` asserts the exact sample values after a recorded reconcile.
- [ ] `metrics.Register()` is idempotent (no double-register panic).

#### DoD
- [ ] `go test ./internal/metrics/... ./internal/controller/...` green; collectors visible on the existing `/metrics` endpoint.

## Phase 2: Read-routing Service

### T2.1 — `buildReadService` + `ensureReadService`

#### Why this step
There is no read-scale endpoint; the application has only the rw gateway Service. This provisions a `<name>-ro`
Service load-balancing ready pods (blueprint Q8/ADR-2; cnpg `services.go:75 CreateClusterReadService`).

#### Files to edit
```
operator/internal/controller/resources.go — buildReadService (<name>-ro, selects ready pods)
operator/internal/controller/resources_test.go — pure builder test
operator/internal/controller/theodbcluster_controller.go — ensureReadService (reuse ensureServiceObject)
operator/internal/controller/theodbcluster_controller_test.go — envtest asserts the read Service
```

#### Deep file dependency analysis
`buildReadService` mirrors `buildService` (gateway) but names it `<name>-ro` and (today) selects the same ready
pods; `ensureReadService` reuses the existing `ensureServiceObject` create-or-reconcile helper + owner ref.

#### TDD
```
RED: TestBuildReadService_NameAndSelector — name is <cluster>-ro, ClusterIP, selects the cluster labels, port matches spec
RED (envtest): TestReconcile_CreatesReadService — after a reconcile the <name>-ro Service exists with one owner ref
GREEN: implement builder + ensure
REFACTOR: share the port via the existing servicePort helper
VERIFY: cd operator && KUBEBUILDER_ASSETS=... go test ./internal/controller/... -run ReadService
```

#### Concurrency tests
(none — single-threaded). Reuses the existing serialized reconcile path; no new shared state.

#### Acceptance Criteria
- [ ] A `<name>-ro` ClusterIP Service is created, owner-referenced, idempotent (no churn on re-reconcile).
- [ ] The benchmark doc honestly states read-scale is endpoint-level until M25 replication.

#### DoD
- [ ] `go test ./internal/controller/... -run ReadService` green; `Owns(&corev1.Service{})` already covers GC.

## Phase 3: Read-only MCP server

### T3.1 — `internal/mcpserver` + `cmd/theodb-mcp` (list_clusters, get_cluster)

#### Why this step
TheoDB has no AI-agent surface. This adds a read-only MCP server exposing two tools over stdio (blueprint
Q9/ADR-3; MCP SDK `server.go:181,560,1255`).

#### Files to edit
```
operator/internal/mcpserver/server.go (NEW) — NewServer(client) + list_clusters + get_cluster tools
operator/internal/mcpserver/server_test.go (NEW) — in-memory transport handshake + tool call
operator/cmd/theodb-mcp/main.go (NEW) — stdio (default) / -http transport wiring
operator/go.mod — add modelcontextprotocol/go-sdk
operator/Makefile — build-mcp target
```

#### Deep file dependency analysis
`internal/mcpserver` depends on `client.Client` (DIP) + the MCP SDK; the two tools call `List`/`Get` on the
TheoDBCluster API. `cmd/theodb-mcp` builds the client (same as `theodbctl`) and runs the server over the chosen
transport. No write tools (ADR-3).

#### TDD
```
RED: TestMCP_ListClusters_ReturnsRegistered — in-memory client↔server; a fake-client with 2 clusters → list_clusters returns 2 typed entries
RED: TestMCP_GetCluster_NotFound — get_cluster on a missing name → a typed MCP error result, no panic
RED: TestMCP_Handshake — client.Connect + initialize succeeds; server advertises the 2 tools
GREEN: implement server + tools + entrypoint
REFACTOR: share the tool-result mapping; tests stay green
VERIFY: cd operator && go test ./internal/mcpserver/...
```

#### Concurrency tests
(none — single-threaded). The MCP SDK owns transport concurrency; each tool handler is a pure read against the
client and introduces no shared mutable state. The in-memory transport test drives one call at a time.

#### Acceptance Criteria
- [ ] `list_clusters` returns the clusters from the injected client; `get_cluster{name,namespace}` returns one or a typed not-found error.
- [ ] The server registers exactly the 2 read tools (no write tools — ADR-3).
- [ ] `cmd/theodb-mcp` defaults to stdio; `-http` selects streamable HTTP.

#### DoD
- [ ] `go test ./internal/mcpserver/...` green; `make build-mcp` produces `bin/theodb-mcp`.

## Failure scenarios (external I/O — K8s API via the client; MCP transport)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| K8s API (MCP `list`/`get`) | API returns NotFound for get_cluster | fake-client without the named cluster | tool returns a typed MCP error result, never a panic |
| K8s API (MCP `list`) | client List error (transport) | fake-client wired to error on List | tool returns an error result; server stays up |
| MCP transport | client disconnects mid-session | in-memory transport closed | `server.Run` returns cleanly, no goroutine leak |
| Metrics registry | double Register() | call Register() twice in a test | idempotent — no panic, no duplicate collector |
| K8s API (reconcile) | read Service Get transient error | (covered by existing ensureServiceObject error path) | reconcile returns the error → requeue |

## Coverage Matrix

| # | Goal claim / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Domain Prometheus metrics (own Go) | T1.1 | `internal/metrics` collectors + reconciler instrumentation |
| 2 | Metrics asserted by benchmark/test | T1.1 | `testutil.CollectAndCompare` + envtest emits-metrics test |
| 3 | Read-routing Service provisioned | T2.1 | `buildReadService` + `ensureReadService` |
| 4 | MCP server (own Go, read tools) | T3.1 | `internal/mcpserver` + `cmd/theodb-mcp` |
| 5 | MCP tool-call benchmark | T3.1 + Final | in-memory tool-call latency recorded in the benchmark doc |
| 6 | No new dep for metrics; MCP dep Rule-9-evaluated | T1.1, T3.1 | controller-runtime registry reused; MCP SDK justified in Dependencies |
| 7 | Streaming replication honestly deferred (M25) | T2.1 | benchmark doc + ADR-2 state the no-replication caveat |
| 8 | All green via `cd operator && make test` | T4.1 | integration validation phase (T4.1) |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Read Service gives no real read-scale until replication (M25) | MEDIUM | ADR-2 + benchmark doc state it honestly; the endpoint is real infra, value lands in M25 | paulo |
| MCP SDK is young (API churn) | MEDIUM | pin the version; encapsulate behind `internal/mcpserver` (DIP) so a swap is localized | paulo |
| Metric cardinality blow-up if labels become unbounded | LOW | labels are closed enums only (result/phase); a test asserts the label set | paulo |
| MCP server exposes cluster topology to agents | LOW | read-only + no secrets in tool output; write tools deferred (ADR-3) | paulo |

## Unresolved Questions

- Should the MCP server be a separate binary or a subcommand of `theodbctl`? (Plan: separate `cmd/theodb-mcp` for a clean agent-spawn contract; revisit if packaging overhead is high.) Resolved at plan time: separate binary.
- (none others — every other decision is resolved at plan time.)

## Global Definition of Done

- [ ] `cd operator && make test` exit 0 (all new metrics/read-service/MCP tests green, real envtest where applicable).
- [ ] `golangci-lint run ./...` 0 issues; `deadcode ./...` none; `gofmt` clean.
- [ ] `docs/benchmarks/m24-observability-readpool-mcp.md` records: asserted metric samples, MCP tool-call latency (p50/p95 over N in-memory calls), read-Service creation proof.
- [ ] CHANGELOG `[Unreleased]` updated.
- [ ] License gate: MCP SDK is permissive (Apache-only rule); no AGPL.
- [ ] Every changed/new Go file stays within the 500-LoC file-size budget (`rules/architecture.md`); split if exceeded (as M21 split `ann.rs`).

## Final Phase: Integration Validation

### T4.1 — Integration validation + benchmark

Run `cd operator && make test` (envtest), `golangci-lint run ./...`, `deadcode ./...`; build all three binaries
(`make build`, `make build-cli`, `make build-mcp`); run the MCP tool-call benchmark; real-kind smoke of the
read Service + a `theodb-mcp` stdio handshake. Record everything in `docs/benchmarks/m24-observability-readpool-mcp.md`.
The plan is NOT complete until the full chain is green with recorded numbers.

#### Concurrency tests
(none — single-threaded). The validation runs the existing serialized suites; no new concurrent code paths.

#### Acceptance Criteria
- [ ] `make test` exit 0; `golangci-lint` 0 issues; `deadcode` none; all three binaries build.
- [ ] Benchmark doc records asserted metric samples + MCP tool-call p50/p95 + read-Service creation proof.

#### DoD
- [ ] The full chain is green with recorded numbers in the benchmark doc; CHANGELOG updated.

## Cross-references

- Blueprint: `.claude/knowledge-base/discoveries/blueprints/m24-observability-readpool-mcp-blueprint.md`
- Cycle: `rules/cycle-plan.md` → `rules/cycle-implement.md` · Conventions: `rules/architecture.md`, `rules/testing.md`, `rules/error-handling.md`, `rules/parsimony-ladder.md`
- Builds on: M23 operator (`operator/`, v0.23.0)
