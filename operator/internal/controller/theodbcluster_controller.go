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

	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/controller/controllerutil"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

// TheoDBClusterReconciler reconciles a TheoDBCluster object into a StatefulSet + Service (plan M23 T2.1).
type TheoDBClusterReconciler struct {
	client.Client
	Scheme *runtime.Scheme
}

// +kubebuilder:rbac:groups=core.theodb.io,resources=theodbclusters,verbs=get;list;watch;create;update;patch;delete
// +kubebuilder:rbac:groups=core.theodb.io,resources=theodbclusters/status,verbs=get;update;patch
// +kubebuilder:rbac:groups=core.theodb.io,resources=theodbclusters/finalizers,verbs=update
// +kubebuilder:rbac:groups=apps,resources=statefulsets,verbs=get;list;watch;create;update;patch;delete
// +kubebuilder:rbac:groups="",resources=services,verbs=get;list;watch;create;update;patch;delete

// Reconcile drives the TheoDBCluster towards its desired state: ensure a StatefulSet + Service, then update
// status. Idempotent (EC-2: a converged second pass makes no change). Fail-fast on a missing image (Rule 8).
func (r *TheoDBClusterReconciler) Reconcile(ctx context.Context, req ctrl.Request) (ctrl.Result, error) {
	var cluster theodbv1.TheoDBCluster
	if err := r.Get(ctx, req.NamespacedName, &cluster); err != nil {
		// NotFound: the CR was deleted; owned resources are GC'd by owner refs. No requeue.
		return ctrl.Result{}, client.IgnoreNotFound(err)
	}

	// Fail-fast: a missing image cannot produce a runnable pod (typed error + Error phase).
	if cluster.Spec.Image == "" {
		_ = r.setPhase(ctx, &cluster, "Error", metav1.ConditionFalse, "MissingImage", "spec.image is required")
		return ctrl.Result{}, fmt.Errorf("theodbcluster %s/%s: spec.image is required", cluster.Namespace, cluster.Name)
	}

	if err := r.ensureStatefulSet(ctx, &cluster); err != nil {
		return ctrl.Result{}, err
	}
	if err := r.ensureService(ctx, &cluster); err != nil {
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
func (r *TheoDBClusterReconciler) ensureStatefulSet(ctx context.Context, c *theodbv1.TheoDBCluster) error {
	desired := buildStatefulSet(c)
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
	// Mutable-fields-only update (EC-1): replicas + container image. Skip if already converged (EC-2 — no churn).
	changed := false
	if existing.Spec.Replicas == nil || *existing.Spec.Replicas != c.Spec.Instances {
		existing.Spec.Replicas = &c.Spec.Instances
		changed = true
	}
	if len(existing.Spec.Template.Spec.Containers) > 0 && existing.Spec.Template.Spec.Containers[0].Image != c.Spec.Image {
		existing.Spec.Template.Spec.Containers[0].Image = c.Spec.Image
		changed = true
	}
	if changed {
		return r.Update(ctx, &existing)
	}
	return nil
}

// ensureService creates the gateway Service, or leaves it (its spec is stable for a fixed Port).
func (r *TheoDBClusterReconciler) ensureService(ctx context.Context, c *theodbv1.TheoDBCluster) error {
	desired := buildService(c)
	if err := controllerutil.SetControllerReference(c, desired, r.Scheme); err != nil {
		return err
	}
	var existing corev1.Service
	err := r.Get(ctx, client.ObjectKeyFromObject(desired), &existing)
	if apierrors.IsNotFound(err) {
		return r.Create(ctx, desired)
	}
	return err
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
	if c.Status.Phase == phase && c.Status.ReadyInstances == ready {
		return nil // no churn (EC-2)
	}
	c.Status.ReadyInstances = ready
	return r.setPhase(ctx, c, phase, condStatus, "Reconciled", fmt.Sprintf("%d/%d instances ready", ready, c.Spec.Instances))
}

// setPhase writes the status subresource (Phase + the Ready condition).
func (r *TheoDBClusterReconciler) setPhase(ctx context.Context, c *theodbv1.TheoDBCluster, phase string, condStatus metav1.ConditionStatus, reason, msg string) error {
	c.Status.Phase = phase
	meta := metav1.Condition{Type: "Ready", Status: condStatus, Reason: reason, Message: msg, LastTransitionTime: metav1.Now()}
	upsertCondition(&c.Status.Conditions, meta)
	return r.Status().Update(ctx, c)
}

// upsertCondition sets/updates a condition by type (preserving LastTransitionTime when the status is unchanged).
func upsertCondition(conds *[]metav1.Condition, cond metav1.Condition) {
	for i := range *conds {
		if (*conds)[i].Type == cond.Type {
			if (*conds)[i].Status == cond.Status {
				cond.LastTransitionTime = (*conds)[i].LastTransitionTime
			}
			(*conds)[i] = cond
			return
		}
	}
	*conds = append(*conds, cond)
}

// SetupWithManager wires the reconciler: watch TheoDBCluster, own the StatefulSet + Service it provisions.
func (r *TheoDBClusterReconciler) SetupWithManager(mgr ctrl.Manager) error {
	return ctrl.NewControllerManagedBy(mgr).
		For(&theodbv1.TheoDBCluster{}).
		Owns(&appsv1.StatefulSet{}).
		Owns(&corev1.Service{}).
		Named("theodbcluster").
		Complete(r)
}
