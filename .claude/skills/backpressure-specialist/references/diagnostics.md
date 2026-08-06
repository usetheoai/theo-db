# Diagnostics: metrics, Little's Law, anti-patterns

## Contents

1. [Metrics](#metrics)
2. [Little's Law](#littles-law)
3. [Anti-patterns](#anti-patterns)
4. [Distinctions from neighbouring concepts](#distinctions-from-neighbouring-concepts)
5. [User experience](#user-experience)

---

## Metrics

A backpressure analysis should consider: input rate, output rate, queue depth, **age of the oldest item**, wait time, processing latency, CPU use, memory use, in-flight operations, busy connections, drop rate, rejection rate, retry count, timeouts, overload errors, consumer lag, throughput per partition, drain rate.

### Reading the signals

A large queue is not automatically a problem. The trend matters more than the depth:

```text
Queue stable            → the system may be healthy
Queue oscillating       → it may be absorbing peaks
Queue growing steadily  → capacity is insufficient
Oldest-item age rising  → work is not draining
```

Age is the metric people most often omit and most often need. Throughput can hold perfectly steady while the oldest item gets older every minute — a system quietly falling behind while its dashboard looks fine.

## Little's Law

```text
L = λ × W
```

* **L** — average number of items in the system
* **λ** — average arrival rate
* **W** — average time in the system

```text
Rate:            100 requests per second
Average latency: 2 seconds
In flight:       200 items
```

Useful for estimating queue sizes, required concurrency, the effect of rising latency, and in-flight capacity.

**Do not apply it mechanically** when the system is not in a reasonably steady state.

## Anti-patterns

### Unbounded buffer

```text
Fast producer → infinite queue → slow consumer
```

Consequence: failure through memory or storage exhaustion.

### Retry storm

```text
Service fails
↓
Thousands of clients retry
↓
Load increases
↓
The service keeps failing
```

### Unbounded concurrency

Spawning one parallel operation per incoming item saturates shared resources.

### Timeout without cancellation

The client abandons the operation, but the server keeps processing work that no longer has value.

### Queue without priority

Critical work sits behind a large volume of unimportant operations.

### Acceptance without limit

The system accepts more work while knowing it cannot finish it within the deadline.

### Monitoring throughput only

Stable throughput hides a queue whose age climbs continuously. See [Metrics](#metrics).

## Distinctions from neighbouring concepts

**Rate limiting** controls how many operations are permitted within an interval. Backpressure is about how the system reacts when processing capacity is below the input rate. A rate limiter can be used as a mechanism to prevent or control backpressure.

**Throttling** reduces or caps execution or send speed. It can be used to control the producer and protect the consumer.

**Debounce** waits for a quiet period before acting. Suitable for search fields, window resizing, rapid input — anywhere intermediate updates can be ignored.

**Sampling** selects only part of the events. Suitable when the full volume is unnecessary and an approximation suffices.

**Load shedding** rejects or drops work to keep the system operational during overload. It is a deliberate service-preservation policy.

### Related concepts worth connecting

Flow control · rate limiting · load shedding · circuit breaker · bulkhead · queues · streams · reactive systems · concurrency · elasticity · batching · debounce · throttle · sampling · retries · timeouts · idempotency · scalability · observability · capacity planning.

## User experience

Do not recommend processing every event without checking whether it improves the user's experience.

* Can the user perceive every update?
* Does updating more often improve the decision?
* Can the information be summarised?
* Is showing the latest state enough?
* Would one update per second be better?
* Should the delay be shown?
* Should the system signal that it is under load?

A table does not need updating 100,000 times per second, even when the system can receive that many events.

## Pseudocode

**Controlled producer**

```text
while there is data:
    wait for the consumer to request capacity
    amount = consumer.available_capacity()
    send up to `amount` items
```

**Bounded buffer**

```text
on receiving an item:
    if the queue is not full:
        add the item
    else:
        apply the overload policy
```

**Drop oldest**

```text
on receiving an item:
    if the queue is full:
        remove the oldest item
    add the new item
```

**Keep latest state**

```text
on receiving an update:
    state[identifier] = update

periodically:
    render the current values of state
```

**Concurrency limit**

```text
if operations_in_flight < limit:
    start the operation
else:
    queue or reject
```
