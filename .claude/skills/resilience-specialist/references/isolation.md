# Isolation: circuit breakers, bulkheads, concurrency, queues, cells

## Contents

1. [Circuit breaker](#circuit-breaker)
2. [Bulkhead](#bulkhead)
3. [Concurrency limits](#concurrency-limits)
4. [Bounded queues](#bounded-queues)
5. [Cellular architecture](#cellular-architecture)
6. [Redundancy](#redundancy)

---

## Circuit breaker

A circuit breaker temporarily stops calls to a failing dependency.

| State | Behaviour |
|---|---|
| **Closed** | Calls pass normally |
| **Open** | Calls are blocked immediately |
| **Half-open** | A limited number of probe calls is allowed |

```text
Closed → (failures exceed the threshold) → Open
Open   → (wait period)                   → Half-open
Half-open → (success) → Closed
Half-open → (failure) → Open
```

**Benefits.** Avoids useless calls, reduces resource consumption, fails fast, protects a degraded dependency, limits cascading failure, allows recovery, improves predictability.

### Configuration

Analyse the observation window, the minimum number of calls, the failure rate, the slow-call rate, how long the open state lasts, how many calls are allowed in half-open, which failures count, and the fallback behaviour.

**Do not open the circuit on a tiny sample.** `1 failure in 1 call = 100% failure rate` is not evidence — there is no sample.

### A circuit breaker does not replace a timeout

Without a timeout, calls can block for a long time before the breaker registers a failure at all. Use them together:

```text
Timeout + circuit breaker + concurrency limit
```

**Do not use one global breaker.** A failure in one operation would open the circuit for unrelated operations. Scope breakers per dependency.

## Bulkhead

A bulkhead isolates resources so that one dependency cannot consume the whole capacity — a ship divided into compartments, where flooding one leaves the others intact.

Isolate by thread pool, workers, queues, connections, memory, CPU, client, dependency, tenant, operation or region.

```text
Recommendations: 20 workers
Payments:        50 workers
Search:          30 workers
```

If recommendations get slow, they must not consume the workers reserved for payments.

Avoid a single shared pool for all external calls. Prefer:

```text
Pool A → Payments
Pool B → Catalogue
Pool C → Recommendations
```

Size them by priority, volume and criticality.

## Concurrency limits

Control how many operations run at once:

```text
Maximum simultaneous database calls: 100
```

Excess requests can wait in a bounded queue, be rejected, receive a fallback, be redirected, or be processed at lower priority.

More concurrency can *reduce* performance when shared resources are saturated. When throughput falls as concurrency rises, the limit is contention.

## Bounded queues

Queues absorb temporary peaks, but they need a limit.

Avoid a queue with no maximum size. Prefer:

```text
Capacity:      5,000 items
Maximum wait:  10 seconds
```

On reaching the limit, define an explicit policy: reject new items, drop old ones, prioritise critical operations, spill to alternative storage, apply backpressure, or degrade functionality.

## Cellular architecture

Split the system into independent cells:

```text
Customers 1–10,000     → Cell A
Customers 10,001–20,000 → Cell B
Customers 20,001–30,000 → Cell C
```

A failure in one cell affects only part of the users.

## Redundancy

Redundancy can raise availability but adds complexity: multiple instances, zones, regions, read replicas, alternative providers, redundant queues, replicated storage.

Analyse failover, consistency, replication, recovery, cost, shared dependencies, and whether the alternative path is ever exercised.

**Redundancy is useless when every replica depends on the same single point of failure.**
