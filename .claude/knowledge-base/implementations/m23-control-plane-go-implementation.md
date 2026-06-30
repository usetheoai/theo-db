# M23 — Control plane in Go — Implementation Summary

**Plan:** `.claude/knowledge-base/plans/m23-control-plane-go-plan.md` (v1.1, plan-confidence SHIPPABLE 96.4)
**Milestone:** M23 (`ROADMAP-v2.md:139`) · **Date:** 2026-06-30 · **Branch:** develop

## What shipped

A new `operator/` Go module (kubebuilder/controller-runtime v0.23.1) — TheoDB's own Kubernetes
control plane. It reconciles a `TheoDBCluster` CRD into a **StatefulSet** of N theo-db instances, a
**governing headless Service** (ClusterIP=None, stable pod DNS) + a **gateway ClusterIP Service**, all
owner-referenced, with status (`Phase`/`ReadyInstances`/`ObservedGeneration`/`Ready` condition). Plus a
`theodbctl` cobra CLI and a reproducible `config/` kustomize deploy.

> **Post-review hardening (6-agent `/review`, 2026-06-30).** The first cut was audited by 6 specialist
> agents; this summary reflects the fixed state. Fixes: (H1) added the missing governing **headless
> Service** the StatefulSet referenced; (H-sec) replaced `resource.MustParse(storageSize)` — which
> **panicked** on malformed user input — with boundary CRD validation (`pattern`) + `ParseQuantity`
> defense-in-depth; made `image` CRD-**required** (MinLength=1) and `port` bounded (1..65535); added
> `observedGeneration`; replaced the hand-rolled condition helper with `apimeta.SetStatusCondition`
> (Rule 9); reconcile the gateway/headless Service port + adopt owner refs on update; removed the
> broken ginkgo e2e CI target + workflow (ADR D2); added a `theodbctl` build target. New tests cover
> EC-1 immutable-VCT, CR-deleted no-op, and 4 CRD boundary-rejection cases.

## Tasks → evidence (wiring triad per task)

| Task | Deliverable | Caller / wiring | Test (RED→GREEN) | Runtime signal |
|---|---|---|---|---|
| T1.1 | `api/v1/theodbcluster_types.go` — CRD spec/status + deepcopy + CRD manifest | manager registers scheme; CRD installed into real apiserver | `make manifests` generates CRD; envtest loads it | CRD `theodbclusters.core.theodb.io` (printcolumns Instances/Ready/Phase) |
| T2.1 | `internal/controller/resources.go` (pure builders: SS + headless + gateway) + `theodbcluster_controller.go` (Reconcile) | `cmd/main.go` `SetupWithManager().Owns(StatefulSet,Service)` | `TestBuildStatefulSet_*`, `TestBuildHeadlessService_*`, `TestBuildService_*`, `TestReconcile_*` | status `Phase`/`ReadyInstances`/`ObservedGeneration` + `Ready` condition |
| T2.2 | Reconcile gate against real envtest | reconciler bound to envtest client | `TestReconcile_*` (create+headless / idempotent / scale / storage-immutable / cr-deleted / status) + `TestCRD_RejectsInvalidSpec` | StatefulSet+2 Services created with owner refs |
| T3.1 | Real-envtest gate (milestone evidence) | `TestMain` boots kube-apiserver+etcd | `make test` exit 0, controller pkg 69.4% cov | `docs/benchmarks/m23-operator-reconcile.md` |
| T3.2 | `cmd/theodbctl/main.go` (cobra apply/get/delete) + `config/` kustomize + `cmd/main.go` manager | CLI builds controller-runtime client from kubeconfig | `cmd/theodbctl` decode tests (valid/wrong-kind/malformed/no-name) | real-kind smoke: apply/get/delete + 15-resource server dry-run |

## Edge cases absorbed (from plan v1.1)

- **EC-1** — scaling patches ONLY `Spec.Replicas` + container `Image` on the existing StatefulSet, never re-applies the immutable spec (Selector/ServiceName/VolumeClaimTemplates). `TestReconcile_ScaleUpUpdatesReplicas`.
- **EC-2** — converged re-reconcile is a no-op (asserts unchanged `resourceVersion`). `TestReconcile_Idempotent`.
- **EC-3** — CLI rejects wrong-kind / malformed / no-name YAML with a typed decode error before any client call. `TestTheodbctl_Apply*_Errors`.
- **EC-4** — under envtest (no kubelet) ReadyReplicas stays 0 → `Phase=Initializing` honestly. `TestReconcile_StatusInitializingWithoutKubelet`.
- **EC-5** — `Instances` carries `+kubebuilder:validation:Minimum=1` + `+default:=1` in the CRD schema.

## Fail-fast / error handling (Rule 8) — validate at the boundary

- Invalid spec is rejected **at the API boundary** by CRD validation (image MinLength=1, storageSize `pattern`, port 1..65535, instances ≥ 1) — `TestCRD_RejectsInvalidSpec` (4 sub-cases) proves the real apiserver returns `Invalid` before the object is stored. This is the fail-fast-at-the-boundary discipline (`error-handling.md §2`), stronger than a mid-reconcile check.
- Malformed `storageSize` that bypassed admission → `ParseQuantity` (never `MustParse`/panic) → `Phase=Error`, no requeue hot-loop (defense-in-depth).
- CR deleted mid-reconcile → `client.IgnoreNotFound` → `{}, nil`. `TestReconcile_CRDeleted_NoOp`.
- Status-update conflict → `apierrors.IsConflict` → requeue.

## Gate results (2026-06-30, after 6-agent review fixes)

- `go build ./...` ✓ · `go vet ./...` ✓ · `gofmt -l .` clean ✓ · `golangci-lint run` **0 issues** ✓ · `deadcode ./...` none ✓
- `make test` exit 0 — 14 tests PASS (incl. 4 CRD-rejection sub-cases; 6 real-envtest reconcile gates), controller pkg **69.4%** coverage
- Real-kind: CRD installed; full kustomize bundle (15 resources) server-side dry-run validated; `theodbctl apply/get/delete` end-to-end + typed not-found error

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
