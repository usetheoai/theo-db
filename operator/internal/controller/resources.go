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
	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/api/resource"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/util/intstr"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

const (
	containerName = "theodb"
	dataVolume    = "data"
	dataPath      = "/var/lib/postgresql/data"
)

// labelsFor returns the selector/template labels for a cluster's workload (plan M23 T2.1).
func labelsFor(c *theodbv1.TheoDBCluster) map[string]string {
	return map[string]string{
		"app.kubernetes.io/name":     "theodb",
		"app.kubernetes.io/instance": c.Name,
	}
}

// headlessServiceName is the StatefulSet's governing (headless) Service name.
func headlessServiceName(c *theodbv1.TheoDBCluster) string { return c.Name + "-hl" }

// buildStatefulSet is a PURE builder (envtest-free unit-testable): N replicas of the theo-db image with a
// per-replica PVC of `storage`, exposing Port. Adapts the cloudnative-pg pattern to a standard StatefulSet
// (ADR D1 — KISS; cnpg uses pods-per-instance, M23 uses a StatefulSet). `storage` is parsed by the caller
// (Reconcile validates spec.StorageSize at the boundary), so this builder is panic-free on any CR input.
func buildStatefulSet(c *theodbv1.TheoDBCluster, storage resource.Quantity) *appsv1.StatefulSet {
	labels := labelsFor(c)
	replicas := c.Spec.Instances
	return &appsv1.StatefulSet{
		ObjectMeta: metav1.ObjectMeta{Name: c.Name, Namespace: c.Namespace, Labels: labels},
		Spec: appsv1.StatefulSetSpec{
			Replicas:    &replicas,
			ServiceName: headlessServiceName(c),
			Selector:    &metav1.LabelSelector{MatchLabels: labels},
			Template: corev1.PodTemplateSpec{
				ObjectMeta: metav1.ObjectMeta{Labels: labels},
				Spec: corev1.PodSpec{
					Containers: []corev1.Container{{
						Name:  containerName,
						Image: c.Spec.Image,
						Ports: []corev1.ContainerPort{{Name: "postgres", ContainerPort: c.Spec.Port}},
						VolumeMounts: []corev1.VolumeMount{{
							Name:      dataVolume,
							MountPath: dataPath,
						}},
					}},
				},
			},
			VolumeClaimTemplates: []corev1.PersistentVolumeClaim{{
				ObjectMeta: metav1.ObjectMeta{Name: dataVolume},
				Spec: corev1.PersistentVolumeClaimSpec{
					AccessModes: []corev1.PersistentVolumeAccessMode{corev1.ReadWriteOnce},
					Resources: corev1.VolumeResourceRequirements{
						Requests: corev1.ResourceList{
							corev1.ResourceStorage: storage,
						},
					},
				},
			}},
		},
	}
}

// buildService is a PURE builder: the cluster's connection gateway (ADR D3 — gateway = the Service), a
// ClusterIP load-balancing the StatefulSet pods on Port.
func buildService(c *theodbv1.TheoDBCluster) *corev1.Service {
	labels := labelsFor(c)
	return &corev1.Service{
		ObjectMeta: metav1.ObjectMeta{Name: c.Name, Namespace: c.Namespace, Labels: labels},
		Spec: corev1.ServiceSpec{
			Type:     corev1.ServiceTypeClusterIP,
			Selector: labels,
			Ports:    []corev1.ServicePort{servicePort(c)},
		},
	}
}

// buildHeadlessService is a PURE builder: the StatefulSet's GOVERNING (headless) Service. A StatefulSet
// requires a headless Service (ClusterIP: None) named `ServiceName` to give each pod stable DNS
// (`<pod>.<name>-hl.<ns>.svc`). Without it, per-pod network identity — the reason a StatefulSet exists —
// never resolves for Instances > 1. Distinct from the gateway Service above (which has a VIP).
func buildHeadlessService(c *theodbv1.TheoDBCluster) *corev1.Service {
	labels := labelsFor(c)
	return &corev1.Service{
		ObjectMeta: metav1.ObjectMeta{Name: headlessServiceName(c), Namespace: c.Namespace, Labels: labels},
		Spec: corev1.ServiceSpec{
			Type:                     corev1.ServiceTypeClusterIP,
			ClusterIP:                corev1.ClusterIPNone,
			Selector:                 labels,
			PublishNotReadyAddresses: true, // peers must discover each other before Ready (replication bootstrap)
			Ports:                    []corev1.ServicePort{servicePort(c)},
		},
	}
}

// servicePort is the shared postgres port definition for both Services.
func servicePort(c *theodbv1.TheoDBCluster) corev1.ServicePort {
	return corev1.ServicePort{
		Name:       "postgres",
		Port:       c.Spec.Port,
		TargetPort: intstr.FromInt32(c.Spec.Port),
	}
}
