/*
Copyright 2026.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package controller

import (
	"context"
	"testing"

	"github.com/prometheus/client_golang/prometheus/testutil"
	dbmetrics "github.com/usetheodev/theo-db/operator/internal/metrics"
	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	apimeta "k8s.io/apimachinery/pkg/api/meta"
	"k8s.io/apimachinery/pkg/api/resource"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/types"
	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

func nn(name string) types.NamespacedName {
	return types.NamespacedName{Name: name, Namespace: "default"}
}

func ctrlReq(name, namespace string) ctrl.Request {
	return ctrl.Request{NamespacedName: types.NamespacedName{Name: name, Namespace: namespace}}
}

// createCluster persists a TheoDBCluster CR in envtest and registers cleanup.
func createCluster(t *testing.T, name string, spec theodbv1.TheoDBClusterSpec) *theodbv1.TheoDBCluster {
	t.Helper()
	c := &theodbv1.TheoDBCluster{
		ObjectMeta: metav1.ObjectMeta{Name: name, Namespace: "default"},
		Spec:       spec,
	}
	if err := k8sClient.Create(context.Background(), c); err != nil {
		t.Fatalf("create cluster %s: %v", name, err)
	}
	t.Cleanup(func() {
		_ = k8sClient.Delete(context.Background(), c)
	})
	return c
}

// T2.2 RED: reconcile creates a StatefulSet + Service owned by the CR (the core evidence gate against real envtest).
func TestReconcile_CreatesStatefulSetAndService(t *testing.T) {
	name := "tc-create"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 2, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})

	reconcileOnce(t, name)

	var ss appsv1.StatefulSet
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &ss); err != nil {
		t.Fatalf("statefulset not created: %v", err)
	}
	if ss.Spec.Replicas == nil || *ss.Spec.Replicas != 2 {
		t.Errorf("replicas: got %v, want 2", ss.Spec.Replicas)
	}
	if len(ss.OwnerReferences) != 1 || ss.OwnerReferences[0].Name != name {
		t.Errorf("owner ref: got %+v, want owner %s", ss.OwnerReferences, name)
	}

	var svc corev1.Service
	if err := k8sClient.Get(context.Background(), nn(name), &svc); err != nil {
		t.Fatalf("gateway service not created: %v", err)
	}
	if len(svc.OwnerReferences) != 1 {
		t.Errorf("gateway service owner ref missing: %+v", svc.OwnerReferences)
	}

	// H1 regression: the governing headless Service (ClusterIP=None) must exist for stable pod DNS.
	var hl corev1.Service
	if err := k8sClient.Get(context.Background(), nn(name+"-hl"), &hl); err != nil {
		t.Fatalf("headless service %s-hl not created: %v", name, err)
	}
	if hl.Spec.ClusterIP != "None" {
		t.Errorf("headless clusterIP: got %q, want None", hl.Spec.ClusterIP)
	}
	if len(hl.OwnerReferences) != 1 {
		t.Errorf("headless service owner ref missing: %+v", hl.OwnerReferences)
	}
	if ss.Spec.ServiceName != name+"-hl" {
		t.Errorf("statefulset governing serviceName: got %q, want %s-hl", ss.Spec.ServiceName, name)
	}
}

// T2.2 RED — EC-2: a second reconcile on a converged cluster is a no-op (idempotent, no resourceVersion churn).
func TestReconcile_Idempotent(t *testing.T) {
	name := "tc-idem"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 1, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})

	reconcileOnce(t, name)
	var first appsv1.StatefulSet
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &first); err != nil {
		t.Fatal(err)
	}

	reconcileOnce(t, name)
	var second appsv1.StatefulSet
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &second); err != nil {
		t.Fatal(err)
	}
	if first.ResourceVersion != second.ResourceVersion {
		t.Errorf("idempotency broken: statefulset resourceVersion changed %s → %s on no-op reconcile",
			first.ResourceVersion, second.ResourceVersion)
	}
}

// T2.2 RED — EC-1: scaling up patches ONLY the mutable field (replicas), never re-applying the immutable spec.
func TestReconcile_ScaleUpUpdatesReplicas(t *testing.T) {
	name := "tc-scale"
	c := createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 1, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})
	reconcileOnce(t, name)

	// Scale to 3 instances.
	if err := k8sClient.Get(context.Background(), client.ObjectKeyFromObject(c), c); err != nil {
		t.Fatal(err)
	}
	c.Spec.Instances = 3
	if err := k8sClient.Update(context.Background(), c); err != nil {
		t.Fatalf("update spec: %v", err)
	}
	reconcileOnce(t, name)

	var ss appsv1.StatefulSet
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &ss); err != nil {
		t.Fatal(err)
	}
	if ss.Spec.Replicas == nil || *ss.Spec.Replicas != 3 {
		t.Errorf("replicas after scale: got %v, want 3", ss.Spec.Replicas)
	}
}

// T2.2 RED — EC-1 (immutable fields): a StatefulSet's VolumeClaimTemplates cannot change in place, so the CRD
// marks storageSize immutable. The API server rejects an edit at the boundary (rather than the operator
// silently dropping it), and the persisted StatefulSet VCT therefore never diverges from intent.
func TestCRD_RejectsStorageSizeChange(t *testing.T) {
	name := "tc-storage"
	c := createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 1, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})
	reconcileOnce(t, name)

	if err := k8sClient.Get(context.Background(), client.ObjectKeyFromObject(c), c); err != nil {
		t.Fatal(err)
	}
	c.Spec.StorageSize = "5Gi"
	err := k8sClient.Update(context.Background(), c)
	if err == nil {
		t.Fatal("expected the apiserver to reject the immutable storageSize change, got nil")
	}
	if !apierrors.IsInvalid(err) {
		t.Errorf("want Invalid error for storageSize change, got %v", err)
	}

	// The persisted StatefulSet VCT stays at the original 1Gi (no drift).
	var ss appsv1.StatefulSet
	if err := k8sClient.Get(context.Background(), nn(name), &ss); err != nil {
		t.Fatal(err)
	}
	got := ss.Spec.VolumeClaimTemplates[0].Spec.Resources.Requests["storage"]
	want := resource.MustParse("1Gi")
	if got.Cmp(want) != 0 {
		t.Errorf("VCT storage: got %v, want 1Gi unchanged", got)
	}
}

// T2.2 RED — failure scenario: a reconcile request for a deleted CR is a no-op (IgnoreNotFound), never an error.
func TestReconcile_CRDeleted_NoOp(t *testing.T) {
	res, err := newReconciler().Reconcile(context.Background(), ctrlReq("does-not-exist", "default"))
	if err != nil {
		t.Errorf("reconcile of a missing CR must return nil, got %v", err)
	}
	if res.RequeueAfter != 0 {
		t.Errorf("missing CR must not requeue, got %+v", res)
	}
}

// T2.2 RED — boundary validation (fail-fast at the API, error-handling.md): the CRD rejects an empty image,
// a malformed storageSize, and an out-of-range port BEFORE the object is ever stored.
func TestCRD_RejectsInvalidSpec(t *testing.T) {
	cases := []struct {
		name string
		spec theodbv1.TheoDBClusterSpec
	}{
		{"empty-image", theodbv1.TheoDBClusterSpec{Instances: 1, Image: "", StorageSize: "1Gi", Port: 5432}},
		{"bad-storage", theodbv1.TheoDBClusterSpec{Instances: 1, Image: "theo-db:test", StorageSize: "garbage", Port: 5432}},
		{"port-too-high", theodbv1.TheoDBClusterSpec{Instances: 1, Image: "theo-db:test", StorageSize: "1Gi", Port: 99999}},
		{"instances-zero", theodbv1.TheoDBClusterSpec{Instances: 0, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			obj := &theodbv1.TheoDBCluster{ObjectMeta: metav1.ObjectMeta{Name: "rej-" + tc.name, Namespace: "default"}, Spec: tc.spec}
			err := k8sClient.Create(context.Background(), obj)
			if err == nil {
				_ = k8sClient.Delete(context.Background(), obj)
				t.Fatalf("apiserver accepted invalid spec %s — CRD validation missing", tc.name)
			}
			if !apierrors.IsInvalid(err) {
				t.Errorf("want Invalid error for %s, got %v", tc.name, err)
			}
		})
	}
}

// T2.2 RED — status: with no kubelet under envtest, ReadyReplicas stays 0 → phase Initializing (EC-4, honest).
func TestReconcile_StatusInitializingWithoutKubelet(t *testing.T) {
	name := "tc-status"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 2, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})
	reconcileOnce(t, name)

	var c theodbv1.TheoDBCluster
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &c); err != nil {
		t.Fatal(err)
	}
	if c.Status.Phase != "Initializing" {
		t.Errorf("phase: got %q, want Initializing", c.Status.Phase)
	}
	if c.Status.ReadyInstances != 0 {
		t.Errorf("readyInstances: got %d, want 0", c.Status.ReadyInstances)
	}
	ready := apimeta.FindStatusCondition(c.Status.Conditions, "Ready")
	if ready == nil || ready.Status != metav1.ConditionFalse {
		t.Errorf("Ready condition: got %+v, want status False", ready)
	}
	if c.Status.ObservedGeneration != c.Generation {
		t.Errorf("observedGeneration: got %d, want %d", c.Status.ObservedGeneration, c.Generation)
	}
}

// T1.1 RED: a reconcile emits the domain gauges (ready=0 without kubelet, desired=spec) + a success counter.
func TestReconcile_EmitsDomainMetrics(t *testing.T) {
	dbmetrics.ReconcileTotal.Reset()
	dbmetrics.ClusterReadyInstances.Reset()
	dbmetrics.ClusterDesiredInstances.Reset()
	name := "tc-metrics"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 2, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})

	reconcileOnce(t, name)

	if got := testutil.ToFloat64(dbmetrics.ClusterDesiredInstances.WithLabelValues("default", name)); got != 2 {
		t.Errorf("desired_instances gauge: got %v, want 2", got)
	}
	if got := testutil.ToFloat64(dbmetrics.ClusterReadyInstances.WithLabelValues("default", name)); got != 0 {
		t.Errorf("ready_instances gauge: got %v, want 0 (no kubelet)", got)
	}
	if got := testutil.ToFloat64(dbmetrics.ReconcileTotal.WithLabelValues(dbmetrics.ResultSuccess)); got < 1 {
		t.Errorf("reconcile_total{success}: got %v, want >= 1", got)
	}
}

// T2.1 RED: a reconcile provisions the read Service <name>-ro, owner-referenced.
func TestReconcile_CreatesReadService(t *testing.T) {
	name := "tc-read"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 2, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})
	reconcileOnce(t, name)

	var ro corev1.Service
	if err := k8sClient.Get(context.Background(), nn(name+"-ro"), &ro); err != nil {
		t.Fatalf("read service %s-ro not created: %v", name, err)
	}
	if len(ro.OwnerReferences) != 1 {
		t.Errorf("read service owner ref missing: %+v", ro.OwnerReferences)
	}
	if ro.Spec.Ports[0].Port != 5432 {
		t.Errorf("read service port: got %d, want 5432", ro.Spec.Ports[0].Port)
	}
}
