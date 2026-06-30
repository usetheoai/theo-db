# M24 — Observability + read-scale + MCP — Implementation Summary

**Plan:** `.claude/knowledge-base/plans/m24-observability-readpool-mcp-plan.md` (SHIPPABLE 96.4)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m24-observability-readpool-mcp-blueprint.md` (SHIPPABLE 100)
**Milestone:** M24 (`ROADMAP-v2.md:151`) · **Date:** 2026-06-30 · **Branch:** develop

## What shipped (3 deployable-today capabilities on the M23 operator)

| Phase | Deliverable | Caller / wiring | Test (RED→GREEN) | Runtime signal |
|---|---|---|---|---|
| T1.1 metrics | `internal/metrics` collectors (counter/histogram/3 gauges) | `cmd/main.go` `dbmetrics.Register(ctrlmetrics.Registry)`; reconciler instrumented (defer + updateStatus) | `TestMetrics_*` (CollectAndCompare) + envtest `TestReconcile_EmitsDomainMetrics` | the metrics on the existing `/metrics` endpoint |
| T2.1 read Service | `buildReadService` + `ensureReadService` (`<name>-ro`) | `Reconcile` calls `ensureReadService`; `Owns(Service)` GC | `TestBuildReadService_*` + envtest `TestReconcile_CreatesReadService` | the `<name>-ro` ClusterIP Service |
| T3.1 MCP | `internal/mcpserver` (2 read tools) + `cmd/theodb-mcp` (stdio/-http) | `New(client.Client)` (DIP); entrypoint builds the client | `TestMCP_*` (in-memory transport) + `BenchmarkMCP_ListClusters` | MCP tools over stdio/HTTP |
| T4.1 validation | `make test` + 3 binaries + benchmark + govulncheck | — | the full gate | `docs/benchmarks/m24-observability-readpool-mcp.md` |

## Edge cases absorbed (from edge-case-plan)

- **EC-1** — `metrics.Register` tolerates `AlreadyRegisteredError` (no double-register panic). `TestMetrics_Register_Idempotent`.
- **EC-2** — MCP `get_cluster` on a missing/empty name returns an `IsError` tool result (typed, session survives), never a panic. `TestMCP_GetCluster_NotFound`/`_EmptyName`.
- **EC-6** — read Service has no real read-scale until M25 replication — documented (ADR-2 + benchmark doc).

## Gate results (2026-06-30)

- `make test` exit 0 — metrics 90.9%, mcpserver 82.1%, controller 71.9% coverage (real envtest gates).
- `golangci-lint run ./...` 0 issues · `deadcode ./...` none · `gofmt` clean · `go vet` clean.
- 3 binaries build: `bin/manager`, `bin/theodbctl`, `bin/theodb-mcp`.
- **`govulncheck ./...` → 0 reachable vulnerabilities** (toolchain go1.25.11; `x/net`@v0.55.0; `otel/sdk`@v1.40.0 — clean bumps, no allowlist).
- MCP tool-call benchmark: **~208 µs/op** (3 runs, in-memory round-trip).

## Honest scope (ADRs)

- **ADR-1** — domain metrics on controller-runtime's registry; no second HTTP server, no new dep.
- **ADR-2** — read "pool" = a K8s `<name>-ro` Service (not PgBouncer); read-scale value lands with M25 replication.
- **ADR-3** — MCP server is read-only over stdio by default; write tools deferred (auth story first).

## Dependencies

- New: `github.com/modelcontextprotocol/go-sdk` v1.6.1 (MIT→Apache-2.0, D1 satisfied).
- Bumped for security: `golang.org/x/net`@v0.55.0, `go.opentelemetry.io/otel/*`@v1.40.0; toolchain go1.25.11.
- Metrics: zero new deps (`prometheus/client_golang` already transitive).
