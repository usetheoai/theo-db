# Review — M24 Observability + read-scale + MCP (Go)

**Date:** 2026-07-01 · **Slug:** m24-observability-readpool-mcp · **Branch:** develop
**Verdict:** READY_TO_MERGE

## Process

6 specialist agents reviewed the first cut (commits `472bea9`, `daa0034`, `e22a243`): architecture,
tests, wiring, cross-validation, observability-correctness, MCP-security. 3 returned READY_TO_MERGE
(architecture, wiring, cross-validation); 3 flagged real HIGH/MEDIUM findings. The batch was fixed
(`31a7b7d`, `<msg-guard>`) and re-reviewed by 3 agents — **all returned READY_TO_MERGE**.

## Findings consolidated (severity matrix)

| # | Severity | Finding | Resolution | Status |
|---|---|---|---|---|
| F1 | HIGH | Per-cluster gauge series leaked on CR deletion → stale exports + unbounded cardinality + false alerts | `metrics.DeleteCluster` called on the NotFound branch (`req.NamespacedName` carries the labels); unit + envtest guards | FIXED |
| F2 | MEDIUM | `Error` phase from an invalid spec never reached `theodb_cluster_phase` | `RecordPhase` routed through `setPhase` (single phase-write chokepoint) | FIXED |
| F3 | MEDIUM | Plan Failure-scenarios promised but untested: MCP list-error → IsError; transport-close → clean shutdown (EC-5) | `TestMCP_ListClusters_ClientError` (interceptor.Funcs) + `TestMCP_TransportClose_CleanShutdown` (done-channel) | FIXED |
| F4 | MEDIUM | `-http` transport unauthenticated/no-TLS not documented | Flag help + startup log + benchmark doc warn explicitly (edge-front only) | FIXED |
| F5 | LOW | Raw K8s errors forwarded to the agent → leak SA identity/RBAC | Generic tool error messages ("see operator logs") | FIXED |
| F6 | LOW | `http.ListenAndServe` no timeouts (Slowloris / gosec G114) | `&http.Server{ReadHeaderTimeout: 10s}` | FIXED |
| F7 | LOW | Generic error message not locked by a test (could silently regress) | Security regression guard asserts the raw error is absent from the result | FIXED |
| — | INFO | `theodb_reconcile_*` overlaps controller-runtime's built-in reconcile metrics (intentional, product-namespaced) | documented | ACCEPTED |
| — | INFO | Benchmark ~208µs is hardware-sensitive (re-runs 270–314µs under load) — doc honestly labels it a "floor" | no overclaim | ACCEPTED |

## Gate evidence (post-fix, 2026-07-01)

- `make test` exit 0 — metrics 90.9% / mcpserver 82.1% / controller 71.9% coverage, real envtest gates, `-race` clean
- `golangci-lint run ./...` → 0 issues · `deadcode ./...` → none · `gofmt` clean · `go vet` clean
- **`govulncheck ./...` → 0 reachable vulnerabilities** (toolchain go1.25.11; `x/net`@v0.55.0; `otel/sdk`@v1.40.0)
- 3 binaries build (`manager`, `theodbctl`, `theodb-mcp`); MCP tool-call benchmark ~208 µs/op
- License (D1): MCP SDK MIT→Apache-2.0; no AGPL

## Verdict rationale

No BLOCKER, no open HIGH, no open MEDIUM (F1–F7 fixed). Per `cycle-review.md` (READY_TO_MERGE = no
BLOCKER, ≤2 HIGH with mitigation), the milestone is **READY_TO_MERGE**.

## Deferred (M25 follow-ups, honestly documented)

- Read-pool real read-scale (streaming replication + replica-only routing / PgBouncer) — ADR-2.
- MCP write tools (apply/delete) — need the auth story extended first — ADR-3.
