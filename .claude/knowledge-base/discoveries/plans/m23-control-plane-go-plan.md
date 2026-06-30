# Discovery Plan: M23 — Control plane in Go (K8s operator + CLI + gateway)

> **Version 1.1** (edge-cases absorbed: EC-1 real-envtest gate not fake client; EC-2 envtest binary precondition; EC-3 std testing not ginkgo; EC-4 gateway=Service; EC-5 StatefulSet divergence honest) — Investigate how to build TheoDB's own **Kubernetes operator** in Go (model: cloudnative-pg) —
> a `TheoDBCluster` CRD + a controller-runtime reconciler that provisions/manages a running TheoDB cluster
> (StatefulSet + Service), a CLI, and a reproducible deploy. In scope: `cloudnative-pg` (the Go Postgres-operator
> SOTA we model) and `patroni`/`citus` (HA/topology datapoints). Output: a blueprint that decides the operator
> architecture (CRD shape, reconcile loop, resource provisioning, deploy + CLI) and the evidence design (envtest
> reconcile proof), so `/to-plan` for M23 can scope the implementation honestly (measurement-first; standard K8s
> ecosystem deps only, per the DoD).

**Slug:** `m23-control-plane-go`
**Owner:** paulohenriquevn
**Created:** 2026-06-30
**Time budget:** 6h (per-project breakdown in ADR D1)

## Context

M23 (`ROADMAP-v2.md:139`) requires an **own Go control plane**: a Kubernetes operator (cloudnative-pg model) +
CLI + gateway that makes TheoDB deployable/manageable (the path to managed). DoD (`ROADMAP-v2.md:146-147`): the
operator provisions/manages a TheoDB cluster (CRD + reconciliation); a CLI; a reproducible deploy; **own Go code
with tests; no external dependency beyond the standard K8s ecosystem**. This is the first **Go** pillar — the
prior milestones (M17–M22) built the Rust `theodb_rs` database engine; M23 builds the operational layer around the
shipped `theo-db` container image.

This is a NEW pillar (not an extension of the Rust engine). The toolchain is present (Go 1.24, kubebuilder,
controller-gen, setup-envtest, kind, kubectl, docker). The honest evidence for a control plane is NOT a recall
benchmark — it is a **reconcile proof**: the operator, run against a REAL kube-apiserver (controller-runtime
`envtest`), reconciles a `TheoDBCluster` CR into the desired resources (StatefulSet + Service), with measured
reconcile timing + idempotency. cloudnative-pg is the model (`api/v1/cluster_types.go`,
`internal/controller/cluster_controller.go`, `config/`).

## Objective

Decide the architecture of TheoDB's own Go K8s operator (CRD + reconcile + provisioning + deploy + CLI) and the
envtest-based evidence design, so the M23 implementation plan is evidence-backed and the reconcile gate is defined
before any code is written.

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for every in-scope reference project (cloudnative-pg / patroni / citus)
- [ ] Recommendations section provides at least one concrete decision proposal per in-scope research question
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/cloudnative-pg/` | `api/v1/` (CRD types + status/conditions + deepcopy), `internal/controller/` (reconcile + suite_test), `config/` (kustomize deploy), `cmd/` (manager + kubectl plugin), `Makefile`, `go.mod` | The Go Postgres-operator SOTA — the exact model for CRD/reconcile/deploy/CLI |
| `.claude/knowledge-base/references/patroni/` | top-level + `patroni/` (HA/topology concepts) | HA/primary-replica topology datapoint (concept-only; M23 keeps HA minimal) |
| `.claude/knowledge-base/references/citus/` | top-level docs (distributed topology) | Distributed-topology datapoint (concept-only) |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/cloudnative-pg/internal/management/`, `pkg/` (the instance manager / WAL / backup machinery) | Production backup/WAL/failover is beyond a measurement-first M23 (a StatefulSet provisions instances; full instance-manager is a follow-up) |
| `.claude/knowledge-base/references/patroni/` deep DCS internals (etcd/consul/zookeeper drivers) | M23 uses the K8s API as the source of truth (operator pattern), not a Patroni DCS |
| Any project NOT cloned into `.claude/knowledge-base/references/` | Cross-Project Rule: never claim a project feature without reading its source |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** cloudnative-pg: 4h (the operator model we copy — CRD + reconcile + provisioning + deploy + CLI);
patroni + citus: 2h combined (HA/topology concept datapoints only).

**Rationale:** cloudnative-pg is the model TheoDB's operator mirrors; its CRD/reconcile/deploy patterns earn the
deepest read. patroni/citus inform the HA/topology decision (which M23 keeps minimal) but don't need a full read.

**Alternatives considered:** equal split (rejected — cloudnative-pg is the crux); cloudnative-pg-only (rejected —
violates PhD-rigor ≥2 sources for the HA/topology technique).

**Stop condition — per question (mandatory):** Fase A empty after 3 retries → mark BLOCKED "Fase A exhausted",
continue; never pad from another question's scope.

**Stop condition — per project (mandatory):** budget exhausted with pending questions → mark BLOCKED "budget
exhausted", continue; if all remaining projects exhausted, emit `<promise>BLUEPRINT_BLOCKED</promise>`.

**Anti-pattern:** NEVER fabricate Fase B answers (Unbreakable Rule 3).

**Consequences:** the halt-loop stops on budget exhaustion; blocked questions surface as next-discovery seed.

### D2 — Investigation depth

**Decision:** Read cloudnative-pg's `cluster_types.go`, `cluster_controller.go` (Reconcile), a resource-builder
file, `suite_test.go`, the `config/` kustomize layout, and `cmd/` end-to-end. For patroni/citus, Grep the
HA/topology concepts (Fase A) then read the hotspot (Fase B). Do NOT read the instance-manager/WAL machinery.

**Rationale:** the operator parity demands a full read of the CRD + reconcile + deploy; HA is a concept best
located by grep then read at the hotspot (KISS). Reading the full instance manager blows the budget (YAGNI).

**Alternatives considered:** full read of cloudnative-pg (rejected — budget + out-of-scope machinery); grep-only
(rejected — loses the reconcile + deploy patterns).

**Consequences:** the blueprint cites line-exact CRD/reconcile/deploy patterns; the instance-manager is deferred.

### D3 — The architecture + evidence decisions are blueprint deliverables, not single questions

**Decision:** the operator architecture (StatefulSet vs pod-per-instance; HA scope) and the evidence design
(envtest reconcile gate) are synthesized in the blueprint's Cross-cutting Comparison + ADRs from the technique
questions (Q1 CRD, Q2 reconcile, Q3 provisioning) + the tooling questions (Q4 scaffold/envtest, Q5 deploy/CLI) +
the test question (Q7) — decisions we make, fed by the questions.

**Rationale:** per `/discover-plan` rule "a plan ASKS questions; it doesn't answer them." Mirrors M21/M22 ADR D3.

**Alternatives considered:** a dedicated "StatefulSet or pods?" question (rejected — it is a TheoDB design choice,
fed by Q3); deferring the evidence design to `/to-plan` (rejected — the DoD demands a reconcile proof, designed in
the blueprint).

**Consequences:** the blueprint contains an explicit ADR on the operator architecture + the envtest evidence gate.

### D4 — Standard K8s ecosystem deps only (DoD); minimal HA for measurement-first

**Decision:** M23 uses only the standard K8s ecosystem (controller-runtime, client-go, apimachinery, k8s.io/api —
all Apache-2.0) + the Go stdlib + cobra for the CLI (the kubebuilder-standard). No exotic deps (the DoD says "sem
dep externa além do ecossistema K8s padrão"). HA is minimal (StatefulSet replicas; primary/replica via the image's
existing mechanisms) — full Patroni-style failover is a follow-up (M23 delivers provision/manage, not HA-grade
failover).

**Rationale:** the DoD constrains deps to the K8s ecosystem; measurement-first scopes HA to "provisions/manages a
cluster", not production failover. patroni/citus are concept references, not borrowed.

**Alternatives considered:** copy cloudnative-pg's pod-per-instance + instance-manager (rejected — multi-month,
out of scope); add a DCS like patroni (rejected — the K8s API is the operator's source of truth).

**Consequences:** the blueprint recommends a StatefulSet-based operator with standard deps; HA hardening deferred.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — grep map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does cloudnative-pg design its **CRD** — `ClusterSpec`/`ClusterStatus`, required fields (instances, image, storage), status conditions, and the generated deepcopy? | techniques | `.claude/knowledge-base/references/cloudnative-pg/` | `grep -nE "type ClusterSpec\|type ClusterStatus\|Instances\|ImageName\|StorageConfiguration\|\+kubebuilder" cloudnative-pg/api/v1/cluster_types.go cloudnative-pg/api/v1/cluster_conditions.go` | Read `api/v1/cluster_types.go` (`ClusterSpec:217`, `Instances:264`, `ClusterStatus`) + `cluster_conditions.go`; capture the spec/status shape + the kubebuilder markers + condition pattern | The minimal `TheoDBClusterSpec` (instances, image, storage, port) + `Status` (phase, conditions, readyInstances) + kubebuilder markers with `path:line` |
| Q2 | How does the **reconcile loop** work — `Reconcile(ctx, req)`, fetch-or-requeue, owned-resource creation, `ctrl.Result{RequeueAfter}`, finalizers, `SetupWithManager`/`Owns`? | techniques | `.claude/knowledge-base/references/cloudnative-pg/` | `grep -nE "func.*Reconcile\|SetupWithManager\|Owns\(\|ctrl.Result\|RequeueAfter\|Finalizer\|client.IgnoreNotFound" cloudnative-pg/internal/controller/cluster_controller.go cloudnative-pg/internal/controller/finalizers_delete.go` | Read `cluster_controller.go` (`Reconcile:169`, `NewClusterReconciler:111`, the requeue paths) + finalizer handling; capture the reconcile skeleton + owned-resource + requeue + finalizer patterns | The reconcile skeleton (get → default → ensure resources → status → requeue) + `SetupWithManager().Owns(StatefulSet,Service)` with `path:line` |
| Q3 | How does the operator **provision the workload** — does it create a StatefulSet or pods directly, the Service, the ConfigMap, owner references, and labels? (M23 will use a StatefulSet — note the divergence honestly) | techniques | `.claude/knowledge-base/references/cloudnative-pg/` | `grep -rnE "appsv1\.\|corev1.Service\|OwnerReference\|SetControllerReference\|StatefulSet\|PodSpec\|VolumeClaimTemplate" cloudnative-pg/internal/controller/` | Read the resource-builder hotspots (Service/Pod/PVC construction); capture how the workload + Service + owner refs + labels are built; HONESTLY note cloudnative-pg uses pods-per-instance, M23 will use a StatefulSet (KISS) | The StatefulSet + Service + ConfigMap builder sketch (owner refs, labels, the theo-db image, PVC template) — labelled as a TheoDB design adapting the cloudnative-pg pattern, with `path:line` |
| Q4 | What **scaffolding + envtest** does a controller-runtime operator use — the manager `main`, scheme registration, `manager.Options`, and the `envtest` test environment (`suite_test.go`, `setup-envtest`)? | tools | `.claude/knowledge-base/references/cloudnative-pg/` | `grep -nE "ctrl.NewManager\|manager.Options\|AddToScheme\|envtest\|testEnv\|RunSpecs\|NewClientBuilder\|SetupWithManager" cloudnative-pg/cmd/manager cloudnative-pg/internal/controller/suite_test.go cloudnative-pg/Makefile` | Read `cmd/manager` main + `suite_test.go` + the Makefile `test`/`envtest` targets; capture the manager setup + the test-env pattern (note: cnpg uses a fake client in this suite; M23 will use real envtest for the reconcile proof) | The manager `main` skeleton + the `envtest` setup (`testEnv.Start()`, scheme, `k8sClient`) + the Makefile targets with `path:line` |
| Q5 | How is the operator **deployed reproducibly** + what is the **CLI** shape — the `config/` kustomize (crd/default/manager/rbac), RBAC markers, and the `kubectl` plugin (`cmd/kubectl-cnpg`)? | tools | `.claude/knowledge-base/references/cloudnative-pg/` | `grep -rnE "kustomization\|namespace\|ClusterRole\|kubebuilder:rbac\|cobra.Command\|kubectl-cnpg" cloudnative-pg/config/ cloudnative-pg/cmd/kubectl-cnpg/` + `ls cloudnative-pg/config/{crd,default,manager,rbac}` | Read the `config/` kustomize layout + RBAC + a `cmd/kubectl-cnpg` command; capture the deploy bundle (CRD+RBAC+manager) + the CLI command pattern (cobra) | The reproducible-deploy layout (`config/{crd,rbac,manager,default}` + `make deploy`/`make install`) + the CLI command skeleton with `path:line` |
| Q6 | What runtime **dependencies** does the operator pull in (controller-runtime, client-go, apimachinery, k8s.io/api, cobra) with versions + **licenses** (Apache/MIT/BSD gate — all standard K8s ecosystem)? | deps | `.claude/knowledge-base/references/cloudnative-pg/` | `grep -nE "sigs.k8s.io/controller-runtime\|k8s.io/(api\|apimachinery\|client-go)\|spf13/cobra\|ginkgo\|gomega" cloudnative-pg/go.mod` | Read `go.mod`; capture each K8s-ecosystem dep + version + license (all Apache-2.0); confirm M23's deps stay inside the standard ecosystem (DoD constraint) | Table: dep → version → license → purpose → standard-K8s-ecosystem? (yes/no) |
| Q7 | How does cloudnative-pg **test** its reconciler, so M23 mirrors the test shape and designs a **REAL-envtest reconcile gate** (CR → StatefulSet+Service) as the milestone evidence? (EC-1: cnpg's `suite_test.go` uses a FAKE client + ginkgo for unit speed; the REAL apiserver path is the Makefile `envtest` target — M23's EVIDENCE gate MUST use real envtest, not the fake client) | tests | `.claude/knowledge-base/references/cloudnative-pg/` | `grep -rnE "envtest\|fake.NewClientBuilder\|Reconcile(\|Expect(\|RunSpecs\|func Test\|testEnv" cloudnative-pg/internal/controller/suite_test.go cloudnative-pg/internal/controller/cluster_controller_test.go cloudnative-pg/Makefile` | Read representative controller tests + the Makefile envtest target; capture how a reconcile is driven + asserted; design the M23 gate on a REAL `envtest.Environment{}.Start()` apiserver → create CR → Reconcile → assert StatefulSet+Service exist + owner refs + status; + reconcile timing. NOTE the precondition (EC-2): `setup-envtest use` must install the apiserver/etcd binaries (KUBEBUILDER_ASSETS) | Table: test → what it drives/asserts → `path:line`; + a sketch of the M23 REAL-envtest reconcile gate (the milestone evidence) + the binary precondition |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q7 | Covered |
| Dependencies | Q6 | Covered |
| Tools | Q4, Q5 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

**Coverage: 4/4 corners covered (100%)**

Question budget: 7 total (within the 5–10 default window); techniques carries 3 (≥2 per PhD-rigor R4, within the
≤3-per-corner budget); tools 2; deps + tests 1 each. Each question maps to exactly one corner.

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | every `.claude/knowledge-base/references/{project}/{path}` declared in its Fase A exists | Mark Qx BLOCKED "path not found", continue |
| Per-question Fase A budget | Fase A returned ≥1 hotspot OR 3 query-variant retries attempted | After 3 retries empty, mark Qx BLOCKED "Fase A exhausted"; continue |
| After answering Qx | blueprint section under Qx has ≥1 citation | Re-iterate Qx (1 retry max) |
| StatefulSet honesty (Q3) | the blueprint states HONESTLY that cloudnative-pg uses pods-per-instance and M23 adopts a StatefulSet (KISS) — not a borrowed fact misrepresented | Re-state honestly before DONE |
| Evidence design (Q7, EC-1) | the gate uses a REAL `envtest.Environment` apiserver (NOT cloudnative-pg's fake client) → CR → Reconcile → assert StatefulSet+Service; concrete, not hand-waved | Re-state the real-envtest gate before DONE |
| envtest precondition (Q7, EC-2) | the blueprint notes the gate needs `setup-envtest use` to install apiserver/etcd binaries (one-time network download); offline → honest BLOCKED, not silent skip | Add the precondition note before DONE |
| Framework/gateway honesty (EC-3/EC-4) | blueprint records: std `testing` (not ginkgo) per D4; "gateway" = the K8s Service the operator creates (not a separate HTTP gateway) | State both honestly before DONE |
| Per-project time budget | project budget not exhausted | When exhausted, mark remaining Qx BLOCKED "budget exhausted"; advance |
| Before promising complete | all 4 coverage corners populated AND an operator-architecture + evidence ADR exists | Refuse promise, continue iterating |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly marked BLOCKED with reason
- [ ] All four coverage corners have populated sections in the blueprint
- [ ] Every citation in the blueprint points to a real `.claude/knowledge-base/references/{...}` path
- [ ] At least one ADR synthesizes the **operator architecture (StatefulSet/CRD/reconcile/deploy/CLI)** + the **envtest evidence** design (D3)
- [ ] The evidence gate reuses controller-runtime `envtest` (Q7) — a real-apiserver reconcile proof
- [ ] Time budget respected per project
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint saved at `.claude/knowledge-base/discoveries/blueprints/m23-control-plane-go-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed → re-score)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100% covered
- [ ] ADRs reference at least one principle from project rules (Rule 9 Don't-Reinvent; KISS; `.claude/rules/discover-phd-rigor.md`; `architecture.md` for the operator↔CRD boundary)
