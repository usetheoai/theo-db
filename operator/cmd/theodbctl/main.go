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

// Command theodbctl is a thin cobra CLI to apply / get / delete TheoDBCluster resources (plan M23 T3.2).
package main

import (
	"context"
	"fmt"
	"os"
	"text/tabwriter"

	"github.com/spf13/cobra"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/client/config"
	"sigs.k8s.io/yaml"

	theodbv1 "github.com/usetheodev/theo-db/operator/api/v1"
)

func main() {
	if err := rootCmd().Execute(); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func rootCmd() *cobra.Command {
	root := &cobra.Command{
		Use:           "theodbctl",
		Short:         "Manage TheoDBCluster resources",
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	root.AddCommand(applyCmd(), getCmd(), deleteCmd())
	return root
}

// decodeCluster decodes a TheoDBCluster from YAML/JSON bytes (the unit-testable seam — EC-3 wrong-kind rejection).
func decodeCluster(data []byte) (*theodbv1.TheoDBCluster, error) {
	var c theodbv1.TheoDBCluster
	if err := yaml.UnmarshalStrict(data, &c); err != nil {
		return nil, fmt.Errorf("decode TheoDBCluster: %w", err)
	}
	if c.Kind != "" && c.Kind != "TheoDBCluster" {
		return nil, fmt.Errorf("expected kind TheoDBCluster, got %q", c.Kind)
	}
	if c.Name == "" {
		return nil, fmt.Errorf("metadata.name is required")
	}
	return &c, nil
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

func applyCmd() *cobra.Command {
	var file string
	cmd := &cobra.Command{
		Use:   "apply -f FILE",
		Short: "Create or update a TheoDBCluster from a YAML file",
		RunE: func(cmd *cobra.Command, _ []string) error {
			if file == "" {
				return fmt.Errorf("-f FILE is required")
			}
			data, err := os.ReadFile(file)
			if err != nil {
				return fmt.Errorf("read %s: %w", file, err)
			}
			desired, err := decodeCluster(data)
			if err != nil {
				return err
			}
			cl, err := newClient()
			if err != nil {
				return err
			}
			ctx := context.Background()
			existing := &theodbv1.TheoDBCluster{}
			key := client.ObjectKeyFromObject(desired)
			getErr := cl.Get(ctx, key, existing)
			if apierrors.IsNotFound(getErr) {
				if err := cl.Create(ctx, desired); err != nil {
					return err
				}
				fmt.Fprintf(cmd.OutOrStdout(), "theodbcluster/%s created\n", desired.Name)
				return nil
			}
			if getErr != nil {
				return getErr
			}
			existing.Spec = desired.Spec
			if err := cl.Update(ctx, existing); err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "theodbcluster/%s configured\n", desired.Name)
			return nil
		},
	}
	cmd.Flags().StringVarP(&file, "file", "f", "", "path to a TheoDBCluster YAML")
	return cmd
}

func getCmd() *cobra.Command {
	var namespace string
	cmd := &cobra.Command{
		Use:   "get",
		Short: "List TheoDBCluster resources",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cl, err := newClient()
			if err != nil {
				return err
			}
			var list theodbv1.TheoDBClusterList
			opts := []client.ListOption{}
			if namespace != "" {
				opts = append(opts, client.InNamespace(namespace))
			}
			if err := cl.List(context.Background(), &list, opts...); err != nil {
				return err
			}
			if len(list.Items) == 0 {
				fmt.Fprintln(cmd.OutOrStdout(), "no clusters found")
				return nil
			}
			w := tabwriter.NewWriter(cmd.OutOrStdout(), 0, 0, 3, ' ', 0)
			fmt.Fprintln(w, "NAMESPACE\tNAME\tINSTANCES\tREADY\tPHASE")
			for i := range list.Items {
				c := &list.Items[i]
				fmt.Fprintf(w, "%s\t%s\t%d\t%d\t%s\n", c.Namespace, c.Name, c.Spec.Instances, c.Status.ReadyInstances, c.Status.Phase)
			}
			return w.Flush()
		},
	}
	cmd.Flags().StringVarP(&namespace, "namespace", "n", "", "namespace (default: all)")
	return cmd
}

func deleteCmd() *cobra.Command {
	var namespace string
	cmd := &cobra.Command{
		Use:   "delete NAME",
		Short: "Delete a TheoDBCluster by name",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := newClient()
			if err != nil {
				return err
			}
			c := &theodbv1.TheoDBCluster{ObjectMeta: metav1.ObjectMeta{Name: args[0], Namespace: namespace}}
			if err := cl.Delete(context.Background(), c); err != nil {
				if apierrors.IsNotFound(err) {
					return fmt.Errorf("theodbcluster %q not found in namespace %q", args[0], namespace)
				}
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "theodbcluster/%s deleted\n", args[0])
			return nil
		},
	}
	cmd.Flags().StringVarP(&namespace, "namespace", "n", "default", "namespace")
	return cmd
}
