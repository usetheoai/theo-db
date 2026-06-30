# M23 — Control plane in Go — Implementation Summary

**Plan:** `.claude/knowledge-base/plans/m23-control-plane-go-plan.md` (v1.1, plan-confidence SHIPPABLE 96.4)
**Milestone:** M23 (`ROADMAP-v2.md:139`) · **Date:** 2026-06-30 · **Branch:** develop

## What shipped

A new `operator/` Go module (kubebuilder/controller-runtime v0.23.1) — TheoDB's own Kubernetes
control plane. It reconciles a `TheoDBCluster` CRD into a **StatefulSet** of N theo-db instances + a
**ClusterIP Service** gateway, owner-referenced, with status. Plus a `theodbctl` cobra CLI and a
reproducible `config/` kustomize deploy.

## Tasks → evidence (wiring triad per task)

| Task | Deliverable | Caller / wiring | Test (RED→GREEN) | Runtime signal |
|---|---|---|---|---|
| T1.1 | `api/v1/theodbcluster_types.go` — CRD spec/status + deepcopy + CRD manifest | manager registers scheme; CRD installed into real apiserver | `make manifests` generates CRD; envtest loads it | CRD `theodbclusters.core.theodb.io` (printcolumns Instances/Ready/Phase) |
| T2.1 | `internal/controller/resources.go` (pure builders) + `theodbcluster_controller.go` (Reconcile) | `cmd/main.go` `SetupWithManager().Owns(StatefulSet,Service)` | `TestBuildStatefulSet_*`, `TestBuildService_*`, `TestReconcile_*` | status `Phase`/`ReadyInstances` + `Ready` condition |
| T2.2 | Reconcile gate against real envtest | reconciler bound to envtest client | 5 `TestReconcile_*` (create/idempotent/scale/fail-fast/status) | StatefulSet+Service created with owner refs |
| T3.1 | Real-envtest gate (milestone evidence) | `TestMain` boots kube-apiserver+etcd | `make test` exit 0, controller pkg 72.4% cov | `docs/benchmarks/m23-operator-reconcile.md` |
| T3.2 | `cmd/theodbctl/main.go` (cobra apply/get/delete) + `config/` kustomize + `cmd/main.go` manager | CLI builds controller-runtime client from kubeconfig | `cmd/theodbctl` decode tests (valid/wrong-kind/malformed/no-name) | real-kind smoke: apply/get/delete + 16-resource server dry-run |

## Edge cases absorbed (from plan v1.1)

- **EC-1** — scaling patches ONLY `Spec.Replicas` + container `Image` on the existing StatefulSet, never re-applies the immutable spec (Selector/ServiceName/VolumeClaimTemplates). `TestReconcile_ScaleUpUpdatesReplicas`.
- **EC-2** — converged re-reconcile is a no-op (asserts unchanged `resourceVersion`). `TestReconcile_Idempotent`.
- **EC-3** — CLI rejects wrong-kind / malformed / no-name YAML with a typed decode error before any client call. `TestTheodbctl_Apply*_Errors`.
- **EC-4** — under envtest (no kubelet) ReadyReplicas stays 0 → `Phase=Initializing` honestly. `TestReconcile_StatusInitializingWithoutKubelet`.
- **EC-5** — `Instances` carries `+kubebuilder:validation:Minimum=1` + `+default:=1` in the CRD schema.

## Fail-fast / error handling (Rule 8)

- Missing `spec.image` → typed error `theodbcluster %s/%s: spec.image is required` + `Phase=Error`, no StatefulSet. `TestReconcile_MissingImageFailsFast`.
- CR deleted mid-reconcile → `client.IgnoreNotFound` → `{}, nil` (no orphan, no error).
- Status-update conflict → `apierrors.IsConflict` → requeue.

## Gate results (2026-06-30)

- `go build ./...` ✓ · `go vet ./...` ✓ · `gofmt -l .` clean ✓
- `make test` exit 0 — 8 tests PASS (5 real-envtest), controller pkg **72.4%** coverage
- Real-kind: CRD installed; full kustomize bundle (16 resources) server-side dry-run validated; `theodbctl apply/get/delete` end-to-end + typed not-found error

## Honest scope (ADRs)

- **D1** — StatefulSet (not cnpg's pod-per-instance): KISS, standard primitive with stable identity + per-instance PVCs.
- **D2** — std `testing` + real envtest (no ginkgo/gomega — removed from scaffold).
- **D3** — "gateway" = K8s ClusterIP Service (L4); HTTP/pooler is M24+.
- **D4** — HA failover orchestration is M24+; M23 ships provisioning + status only.

## Toolchain note

The auto-downloaded module toolchain `golang.org/toolchain@…go1.25.3` ships a reduced tool set
missing `covdata`, which broke `go test -coverprofile` on the test-less `api/v1`+`cmd` packages
(not a code defect — the behavior-bearing controller package passes). Repaired locally by providing
`covdata` from the base SDK; a CI runner using `actions/setup-go` gets a complete SDK and is unaffected.
