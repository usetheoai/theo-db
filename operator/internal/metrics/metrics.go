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

// Package metrics holds TheoDB's own domain Prometheus collectors for the operator (plan M24 T1.1).
// They are registered on controller-runtime's shared registry (ADR-1) and scraped through the existing
// /metrics endpoint — no new dependency, no second HTTP server.
package metrics

import (
	"github.com/prometheus/client_golang/prometheus"
)

// Label values for the closed-enum labels (bounded cardinality — never a free-form user value).
const (
	ResultSuccess = "success"
	ResultError   = "error"
)

var (
	// ReconcileTotal counts reconcile outcomes by result (closed enum: success|error).
	ReconcileTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Name: "theodb_reconcile_total",
		Help: "Total number of TheoDBCluster reconciliations by result.",
	}, []string{"result"})

	// ReconcileDuration is the reconcile wall-time histogram (default buckets cover ms→s).
	ReconcileDuration = prometheus.NewHistogram(prometheus.HistogramOpts{
		Name:    "theodb_reconcile_duration_seconds",
		Help:    "Wall-clock duration of a TheoDBCluster reconcile.",
		Buckets: prometheus.DefBuckets,
	})

	// ClusterPhase is 1 for the cluster's current phase, 0 otherwise (per namespace/cluster/phase).
	ClusterPhase = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Name: "theodb_cluster_phase",
		Help: "Current phase of a TheoDBCluster (1 = active phase).",
	}, []string{"namespace", "cluster", "phase"})

	// ClusterReadyInstances is the number of ready instances per cluster.
	ClusterReadyInstances = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Name: "theodb_cluster_ready_instances",
		Help: "Number of ready instances for a TheoDBCluster.",
	}, []string{"namespace", "cluster"})

	// ClusterDesiredInstances is the desired instance count per cluster.
	ClusterDesiredInstances = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Name: "theodb_cluster_desired_instances",
		Help: "Desired instance count for a TheoDBCluster.",
	}, []string{"namespace", "cluster"})
)

// collectors returns every domain collector in a stable order.
func collectors() []prometheus.Collector {
	return []prometheus.Collector{
		ReconcileTotal, ReconcileDuration, ClusterPhase, ClusterReadyInstances, ClusterDesiredInstances,
	}
}

// Register adds the domain collectors to reg. It is IDEMPOTENT (EC-1): an already-registered collector is
// tolerated (controller-runtime's registry is process-global; a double Register must never panic).
func Register(reg prometheus.Registerer) {
	for _, c := range collectors() {
		if err := reg.Register(c); err != nil {
			if _, ok := err.(prometheus.AlreadyRegisteredError); ok {
				continue
			}
			panic(err) // a non-AlreadyRegistered error is a programming bug (e.g., duplicate metric name)
		}
	}
}

// RecordPhase sets the phase gauge (1 for the active phase, 0 for the others) for one cluster.
func RecordPhase(namespace, cluster, phase string) {
	for _, p := range []string{"Initializing", "Healthy", "Error"} {
		v := 0.0
		if p == phase {
			v = 1.0
		}
		ClusterPhase.WithLabelValues(namespace, cluster, p).Set(v)
	}
}
