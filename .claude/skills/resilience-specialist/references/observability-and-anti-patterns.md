# Observability and anti-patterns

## Contents

1. [Metrics](#metrics)
2. [RED and USE](#red-and-use)
3. [Logs](#logs)
4. [Distributed tracing](#distributed-tracing)
5. [SLI, SLO and error budget](#sli-slo-and-error-budget)
6. [Tail latency](#tail-latency)
7. [Fan-out](#fan-out)
8. [Anti-patterns](#anti-patterns)

---

## Metrics

Request rate · error rate · latency · saturation · queue depth · **age of the oldest item** · retries · timeouts · open circuit breakers · rejections · drops · fallback rate · concurrency · pool utilisation · available connections · cache hit rate · deadline propagation · cancellations.

## RED and USE

For request-driven services — **RED**: Rate, Errors, Duration.
For resources — **USE**: Utilization, Saturation, Errors.

## Logs

Record: correlation id, the dependency called, the configured timeout, the duration, the attempt number, the retry reason, the circuit breaker state, whether a fallback was used, the remaining deadline, rejection for capacity, and the final outcome.

Never log sensitive data.

## Distributed tracing

A trace should show the call chain, per-dependency latency, retries, timeouts, parallel calls, bottlenecks, context propagation, cancellations and fallbacks.

## SLI, SLO and error budget

* **SLI** — the measured indicator: success rate, latency, availability, consistency, processing time.
* **SLO** — the target. *"99.9% of requests complete in under 500 ms."*
* **Error budget** — the failure tolerable within the SLO.

The resilience policy exists to protect the SLO, not merely to keep processes running.

## Tail latency

Averages hide the problem.

```text
Average: 100 ms
p95:     400 ms
p99:     2 seconds
p99.9:   10 seconds
```

In a distributed system with fan-out, the operation is bounded by its slowest call. Monitor p50, p90, p95, p99, p99.9, timeout rate and slow-call rate.

## Fan-out

One operation generating many downstream calls:

```text
1 request → 20 downstream calls
1,000 requests → 20,000 downstream calls
```

**Risks.** Load amplification, higher failure probability, worse tail latency, saturation, difficult cancellation.

Mitigate with a fan-out limit, aggregation, cache, batching, bounded concurrency, partial calls, deadlines and cancellation.

## Anti-patterns

### No timeout

Calls stay open indefinitely.

### Excessive timeout

Resources stay busy and the failure takes too long to detect.

### Infinite retry

Adds load and prevents recovery.

### Immediate retry

Creates synchronised spikes.

### Retry at every layer

```text
Gateway:   3 attempts
Service A: 3 attempts
Service B: 3 attempts
Potential: 27 executions
```

### Global circuit breaker

A failure in one operation opens the circuit for unrelated ones.

### Shared pool

One slow dependency consumes every resource.

### Unbounded queue

Overload becomes out-of-memory and ever-growing latency.

### Health check coupled to every dependency

An external failure restarts healthy instances — converting a partial outage into a total one.

### Fallback without visibility

The system looks healthy while continuously serving degraded responses. Nobody notices until the primary path has been broken for weeks.

### Untested failover

The alternative path fails exactly during the incident.

### Instant recovery

The full load returns at once and causes a second failure.

### Cache as absolute truth

Stale data is treated as current, with no indication.

### Monitoring throughput only

Stable throughput hides a queue whose oldest item keeps getting older.
