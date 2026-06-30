# Review — M23 Control plane in Go (K8s operator)

**Date:** 2026-06-30 · **Slug:** m23-control-plane-go · **Branch:** develop
**Verdict:** READY_TO_MERGE

## Process

6 specialist agents reviewed the first cut (commits `9b3a405`, `7237d07`); 5/6 returned
NEEDS_FIXES with a converging set of real correctness/security/test/CI findings. The batch was fixed
(`7661e6f`, `a33edfd`) and re-reviewed by 5 agents — **all 5 returned READY_TO_MERGE**.

## Findings consolidated (severity matrix)

| # | Severity | Finding | Resolution | Status |
|---|---|---|---|---|
| F1 | HIGH | StatefulSet referenced a governing headless Service `<name>-hl` that was never created → broken per-pod DNS for N>1 | Added `buildHeadlessService` (ClusterIP=None) + `ensureHeadlessService` + Owns; envtest asserts it | FIXED |
| F2 | HIGH | `resource.MustParse(storageSize)` panics on malformed CR input → DoS hot-loop | CRD `pattern` boundary validation + `ParseQuantity` defense-in-depth; builder takes a pre-parsed Quantity (panic-free) | FIXED |
| F3 | HIGH | Promised EC-1 immutable-VCT test missing (scale test only covered replicas) | `storageSize` made immutable via CEL `x-kubernetes-validations`; `TestCRD_RejectsStorageSizeChange` asserts boundary rejection | FIXED |
| F4 | HIGH | CI `test-e2e` target + workflow referenced removed `test/e2e/` → red CI | Removed the ginkgo e2e target + `test-e2e.yml` workflow (ADR D2 — std testing) | FIXED |
| F5 | HIGH | `theodbctl` CLI had no build/ship path | Added `make build-cli` target | FIXED |
| F6 | MEDIUM | `ensureService` never reconciled port on existing Service → drift | `ensureServiceObject` diffs ports + adopts owner ref, idempotent | FIXED |
| F7 | MEDIUM | Owner ref not ensured on StatefulSet adopt/update path | `IsControlledBy` check + `SetControllerReference` on existing | FIXED |
| F8 | MEDIUM | Status lacked `observedGeneration` | Added to status + condition | FIXED |
| F9 | MEDIUM | `upsertCondition` reinvented apimachinery (Rule 9) | Replaced with `apimeta.SetStatusCondition` / `FindStatusCondition` | FIXED |
| F10 | MEDIUM | Missing-image returned error → backoff hot-loop | `image` made CRD-required (MinLength=1) — rejected at boundary | FIXED |
| F11 | MEDIUM | CR-deleted + boundary negative cases untested | `TestReconcile_CRDeleted_NoOp` + `TestCRD_RejectsInvalidSpec` (4 sub) | FIXED |
| F12 | MEDIUM | `storageSize` edit silently dropped (no drift signal) | CEL immutability rule rejects the edit at admission | FIXED |
| F13 | LOW | `corev1` alias for own API in `cmd/main.go` | Renamed to `theodbv1` | FIXED |
| F14 | LOW | Docs claimed "16 resources" (actual 15); no reconcile-timing | Corrected to 15; honest latency note | FIXED |
| F15 | LOW | `spec.Port` change updates Services but not the (cosmetic) container port | Documented M24 follow-up | DEFERRED (M24) |

## Gate evidence (post-fix, 2026-06-30)

- `make test` exit 0 — 14 tests PASS (6 real-envtest reconcile gates + 4 CRD-rejection sub-cases), controller pkg 69.4% coverage, `-race` green
- `golangci-lint run ./...` → 0 issues · `deadcode ./...` → none · `gofmt -l .` → clean · `go vet ./...` → clean
- Real-kind: CRD installed, 15-resource bundle server-side dry-run validated, `theodbctl apply/get/delete` end-to-end
- License (D1): controller-runtime + k8s.io/* + cobra + sigs.k8s.io/yaml — all Apache-2.0/BSD, no AGPL, no ginkgo

## Verdict rationale

No BLOCKER, no open HIGH, no open MEDIUM (F1–F12 fixed; F15 is a cosmetic LOW deferred to M24 with
honest documentation). Per `cycle-review.md` (READY_TO_MERGE = no BLOCKER, ≤2 HIGH with mitigation),
the milestone is **READY_TO_MERGE**.

## Deferred (M24 follow-ups, honestly documented — no tracker configured yet)

- F15: converge the StatefulSet container port on a `spec.Port` change (cosmetic; Service TargetPort already routes correctly).
- HA-failover orchestration + HTTP/pooler gateway (explicit M23 out-of-scope per ADR D3/D4).
