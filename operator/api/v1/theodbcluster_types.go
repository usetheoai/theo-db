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

package v1

import (
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// TheoDBClusterSpec defines the desired state of a TheoDBCluster (plan M23, blueprint Corner 4).
// Minimal — adapted from cloudnative-pg's ClusterSpec (instances/image/storage).
type TheoDBClusterSpec struct {
	// Instances is the number of TheoDB database instances (StatefulSet replicas).
	// +kubebuilder:validation:Minimum=1
	// +kubebuilder:default:=1
	Instances int32 `json:"instances"`

	// Image is the container image for the TheoDB instances (the shipped theo-db image).
	// Required — a cluster cannot run without an image; the API server rejects an empty value.
	// +kubebuilder:validation:MinLength=1
	Image string `json:"image"`

	// StorageSize is the per-instance persistent volume size (e.g. "10Gi"). Validated as a
	// Kubernetes quantity at the API boundary so a malformed value never reaches the reconciler.
	// Immutable: a StatefulSet's VolumeClaimTemplates cannot be changed in place, so the API server
	// rejects an edit rather than letting the operator silently drop it (recreate the cluster to resize).
	// +kubebuilder:validation:Pattern=`^[0-9]+(\.[0-9]+)?(Ei|Pi|Ti|Gi|Mi|Ki|E|P|T|G|M|k)?$`
	// +kubebuilder:validation:XValidation:rule="self == oldSelf",message="storageSize is immutable; recreate the cluster to resize"
	// +kubebuilder:default:="1Gi"
	// +optional
	StorageSize string `json:"storageSize,omitempty"`

	// Port is the database port exposed by the Service.
	// +kubebuilder:validation:Minimum=1
	// +kubebuilder:validation:Maximum=65535
	// +kubebuilder:default:=5432
	// +optional
	Port int32 `json:"port,omitempty"`
}

// TheoDBClusterStatus defines the observed state of a TheoDBCluster.
type TheoDBClusterStatus struct {
	// Phase is a human-readable cluster phase: "Initializing", "Healthy", "Error".
	// +optional
	Phase string `json:"phase,omitempty"`

	// ReadyInstances is the number of ready instances (from the StatefulSet's ReadyReplicas).
	// +optional
	ReadyInstances int32 `json:"readyInstances,omitempty"`

	// ObservedGeneration is the .metadata.generation the status was last reconciled against,
	// so clients can tell whether the status reflects the current spec.
	// +optional
	ObservedGeneration int64 `json:"observedGeneration,omitempty"`

	// Conditions represent the current state of the TheoDBCluster (e.g. "Ready").
	// +listType=map
	// +listMapKey=type
	// +optional
	Conditions []metav1.Condition `json:"conditions,omitempty"`
}

// +kubebuilder:object:root=true
// +kubebuilder:subresource:status
// +kubebuilder:printcolumn:name="Instances",type="integer",JSONPath=".spec.instances"
// +kubebuilder:printcolumn:name="Ready",type="integer",JSONPath=".status.readyInstances"
// +kubebuilder:printcolumn:name="Phase",type="string",JSONPath=".status.phase"

// TheoDBCluster is the Schema for the theodbclusters API.
type TheoDBCluster struct {
	metav1.TypeMeta `json:",inline"`

	// +optional
	metav1.ObjectMeta `json:"metadata,omitzero"`

	// +required
	Spec TheoDBClusterSpec `json:"spec"`

	// +optional
	Status TheoDBClusterStatus `json:"status,omitzero"`
}

// +kubebuilder:object:root=true

// TheoDBClusterList contains a list of TheoDBCluster.
type TheoDBClusterList struct {
	metav1.TypeMeta `json:",inline"`
	metav1.ListMeta `json:"metadata,omitzero"`
	Items           []TheoDBCluster `json:"items"`
}

func init() {
	SchemeBuilder.Register(&TheoDBCluster{}, &TheoDBClusterList{})
}
