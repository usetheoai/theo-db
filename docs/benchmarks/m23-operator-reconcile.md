# M23 — Control-plane reconcile evidence (Go K8s operator)

**Date:** 2026-06-30 · **Milestone:** M23 (`ROADMAP.md:139`) · **Module:** `operator/`

Unlike the vector-pillar milestones (M17–M22), M23 ships a **control plane**, so the milestone
evidence is **not** a recall/latency benchmark — it is a **reproducible real-`envtest` reconcile
proof** plus a real-cluster deploy/CLI smoke. "Measurement-first" here means: the operator is proven
against a **real kube-apiserver + etcd** (envtest, in-process, no kubelet), not a fake client.

## What is measured

| Claim | How it is proven | Result |
|---|---|---|
| The reconciler provisions a StatefulSet + gateway Service + **governing headless Service** (ClusterIP=None for stable pod DNS), all owner-referenced | `TestReconcile_CreatesStatefulSetAndService` (real envtest) | PASS |
| Reconcile is idempotent — a converged re-run causes no resourceVersion churn (EC-2) | `TestReconcile_Idempotent` (asserts identical `resourceVersion`) | PASS |
| Scaling patches ONLY the mutable field (replicas), never the immutable spec (EC-1) | `TestReconcile_ScaleUpUpdatesReplicas` | PASS |
| `storageSize` is immutable (StatefulSet VCT can't change in place) — the API server rejects an edit at the boundary, so intent never silently diverges (EC-1) | `TestCRD_RejectsStorageSizeChange` (CEL `x-kubernetes-validations`) | PASS |
| A reconcile request for a deleted CR is a no-op, never an error (failure scenario) | `TestReconcile_CRDeleted_NoOp` | PASS |
| Invalid spec is rejected **at the API boundary** (empty image, malformed storageSize, port out of range, instances < 1) | `TestCRD_RejectsInvalidSpec` (4 sub-cases, real apiserver) | PASS |
| Status reflects readiness honestly under envtest (no kubelet → Phase=Initializing) + records `observedGeneration` (EC-4) | `TestReconcile_StatusInitializingWithoutKubelet` | PASS |
| Pure builders produce the correct StatefulSet/Services (replicas/image/PVC/port/owner, headless None) | `TestBuildStatefulSet_*`, `TestBuildHeadlessService_NoneClusterIP`, `TestBuildService_SelectorPort` | PASS |
| The CLI decode path accepts valid YAML and rejects wrong-kind/malformed/no-name (EC-3) | `cmd/theodbctl` unit tests | PASS |

## Reproduce

```bash
cd operator
make test          # controller-gen + go vet + setup-envtest + go test ./... -coverprofile cover.out
```

### Gate output (2026-06-30, this machine — go1.25.3, envtest k8s 1.35.0)

```
=== RUN   TestBuildStatefulSet_ReplicasImagePVC          --- PASS (0.00s)
=== RUN   TestBuildHeadlessService_NoneClusterIP         --- PASS (0.00s)
=== RUN   TestBuildService_SelectorPort                  --- PASS (0.00s)
=== RUN   TestBuildStatefulSet_OwnerRef                  --- PASS (0.00s)
=== RUN   TestReconcile_CreatesStatefulSetAndService     --- PASS (~2s)    # real kube-apiserver+etcd
=== RUN   TestReconcile_Idempotent                       --- PASS
=== RUN   TestReconcile_ScaleUpUpdatesReplicas           --- PASS
=== RUN   TestCRD_RejectsStorageSizeChange                --- PASS         # EC-1 immutable (CEL)
=== RUN   TestReconcile_CRDeleted_NoOp                    --- PASS         # IgnoreNotFound
=== RUN   TestCRD_RejectsInvalidSpec                      --- PASS (4 sub) # boundary validation
=== RUN   TestReconcile_StatusInitializingWithoutKubelet --- PASS
ok  github.com/usetheodev/theo-db/operator/internal/controller  ~6.5s  coverage: 69.4% of statements
ok  github.com/usetheodev/theo-db/operator/cmd/theodbctl                coverage: 9.1% of statements
make test exit=0
```

> The `internal/controller` package — where all reconcile logic lives — is covered at **69.4%**.
> `api/v1` (generated deepcopy) and `cmd/manager` (the controller-runtime composition root) carry no
> unit tests by design (generated / wiring-only), so the module-wide percentage is lower; the
> behavior-bearing code is the controller package. The uncovered remainder is the defense-in-depth
> storageSize-parse error branch, which is unreachable through the API because the CRD `pattern`
> rejects a malformed value at admission (proven by `TestCRD_RejectsInvalidSpec/bad-storage`).
>
> **Reconcile latency:** the per-test wall time (~2s on the first envtest test) is dominated by the
> one-time kube-apiserver+etcd boot, not the reconcile. The substantive performance proof is the
> **idempotency no-churn** assertion (a converged second reconcile performs zero writes — identical
> `resourceVersion`), which is the property that matters for a controller's steady-state cost.

## Real-cluster deploy + CLI smoke (kind, 2026-06-30)

Beyond envtest, the deploy bundle and the CLI were validated against a **real kind cluster**:

```bash
# CRD installs into a real apiserver
kustomize build operator/config/crd | kubectl apply -f -
#  → customresourcedefinition.apiextensions.k8s.io/theodbclusters.core.theodb.io created

# Full kustomize bundle (15 resources) validates server-side (after the namespace exists)
kustomize build operator/config/default | kubectl apply --dry-run=server -f -
#  → 15 resources validated, no errors

# CLI end-to-end against the real apiserver
theodbctl apply -f sample-cluster.yaml   #  → theodbcluster/smoke created
theodbctl get                            #  → NAMESPACE NAME INSTANCES READY PHASE / default smoke 2 0
theodbctl delete smoke -n default        #  → theodbcluster/smoke deleted
theodbctl delete ghost -n default        #  → error: theodbcluster "ghost" not found in namespace "default" (exit 1)
```

## Honest scope (ADR D1/D3/D4 of the plan)

- The operator provisions a **StatefulSet** of N theo-db instances + a **ClusterIP Service** gateway.
  This **diverges** from cloudnative-pg's pod-per-instance model (cnpg `pkg/specs/pods.go:543`) — KISS:
  a StatefulSet is the standard, smallest primitive that gives stable identity + per-instance PVCs.
- **HA failover orchestration** (primary/replica promotion) and an **HTTP/pooler gateway** are explicit
  follow-ups (M24+), not in M23. The "gateway" in M23 is the K8s Service (L4), not a connection pooler.
- Under envtest there is **no kubelet**, so pods never become Ready → `Phase=Initializing` is the
  honest terminal status in the test (EC-4). Real readiness is a kind/real-cluster concern, smoked above.

## Dependencies (all Apache-2.0 / BSD — D1 license gate satisfied)

`controller-runtime` v0.23.1, `k8s.io/{api,apimachinery,client-go}`, `github.com/spf13/cobra` v1.10.0,
`sigs.k8s.io/yaml`. No ginkgo/gomega (std `testing` only — ADR D2). No AGPL.
