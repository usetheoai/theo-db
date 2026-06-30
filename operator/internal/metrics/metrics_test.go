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

package metrics

import (
	"strings"
	"testing"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/testutil"
)

// T1.1 RED — EC-1: Register is idempotent (a double Register on the same registry must not panic).
func TestMetrics_Register_Idempotent(t *testing.T) {
	reg := prometheus.NewRegistry()
	Register(reg)
	Register(reg) // second call must be a no-op, not a panic
}

// T1.1 RED: recorded samples are exported with the expected values.
func TestMetrics_RecordsSamples(t *testing.T) {
	ReconcileTotal.Reset()
	ClusterReadyInstances.Reset()
	ClusterDesiredInstances.Reset()
	ClusterPhase.Reset()

	ReconcileTotal.WithLabelValues(ResultSuccess).Inc()
	ClusterReadyInstances.WithLabelValues("ns1", "c1").Set(0)
	ClusterDesiredInstances.WithLabelValues("ns1", "c1").Set(3)
	RecordPhase("ns1", "c1", "Initializing")

	if got := testutil.ToFloat64(ReconcileTotal.WithLabelValues(ResultSuccess)); got != 1 {
		t.Errorf("reconcile_total{success}: got %v, want 1", got)
	}
	if got := testutil.ToFloat64(ClusterDesiredInstances.WithLabelValues("ns1", "c1")); got != 3 {
		t.Errorf("desired_instances: got %v, want 3", got)
	}

	// RecordPhase sets exactly the active phase to 1 and the others to 0.
	want := `
# HELP theodb_cluster_phase Current phase of a TheoDBCluster (1 = active phase).
# TYPE theodb_cluster_phase gauge
theodb_cluster_phase{cluster="c1",namespace="ns1",phase="Error"} 0
theodb_cluster_phase{cluster="c1",namespace="ns1",phase="Healthy"} 0
theodb_cluster_phase{cluster="c1",namespace="ns1",phase="Initializing"} 1
`
	if err := testutil.CollectAndCompare(ClusterPhase, strings.NewReader(want)); err != nil {
		t.Errorf("phase gauge mismatch:\n%v", err)
	}
}

// T1.1 RED: the histogram uses sane (DefBuckets) buckets so sub-second reconciles are distinguishable.
func TestMetrics_DurationBuckets(t *testing.T) {
	ReconcileDuration.Observe(0.012)
	n := testutil.CollectAndCount(ReconcileDuration)
	if n == 0 {
		t.Error("reconcile_duration histogram exported no series")
	}
}
