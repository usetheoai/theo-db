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

// Package mcpserver exposes TheoDB read operations to AI agents via the Model Context Protocol (plan M24 T3.1).
// It is READ-ONLY by design (ADR-3): two tools, list_clusters and get_cluster — no mutation surface.
package mcpserver

import (
	"context"
	"fmt"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	"k8s.io/apimachinery/pkg/types"
	"sigs.k8s.io/controller-runtime/pkg/client"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

// ClusterSummary is the read-only projection of a TheoDBCluster returned to agents.
type ClusterSummary struct {
	Namespace      string `json:"namespace"`
	Name           string `json:"name"`
	Instances      int32  `json:"instances"`
	ReadyInstances int32  `json:"readyInstances"`
	Phase          string `json:"phase"`
}

// listInput optionally narrows the listing to one namespace.
type listInput struct {
	Namespace string `json:"namespace,omitempty"`
}

// listOutput is the structured result of list_clusters.
type listOutput struct {
	Clusters []ClusterSummary `json:"clusters"`
}

// getInput identifies one cluster.
type getInput struct {
	Name      string `json:"name"`
	Namespace string `json:"namespace"`
}

// New builds an MCP server bound to a controller-runtime client (DIP — the client is injected, not built here).
// It registers exactly the two read-only tools.
func New(c client.Client) *mcp.Server {
	s := mcp.NewServer(&mcp.Implementation{Name: "theodb", Version: "v0.24.0"}, nil)

	mcp.AddTool(s, &mcp.Tool{
		Name:        "list_clusters",
		Description: "List TheoDBCluster resources, optionally filtered by namespace.",
	}, func(ctx context.Context, _ *mcp.CallToolRequest, in listInput) (*mcp.CallToolResult, listOutput, error) {
		var list theodbv1.TheoDBClusterList
		opts := []client.ListOption{}
		if in.Namespace != "" {
			opts = append(opts, client.InNamespace(in.Namespace))
		}
		if err := c.List(ctx, &list, opts...); err != nil {
			return errorResult(fmt.Sprintf("list failed: %v", err)), listOutput{}, nil
		}
		out := listOutput{Clusters: make([]ClusterSummary, 0, len(list.Items))}
		for i := range list.Items {
			out.Clusters = append(out.Clusters, summarize(&list.Items[i]))
		}
		return nil, out, nil
	})

	mcp.AddTool(s, &mcp.Tool{
		Name:        "get_cluster",
		Description: "Get a single TheoDBCluster by name and namespace.",
	}, func(ctx context.Context, _ *mcp.CallToolRequest, in getInput) (*mcp.CallToolResult, ClusterSummary, error) {
		if in.Name == "" {
			return errorResult("name is required"), ClusterSummary{}, nil
		}
		ns := in.Namespace
		if ns == "" {
			ns = "default"
		}
		var cluster theodbv1.TheoDBCluster
		err := c.Get(ctx, types.NamespacedName{Name: in.Name, Namespace: ns}, &cluster)
		if apierrors.IsNotFound(err) {
			return errorResult(fmt.Sprintf("theodbcluster %q not found in namespace %q", in.Name, ns)), ClusterSummary{}, nil
		}
		if err != nil {
			return errorResult(fmt.Sprintf("get failed: %v", err)), ClusterSummary{}, nil
		}
		return nil, summarize(&cluster), nil
	})

	return s
}

// summarize projects a TheoDBCluster into the read-only summary.
func summarize(c *theodbv1.TheoDBCluster) ClusterSummary {
	return ClusterSummary{
		Namespace:      c.Namespace,
		Name:           c.Name,
		Instances:      c.Spec.Instances,
		ReadyInstances: c.Status.ReadyInstances,
		Phase:          c.Status.Phase,
	}
}

// errorResult builds an MCP tool result flagged as an error (IsError=true) carrying a human message — the
// session survives (no Go error returned), so a bad request never drops the agent connection (EC-2).
func errorResult(msg string) *mcp.CallToolResult {
	return &mcp.CallToolResult{
		IsError: true,
		Content: []mcp.Content{&mcp.TextContent{Text: msg}},
	}
}
