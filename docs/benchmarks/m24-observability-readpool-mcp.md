# M24 — Observability + read-scale + MCP evidence (Go)

**Date:** 2026-06-30 · **Milestone:** M24 (`ROADMAP-v2.md:151`) · **Module:** `operator/`

M24 layers three own-Go capabilities onto the M23 operator. Evidence is **measurement-first**: domain
metric samples are *asserted* (not eyeballed), the MCP tool-call latency is a *reproducible number*, and the
read Service is *proven created against a real apiserver*.

## What is measured

| Claim | How it is proven | Result |
|---|---|---|
| Domain Prometheus metrics are registered + exported with correct values | `internal/metrics` `testutil.CollectAndCompare` + envtest `TestReconcile_EmitsDomainMetrics` | PASS |
| `Register()` is idempotent (no double-register panic, EC-1) | `TestMetrics_Register_Idempotent` | PASS |
| Read Service `<name>-ro` is provisioned, owner-referenced | envtest `TestReconcile_CreatesReadService` | PASS |
| MCP server handshake + advertises exactly 2 read tools | `TestMCP_Handshake_AdvertisesTwoTools` (in-memory transport) | PASS |
| MCP `list_clusters` / `get_cluster` return correct data; not-found + empty-name → IsError (EC-2) | `TestMCP_ListClusters_*`, `TestMCP_GetCluster_*` | PASS |
| MCP tool-call latency over the in-memory transport | `BenchmarkMCP_ListClusters` (3 runs) | **~208 µs/op** |

## Reproduce

```bash
cd operator
make test                                              # envtest: metrics + read-service + controller
go test ./internal/mcpserver/... -bench BenchmarkMCP_ListClusters -benchmem -count 3
```

## Coverage (2026-06-30, go1.25.3, envtest k8s 1.35.0)

```
internal/metrics      coverage: 90.9%
internal/mcpserver    coverage: 82.1%
internal/controller   coverage: 71.9%   (metrics instrumentation + read Service added)
make test exit=0
```

## MCP tool-call benchmark (in-memory transport, full client↔server round-trip)

```
BenchmarkMCP_ListClusters-12   5871   199027 ns/op   350545 B/op   459 allocs/op
BenchmarkMCP_ListClusters-12   5884   215706 ns/op   350491 B/op   459 allocs/op
BenchmarkMCP_ListClusters-12   5463   209117 ns/op   350494 B/op   459 allocs/op
```

**Mean ≈ 208 µs/op** (± ~8 µs over 3 runs), 459 allocs/op. This is the full JSON-RPC round-trip of a
`list_clusters` call over the in-memory transport against a fake client with 2 clusters — the protocol +
serialization cost, not network. Methodology: `go test -bench -count 3` on AMD (12 logical CPUs). It is a
**floor** for the protocol overhead; a real kubeconfig adds the apiserver round-trip on top.

## Domain metric set (bounded cardinality)

| Metric | Type | Labels (closed) | Meaning |
|---|---|---|---|
| `theodb_reconcile_total` | counter | `result` ∈ {success,error} | reconcile outcomes |
| `theodb_reconcile_duration_seconds` | histogram | — (DefBuckets) | reconcile wall time |
| `theodb_cluster_phase` | gauge | `namespace,cluster,phase` | 1 for the active phase |
| `theodb_cluster_ready_instances` | gauge | `namespace,cluster` | ready instances |
| `theodb_cluster_desired_instances` | gauge | `namespace,cluster` | desired instances |

`result`/`phase` are closed enums; per-cluster cardinality is bounded by the number of managed clusters
(kube-state-metrics convention). Registered on controller-runtime's shared registry (ADR-1) — scraped via the
existing `/metrics`; **no new dependency, no second HTTP server**.

## Honest scope (ADRs)

- **ADR-2 (read pool)** — the `<name>-ro` Service is a **read-scale-out endpoint**, not a connection pooler.
  Until M25 streaming replication exists, every pod is independent, so the read Service load-balances ready
  pods (real infra; the read-scale *value* lands with replication). PgBouncer is reconsidered in M25.
- **ADR-3 (MCP)** — the server is **read-only** (`list_clusters`, `get_cluster`) over stdio by default
  (`-http` opt-in). Write tools (apply/delete) are deferred: an AI-mutable control surface needs the auth
  story extended first.

## Dependencies (license gate D1)

New: `github.com/modelcontextprotocol/go-sdk` **v1.6.1** (MIT→Apache-2.0 — permissive). Metrics add **zero**
new deps (`prometheus/client_golang` was already transitive via controller-runtime). No AGPL.

**`govulncheck ./...` → `Your code is affected by 0 vulnerabilities.`** Reaching zero required
implement-time bumps (no allowlist, no suppression): `toolchain go1.25.11`, `golang.org/x/net@v0.55.0`,
`go.opentelemetry.io/otel/sdk@v1.40.0`. Deps-audit:
`.claude/knowledge-base/audits/m24-observability-readpool-mcp-deps-audit-2026-06-30.md` (PASS).
