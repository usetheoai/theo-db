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
	"testing"

	"k8s.io/apimachinery/pkg/api/resource"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"sigs.k8s.io/controller-runtime/pkg/controller/controllerutil"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

func sampleCluster() *theodbv1.TheoDBCluster {
	return &theodbv1.TheoDBCluster{
		ObjectMeta: metav1.ObjectMeta{Name: "c1", Namespace: "ns1", UID: "uid-1"},
		Spec:       theodbv1.TheoDBClusterSpec{Instances: 3, Image: "theo-db:test", StorageSize: "5Gi", Port: 5432},
	}
}

// T2.1 RED: TestBuildStatefulSet_ReplicasImagePVC.
func TestBuildStatefulSet_ReplicasImagePVC(t *testing.T) {
	ss := buildStatefulSet(sampleCluster())
	if ss.Spec.Replicas == nil || *ss.Spec.Replicas != 3 {
		t.Fatalf("replicas: got %v, want 3", ss.Spec.Replicas)
	}
	if got := ss.Spec.Template.Spec.Containers[0].Image; got != "theo-db:test" {
		t.Errorf("image: got %q, want theo-db:test", got)
	}
	if len(ss.Spec.VolumeClaimTemplates) != 1 {
		t.Fatalf("want 1 volumeClaimTemplate, got %d", len(ss.Spec.VolumeClaimTemplates))
	}
	wantQty := resource.MustParse("5Gi")
	if got := ss.Spec.VolumeClaimTemplates[0].Spec.Resources.Requests["storage"]; got.Cmp(wantQty) != 0 {
		t.Errorf("storage: got %v, want 5Gi", got)
	}
	if ss.Spec.Template.Spec.Containers[0].Ports[0].ContainerPort != 5432 {
		t.Errorf("port: got %d, want 5432", ss.Spec.Template.Spec.Containers[0].Ports[0].ContainerPort)
	}
}

// T2.1 RED: TestBuildService_SelectorPort.
func TestBuildService_SelectorPort(t *testing.T) {
	svc := buildService(sampleCluster())
	if svc.Spec.Selector["app.kubernetes.io/instance"] != "c1" {
		t.Errorf("selector instance: got %q, want c1", svc.Spec.Selector["app.kubernetes.io/instance"])
	}
	if svc.Spec.Ports[0].Port != 5432 {
		t.Errorf("service port: got %d, want 5432", svc.Spec.Ports[0].Port)
	}
}

// T2.1 RED: TestBuildStatefulSet_OwnerRef — SetControllerReference marks the CR as controller-owner.
func TestBuildStatefulSet_OwnerRef(t *testing.T) {
	scheme := runtime.NewScheme()
	if err := theodbv1.AddToScheme(scheme); err != nil {
		t.Fatal(err)
	}
	c := sampleCluster()
	ss := buildStatefulSet(c)
	if err := controllerutil.SetControllerReference(c, ss, scheme); err != nil {
		t.Fatal(err)
	}
	refs := ss.GetOwnerReferences()
	if len(refs) != 1 || refs[0].Name != "c1" || refs[0].Controller == nil || !*refs[0].Controller {
		t.Errorf("owner ref: got %+v, want controller=true owner c1", refs)
	}
}
