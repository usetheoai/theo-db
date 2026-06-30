# Blueprint: M23 — Control plane in Go (K8s operator + CLI + gateway)

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS (89.0, weighted 100.0; sole caveat: citation-density metric). Scored 2026-06-30 by /discover-confidence — synthesized 2026-06-30 from the m23-control-plane-go
> discovery plan (v1.1). Source: cloudnative-pg (the Go Postgres-operator SOTA we model — Apache-2.0). **Decision:
> kubebuilder operator with a `TheoDBCluster` CRD → reconciler that provisions a StatefulSet + Service, proven by
> a REAL `envtest` apiserver reconcile gate; `theodbctl` CLI; reproducible kustomize deploy. Standard K8s ecosystem
> deps only.**

**Slug:** `m23-control-plane-go`
**Owner:** paulohenriquevn
**Created:** 2026-06-30

## Context

M23 (`ROADMAP-v2.md:139`) requires an **own Go control plane**: a Kubernetes operator (cloudnative-pg model) +
CLI + gateway that makes TheoDB deployable/manageable. DoD: the operator provisions/manages a TheoDB cluster (CRD
+ reconciliation); a CLI; a reproducible deploy; **own Go code with tests; no external dependency beyond the
standard K8s ecosystem**. First **Go** pillar — wraps the shipped `theo-db` container image. The honest evidence
for a control plane is a **reconcile proof**: the operator, run against a REAL kube-apiserver (controller-runtime
`envtest`), reconciles a `TheoDBCluster` CR into a StatefulSet + Service, with reconcile timing + idempotency.
Toolchain present: Go 1.24, kubebuilder, controller-gen, setup-envtest, kind, kubectl, docker.

## Objective

Decide the architecture of TheoDB's own Go K8s operator + the envtest evidence design. **Decision reached:** a
kubebuilder-scaffolded operator with a `TheoDBCluster` CRD (instances/image/storage/port spec; phase/readyInstances/
conditions status) → a controller-runtime reconciler that ensures a **StatefulSet** (N instances of the theo-db
image, PVC template) + a **Service** (the gateway endpoint) with owner references → updates status; proven by a
**real-`envtest`** reconcile gate (CR → resources, idempotent, timed); a `theodbctl` cobra CLI; a reproducible
`config/` kustomize deploy. Standard K8s deps only (controller-runtime, k8s.io/*, cobra); **std `testing`**, not
ginkgo.

## Coverage Corner 1 — Integration Tests

How cloudnative-pg tests the reconciler + the M23 real-envtest reconcile gate (the milestone evidence).

**cloudnative-pg (honest):** its `internal/controller/suite_test.go:51-52,59-78` uses **ginkgo/gomega + a FAKE
client** (`fake.NewClientBuilder()`) for unit-speed reconcile tests (`cluster_controller_test.go:46-103`: create
CR → `reconcilePods` → `Expect(jobs).To(HaveLen(1))`). The REAL apiserver path is the Makefile `envtest` target
(`Makefile:130-135,393`, `setup-envtest`).

**M23 gate (EC-1 — REAL envtest, NOT the fake client):** the milestone evidence is a real-apiserver reconcile proof
with **std `testing`**:
1. `testEnv := &envtest.Environment{CRDDirectoryPaths: []string{"config/crd/bases"}}; cfg,_ := testEnv.Start()`
   (downloads/starts kube-apiserver + etcd; binary precondition EC-2: `setup-envtest use` → `KUBEBUILDER_ASSETS`).
2. `k8sClient.Create(ctx, &TheoDBCluster{Spec:{Instances:3, Image:"theo-db:…"}})`.
3. `reconciler.Reconcile(ctx, req)` (timed — reconcile wall time).
4. Assert via `k8sClient.Get`: the **StatefulSet** exists (`*Replicas==3`, container image matches, owner ref →
   the CR) + the **Service** exists + `cluster.Status.Phase` is set.
5. **Idempotency:** a second `Reconcile` returns no error + creates no duplicate (List asserts exactly one
   StatefulSet/Service).

This is the "100% functional evidence" — a real K8s API reconciliation, not a fake-client simulation.

## Coverage Corner 2 — Dependencies

What the operator pulls in + licenses (DoD: standard K8s ecosystem only; AGPL forbidden — none present).

| Dependency | Version (cnpg ref) | License | Purpose | Standard K8s? |
|---|---|---|---|---|
| sigs.k8s.io/controller-runtime | v0.24.1 | Apache-2.0 | Manager, Reconciler, client, **envtest** | ✅ |
| k8s.io/api | v0.36.2 | Apache-2.0 | apps/v1 (StatefulSet), core/v1 (Service, Pod, PVC) | ✅ |
| k8s.io/apimachinery | v0.36.2 | Apache-2.0 | ObjectMeta, runtime.Scheme, metav1.Condition | ✅ |
| k8s.io/client-go | v0.36.2 | Apache-2.0 | client config / rest | ✅ |
| github.com/spf13/cobra | v1.10.2 | Apache-2.0 | the `theodbctl` CLI | ✅ (kubebuilder-standard) |

**Source:** `.claude/knowledge-base/references/cloudnative-pg/go.mod:1-49`; licenses under
`.claude/knowledge-base/references/cloudnative-pg/licenses/go-licenses/`. **NO AGPL/GPL** — all Apache-2.0 (ginkgo/
gomega are MIT but **M23 omits them**, using std `testing`). **M23 minimal set:** controller-runtime + k8s.io/* +
cobra + Go stdlib (`testing`) — envtest is part of controller-runtime (no extra dep). The DoD constraint ("sem dep
externa além do ecossistema K8s padrão") is satisfied.

## Coverage Corner 3 — Tools

The kubebuilder/controller-runtime scaffold + envtest (Q4) and the reproducible deploy + CLI (Q5).

### Scaffold + manager + envtest (Q4)

- **Manager `main`:** `mgr, _ := ctrl.NewManager(ctrl.GetConfigOrDie(), manager.Options{Scheme: scheme});
  utilruntime.Must(theodbv1.AddToScheme(scheme)); (&TheoDBClusterReconciler{...}).SetupWithManager(mgr);
  mgr.Start(ctrl.SetupSignalHandler())` (model:
  `.claude/knowledge-base/references/cloudnative-pg/internal/cmd/manager/controller/controller.go:55,176,261-280`).
- **envtest (std testing):** `TestMain` starts `envtest.Environment{CRDDirectoryPaths:…}`, registers the scheme,
  builds `k8sClient := client.New(cfg, …)`, `defer testEnv.Stop()`. Model: cnpg `suite_test.go:73-137` (fake) +
  the Makefile real-envtest target (`Makefile:130-135,393`). M23 uses the REAL path.
- **Makefile targets:** `manifests` (controller-gen → CRD + RBAC from `+kubebuilder:rbac` markers), `generate`
  (deepcopy), `envtest` (`setup-envtest`), `test` (KUBEBUILDER_ASSETS + `go test ./...`).

### Reproducible deploy + CLI (Q5)

- **Deploy bundle (kustomize):** `config/{crd/bases, rbac/{role,role_binding}, manager/manager.yaml,
  default/kustomization.yaml}` — `default` composes crd+rbac+manager with a namespace + name prefix (model:
  `.claude/knowledge-base/references/cloudnative-pg/config/default/kustomization.yaml:4,11,17-22`;
  `config/rbac/role.yaml`). `make install` (`kustomize build config/crd | kubectl apply`), `make deploy`
  (`kustomize build config/default | kubectl apply`).
- **RBAC:** `+kubebuilder:rbac:groups=theodb.io,resources=theodbclusters;…/status,verbs=…` markers on the
  reconciler → controller-gen aggregates `config/rbac/role.yaml` (model:
  `.claude/knowledge-base/references/cloudnative-pg/internal/controller/cluster_controller.go` rbac markers).
- **CLI (`theodbctl`, cobra):** root `&cobra.Command{Use:"theodbctl"}` + subcommands `apply -f` (parse YAML → CR →
  `client.Create`), `get` (`client.List` → table), `delete` (`client.Delete`). Model:
  `.claude/knowledge-base/references/cloudnative-pg/cmd/kubectl-cnpg/main.go:29,58-147`;
  `internal/cmd/plugin/install/cmd.go:29-38` (the `NewCmd()` subcommand pattern).

## Coverage Corner 4 — Techniques

The CRD (Q1), the reconcile loop (Q2), and the workload provisioning (Q3).

### CRD (Q1) — cloudnative-pg model → TheoDBCluster

- `ClusterSpec` (`.claude/knowledge-base/references/cloudnative-pg/api/v1/cluster_types.go:217`): `Instances int`
  (`:264`, `+kubebuilder:validation:Minimum=1 +default:=1`), `ImageName` (`:230`), `StorageConfiguration`
  (`:333`, struct `:2254` — `Size`, `StorageClass`).
- `ClusterStatus` (`:900`): `Instances`, `ReadyInstances` (`:907`), `Phase` (`:1006`), `Conditions
  []metav1.Condition` (`:1099`); condition builders in `api/v1/cluster_conditions.go:25-54`.
- Root markers (`:2758-2770`): `+kubebuilder:object:root=true`, `+kubebuilder:subresource:status`,
  `+kubebuilder:printcolumn` (Ready, Phase). deepcopy via controller-gen (`zz_generated.deepcopy.go`).
- **M23 TheoDBCluster:** `Spec{ Instances int (≥1, default 1); Image string; StorageSize string; Port int
  (default 5432) }` + `Status{ Phase string; ReadyInstances int; Conditions []metav1.Condition }`.

### Reconcile loop (Q2)

- `Reconcile(ctx, req)` (`.claude/knowledge-base/references/cloudnative-pg/internal/controller/cluster_controller.go:169`):
  `Get` the CR (`client.IgnoreNotFound`), handle deletion (finalizer), ensure resources, update status, requeue
  (`ctrl.Result{RequeueAfter: …}`).
- `SetupWithManager` (`:1294-1337`): `ctrl.NewControllerManagedBy(mgr).For(&Cluster{}).Owns(&corev1.Service{})
  .Owns(&corev1.PersistentVolumeClaim{})…Complete(r)` — M23 owns `appsv1.StatefulSet` + `corev1.Service`.
- Finalizers (`finalizers_delete.go:152`): `controllerutil.RemoveFinalizer` + patch on delete.
- **M23 skeleton:** Get TheoDBCluster → add finalizer → ensure StatefulSet (create with
  `controllerutil.SetControllerReference`, else update) → ensure Service → `updateStatus` (ReadyInstances from
  `ss.Status.ReadyReplicas`; Phase Healthy/Initializing; set `Ready` condition) → requeue if not Healthy.

### Workload provisioning (Q3) — HONEST divergence

cloudnative-pg manages **individual Pods + PVCs directly (NOT a StatefulSet)** via its instance manager —
confirmed: `pkg/specs/pods.go:543-564` builds `&corev1.Pod{}`; `internal/controller/cluster_create.go:1537`
`ctrl.SetControllerReference(cluster, instanceToCreate, …)`; **zero `appsv1.StatefulSet{}` matches**. The Service
pattern is reusable (`pkg/specs/services.go:132-156,223` — selector on cluster+role labels, owner via
`SetInheritedDataAndOwnership`).

**M23 design (TheoDB choice — StatefulSet, KISS):** build an `appsv1.StatefulSet{ Replicas: Instances; Selector/
Template labels {cluster: name}; ServiceName: name+"-headless"; Template.Spec.Containers[0]{ Image, Ports[Port] };
VolumeClaimTemplates: [{ "data", RWO, Requests{storage: StorageSize} }] }` + a `corev1.Service{ Selector{cluster},
Ports[Port] }`, BOTH with `controllerutil.SetControllerReference(cluster, obj, scheme)`. A StatefulSet gives stable
identity + PVC-per-replica + replica management for free (standard K8s) — the right measurement-first call vs
copying cnpg's bespoke pod-per-instance instance-manager (multi-month, out of scope).

## Cross-cutting Comparison

| Dimension | cloudnative-pg (model) | M23 implication |
|---|---|---|
| Scaffold | kubebuilder + controller-runtime | kubebuilder init + create api (same) |
| CRD | rich `Cluster` (backup/WAL/…) | minimal `TheoDBCluster` (instances/image/storage/port) |
| Reconcile | Get → finalizer → ensure → status → requeue | same skeleton, StatefulSet+Service owned |
| Workload | Pods + PVCs direct (instance manager) | **StatefulSet** (KISS, standard) — honest divergence |
| Service | role-selected RW/RO services | one Service (the gateway endpoint) |
| Test | ginkgo + **fake client** (suite) | **real envtest + std testing** (the evidence gate) |
| Deploy | config/ kustomize + manifest | config/{crd,rbac,manager,default} + make install/deploy |
| CLI | `kubectl-cnpg` (cobra plugin) | `theodbctl` (cobra: apply/get/delete) |
| Deps | controller-runtime, k8s.io/*, cobra, ginkgo | same minus ginkgo (std testing) — all Apache/std |

## ADRs

### D1 — StatefulSet workload (NOT cloudnative-pg's pod-per-instance)

**Decision:** the reconciler provisions a single `appsv1.StatefulSet` (replicas = `Instances`, PVC volumeClaimTemplate
of `StorageSize`, the theo-db image) + a `corev1.Service`, both owned by the CR.

**Rationale:** cloudnative-pg uses a bespoke pod-per-instance instance manager (`pkg/specs/pods.go:543`) for
fine-grained control + WAL/backup — multi-month, out of M23 scope. A StatefulSet gives stable identity + per-replica
PVC + replica management from standard K8s (KISS, Rule 9 — don't reinvent what apps/v1 already does). Honest
divergence, not a misrepresentation of cnpg.

**Alternatives considered:** pod-per-instance like cnpg (rejected — bespoke, multi-month); a Deployment (rejected —
no stable identity / per-replica PVC, wrong for a database).

**Consequences:** HA failover (primary election) is not M23 — the StatefulSet provides provision/manage; failover
is a follow-up. The DoD ("provisiona/gerencia um cluster") is met.

### D2 — Real envtest + std `testing` (NOT fake client + ginkgo) for the evidence gate

**Decision:** the milestone evidence is a **real `envtest.Environment` apiserver** reconcile gate written with the
Go stdlib `testing` package (no ginkgo/gomega).

**Rationale:** EC-1 — a fake client doesn't prove real reconciliation (no real apiserver, admission, status
subresource). A real envtest apiserver does — the "100% functional evidence" the milestone demands. std `testing`
minimizes deps (DoD: standard K8s ecosystem only; ginkgo/gomega omitted). envtest is part of controller-runtime
(no extra dep).

**Alternatives considered:** fake client like cnpg's suite (rejected — not real evidence); ginkgo (rejected —
extra BDD dep, parsimony).

**Consequences:** the gate needs the apiserver/etcd binaries (`setup-envtest use` → KUBEBUILDER_ASSETS, EC-2) — a
one-time network download; offline → honest BLOCKED.

### D3 — Standard K8s ecosystem deps only; gateway = the K8s Service

**Decision:** deps are controller-runtime + k8s.io/{api,apimachinery,client-go} + cobra (CLI) + Go stdlib — all
Apache-2.0/std (DoD constraint). The DoD's "gateway" = the **K8s Service** the operator provisions (the cluster's
stable connection endpoint); a separate HTTP/connection-pooling gateway is a follow-up.

**Rationale:** the DoD forbids non-K8s-ecosystem deps; the Service IS the operational gateway in the operator model
(EC-4). No AGPL anywhere (Q6).

**Alternatives considered:** a bespoke HTTP gateway (rejected — out of scope, the Service suffices); a pooler like
pgbouncer (rejected — follow-up, M24-adjacent).

**Consequences:** the operator + Service + CLI + deploy is the M23 deliverable; an HTTP gateway/pooler is deferred.

### D4 — Measurement-first reconcile gate + reproducible deploy as the acceptance metric

**Decision:** the acceptance metric is (a) the real-envtest reconcile gate green (CR → StatefulSet+Service+status,
idempotent, timed) + (b) a reproducible `make deploy` (kustomize bundle applies cleanly) — both demonstrable, in a
benchmark-style record (`docs/benchmarks/m23-operator-reconcile.md` with reconcile wall time + idempotency).

**Rationale:** for a control plane, "benchmark/data" = the reconcile proof + deploy reproducibility, not recall/perf.

**Alternatives considered:** a kind-cluster e2e (rejected for the gate — heavier + flakier than envtest; envtest is
the standard operator evidence; a kind smoke can be an optional extra).

**Consequences:** the milestone ships the operator + the envtest evidence + the deploy bundle + the CLI.

## Recommendations

1. **Scaffold a kubebuilder operator (`api/v1` + `internal/controller`) with a `TheoDBCluster` CRD, per ADR D1 +
   Corner 4** — Spec{Instances,Image,StorageSize,Port} + Status{Phase,ReadyInstances,Conditions}. (Q1/Q2)
2. **Reconciler provisions a StatefulSet + Service with owner refs + status, per ADR D1** — Get → finalizer →
   ensure StatefulSet → ensure Service → updateStatus → requeue; `SetupWithManager().Owns(StatefulSet, Service)`. (Q2/Q3)
3. **Prove it with a REAL-envtest gate (std testing), per ADR D2 + Corner 1** — CR → Reconcile → assert resources +
   owner refs + status, idempotent + timed; `setup-envtest` precondition. (Q7)
4. **Ship a `theodbctl` cobra CLI (apply/get/delete) + a reproducible `config/` kustomize deploy (make install/
   deploy), per Corner 3.** (Q4/Q5)
5. **Standard K8s deps only; gateway = the Service; HA-failover + HTTP gateway deferred, per ADR D3/D4 + Corner
   2.** (Q6)

## Blocked questions (if any)

(none — all 7 questions answered with citations.)
