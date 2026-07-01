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

package mcpserver

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/client/fake"
	"sigs.k8s.io/controller-runtime/pkg/client/interceptor"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

func newFakeClient(objs ...client.Object) client.Client {
	scheme := runtime.NewScheme()
	_ = theodbv1.AddToScheme(scheme)
	return fake.NewClientBuilder().WithScheme(scheme).WithObjects(objs...).Build()
}

func cluster(name, ns string, instances int32) *theodbv1.TheoDBCluster {
	return &theodbv1.TheoDBCluster{
		ObjectMeta: metav1.ObjectMeta{Name: name, Namespace: ns},
		Spec:       theodbv1.TheoDBClusterSpec{Instances: instances, Image: "theo-db:test", StorageSize: "1Gi", Port: 5432},
	}
}

// connect wires an in-memory client↔server pair and returns the connected client session.
func connect(t *testing.T, c client.Client) (*mcp.ClientSession, context.Context) {
	t.Helper()
	ctx := context.Background()
	serverT, clientT := mcp.NewInMemoryTransports()
	srv := New(c)
	go func() { _ = srv.Run(ctx, serverT) }()

	cl := mcp.NewClient(&mcp.Implementation{Name: "test-client", Version: "v0"}, nil)
	session, err := cl.Connect(ctx, clientT, nil)
	if err != nil {
		t.Fatalf("client connect (handshake): %v", err)
	}
	t.Cleanup(func() { _ = session.Close() })
	return session, ctx
}

// T3.1 RED: handshake succeeds and the server advertises exactly the two read tools.
func TestMCP_Handshake_AdvertisesTwoTools(t *testing.T) {
	session, ctx := connect(t, newFakeClient())
	res, err := session.ListTools(ctx, nil)
	if err != nil {
		t.Fatalf("list tools: %v", err)
	}
	names := map[string]bool{}
	for _, tool := range res.Tools {
		names[tool.Name] = true
	}
	if len(res.Tools) != 2 || !names["list_clusters"] || !names["get_cluster"] {
		t.Errorf("tools: got %v, want exactly [list_clusters get_cluster]", names)
	}
}

// T3.1 RED: list_clusters returns the clusters from the injected client.
func TestMCP_ListClusters_ReturnsRegistered(t *testing.T) {
	session, ctx := connect(t, newFakeClient(cluster("a", "default", 1), cluster("b", "default", 3)))
	res, err := session.CallTool(ctx, &mcp.CallToolParams{Name: "list_clusters"})
	if err != nil {
		t.Fatalf("call list_clusters: %v", err)
	}
	if res.IsError {
		t.Fatalf("list_clusters returned IsError: %+v", res.Content)
	}
	sc, ok := res.StructuredContent.(map[string]any)
	if !ok {
		t.Fatalf("structured content: got %T, want map", res.StructuredContent)
	}
	clusters, _ := sc["clusters"].([]any)
	if len(clusters) != 2 {
		t.Errorf("clusters: got %d, want 2", len(clusters))
	}
}

// T3.1 RED — EC-2: get_cluster on a missing name returns a typed MCP error result (IsError), never a panic.
func TestMCP_GetCluster_NotFound(t *testing.T) {
	session, ctx := connect(t, newFakeClient())
	res, err := session.CallTool(ctx, &mcp.CallToolParams{
		Name:      "get_cluster",
		Arguments: map[string]any{"name": "ghost", "namespace": "default"},
	})
	if err != nil {
		t.Fatalf("call get_cluster: %v", err)
	}
	if !res.IsError {
		t.Error("get_cluster on a missing cluster must return IsError=true, got false")
	}
}

// T3.1 RED — EC-2: get_cluster with an empty name is rejected as an error result (input validation).
func TestMCP_GetCluster_EmptyName(t *testing.T) {
	session, ctx := connect(t, newFakeClient())
	res, err := session.CallTool(ctx, &mcp.CallToolParams{
		Name:      "get_cluster",
		Arguments: map[string]any{"name": ""},
	})
	if err != nil {
		t.Fatalf("call get_cluster: %v", err)
	}
	if !res.IsError {
		t.Error("get_cluster with empty name must return IsError=true")
	}
}

// T3.1 RED: get_cluster returns the projected summary for an existing cluster.
func TestMCP_GetCluster_Found(t *testing.T) {
	session, ctx := connect(t, newFakeClient(cluster("prod", "db", 5)))
	res, err := session.CallTool(ctx, &mcp.CallToolParams{
		Name:      "get_cluster",
		Arguments: map[string]any{"name": "prod", "namespace": "db"},
	})
	if err != nil {
		t.Fatalf("call get_cluster: %v", err)
	}
	if res.IsError {
		t.Fatalf("get_cluster returned IsError: %+v", res.Content)
	}
	sc, ok := res.StructuredContent.(map[string]any)
	if !ok {
		t.Fatalf("structured content: got %T", res.StructuredContent)
	}
	if sc["name"] != "prod" || sc["instances"].(float64) != 5 {
		t.Errorf("summary: got %+v, want name=prod instances=5", sc)
	}
}

// BenchmarkMCP_ListClusters measures tool-call latency over the in-memory transport (M24 T3.1 evidence).
func BenchmarkMCP_ListClusters(b *testing.B) {
	ctx := context.Background()
	serverT, clientT := mcp.NewInMemoryTransports()
	srv := New(newFakeClient(cluster("a", "default", 1), cluster("b", "default", 3)))
	go func() { _ = srv.Run(ctx, serverT) }()
	cl := mcp.NewClient(&mcp.Implementation{Name: "bench", Version: "v0"}, nil)
	session, err := cl.Connect(ctx, clientT, nil)
	if err != nil {
		b.Fatal(err)
	}
	defer func() { _ = session.Close() }()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, err := session.CallTool(ctx, &mcp.CallToolParams{Name: "list_clusters"}); err != nil {
			b.Fatal(err)
		}
	}
}

// T3.1 — failure scenario (plan): a List error from the client → IsError tool result, server stays up.
func TestMCP_ListClusters_ClientError(t *testing.T) {
	scheme := runtime.NewScheme()
	_ = theodbv1.AddToScheme(scheme)
	errClient := fake.NewClientBuilder().WithScheme(scheme).WithInterceptorFuncs(interceptor.Funcs{
		List: func(context.Context, client.WithWatch, client.ObjectList, ...client.ListOption) error {
			return errors.New("boom")
		},
	}).Build()

	session, ctx := connect(t, errClient)
	res, err := session.CallTool(ctx, &mcp.CallToolParams{Name: "list_clusters"})
	if err != nil {
		t.Fatalf("call list_clusters: %v", err)
	}
	if !res.IsError {
		t.Error("list_clusters on a client error must return IsError=true")
	}
	// Security regression guard: the message must NOT leak the raw K8s error (SA identity / RBAC posture).
	if text := errorText(res); strings.Contains(text, "boom") {
		t.Errorf("error message leaked the raw client error: %q", text)
	}
	// Server stays up: a second call still succeeds.
	if _, err := session.ListTools(ctx, nil); err != nil {
		t.Errorf("server did not stay up after a tool error: %v", err)
	}
}

// errorText extracts the text of the first TextContent in an MCP tool result (test helper).
func errorText(res *mcp.CallToolResult) string {
	for _, c := range res.Content {
		if tc, ok := c.(*mcp.TextContent); ok {
			return tc.Text
		}
	}
	return ""
}

// T3.1 — failure scenario (plan, EC-5): closing the transport makes server.Run return cleanly (no leak/hang).
func TestMCP_TransportClose_CleanShutdown(t *testing.T) {
	ctx := context.Background()
	serverT, clientT := mcp.NewInMemoryTransports()
	srv := New(newFakeClient())
	done := make(chan error, 1)
	go func() { done <- srv.Run(ctx, serverT) }()

	cl := mcp.NewClient(&mcp.Implementation{Name: "test", Version: "v0"}, nil)
	session, err := cl.Connect(ctx, clientT, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := session.Close(); err != nil {
		t.Fatalf("session close: %v", err)
	}
	select {
	case <-done: // server.Run returned cleanly
	case <-time.After(5 * time.Second):
		t.Fatal("server.Run did not return within 5s of transport close (goroutine leak / hang)")
	}
}
