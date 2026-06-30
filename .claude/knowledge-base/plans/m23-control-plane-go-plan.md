---
slug: m23-control-plane-go
milestone_id: M23
created_at: 2026-06-30
goal: Ship an own Go Kubernetes operator that reconciles a TheoDBCluster CRD into a StatefulSet + Service, proven by a real-envtest reconcile gate, plus a CLI and a reproducible deploy.
---

# Plan: M23 — Control plane in Go (K8s operator + CLI + gateway)

> **Version 1.1** (edge-cases absorbed: EC-1 StatefulSet mutable-fields-only update; EC-2 no-spurious-update idempotency; EC-3 CLI wrong-kind; EC-4 envtest no-kubelet→Phase=Initializing; EC-5 Instances Minimum=1) — Build TheoDB's own **Kubernetes operator** in Go (new `operator/` module, kubebuilder model:
> cloudnative-pg): a `TheoDBCluster` CRD + a controller-runtime reconciler that provisions/manages a TheoDB cluster
> (a **StatefulSet** of N theo-db instances + a **Service** gateway, owner-referenced, with status), a `theodbctl`
> cobra CLI, and a reproducible `config/` kustomize deploy. The milestone evidence is a **real-`envtest`** reconcile
> gate (CR → StatefulSet+Service, idempotent, timed) — the control-plane "benchmark". Standard K8s ecosystem deps
> only (controller-runtime, k8s.io/*, cobra); std `testing` (no ginkgo). HA-failover + HTTP gateway are follow-ups.

## Goal

> Enable TheoDB to be deployed/managed on Kubernetes via an own Go operator so that a `TheoDBCluster` CR is
> reconciled into a running StatefulSet + Service, measured by `operator/internal/controller/` real-envtest tests
> passing (CR → StatefulSet[replicas==instances]+Service+status, idempotent) against a real kube-apiserver.

## Context

M23 (`ROADMAP-v2.md:139`) requires an **own Go control plane**: a K8s operator (cloudnative-pg model) + CLI +
gateway that makes TheoDB deployable/manageable. DoD (`ROADMAP-v2.md:146-147`): operator provisions/manages a
cluster (CRD + reconciliation); CLI; reproducible deploy; own Go code with tests; **no external dep beyond the
standard K8s ecosystem**. First Go pillar — wraps the shipped `theo-db` image. The discovery blueprint
(`.claude/knowledge-base/discoveries/blueprints/m23-control-plane-go-blueprint.md`, SHIPPABLE_WITH_CAVEATS 89)
locked: kubebuilder operator, StatefulSet workload (D1), real-envtest + std testing (D2), standard deps + gateway =
the Service (D3), reconcile-gate evidence (D4). Toolchain present (Go 1.24, kubebuilder, controller-gen,
setup-envtest, kind, kubectl, docker).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `operator/` (NEW module) | 0 | — | (to create) the Go operator — kubebuilder scaffold | new Go module; isolated from theodb_rs (Rust) + benchmarks (Python) |
| `operator/go.mod` (NEW) | 0 | — | Go module manifest — standard K8s deps only | controller-runtime + k8s.io/* + cobra; no ginkgo |
| `operator/api/v1/theodbcluster_types.go` (NEW) | 0 | — | the `TheoDBCluster` CRD types (Spec/Status) | kubebuilder markers; deepcopy generated |
| `operator/api/v1/zz_generated.deepcopy.go` (NEW, generated) | 0 | — | controller-gen deepcopy | generated — never hand-edit |
| `operator/internal/controller/theodbcluster_controller.go` (NEW) | 0 | — | the reconciler (Reconcile + SetupWithManager + resource builders) | Reconcile idempotent; owner refs set |
| `operator/internal/controller/resources.go` (NEW) | 0 | — | pure StatefulSet/Service builders | pure functions (envtest-free unit tests) |
| `operator/internal/controller/suite_test.go` (NEW) | 0 | — | real-envtest TestMain + reconcile gate | real `envtest.Environment`, std testing |
| `operator/cmd/manager/main.go` (NEW) | 0 | — | the operator binary (manager) | scheme + SetupWithManager + Start |
| `operator/cmd/theodbctl/main.go` (NEW) | 0 | — | the `theodbctl` cobra CLI (apply/get/delete) | client-go config; cobra subcommands |
| `operator/config/{crd,rbac,manager,default}/**` (NEW) | 0 | — | kustomize deploy bundle | `make install`/`make deploy` reproducible |
| `operator/Makefile` (NEW) | 0 | — | generate/manifests/envtest/test/install/deploy | KUBEBUILDER_ASSETS via setup-envtest |
| `docs/benchmarks/m23-operator-reconcile.md` (NEW) | 0 | — | reconcile-gate evidence (wall time + idempotency) | reproducible |
| `CHANGELOG.md` | — | — | public contract | `[Unreleased]` gets one Added entry |

### Current callers / dependents

- **No existing Go code** in the repo (M23 starts the Go pillar; `find -name '*.go'` outside references = empty). The operator is a NEW, isolated `operator/` module — it does NOT touch `theodb_rs` (Rust), `benchmarks` (Python), or the SQL extension. The only coupling: the StatefulSet pod template references the shipped `theo-db` container image (a string, the cluster's `Spec.Image`).
- **External consumers:** a cluster admin applies a `TheoDBCluster` CR (`kubectl apply` / `theodbctl apply`).

### Domain glossary

- **CRD** — CustomResourceDefinition: the `TheoDBCluster` Kubernetes API type the operator watches.
- **reconcile** — the controller loop: observe the CR's desired state, ensure the actual resources (StatefulSet, Service) match, repeat until convergence; idempotent.
- **owner reference** — links a child resource (StatefulSet) to its owner CR so K8s garbage-collects it on CR delete.
- **envtest** — controller-runtime's test env: a real in-process `kube-apiserver` + `etcd` (no kubelet/pods) for reconcile tests.
- **StatefulSet** — the apps/v1 workload providing N replicas with stable identity + per-replica PVC (the TheoDB instances).
- **gateway** — the K8s Service the operator provisions: the cluster's stable connection endpoint (per blueprint ADR D3).
- **KUBEBUILDER_ASSETS** — env var pointing at the apiserver/etcd binaries (installed by `setup-envtest use`).

### Architecture boundaries affected

Per `rules/architecture.md`: the operator is a NEW top-level component (`operator/`) with internal layering —
`api/v1` (the CRD domain types), `internal/controller` (the reconcile use-case + pure resource builders),
`cmd/{manager,theodbctl}` (the composition roots/entrypoints). DIP: `internal/controller` depends on the
controller-runtime client interface (injected), not a concrete apiserver. No cross-pillar import (Rust/Python
untouched).

## Prior Art & Related Work

- **Internal blueprint** — `.claude/knowledge-base/discoveries/blueprints/m23-control-plane-go-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89). Consumed: ADR D1 (StatefulSet), D2 (real envtest + std testing), D3 (standard deps + gateway=Service), D4 (reconcile-gate evidence); Corner 4 (CRD + reconcile + provisioning with cloudnative-pg `path:line`); Corner 1 (the envtest gate design).
- **Reference (model)** — cloudnative-pg: CRD (`.claude/knowledge-base/references/cloudnative-pg/api/v1/cluster_types.go:217,264,900,1099`), reconcile (`internal/controller/cluster_controller.go:169,1294-1337`), Service builder (`pkg/specs/services.go:132-156,223`), manager (`internal/cmd/manager/controller/controller.go:176`), deploy (`config/default/kustomization.yaml:17-22`), CLI (`cmd/kubectl-cnpg/main.go:58-147`), deps (`go.mod:1-49`). HONEST divergence (D1): cnpg uses pods-per-instance (`pkg/specs/pods.go:543`); M23 uses a StatefulSet.
- **External literature** — kubebuilder book (controller-runtime, envtest, kustomize deploy) — the scaffolding standard M23 follows.

## Objective

- [ ] `TheoDBCluster` CRD (`api/v1`): Spec{Instances≥1, Image, StorageSize, Port=5432} + Status{Phase, ReadyInstances, Conditions} + generated deepcopy + CRD manifest.
- [ ] Reconciler (`internal/controller`): ensure StatefulSet (replicas=Instances, theo-db image, PVC template) + Service (gateway), owner-referenced; update status; `SetupWithManager().Owns(StatefulSet, Service)`.
- [ ] Pure resource builders unit-tested (envtest-free); reconcile idempotency proven.
- [ ] Real-envtest reconcile gate (std testing): CR → StatefulSet+Service+status, idempotent, reconcile timed.
- [ ] `theodbctl` cobra CLI (apply/get/delete a TheoDBCluster).
- [ ] Reproducible deploy (`config/` kustomize + `make install`/`make deploy`); RBAC from markers.
- [ ] docs/benchmarks/m23-operator-reconcile.md (reconcile wall time + idempotency evidence).

## ADRs

### D1 — StatefulSet workload (NOT cloudnative-pg's pod-per-instance)

**Decision:** the reconciler provisions a single `appsv1.StatefulSet` (replicas=`Instances`, PVC volumeClaimTemplate
of `StorageSize`, the `Image` container, `Port`) + a `corev1.Service`, both owned by the CR
(`controllerutil.SetControllerReference`).

**Rationale:** cnpg uses a bespoke pod-per-instance instance manager (`pkg/specs/pods.go:543`) — multi-month, out of
scope. A StatefulSet gives stable identity + per-replica PVC + replica management from standard apps/v1 (KISS, Rule
9). Honest divergence from cnpg.

**Alternatives considered:** pod-per-instance like cnpg (rejected — bespoke, multi-month); Deployment (rejected — no
stable identity / per-replica PVC, wrong for a DB).

**Consequences:** HA failover (primary election) is a follow-up; M23 delivers provision/manage (the DoD).

### D2 — Real envtest + std `testing` (NOT fake client + ginkgo) for the evidence gate

**Decision:** the milestone evidence is a real `envtest.Environment` apiserver reconcile gate written with the Go
stdlib `testing` package (no ginkgo/gomega).

**Rationale:** a fake client doesn't prove real reconciliation (blueprint EC-1); a real apiserver does (the "100%
functional evidence"). std `testing` minimizes deps (DoD: standard K8s ecosystem; ginkgo omitted). envtest ships
with controller-runtime (no extra dep).

**Alternatives considered:** fake client like cnpg's suite (rejected — not real evidence); ginkgo (rejected — extra
dep).

**Consequences:** the gate needs the apiserver/etcd binaries (`setup-envtest use` → KUBEBUILDER_ASSETS) — a one-time
network download; offline → honest BLOCKED (Failure scenarios).

### D3 — Standard K8s ecosystem deps only; gateway = the K8s Service

**Decision:** deps are controller-runtime + k8s.io/{api,apimachinery,client-go} + cobra + Go stdlib — all Apache-2.0/
std (DoD constraint). The DoD's "gateway" = the **K8s Service** the operator provisions (the cluster's stable
endpoint); a separate HTTP/pooler gateway is a follow-up.

**Rationale:** the DoD forbids non-K8s-ecosystem deps; the Service IS the operational gateway in the operator model
(blueprint EC-4). No AGPL (Q6).

**Alternatives considered:** bespoke HTTP gateway (rejected — out of scope); pgbouncer pooler (rejected — follow-up).

**Consequences:** operator + Service + CLI + deploy is the deliverable; HTTP gateway/pooler deferred.

### D4 — Measurement-first reconcile gate + reproducible deploy as the acceptance metric

**Decision:** the acceptance metric is (a) the real-envtest reconcile gate green (CR → StatefulSet+Service+status,
idempotent, reconcile-timed) + (b) a reproducible `make deploy` (kustomize bundle builds + dry-run applies cleanly)
— recorded in `docs/benchmarks/m23-operator-reconcile.md`.

**Rationale:** for a control plane, "benchmark/data" = the reconcile proof + deploy reproducibility (not recall/perf).

**Alternatives considered:** a kind-cluster e2e for the gate (rejected — heavier/flakier; envtest is the standard
operator evidence; a kind smoke can be an optional extra, not the gate).

**Consequences:** the milestone ships the operator + envtest evidence + deploy bundle + CLI.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| envtest needs apiserver/etcd binaries (network download) — offline blocks the gate | Medium | `make envtest`/`setup-envtest use` installs once; document KUBEBUILDER_ASSETS; offline → honest BLOCKED, never silent skip | impl |
| StatefulSet ≠ HA-failover — M23 provisions/manages but doesn't do primary election | Medium | scope decision (D1): provision/manage is the DoD; HA failover is a documented follow-up | owner |
| New Go module in a polyglot repo (Rust+Python+Go) — toolchain/CI surface grows | Low | isolated `operator/` module with its own go.mod + Makefile; no cross-pillar import; documented | impl |
| controller-runtime v0.24 / k8s v0.36 pin a recent API surface (Go 1.24) | Low | kubebuilder scaffolds compatible versions; Go 1.24 present; pinned in go.mod | impl |
| "gateway" interpreted as the Service, not an HTTP gateway | Low | blueprint EC-4 + D3 document this honestly; CHANGELOG states it | owner |

## Unresolved Questions

- Q1 — Should the operator scaffold via `kubebuilder init` or a hand-written minimal layout? (resolved: kubebuilder init/create-api for the standard scaffold — Rule 9, the K8s-standard generator; then trim to the minimal CRD.)
- Q2 — envtest K8s version pin? (resolved: pin the setup-envtest version to a recent stable, e.g. 1.31/1.33, matching controller-runtime; documented in the Makefile.)
- Q3 — Does the reconcile gate need a kind e2e too? (resolved: no — envtest is the gate; a kind smoke is an optional extra, not required for the DoD; ADR D4.)

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| Go toolchain | 1.24 | go | the operator language (present) |
| kubebuilder / controller-gen / setup-envtest / kustomize | (present) | k8s tooling | scaffold + codegen + envtest + deploy build |
| `theo-db` image | (built) | container | the StatefulSet pod template image (a string, Spec.Image) |

### New — to be introduced (Go module deps — standard K8s ecosystem, DoD-compliant)

| Package | Version | Ecosystem | Rule 9 rationale (libs evaluated) | Why this one |
|---|---|---|---|---|
| `sigs.k8s.io/controller-runtime` | v0.21+ | go | Evaluated: raw client-go informers (rejected — controller-runtime IS the standard reconcile framework, Rule 9) | Manager, Reconciler, client, **envtest** — the K8s operator standard |
| `k8s.io/api` / `k8s.io/apimachinery` / `k8s.io/client-go` | matching | go | the K8s API types — no alternative (the ecosystem) | StatefulSet/Service/PVC types + ObjectMeta + Condition |
| `github.com/spf13/cobra` | v1.x | go | Evaluated: stdlib `flag` (rejected — cobra is the kubebuilder-standard CLI lib; subcommands) | the `theodbctl` CLI |
| (ginkgo/gomega) | — | go | Evaluated + **REJECTED** (D2) — std `testing` + envtest suffices; fewer deps (DoD) | NOT added — std `testing` |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency Graph

```
Phase 1 (scaffold + TheoDBCluster CRD + deepcopy + manifest)
   │
   ▼
Phase 2 (reconciler: StatefulSet/Service builders + Reconcile + status + SetupWithManager; unit tests)
   │
   ▼
Phase 3 (real-envtest reconcile gate + CLI theodbctl + reproducible deploy + docs/benchmarks)
   │
   ▼
Final Phase (integration validation: go vet/build/test envtest + make deploy dry-run + CLI smoke)
```

All phases sequential.

---

## Phase 1: Scaffold + TheoDBCluster CRD

**Objective:** create the `operator/` Go module (kubebuilder), the `TheoDBCluster` CRD types (Spec/Status) with
generated deepcopy + CRD manifest.

### T1.1 — `operator/` module + `TheoDBCluster` CRD types

#### Objective
Scaffold `operator/` (kubebuilder), define `api/v1/theodbcluster_types.go` (`TheoDBClusterSpec`/`Status`), generate
deepcopy + `config/crd/bases` manifest.

#### Why this step (action + reasoning)
1. **What this step does** — `kubebuilder init` + `create api`, then writes the minimal CRD (Spec{Instances,Image,StorageSize,Port}, Status{Phase,ReadyInstances,Conditions}) + `make generate manifests`.
2. **Why now** — the CRD is the API every later phase (reconciler, gate, CLI, deploy) builds on (ADR D1; blueprint Corner 4 cites cnpg `cluster_types.go:217,264,900,1099`).

#### Evidence
cnpg CRD shape (`.claude/knowledge-base/references/cloudnative-pg/api/v1/cluster_types.go:217` ClusterSpec, `:264` Instances markers, `:900` Status, `:1099` Conditions, `:2758-2770` root markers); deepcopy via controller-gen (`zz_generated.deepcopy.go`).

#### Files to edit
```
operator/go.mod (NEW) — module + standard K8s deps
operator/api/v1/theodbcluster_types.go (NEW) — Spec/Status + kubebuilder markers
operator/api/v1/groupversion_info.go (NEW, scaffold) — scheme registration
operator/api/v1/zz_generated.deepcopy.go (NEW, generated)
operator/config/crd/bases/*.yaml (NEW, generated)
operator/Makefile (NEW) — generate/manifests targets
```

#### Deep file dependency analysis
- `theodbcluster_types.go` (new): defines the API; depends on `k8s.io/apimachinery/pkg/apis/meta/v1`. No cross-pillar import.
- `Makefile`: controller-gen `object` (deepcopy) + `crd` (manifest) targets.

#### Deep Dives
- `TheoDBClusterSpec { Instances int (+kubebuilder:validation:Minimum=1,+default:=1); Image string (+optional); StorageSize string (+optional); Port int (+default:=5432,+optional) }`.
- `TheoDBClusterStatus { Phase string; ReadyInstances int; Conditions []metav1.Condition (+optional) }`.
- Root markers: `+kubebuilder:object:root=true`, `+kubebuilder:subresource:status`, `+kubebuilder:printcolumn{Ready,Phase}`.
- Edge cases: Instances=1 (singleton); missing Image → reconciler must error (validated at reconcile).

#### Pseudo-code / Signatures
```go
// +kubebuilder:object:root=true
// +kubebuilder:subresource:status
type TheoDBCluster struct {
  metav1.TypeMeta `json:",inline"`; metav1.ObjectMeta `json:"metadata,omitempty"`
  Spec TheoDBClusterSpec `json:"spec,omitempty"`; Status TheoDBClusterStatus `json:"status,omitempty"`
}
```

#### Tasks
1. `kubebuilder init --domain theodb.io --repo github.com/usetheodev/theo-db/operator` in `operator/`.
2. `kubebuilder create api --group core --version v1 --kind TheoDBCluster` (with resource+controller).
3. Edit `theodbcluster_types.go` to the minimal Spec/Status.
4. `make generate manifests` (deepcopy + CRD).

#### TDD
```
RED: TestTheoDBClusterSpec_Defaults — a TheoDBCluster with Instances unset round-trips; Port default applies (validated via the CRD schema / a marshal test)
RED: TestTheoDBCluster_DeepCopy — DeepCopyObject() returns an independent copy (generated; assert non-aliased)
GREEN: scaffold + types + make generate
REFACTOR: trim scaffolded boilerplate; none expected
VERIFY: cd operator && go build ./... && go test ./api/...
```

#### Concurrency tests

(none — single-threaded) — CRD type definitions + deepcopy are pure data; no concurrency.

#### Acceptance Criteria
- [ ] `cd operator && go build ./...` exits 0; `go test ./api/...` exits 0.
- [ ] `make manifests` produces `config/crd/bases/*.yaml` with the Spec/Status schema.
- [ ] Pass: vet — `cd operator && go vet ./api/...` clean.
- [ ] Pass: size — `theodbcluster_types.go` ≤ 200 lines.

#### DoD
- [ ] Module builds; CRD manifest generated; deepcopy generated; CHANGELOG `[Unreleased]` Added entry.

---

## Phase 2: Reconciler (StatefulSet + Service + status)

**Objective:** implement the reconcile loop + pure resource builders, unit-tested (envtest-free), idempotent.

### T2.1 — Resource builders + Reconcile + SetupWithManager

#### Objective
Implement `resources.go` (pure `buildStatefulSet`/`buildService`) + `theodbcluster_controller.go` (`Reconcile`,
`updateStatus`, `SetupWithManager().Owns(StatefulSet, Service)`) with owner references + RBAC markers.

#### Why this step (action + reasoning)
1. **What this step does** — the reconcile loop: Get CR → ensure StatefulSet → ensure Service → update status → requeue; pure builders set replicas/image/PVC/owner-refs.
2. **Why now** — the reconciler is the operator's core (the caller the gate drives); pure builders are fast-unit-testable before the envtest gate (ADR D1; blueprint Corner 4 cites cnpg `cluster_controller.go:169,1294`).

#### Evidence
cnpg reconcile (`.claude/knowledge-base/references/cloudnative-pg/internal/controller/cluster_controller.go:169` Reconcile, `:1294-1337` SetupWithManager.Owns); Service builder + ownership (`pkg/specs/services.go:132-156,223`); `controllerutil.SetControllerReference` (`internal/controller/cluster_create.go:1537`).

#### Files to edit
```
operator/internal/controller/resources.go (NEW) — pure buildStatefulSet + buildService
operator/internal/controller/theodbcluster_controller.go (NEW) — Reconcile + updateStatus + SetupWithManager + RBAC markers
```

#### Deep file dependency analysis
- `resources.go`: pure functions over `*TheoDBCluster` → `*appsv1.StatefulSet`/`*corev1.Service` (+ `SetControllerReference`). Unit-testable without a client.
- `theodbcluster_controller.go`: depends on the controller-runtime `client.Client` (interface — injected); reuses `resources.go` builders.

#### Deep Dives
- `buildStatefulSet(c)`: `Replicas=&c.Spec.Instances; Selector/Template labels {app:theodb,cluster:c.Name}; ServiceName=c.Name+"-hl"; Container{Image:c.Spec.Image, Ports[{c.Spec.Port}]}; VolumeClaimTemplates[{"data",RWO,storage:c.Spec.StorageSize}]`.
- `buildService(c)`: `Selector{cluster:c.Name}; Ports[{c.Spec.Port}]; ClusterIP`.
- `Reconcile`: Get (IgnoreNotFound → done); if `Spec.Image==""` → status Phase="Error" + typed error; ensure StatefulSet (create with owner ref else update ONLY mutable fields — EC-1); ensure Service; `updateStatus` (ReadyInstances=ss.Status.ReadyReplicas; Phase Healthy iff ready==Instances else Initializing; set Ready condition); requeue if not Healthy.
- **StatefulSet mutable-fields-only update (EC-1):** K8s REJECTS updates to a StatefulSet's `Selector`/`ServiceName`/`VolumeClaimTemplates`. The update path fetches the EXISTING StatefulSet and patches ONLY `Spec.Replicas` + the container `Image` (the mutable fields), never re-applying the whole built spec. Changing `StorageSize`/selector in-place is NOT supported (would require a recreate — out of M23 scope; documented).
- Idempotency: create-or-update (Get → IsNotFound ? Create : patch-mutable-if-changed); a 2nd reconcile makes NO change — the StatefulSet `ResourceVersion` does NOT churn (EC-2: no spurious Update → no controller hot-loop).
- **envtest has no kubelet (EC-4):** in the T3.1 gate, `StatefulSet.Status.ReadyReplicas` stays 0 (no kubelet starts pods), so Phase resolves to "Initializing", NEVER "Healthy". The gate asserts Phase is SET (=="Initializing"), not Healthy.
- RBAC markers: `+kubebuilder:rbac:groups=core.theodb.io,resources=theodbclusters;theodbclusters/status;theodbclusters/finalizers,verbs=…` + `groups=apps,resources=statefulsets,verbs=…` + `groups="",resources=services,verbs=…`.

#### Pseudo-code / Signatures
```go
func buildStatefulSet(c *v1.TheoDBCluster) *appsv1.StatefulSet { /* replicas, image, PVC, labels */ }
func buildService(c *v1.TheoDBCluster) *corev1.Service { /* selector, port */ }
func (r *Reconciler) Reconcile(ctx, req) (ctrl.Result, error) {
  get c; if NotFound return; if c.Spec.Image=="" { set Error status; return err }
  ensure(buildStatefulSet(c) with SetControllerReference); ensure(buildService(c) ...)
  updateStatus(c); if !healthy return Result{RequeueAfter: 5s}
}
```

#### Tasks
1. Implement `buildStatefulSet`/`buildService` (pure) + `SetControllerReference`.
2. Implement `Reconcile` (get → ensure SS → ensure Svc → updateStatus → requeue) with create-or-update helpers.
3. Implement `updateStatus` (Phase/ReadyInstances/Conditions).
4. Implement `SetupWithManager().For(TheoDBCluster).Owns(StatefulSet).Owns(Service)` + RBAC markers; `make manifests`.

#### TDD
```
RED: TestBuildStatefulSet_ReplicasImagePVC — build for Instances=3,Image=X,StorageSize=10Gi → *Replicas==3, container image==X, VolumeClaimTemplate storage==10Gi, owner ref set
RED: TestBuildService_SelectorPort — selector{cluster}=name, port==Spec.Port
RED: TestReconcile_MissingImage_Errors — Spec.Image=="" → typed error + status Phase=="Error" (fake client OK for this pure-logic check)
RED: TestBuildStatefulSet_OwnerRef — SetControllerReference sets controller=true owner = the CR
RED: TestReconcile_StorageSizeChange_NoImmutableUpdate — changing Spec.StorageSize does NOT attempt a full-spec StatefulSet Update (only replicas+image patched; immutable fields untouched, EC-1)
GREEN: implement resources + reconcile
REFACTOR: extract create-or-update helper; else none
VERIFY: cd operator && go test ./internal/controller/... -run 'Build|MissingImage'
```

#### Concurrency tests

(none — single-threaded). Per-object, controller-runtime serializes reconciles per CR object; the only race is the status-subresource update conflict, handled by `IsConflict` → requeue (asserted in the envtest gate T3.1, not a separate concurrency test). The pure builders have no shared state.

#### Acceptance Criteria
- [ ] All RED unit tests green — `cd operator && go test ./internal/controller/... -run 'Build|MissingImage'` exits 0.
- [ ] Pass: vet — `cd operator && go vet ./...` clean.
- [ ] Pass: lint — `gofmt -l operator/` empty (formatted).
- [ ] Pass: size — `theodbcluster_controller.go` ≤ 300 lines, `resources.go` ≤ 200.

#### DoD
- [ ] Builders + reconcile implemented; RBAC manifest regenerated; unit tests green.

---

## Phase 3: Real-envtest gate + CLI + reproducible deploy

**Objective:** prove the reconciler against a real kube-apiserver (envtest), ship the CLI + reproducible deploy, and
record the reconcile-gate evidence.

### T3.1 — Real-envtest reconcile gate (the milestone evidence)

#### Objective
`suite_test.go` (std testing) starts a real `envtest.Environment`, installs the CRD, creates a `TheoDBCluster`, runs
`Reconcile`, asserts the StatefulSet (replicas==Instances, image) + Service + status exist with owner refs, measures
reconcile wall time, asserts idempotency.

#### Why this step (action + reasoning)
1. **What this step does** — the "100% functional evidence": a real-apiserver reconcile proof (ADR D2/D4).
2. **Why now** — it is the milestone's acceptance metric (Goal); it requires the reconciler (Phase 2) + the CRD (Phase 1) (blueprint Corner 1; cnpg test shape `cluster_controller_test.go:46-103`, real-envtest Makefile `Makefile:130-135`).

#### Evidence
cnpg suite (`.claude/knowledge-base/references/cloudnative-pg/internal/controller/suite_test.go:73-137` — fake client, HONEST: M23 uses REAL envtest) + the Makefile envtest target (`Makefile:130-135,393`).

#### Files to edit
```
operator/internal/controller/suite_test.go (NEW) — TestMain envtest start/stop + scheme + k8sClient
operator/internal/controller/theodbcluster_controller_test.go (NEW) — the reconcile gate (real apiserver)
operator/Makefile — envtest + test targets (setup-envtest, KUBEBUILDER_ASSETS)
docs/benchmarks/m23-operator-reconcile.md (NEW) — reconcile wall time + idempotency evidence
```

#### Deep file dependency analysis
- `suite_test.go`: `TestMain` → `envtest.Environment{CRDDirectoryPaths:[config/crd/bases]}.Start()` → `client.New(cfg)` → scheme register → `m.Run()` → `testEnv.Stop()`.
- `theodbcluster_controller_test.go`: uses the real `k8sClient` + the reconciler.

#### Deep Dives
- Gate: create CR (Instances=3, Image="theo-db:latest") → `reconciler.Reconcile(ctx, req)` (timed) → `k8sClient.Get` the StatefulSet (assert *Replicas==3, image, owner ref controller==CR) + the Service (assert selector/port) + the CR status (Phase set) → second `Reconcile` → `List` asserts exactly one StatefulSet/Service (idempotent, no error).
- Precondition (EC-2): `KUBEBUILDER_ASSETS` set by `setup-envtest use` (the Makefile `test` target). Offline → the test fails loudly with the missing-binary reason (honest BLOCKED, never skipped).
- Evidence doc: the reconcile wall time (ms) + the idempotency assertion + repro command.

#### Pseudo-code / Signatures
```go
func TestMain(m *testing.M) { testEnv := &envtest.Environment{CRDDirectoryPaths: …}; cfg,_ := testEnv.Start(); … ; os.Exit(m.Run()) }
func TestReconcile_ProvisionsStatefulSetAndService(t *testing.T) {
  create CR; t0:=now; reconcile; dt:=since(t0)
  get StatefulSet → assert replicas==3,image,ownerRef; get Service → assert; get CR → assert Status.Phase!=""
  reconcile again → list → assert exactly one SS+Svc (idempotent)
}
```

#### Tasks
1. `make envtest` (`setup-envtest use` → KUBEBUILDER_ASSETS).
2. Implement `suite_test.go` (real envtest TestMain).
3. Implement the reconcile gate (provision + idempotency + timing).
4. Write `docs/benchmarks/m23-operator-reconcile.md` (run the gate, record wall time + idempotency).

#### TDD
```
RED: TestReconcile_ProvisionsStatefulSetAndService — real envtest: CR(3,image) → Reconcile → StatefulSet(replicas==3,image,ownerRef) + Service exist + status.Phase set
RED: TestReconcile_Idempotent — second Reconcile → exactly one StatefulSet + one Service, no error
RED: TestReconcile_NoSpuriousUpdate — StatefulSet ResourceVersion is UNCHANGED after a second reconcile (drift check is a true no-op, no hot-loop, EC-2)
RED: TestReconcile_PhaseInitializingInEnvtest — Status.Phase is SET to "Initializing" (envtest has no kubelet → ReadyReplicas==0, never Healthy, EC-4)
GREEN: (reconciler from T2.1) — make the envtest gate green
REFACTOR: extract a test helper for CR creation; else none
VERIFY: cd operator && make test  (KUBEBUILDER_ASSETS via setup-envtest)
```

#### Concurrency tests

(none — single-threaded). The gate drives `Reconcile` sequentially; controller-runtime's per-object serialization is the production guarantee. The idempotency test (2nd reconcile = no dup) is the convergence proof.

#### Acceptance Criteria
- [ ] `cd operator && make test` exits 0 (real envtest apiserver; KUBEBUILDER_ASSETS set).
- [ ] `docs/benchmarks/m23-operator-reconcile.md` exists with the reconcile wall time + idempotency evidence + repro command.
- [ ] The gate uses a REAL `envtest.Environment` (not a fake client) — asserted by the test importing `sigs.k8s.io/controller-runtime/pkg/envtest`.

#### DoD
- [ ] envtest gate green; evidence doc written.

### T3.2 — `theodbctl` CLI + reproducible deploy

#### Objective
`cmd/theodbctl` cobra CLI (apply/get/delete a TheoDBCluster) + the `config/` kustomize deploy + `make install`/
`make deploy` + `cmd/manager/main.go`.

#### Why this step (action + reasoning)
1. **What this step does** — the CLI + the reproducible deploy bundle + the manager binary (the operational surface).
2. **Why now** — the DoD requires a CLI + reproducible deploy; the manager wires the reconciler to a real cluster (blueprint Corner 3; cnpg `cmd/kubectl-cnpg/main.go:58-147`, `config/default/kustomization.yaml:17-22`).

#### Evidence
cnpg CLI (`.claude/knowledge-base/references/cloudnative-pg/cmd/kubectl-cnpg/main.go:29,58-147`); deploy (`config/default/kustomization.yaml:4,11,17-22`, `config/rbac/role.yaml`); manager (`internal/cmd/manager/controller/controller.go:55,176,280`).

#### Files to edit
```
operator/cmd/theodbctl/main.go (NEW) — cobra root + apply/get/delete subcommands
operator/cmd/manager/main.go (NEW, scaffold) — manager + scheme + SetupWithManager + Start
operator/config/{crd,rbac,manager,default}/** (NEW/generated) — kustomize bundle
operator/Makefile — install/deploy targets
```

#### Deep file dependency analysis
- `theodbctl/main.go`: builds a controller-runtime client (client-go config) + cobra subcommands (Create/List/Delete the CR).
- `cmd/manager/main.go`: scaffolded; registers the scheme + the reconciler; `mgr.Start`.
- `config/`: kubebuilder-scaffolded kustomize; `make deploy` builds + applies.

#### Deep Dives
- CLI: `theodbctl apply -f f.yaml` (decode YAML → `*TheoDBCluster` → `client.Create`); `theodbctl get [-n ns]` (`client.List` → table); `theodbctl delete <name> [-n ns]` (`client.Delete`).
- Deploy: `make install` (`kustomize build config/crd | kubectl apply`), `make deploy IMG=…` (`kustomize build config/default | kubectl apply`); dry-run validated (`kubectl apply --dry-run=client`).
- Edge cases: `apply -f` missing file → typed error; `get` empty list → "no clusters"; `delete` not-found → clear message.

#### Tasks
1. Implement `cmd/theodbctl/main.go` (cobra apply/get/delete).
2. Verify `cmd/manager/main.go` (scaffold) wires the reconciler.
3. Generate/trim `config/` kustomize; implement `make install`/`make deploy`.

#### TDD
```
RED: TestTheodbctl_ApplyParsesYAML — apply -f decodes a TheoDBCluster YAML into the typed object (unit test of the decode path, no cluster)
RED: TestTheodbctl_DeleteMissingFile_Errors — apply with a missing file → typed error
RED: TestTheodbctl_ApplyWrongKind_Errors — apply -f a non-TheoDBCluster / malformed YAML → typed decode error, no client call (EC-3)
RED (deploy, shell): `kustomize build operator/config/default` succeeds AND `kubectl apply --dry-run=client -f -` validates (no apply)
GREEN: implement CLI + deploy bundle + manager
REFACTOR: share the client builder; else none
VERIFY: cd operator && go test ./cmd/... && kustomize build config/default | kubectl apply --dry-run=client -f -
```

#### Concurrency tests

(none — single-threaded) — the CLI is a one-shot command; the manager's concurrency is controller-runtime's (covered by T3.1's per-object serialization).

#### Acceptance Criteria
- [ ] `cd operator && go test ./cmd/...` exits 0 (CLI decode/error tests).
- [ ] `kustomize build operator/config/default` succeeds AND `kubectl apply --dry-run=client` validates the bundle.
- [ ] `theodbctl --help` lists apply/get/delete.

#### DoD
- [ ] CLI builds + tests green; deploy bundle builds + dry-run validates; manager wired.

## Failure scenarios (external I/O — the Kubernetes API via the controller-runtime client)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| K8s apiserver (envtest) | apiserver/etcd binaries missing (KUBEBUILDER_ASSETS unset) | run `make test` without `setup-envtest use` | the gate fails LOUDLY with the missing-binary reason (honest BLOCKED, never a silent skip/pass) |
| K8s API (status update) | status-subresource update conflict (concurrent writers) | (covered by controller-runtime) the reconcile handles `apierrors.IsConflict` → requeue | no crash; requeue + converge (idempotency gate proves convergence) |
| CR (Spec) | `Spec.Image == ""` (missing required image) | reconcile a CR with no image | typed error + status Phase="Error"; no StatefulSet created (fail-fast) |
| CR lifecycle | CR deleted mid-reconcile (NotFound on Get) | delete the CR then reconcile its req | `client.IgnoreNotFound` → `Reconcile` returns `{}, nil` (no error, no orphan) |
| Deploy | malformed kustomize bundle | `kustomize build config/default` on a broken overlay | `make deploy` fails loudly at build (before any apply) |

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | TheoDBCluster CRD (spec/status + deepcopy + manifest) | T1.1 | `api/v1/theodbcluster_types.go` + controller-gen |
| 2 | Reconciler provisions StatefulSet + Service (owner refs, status) | T2.1 | `resources.go` builders + `Reconcile` + `SetupWithManager.Owns` |
| 3 | Real-envtest reconcile evidence (CR → resources, idempotent, timed) | T3.1 | `suite_test.go` real envtest + the gate + `docs/benchmarks/m23-operator-reconcile.md` |
| 4 | CLI | T3.2 | `cmd/theodbctl` cobra (apply/get/delete) |
| 5 | Reproducible deploy | T3.2 | `config/` kustomize + `make install`/`make deploy` + dry-run validation |
| 6 | Gateway (= the K8s Service, D3) | T2.1 | the `corev1.Service` the reconciler provisions |
| 7 | Own Go code with tests | T1.1/T2.1/T3.1/T3.2 | unit tests (builders, CLI decode) + real-envtest gate |
| 8 | No external dep beyond standard K8s ecosystem (DoD) | T1.1 | `operator/go.mod` declares only controller-runtime + k8s.io/* + cobra + std `testing` (no ginkgo); zero AGPL (ADR D3) |
| 9 | Reuse the K8s-standard generator (Rule 9) | T1.1 | kubebuilder/controller-gen scaffold |
| 10 | Fail-fast typed errors + honest BLOCKED | T2.1 (missing image), T3.1 (envtest precondition), Failure scenarios | typed reconcile error + loud envtest-binary failure |

**Coverage: 10/10 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cd operator && make test` (real envtest) + `go test ./...` green
- [ ] Zero vet issues — `cd operator && go vet ./...`; `gofmt -l operator/` empty
- [ ] File-size budget respected (each Go file ≤ 300 lines; split otherwise)
- [ ] CHANGELOG.md updated under `[Unreleased] § Added`
- [ ] Backward compatibility — the operator is a NEW isolated `operator/` module; theodb_rs (Rust) + benchmarks (Python) + the SQL extension are untouched
- [ ] Plan-specific: the **real-envtest reconcile gate** is green (CR → StatefulSet[replicas==Instances]+Service+status, idempotent) AND `make deploy` dry-run validates; evidence in `docs/benchmarks/m23-operator-reconcile.md` (reconcile wall time + idempotency)
- [ ] Runtime-metric proof — the reconcile gate runs against a REAL kube-apiserver (envtest), not a fake client
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merged

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validate the operator end-to-end: build, vet, the real-envtest reconcile gate, the deploy bundle, the CLI.

### Execution
```
cd operator
go build ./...                                        # compiles
go vet ./... && test -z "$(gofmt -l .)"               # vet + format
make test                                             # real-envtest reconcile gate (KUBEBUILDER_ASSETS)
kustomize build config/default | kubectl apply --dry-run=client -f -   # reproducible deploy validates
go run ./cmd/theodbctl --help                         # CLI smoke
```

### Acceptance Criteria
- [ ] `go build ./...` + `go vet ./...` clean; `gofmt -l` empty
- [ ] `make test` green — the real-envtest reconcile gate (CR → StatefulSet+Service+status, idempotent) passes against a real apiserver
- [ ] `kustomize build config/default | kubectl apply --dry-run=client` validates the deploy bundle
- [ ] `theodbctl --help` lists apply/get/delete
- [ ] Failure scenarios exercised (missing image → typed error; missing envtest binary → loud failure; CR NotFound → no-op)
- [ ] `docs/benchmarks/m23-operator-reconcile.md` written with reconcile wall time + idempotency + repro commands

### If Validation Fails
1. Separate plan-caused failures from environment (e.g., envtest binary not installed → run `make envtest`).
2. Fix all plan-caused failures; re-run the chain.
3. If the envtest apiserver genuinely cannot start (offline, no binaries), record an honest BLOCKED — never a silent skip or a fake-client substitute (that would not be the required evidence).
