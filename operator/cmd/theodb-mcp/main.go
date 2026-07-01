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

// Command theodb-mcp serves TheoDB's read-only Model Context Protocol tools to AI agents (plan M24 T3.1).
// Default transport is stdio (the canonical agent-spawn contract); -http selects streamable HTTP (ADR-3).
package main

import (
	"context"
	"flag"
	"fmt"
	"net/http"
	"os"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	"k8s.io/apimachinery/pkg/runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/client/config"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
	"github.com/usetheodev/theo-db/operator/internal/mcpserver"
)

func main() {
	httpAddr := flag.String("http", "",
		"if set, serve streamable HTTP at this address instead of stdio. WARNING: this transport performs NO "+
			"authentication or TLS — run it ONLY behind an authenticating platform edge, never bound to an "+
			"untrusted network.")
	flag.Parse()

	if err := run(*httpAddr); err != nil {
		fmt.Fprintln(os.Stderr, "theodb-mcp:", err)
		os.Exit(1)
	}
}

func run(httpAddr string) error {
	c, err := newClient()
	if err != nil {
		return err
	}
	srv := mcpserver.New(c)
	ctx := context.Background()

	if httpAddr != "" {
		handler := mcp.NewStreamableHTTPHandler(func(*http.Request) *mcp.Server { return srv }, nil)
		fmt.Fprintf(os.Stderr, "theodb-mcp: serving streamable HTTP on %s (UNAUTHENTICATED — edge-front it)\n", httpAddr)
		// ReadHeaderTimeout guards against Slowloris on the unauthenticated path (gosec G114).
		httpSrv := &http.Server{Addr: httpAddr, Handler: handler, ReadHeaderTimeout: 10 * time.Second}
		return httpSrv.ListenAndServe()
	}
	return srv.Run(ctx, &mcp.StdioTransport{})
}

// newClient builds a controller-runtime client from the ambient kubeconfig with the TheoDB scheme registered.
func newClient() (client.Client, error) {
	scheme := runtime.NewScheme()
	if err := theodbv1.AddToScheme(scheme); err != nil {
		return nil, err
	}
	cfg, err := config.GetConfig()
	if err != nil {
		return nil, fmt.Errorf("load kubeconfig: %w", err)
	}
	return client.New(cfg, client.Options{Scheme: scheme})
}
