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

package main

import (
	"strings"
	"testing"
)

// T3.2 RED: apply -f decodes a TheoDBCluster YAML into the typed object.
func TestTheodbctl_ApplyParsesYAML(t *testing.T) {
	yaml := `
apiVersion: core.theodb.io/v1
kind: TheoDBCluster
metadata:
  name: prod
  namespace: db
spec:
  instances: 3
  image: theo-db:v0.23.0
  storageSize: 10Gi
  port: 5432
`
	c, err := decodeCluster([]byte(yaml))
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	if c.Name != "prod" {
		t.Errorf("name: got %q, want prod", c.Name)
	}
	if c.Spec.Instances != 3 {
		t.Errorf("instances: got %d, want 3", c.Spec.Instances)
	}
	if c.Spec.Image != "theo-db:v0.23.0" {
		t.Errorf("image: got %q, want theo-db:v0.23.0", c.Spec.Image)
	}
}

// T3.2 RED — EC-3: a non-TheoDBCluster kind is rejected with a typed decode error (no client call).
func TestTheodbctl_ApplyWrongKind_Errors(t *testing.T) {
	yaml := `
apiVersion: v1
kind: ConfigMap
metadata:
  name: oops
`
	_, err := decodeCluster([]byte(yaml))
	if err == nil {
		t.Fatal("expected an error for kind ConfigMap, got nil")
	}
	if !strings.Contains(err.Error(), "TheoDBCluster") {
		t.Errorf("error should mention expected kind, got: %v", err)
	}
}

// T3.2 RED: malformed YAML → typed decode error.
func TestTheodbctl_ApplyMalformed_Errors(t *testing.T) {
	if _, err := decodeCluster([]byte("not: [valid: yaml")); err == nil {
		t.Fatal("expected a decode error for malformed YAML, got nil")
	}
}

// T3.2 RED: a cluster with no metadata.name is rejected (the API server would reject it too).
func TestTheodbctl_ApplyNoName_Errors(t *testing.T) {
	yaml := `
apiVersion: core.theodb.io/v1
kind: TheoDBCluster
spec:
  instances: 1
  image: theo-db:test
`
	if _, err := decodeCluster([]byte(yaml)); err == nil {
		t.Fatal("expected an error for missing metadata.name, got nil")
	}
}
