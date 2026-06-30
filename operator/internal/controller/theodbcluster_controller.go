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
	"fmt"
	"time"

	dbmetrics "github.com/usetheodev/theo-db/operator/internal/metrics"
	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	apimeta "k8s.io/apimachinery/pkg/api/meta"
	"k8s.io/apimachinery/pkg/api/resource"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/controller/controllerutil"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

// TheoDBClusterReconciler reconciles a TheoDBCluster object into a StatefulSet + Services (plan M23 T2.1).
type TheoDBClusterReconciler struct {
	client.Client
	Scheme *runtime.Scheme
}

// +kubebuilder:rbac:groups=core.theodb.io,resources=theodbclusters,verbs=get;list;watch
// +kubebuilder:rbac:groups=core.theodb.io,resources=theodbclusters/status,verbs=get;update;patch
// +kubebuilder:rbac:groups=core.theodb.io,resources=theodbclusters/finalizers,verbs=update
// +kubebuilder:rbac:groups=apps,resources=statefulsets,verbs=get;list;watch;create;update;patch;delete
// +kubebuilder:rbac:groups="",resources=services,verbs=get;list;watch;create;update;patch;delete

// Reconcile drives the TheoDBCluster towards its desired state: ensure a headless Service (pod identity) + a
// gateway Service + a StatefulSet, then update status. Idempotent (EC-2: a converged second pass makes no
// change). Spec is validated at the API boundary (CRD); storageSize is re-parsed here as defense-in-depth.
func (r *TheoDBClusterReconciler) Reconcile(ctx context.Context, req ctrl.Request) (res ctrl.Result, err error) {
	start := time.Now()
	defer func() {
		dbmetrics.ReconcileDuration.Observe(time.Since(start).Seconds())
		result := dbmetrics.ResultSuccess
		if err != nil {
			result = dbmetrics.ResultError
		}
		dbmetrics.ReconcileTotal.WithLabelValues(result).Inc()
	}()

	var cluster theodbv1.TheoDBCluster
	if err := r.Get(ctx, req.NamespacedName, &cluster); err != nil {
		// NotFound: the CR was deleted; owned resources are GC'd by owner refs. No requeue.
		return ctrl.Result{}, client.IgnoreNotFound(err)
	}

	// storageSize is guarded by a CRD pattern at the boundary; re-parse here so a value that bypassed
	// admission cannot panic the builder. A permanent spec error sets Error phase and does NOT requeue
	// (returning nil) — the spec-change watch re-triggers reconcile when the user fixes it (no hot-loop).
	storage, err := resource.ParseQuantity(cluster.Spec.StorageSize)
	if err != nil {
		msg := fmt.Sprintf("invalid storageSize %q: %v", cluster.Spec.StorageSize, err)
		return ctrl.Result{}, r.setPhase(ctx, &cluster, "Error", metav1.ConditionFalse, "InvalidSpec", msg)
	}

	if err := r.ensureHeadlessService(ctx, &cluster); err != nil {
		return ctrl.Result{}, err
	}
	if err := r.ensureService(ctx, &cluster); err != nil {
		return ctrl.Result{}, err
	}
	if err := r.ensureStatefulSet(ctx, &cluster, storage); err != nil {
		return ctrl.Result{}, err
	}
	if err := r.updateStatus(ctx, &cluster); err != nil {
		if apierrors.IsConflict(err) {
			return ctrl.Result{Requeue: true}, nil
		}
		return ctrl.Result{}, err
	}

	// Requeue until every instance is ready (under envtest there is no kubelet, so this stays Initializing — EC-4).
	if cluster.Status.Phase != "Healthy" {
		return ctrl.Result{RequeueAfter: 10 * time.Second}, nil
	}
	return ctrl.Result{}, nil
}

// ensureStatefulSet creates the StatefulSet, or updates ONLY its mutable fields (replicas + image). K8s rejects
// updates to Selector/ServiceName/VolumeClaimTemplates, so we never re-apply the whole built spec (EC-1).
func (r *TheoDBClusterReconciler) ensureStatefulSet(ctx context.Context, c *theodbv1.TheoDBCluster, storage resource.Quantity) error {
	desired := buildStatefulSet(c, storage)
	if err := controllerutil.SetControllerReference(c, desired, r.Scheme); err != nil {
		return err
	}
	var existing appsv1.StatefulSet
	err := r.Get(ctx, client.ObjectKeyFromObject(desired), &existing)
	if apierrors.IsNotFound(err) {
		return r.Create(ctx, desired)
	}
	if err != nil {
		return err
	}
	// Mutable-fields-only update (EC-1): replicas + container image, plus adopt the owner ref if a legacy
	// object lacks it. Skip the write entirely if already converged (EC-2 — no churn).
	changed := false
	if existing.Spec.Replicas == nil || *existing.Spec.Replicas != c.Spec.Instances {
		existing.Spec.Replicas = &c.Spec.Instances
		changed = true
	}
	if len(existing.Spec.Template.Spec.Containers) > 0 && existing.Spec.Template.Spec.Containers[0].Image != c.Spec.Image {
		existing.Spec.Template.Spec.Containers[0].Image = c.Spec.Image
		changed = true
	}
	if !metav1.IsControlledBy(&existing, c) {
		if err := controllerutil.SetControllerReference(c, &existing, r.Scheme); err != nil {
			return err
		}
		changed = true
	}
	if changed {
		return r.Update(ctx, &existing)
	}
	return nil
}

// ensureHeadlessService ensures the StatefulSet's governing headless Service (pod DNS identity).
func (r *TheoDBClusterReconciler) ensureHeadlessService(ctx context.Context, c *theodbv1.TheoDBCluster) error {
	return r.ensureServiceObject(ctx, c, buildHeadlessService(c))
}

// ensureService ensures the gateway (ClusterIP) Service.
func (r *TheoDBClusterReconciler) ensureService(ctx context.Context, c *theodbv1.TheoDBCluster) error {
	return r.ensureServiceObject(ctx, c, buildService(c))
}

// ensureServiceObject creates the desired Service, or reconciles the mutable port/selector on the existing one
// (so a spec.Port change converges) and adopts a missing owner ref. Idempotent — no write when converged.
func (r *TheoDBClusterReconciler) ensureServiceObject(ctx context.Context, c *theodbv1.TheoDBCluster, desired *corev1.Service) error {
	if err := controllerutil.SetControllerReference(c, desired, r.Scheme); err != nil {
		return err
	}
	var existing corev1.Service
	err := r.Get(ctx, client.ObjectKeyFromObject(desired), &existing)
	if apierrors.IsNotFound(err) {
		return r.Create(ctx, desired)
	}
	if err != nil {
		return err
	}
	changed := false
	if !servicePortsEqual(existing.Spec.Ports, desired.Spec.Ports) {
		existing.Spec.Ports = desired.Spec.Ports
		changed = true
	}
	if !metav1.IsControlledBy(&existing, c) {
		if err := controllerutil.SetControllerReference(c, &existing, r.Scheme); err != nil {
			return err
		}
		changed = true
	}
	if changed {
		return r.Update(ctx, &existing)
	}
	return nil
}

// servicePortsEqual compares the name/port/targetPort of two port lists (the fields the operator owns).
func servicePortsEqual(a, b []corev1.ServicePort) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i].Name != b[i].Name || a[i].Port != b[i].Port || a[i].TargetPort != b[i].TargetPort {
			return false
		}
	}
	return true
}

// updateStatus reflects the StatefulSet's ReadyReplicas into the cluster status (Phase + Ready condition).
func (r *TheoDBClusterReconciler) updateStatus(ctx context.Context, c *theodbv1.TheoDBCluster) error {
	var ss appsv1.StatefulSet
	if err := r.Get(ctx, client.ObjectKey{Namespace: c.Namespace, Name: c.Name}, &ss); err != nil {
		return err
	}
	ready := ss.Status.ReadyReplicas
	phase := "Initializing"
	condStatus := metav1.ConditionFalse
	if ready == c.Spec.Instances {
		phase = "Healthy"
		condStatus = metav1.ConditionTrue
	}

	// Domain metrics (T1.1): per-cluster gauges always reflect the latest observation, even on a no-churn pass.
	dbmetrics.ClusterReadyInstances.WithLabelValues(c.Namespace, c.Name).Set(float64(ready))
	dbmetrics.ClusterDesiredInstances.WithLabelValues(c.Namespace, c.Name).Set(float64(c.Spec.Instances))
	dbmetrics.RecordPhase(c.Namespace, c.Name, phase)

	if c.Status.Phase == phase && c.Status.ReadyInstances == ready && c.Status.ObservedGeneration == c.Generation {
		return nil // no churn (EC-2)
	}
	c.Status.ReadyInstances = ready
	c.Status.ObservedGeneration = c.Generation
	return r.setPhase(ctx, c, phase, condStatus, "Reconciled", fmt.Sprintf("%d/%d instances ready", ready, c.Spec.Instances))
}

// setPhase writes the status subresource (Phase + the Ready condition, via apimachinery's canonical helper).
func (r *TheoDBClusterReconciler) setPhase(ctx context.Context, c *theodbv1.TheoDBCluster, phase string, condStatus metav1.ConditionStatus, reason, msg string) error {
	c.Status.Phase = phase
	apimeta.SetStatusCondition(&c.Status.Conditions, metav1.Condition{
		Type:               "Ready",
		Status:             condStatus,
		Reason:             reason,
		Message:            msg,
		ObservedGeneration: c.Generation,
	})
	return r.Status().Update(ctx, c)
}

// SetupWithManager wires the reconciler: watch TheoDBCluster, own the StatefulSet + Services it provisions.
func (r *TheoDBClusterReconciler) SetupWithManager(mgr ctrl.Manager) error {
	return ctrl.NewControllerManagedBy(mgr).
		For(&theodbv1.TheoDBCluster{}).
		Owns(&appsv1.StatefulSet{}).
		Owns(&corev1.Service{}).
		Named("theodbcluster").
		Complete(r)
}
