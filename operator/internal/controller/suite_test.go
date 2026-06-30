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
	"os"
	"path/filepath"
	"testing"

	"k8s.io/client-go/kubernetes/scheme"
	"k8s.io/client-go/rest"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/envtest"
	logf "sigs.k8s.io/controller-runtime/pkg/log"
	"sigs.k8s.io/controller-runtime/pkg/log/zap"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

// Package-level handles shared by the envtest integration tests (std testing, no ginkgo — ADR D2).
var (
	cfg       *rest.Config
	k8sClient client.Client
	testEnv   *envtest.Environment
)

// TestMain boots a REAL kube-apiserver + etcd (envtest) once for the whole package, then tears it down.
// This is the M23 evidence gate: the reconciler is proven against a real API server, not a fake client (D2).
func TestMain(m *testing.M) {
	logf.SetLogger(zap.New(zap.UseDevMode(true)))

	testEnv = &envtest.Environment{
		CRDDirectoryPaths:     []string{filepath.Join("..", "..", "config", "crd", "bases")},
		ErrorIfCRDPathMissing: true,
	}
	if dir := firstEnvTestBinaryDir(); dir != "" {
		testEnv.BinaryAssetsDirectory = dir
	}

	var err error
	cfg, err = testEnv.Start()
	if err != nil {
		fmt.Fprintf(os.Stderr, "envtest start failed (run `make setup-envtest` or set KUBEBUILDER_ASSETS): %v\n", err)
		os.Exit(1)
	}

	if err := theodbv1.AddToScheme(scheme.Scheme); err != nil {
		fmt.Fprintf(os.Stderr, "add theodb scheme: %v\n", err)
		os.Exit(1)
	}
	// corev1 + appsv1 are already registered in client-go's scheme.Scheme.

	k8sClient, err = client.New(cfg, client.Options{Scheme: scheme.Scheme})
	if err != nil {
		fmt.Fprintf(os.Stderr, "build client: %v\n", err)
		os.Exit(1)
	}

	code := m.Run()

	if err := testEnv.Stop(); err != nil {
		fmt.Fprintf(os.Stderr, "envtest stop: %v\n", err)
	}
	os.Exit(code)
}

// newReconciler returns a reconciler bound to the envtest client (the unit under test for integration cases).
func newReconciler() *TheoDBClusterReconciler {
	return &TheoDBClusterReconciler{Client: k8sClient, Scheme: scheme.Scheme}
}

// reconcileOnce reconciles a named cluster in the default namespace (the only namespace this suite uses).
func reconcileOnce(t *testing.T, name string) {
	t.Helper()
	r := newReconciler()
	if _, err := r.Reconcile(context.Background(), ctrlReq(name, "default")); err != nil {
		t.Fatalf("reconcile default/%s: %v", name, err)
	}
}

// firstEnvTestBinaryDir locates the envtest binaries under bin/k8s (so `go test` works from an IDE too).
func firstEnvTestBinaryDir() string {
	basePath := filepath.Join("..", "..", "bin", "k8s")
	entries, err := os.ReadDir(basePath)
	if err != nil {
		return ""
	}
	for _, entry := range entries {
		if entry.IsDir() {
			return filepath.Join(basePath, entry.Name())
		}
	}
	return ""
}
