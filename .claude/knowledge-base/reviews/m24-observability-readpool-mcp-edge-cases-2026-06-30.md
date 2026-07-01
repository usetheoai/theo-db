# Edge Case Review — m24-observability-readpool-mcp

Date: 2026-06-30
Tasks analyzed: 4 (T1.1 metrics, T2.1 read Service, T3.1 MCP, T4.1 validation)
Cases found: 6 (EDGE: 2, NEGATIVE: 4 | MUST FIX: 2, SHOULD TEST: 3, DOCUMENT: 1)

## MUST FIX

### EC-1: Metric double-registration panics the operator at startup
- **Affected task:** T1.1
- **Kind:** NEGATIVE (invalid state)
- **Family:** State
- **Scenario:** `prometheus.MustRegister` / registry double-register panics if `Register()` runs twice (e.g., a test re-imports the package, or a future second manager). controller-runtime's `metrics.Registry` is process-global.
- **Impact:** operator crashloop OR test panic.
- **Suggested fix:** use `prometheus.Register` (returns `AlreadyRegisteredError`) and ignore that specific error, OR `sync.Once`. Already in plan TDD (`TestMetrics_Register_Idempotent`) — keep it a MUST.

### EC-2: MCP `get_cluster` on a missing/empty name must not panic
- **Affected task:** T3.1
- **Kind:** NEGATIVE (invalid input)
- **Family:** Input
- **Scenario:** an agent calls `get_cluster` with an empty `name` or a non-existent cluster; a naive handler derefs a nil result.
- **Impact:** server panic / dropped session.
- **Suggested fix:** validate `name != ""` → typed MCP error result; map `apierrors.IsNotFound` → a clear not-found tool result (no Go error that kills the session). Already in plan TDD (`TestMCP_GetCluster_NotFound`).

## SHOULD TEST

### EC-3: Histogram bucket choice for reconcile duration
- **Affected task:** T1.1
- **Kind:** EDGE (boundary)
- **Suggested test:** `TestMetrics_DurationBuckets` — assert the histogram uses sane buckets (e.g., `prometheus.DefBuckets` or ms-scale) so sub-second reconciles aren't all in one bucket.

### EC-4: read Service selects pods even when zero are ready
- **Affected task:** T2.1
- **Kind:** EDGE (empty-but-valid)
- **Suggested test:** `TestReconcile_CreatesReadService` already covers creation; assert the selector matches the cluster labels so an empty-endpoints Service is still valid (no error when 0 ready).

### EC-5: MCP server clean shutdown on transport close
- **Affected task:** T3.1
- **Kind:** NEGATIVE (failure)
- **Suggested test:** `TestMCP_Handshake` + a close → assert `server.Run` returns without a goroutine leak (failure-scenario row already lists this).

## DOCUMENT

### EC-6: read Service gives no real read-scale until replication (M25)
- **Kind:** EDGE
- **Accepted risk:** documented in ADR-2 + the benchmark doc; the endpoint is real infra, the read-scale value lands with M25 streaming replication. No code change.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 | 1 | 1 | 1 | 0 |
| T2.1 | 1 | 0 | 0 | 1 | 1 |
| T3.1 | 0 | 3 | 1 | 1 | 0 |
| T4.1 | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** both MUST-FIX items are already in the plan's TDD; the SHOULD-TEST items are absorbed into the
respective tasks' test lists at implement time.

**Verdict:** PLAN OK (both MUST-FIX already covered by plan TDD; no plan rewrite needed).
