# Deps Audit: m24-observability-readpool-mcp

**Date:** 2026-06-30
**Mode:** plan-bound:m24-observability-readpool-mcp
**Verdict:** PASS (after implement-time toolchain + dependency bumps — `govulncheck ./...` reports 0 reachable)
**Hard caps triggered:** [] (no CRITICAL/HIGH on a declared dep)

## Implement-time resolution (govulncheck → 0 reachable)

`govulncheck ./...` on the operator initially flagged 23 reachable vulns — all in the Go **stdlib**
(crypto/x509, net/http2, mime, html/template, net/mail) + transitive `golang.org/x/net` + `otel/sdk`, none in
our code or the MCP SDK's own surface. Resolved cleanly (no allowlist, no suppression):

- `toolchain go1.25.11` (was 1.25.3) — fixes the stdlib advisories.
- `golang.org/x/net` → v0.55.0 (was v0.47.0).
- `go.opentelemetry.io/otel/sdk` (+otel, metric, trace) → v1.40.0 (was v1.36.0).

Final: **`Your code is affected by 0 vulnerabilities.`** `make test` exit 0 after the bumps.

## Summary

- Ecosystem: Go (operator module).
- New declared dep: `github.com/modelcontextprotocol/go-sdk` **v1.6.1** (latest stable; v1.7.0-pre.1 is a pre-release).
- Existing reused: `prometheus/client_golang` (transitive, no version bump), `controller-runtime` v0.23.1.
- Scanner: `osv-scanner --lockfile=go.mod` on the SDK module.

## Findings

| Finding | Severity | Against | Disposition |
|---|---|---|---|
| 14× `GO-2026-*` advisories | (stdlib) | `stdlib` @ **1.25.0** (the SDK's declared minimum go version) | **Not applicable to our build** — the operator pins `toolchain go1.25.3`, which post-dates these stdlib fixes. The binary links go1.25.3 stdlib, not 1.25.0. |
| MCP SDK own deps (`jsonschema-go`, `golang-jwt/jwt/v5`, `segmentio/encoding`, `uritemplate`, `x/oauth2`, `x/time`, `x/tools`) | — | the SDK's direct deps | **No CVE reported** by osv-scanner. All permissive (BSD/MIT/Apache). |

## License (D1 gate)

- `modelcontextprotocol/go-sdk`: MIT → Apache-2.0 transition (both permissive). **No AGPL.** ✓
- All transitive deps: BSD/MIT/Apache. ✓

## Rule 9 evaluation (plan Dependencies § New)

- Evaluated `mark3labs/mcp-go` (community) — rejected for the official co-maintained SDK.
- Evaluated hand-rolling JSON-RPC — rejected (reinvents the MCP spec).

## Caveat (why PASS_WITH_CAVEATS, not PASS)

The SDK's `go.mod` declares `go 1.25.0`; osv-scanner attributes the go1.25.0 stdlib advisories to it. These are
mitigated by our `toolchain go1.25.3` pin. The implement phase MUST confirm `govulncheck ./...` on the operator
(with the SDK added) is clean against the 1.25.3 toolchain — recorded in the M24 benchmark/validation evidence.

## Recommended next step

Proceed to `/plan-confidence` (already SHIPPABLE 96.4). At implement time, run `govulncheck ./...` after `go get
github.com/modelcontextprotocol/go-sdk@v1.6.1` and record a clean result.
