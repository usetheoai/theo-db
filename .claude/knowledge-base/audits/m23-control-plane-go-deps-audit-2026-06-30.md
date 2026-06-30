# Deps Audit: m23-control-plane-go

**Date:** 2026-06-30
**Mode:** plan-bound:m23-control-plane-go
**Verdict:** PASS_WITH_CAVEATS
**Hard caps triggered:** _none_ (no CVE on a declared dep; plan `## Dependencies` complete with Rule-9 evaluation)

## Summary

- Ecosystems: go (NEW `operator/` module — not yet scaffolded), rust + python (existing, untouched by M23)
- New deps introduced by M23 (Go, standard K8s ecosystem): `sigs.k8s.io/controller-runtime`, `k8s.io/api`,
  `k8s.io/apimachinery`, `k8s.io/client-go`, `github.com/spf13/cobra` — all **Apache-2.0**
- AGPL/GPL: **NONE** (the DoD's "no external dep beyond the standard K8s ecosystem" is satisfied; ginkgo/gomega
  deliberately OMITTED — std `testing` used instead, ADR D2)
- CVE scan status: **deferred to implement** — `govulncheck ./...` runs on the REAL `operator/go.mod` once
  kubebuilder scaffolds it (the module does not exist yet); the post-implementation validation gate enforces it

## Plan validation (Mode 2)

| Plan dep | Section | Standard K8s? | License | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| `sigs.k8s.io/controller-runtime` | New | yes | Apache-2.0 | yes (raw client-go informers rejected — controller-runtime is the standard) | OK |
| `k8s.io/api` / `apimachinery` / `client-go` | New | yes | Apache-2.0 | yes (the ecosystem — no alternative) | OK |
| `github.com/spf13/cobra` | New | yes (kubebuilder-standard) | Apache-2.0 | yes (stdlib flag rejected — cobra is the standard CLI lib) | OK |
| (ginkgo/gomega) | New — **REJECTED** | n/a | MIT | yes (std `testing` + envtest chosen, fewer deps, ADR D2) | OK (not added) |
| Go toolchain / kubebuilder / controller-gen / setup-envtest | Existing (tooling) | yes | Apache-2.0/BSD | n/a | OK |

## Caveats (why PASS_WITH_CAVEATS)

1. **CVE scan deferred** — the Go module does not exist yet (scaffolded in implement Phase 1). `govulncheck` runs
   on the real `operator/go.mod` during the implement validation; the implement plan's Final Phase includes
   `go vet`/build/test. This is honest: no CVE scan can run before the manifest exists.
2. The declared deps are the canonical K8s ecosystem (Apache-2.0, widely audited). Versions are pinned by
   kubebuilder's scaffold to a mutually compatible set (controller-runtime v0.21+ / k8s.io v0.31+).

## Recommended next steps

1. Scaffold the module (implement Phase 1); pin versions via `kubebuilder init`.
2. Run `cd operator && govulncheck ./...` in the implement validation; treat any HIGH/CRITICAL as a blocker.
3. Proceed to `/plan-confidence`.
