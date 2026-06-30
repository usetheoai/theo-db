# Edge Case Review — m23-control-plane-go (implementation plan)

Date: 2026-06-30
Tasks analyzed: 4 (T1.1, T2.1, T3.1, T3.2)
Cases found: 5 (EDGE: 3, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 2, DOCUMENT: 2)

The plan is strong (kubebuilder standard, real-envtest gate, standard deps, Failure-scenarios table: missing image,
CR NotFound, envtest-binary missing, status conflict, malformed kustomize). Below are the unforeseen K8s gotchas.

## MUST FIX

### EC-1: StatefulSet spec has IMMUTABLE fields — "update on drift" will be REJECTED by the API
- **Affected task:** T2.1 (`Reconcile` update path)
- **Kind:** NEGATIVE (API rejection)
- **Family:** State
- **Scenario:** Kubernetes rejects updates to a StatefulSet's `Selector`, `ServiceName`, and
  `VolumeClaimTemplates` (and most of `Template` beyond image/labels/resources) — only `Replicas`, `Template`
  (image/labels/probes/resources), and `UpdateStrategy` are mutable. If T2.1's `Reconcile` blindly `Update`s the
  whole built StatefulSet when `StorageSize` or the selector "drifts", the API returns a 422 — the reconciler
  errors forever (hot-loop) instead of converging.
- **Impact:** a `StorageSize` change (or any immutable-field diff) breaks reconciliation permanently.
- **Suggested fix:** T2.1's update path mutates only the **mutable** fields (`Spec.Replicas`, the container
  `Image`) on the EXISTING StatefulSet (fetch → patch replicas+image → Update); changing `StorageSize`/selector is
  **not supported in-place** (documented; would require a recreate, out of M23 scope). Add this to T2.1 Deep Dives
  + a unit test that a StorageSize change does NOT attempt a full-spec Update.

## SHOULD TEST

### EC-2: idempotency must mean NO spurious Update (resourceVersion must not churn)
- **Affected task:** T3.1 (idempotency gate)
- **Kind:** EDGE (convergence)
- **Suggested test:** `TestReconcile_NoSpuriousUpdate` — capture the StatefulSet `ResourceVersion` after the first
  reconcile; run a second reconcile; assert the StatefulSet `ResourceVersion` is UNCHANGED (the drift check is a
  true no-op, not an Update that bumps the version every loop → controller hot-loop). Strengthens the plan's
  "exactly one SS+Svc" idempotency assertion (which doesn't catch version churn).

### EC-3: CLI `apply -f` with a wrong-kind / malformed YAML
- **Affected task:** T3.2 (theodbctl apply)
- **Kind:** NEGATIVE (invalid input)
- **Suggested test:** `TestTheodbctl_ApplyWrongKind_Errors` — `apply -f` a YAML of a non-TheoDBCluster kind (or
  malformed YAML) → typed error ("not a TheoDBCluster" / decode error), no client call. Asserts the decode path
  validates the kind before talking to the API.

## DOCUMENT

### EC-4: envtest has NO kubelet → `ReadyReplicas` is always 0 → Phase never reaches "Healthy" in the gate
- **Kind:** EDGE (test-environment limitation)
- **Accepted risk (critical for writing the gate correctly):** `envtest` runs only kube-apiserver + etcd — there is
  NO kubelet, so created Pods never start and `StatefulSet.Status.ReadyReplicas` stays 0. Therefore the
  reconciler's `Phase = Healthy iff ReadyInstances==Instances` will resolve to **"Initializing"** in envtest, never
  "Healthy". The T3.1 gate MUST assert `Status.Phase` is SET (e.g. == "Initializing"), NOT "Healthy" — asserting
  Healthy would make the gate un-passable on envtest. Document in T3.1 + the gate's comment. (Healthy is only
  observable on a real cluster with a kubelet — an optional kind smoke, not the gate.)

### EC-5: Instances boundary (0 / very large)
- **Kind:** EDGE
- **Accepted risk:** `Spec.Instances` carries `+kubebuilder:validation:Minimum=1` (T1.1) → the apiserver rejects 0
  at admission (envtest enforces CRD schema validation), so the reconciler never sees Instances<1. A very large
  Instances is a cluster-capacity concern, not a reconciler bug (the StatefulSet just requests N replicas).
  Document the Minimum=1 guard; no extra reconciler code needed.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T2.1 | 1 (EC-5) | 1 (EC-1) | 1 (EC-1) | 0 | 1 (EC-5) |
| T3.1 | 2 (EC-2,EC-4) | 0 | 0 | 1 (EC-2) | 1 (EC-4) |
| T3.2 | 0 | 1 (EC-3) | 0 | 1 (EC-3) | 0 |

**Verdict:** PLAN NEEDS ADJUSTMENT (1 MUST FIX — EC-1 StatefulSet immutable fields; absorbed into plan v1.1 + SHOULD TEST/DOCUMENT)
