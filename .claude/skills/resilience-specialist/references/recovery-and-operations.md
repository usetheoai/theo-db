# Recovery and operations

## Contents

1. [Recovery](#recovery)
2. [Gradual ramp-up](#gradual-ramp-up)
3. [Thundering herd](#thundering-herd)
4. [Failover](#failover)
5. [Graceful shutdown](#graceful-shutdown)
6. [Health checks](#health-checks)
7. [Clocks](#clocks)
8. [Chaos engineering](#chaos-engineering)

---

## Recovery

Resilience includes what happens **after** the failure. Plan for: rebuilding caches, draining queues, resuming traffic gradually, reconciling data, reprocessing, compensation, deduplication, consistency validation, restoring capacity, reviewing circuit breakers, and shedding accumulated retries.

Recovery is the half most often left undesigned, and it is where a second outage usually starts.

## Gradual ramp-up

Do not release the full load the instant a dependency recovers.

```text
Dependency responds again
↓
5% of traffic
↓
20%
↓
50%
↓
100%
```

This is what prevents the relapse.

## Thundering herd

Many clients or workers resume simultaneously — after a service recovers, a cache expires, a lock releases, an application restarts, a circuit breaker opens, or the network returns.

Mitigate with jitter, gradual resumption, concurrency limits, queues, priorities, token buckets and single-flight.

## Failover

Moving work to an alternative component — automatic or manual, active-passive or active-active, by region, by dependency, or by customer.

**Cautions.** Avoid split-brain; validate the target's health; bound the failover time; test the return to the primary; confirm the secondary actually has the capacity; account for data not yet replicated.

An untested failover path tends to fail precisely during the incident.

## Graceful shutdown

1. Stop accepting new requests.
2. Signal unavailability to the load balancer.
3. Wait for in-flight operations.
4. Interrupt operations that exceed the grace period.
5. Acknowledge processed messages.
6. Release connections.
7. Exit the process.

Avoid shutdowns that cut operations off uncontrolled.

## Health checks

| Check | Answers |
|---|---|
| **Liveness** | Is the process working? A failure may justify a restart |
| **Readiness** | Is this instance ready for traffic? An instance can be alive but not ready |
| **Startup** | Has initialisation finished? Prevents premature restarts of slow-starting apps |

**Do not make health checks heavy, and do not check every dependency in liveness.** An unavailable external dependency does not mean the process should be restarted — and restart loops make incidents worse. This is a common and expensive mistake: it converts a partial outage into a total one.

## Clocks

Do not depend on perfectly synchronised clocks. Related failures: clock skew, out-of-order timestamps, tokens considered expired, invalid locks, wrong ordering, inconsistent TTLs.

Where possible use monotonic clocks for duration, tolerance margins, logical versions, sortable identifiers, and protocols that account for skew.

## Chaos engineering

Introduce controlled failures to validate resilience: added latency, killing an instance, blocking communication, reducing capacity, exhausting a pool, simulating a dependency error, raising the failure rate, taking out a region, delaying queue processing.

### Rules for experiments

Before running one: define the hypothesis, define the maximum impact, establish the metrics, prepare an immediate abort, limit the scope, inform the responsible people, confirm observability is in place, and validate recovery.

**Never run an experiment without explicit limits.**
