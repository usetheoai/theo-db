# Discover Edge Case Review — m23-control-plane-go

Date: 2026-06-30
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/m23-control-plane-go-plan.md
Research questions analyzed: 7
Edge cases found: 5 (MUST FIX: 1, SHOULD TEST: 1, DOCUMENT: 3)

The plan is well-formed (paths validated, 4 corners, 4 ADRs, budget + stop conditions). Findings below are the
ones not yet foreseen for `/discover-execute`.

## MUST FIX

### EC-1: the evidence gate MUST use a REAL envtest apiserver, not cloudnative-pg's fake client
- **Affected question:** Q7 (test shape / evidence design)
- **Family:** Interpretation
- **Scenario:** Verified: cloudnative-pg's `internal/controller/suite_test.go` uses `fake.NewClientBuilder()` (an
  in-memory fake client) + ginkgo for unit-speed tests; the REAL apiserver path is the Makefile `envtest` target
  (`setup-envtest`). If `/discover-execute` borrows the fake-client pattern as the M23 evidence gate, the
  "reconcile proof" would NOT prove real reconciliation (the fake client doesn't run admission, defaulting, or a
  real apiserver) — failing the goal's "100% functional evidence".
- **Impact:** a fake-client "gate" is not measurement-first evidence; the milestone would ship an unvalidated claim.
- **Suggested fix:** Q7's blueprint answer MUST design the M23 gate on a **real `envtest` apiserver** (`testEnv :=
  &envtest.Environment{CRDDirectoryPaths: …}; testEnv.Start()`) — create the CR, run `Reconcile`, assert the
  StatefulSet+Service exist with owner refs. The fake client is acceptable only for fast unit tests of pure
  resource builders, never as the reconcile evidence.

## SHOULD TEST

### EC-2: envtest requires the kube-apiserver + etcd binaries (network download via setup-envtest)
- **Affected question:** Q7 (evidence gate prerequisite)
- **Suggested halt-loop checkpoint:** before designing the gate DONE, the blueprint notes the gate's PRECONDITION —
  `setup-envtest use` must have installed the `kube-apiserver`/`etcd` binaries (KUBEBUILDER_ASSETS), a one-time
  network download. The implement phase must run it before the envtest gate; if offline, the gate is BLOCKED
  honestly (not silently skipped).

## DOCUMENT

### EC-3: test framework — std `testing` (not ginkgo/gomega), per the DoD "standard K8s ecosystem" + parsimony
- **Accepted risk (decision):** cloudnative-pg uses ginkgo/gomega (`suite_test.go:51-52`). The DoD says "sem dep
  externa além do ecossistema K8s padrão" + own-code-with-tests; ginkgo/gomega are extra BDD deps. M23 should use
  the Go stdlib `testing` + controller-runtime `envtest` (envtest IS standard K8s ecosystem,
  `sigs.k8s.io/controller-runtime/pkg/envtest`) to minimize deps. Record this as a blueprint ADR (std testing over
  ginkgo). This is a deliberate divergence from cloudnative-pg, justified by D4.

### EC-4: "gateway" (DoD) = the K8s Service the operator creates, not a separate HTTP gateway
- **Accepted risk:** the M23 DoD lists "gateway"; cloudnative-pg's equivalent is the Service (+ optional pooler).
  For measurement-first M23, the **K8s Service** the operator provisions IS the cluster's connection gateway
  (stable endpoint to the primary). A separate HTTP/connection-pooling gateway is a follow-up. The blueprint must
  state this honestly (gateway = Service in this scope), not over-promise an HTTP gateway.

### EC-5: StatefulSet vs cloudnative-pg's pod-per-instance — honest divergence
- **Accepted risk:** already encoded in ADR D4 + Q3's halt-loop checkpoint. cloudnative-pg manages individual Pods
  (fine-grained control + instance manager); M23 uses a **StatefulSet** (KISS, standard, provisions N replicas
  with stable identity + PVC templates). The blueprint must present the StatefulSet as a TheoDB design choice
  adapting the cloudnative-pg reconcile pattern, NOT misrepresent cloudnative-pg as using a StatefulSet.

## Summary

| Question | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|----------|-------------|----------|
| Q3 | 0 | 0 | 1 (EC-5) |
| Q7 | 1 (EC-1) | 1 (EC-2) | 1 (EC-3) |
| (cross/DoD) | 0 | 0 | 1 (EC-4) |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT (1 MUST FIX — Q7 real-envtest gate; absorbed into plan v1.1 + checkpoints/ADR seeds)
