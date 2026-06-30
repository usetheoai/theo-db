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

	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/types"
	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

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

	reconcileOnce(t, name, "default")

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
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &svc); err != nil {
		t.Fatalf("service not created: %v", err)
	}
	if len(svc.OwnerReferences) != 1 {
		t.Errorf("service owner ref missing: %+v", svc.OwnerReferences)
	}
}

// T2.2 RED — EC-2: a second reconcile on a converged cluster is a no-op (idempotent, no resourceVersion churn).
func TestReconcile_Idempotent(t *testing.T) {
	name := "tc-idem"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 1, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})

	reconcileOnce(t, name, "default")
	var first appsv1.StatefulSet
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &first); err != nil {
		t.Fatal(err)
	}

	reconcileOnce(t, name, "default")
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
	reconcileOnce(t, name, "default")

	// Scale to 3 instances.
	if err := k8sClient.Get(context.Background(), client.ObjectKeyFromObject(c), c); err != nil {
		t.Fatal(err)
	}
	c.Spec.Instances = 3
	if err := k8sClient.Update(context.Background(), c); err != nil {
		t.Fatalf("update spec: %v", err)
	}
	reconcileOnce(t, name, "default")

	var ss appsv1.StatefulSet
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &ss); err != nil {
		t.Fatal(err)
	}
	if ss.Spec.Replicas == nil || *ss.Spec.Replicas != 3 {
		t.Errorf("replicas after scale: got %v, want 3", ss.Spec.Replicas)
	}
}

// T2.2 RED — fail-fast (Rule 8): a cluster with no image yields a typed error + Error phase, no StatefulSet.
func TestReconcile_MissingImageFailsFast(t *testing.T) {
	name := "tc-noimage"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 1, StorageSize: "1Gi", Port: 5432})

	r := newReconciler()
	_, err := r.Reconcile(context.Background(), ctrlReq(name, "default"))
	if err == nil {
		t.Fatal("expected a typed error for missing image, got nil")
	}

	var ss appsv1.StatefulSet
	getErr := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &ss)
	if !apierrors.IsNotFound(getErr) {
		t.Errorf("statefulset should NOT exist for an image-less cluster, got err=%v", getErr)
	}

	var c theodbv1.TheoDBCluster
	if err := k8sClient.Get(context.Background(), types.NamespacedName{Name: name, Namespace: "default"}, &c); err != nil {
		t.Fatal(err)
	}
	if c.Status.Phase != "Error" {
		t.Errorf("phase: got %q, want Error", c.Status.Phase)
	}
}

// T2.2 RED — status: with no kubelet under envtest, ReadyReplicas stays 0 → phase Initializing (EC-4, honest).
func TestReconcile_StatusInitializingWithoutKubelet(t *testing.T) {
	name := "tc-status"
	createCluster(t, name, theodbv1.TheoDBClusterSpec{Instances: 2, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432})
	reconcileOnce(t, name, "default")

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
	ready := meta_FindCondition(c.Status.Conditions, "Ready")
	if ready == nil || ready.Status != metav1.ConditionFalse {
		t.Errorf("Ready condition: got %+v, want status False", ready)
	}
}

// meta_FindCondition is a tiny local helper (avoids pulling apimachinery/api/meta just for tests).
func meta_FindCondition(conds []metav1.Condition, condType string) *metav1.Condition {
	for i := range conds {
		if conds[i].Type == condType {
			return &conds[i]
		}
	}
	return nil
}
