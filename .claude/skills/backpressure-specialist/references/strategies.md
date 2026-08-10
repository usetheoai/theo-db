# The seven strategies, in full

Read this before recommending any strategy. The summary table in `SKILL.md` omits the costs, and several of these are commonly recommended where they do not apply.

## Contents

1. [Control the producer](#1-control-the-producer)
2. [Buffering](#2-buffering)
3. [Dropping and reducing](#3-dropping-and-reducing)
4. [Rejection and admission control](#4-rejection-and-admission-control)
5. [Scaling](#5-scaling)
6. [Concurrency control](#6-concurrency-control)
7. [Degradation](#7-degradation)
8. [Batching and microbatching](#8-batching-and-microbatching)

---

## 1. Control the producer

The consumer signals how much it can receive or process.

* pause and resume streams;
* request a limited number of items;
* reduce the send rate;
* use credits or permits;
* limit concurrency;
* control the transmission window;
* wait for acknowledgement before sending more.

```text
Consumer: I can take 10 items
Producer: sends 10 items
Consumer: processes them
Consumer: requests more
```

**Benefits.** Reduces the need for large buffers, avoids data loss, protects the consumer, bounds memory use, improves predictability.

**Limits.** The producer often cannot be controlled: user actions, external sensors, third-party systems, market events, public traffic, protocols with no flow control, sources that cannot be paused.

## 2. Buffering

Store data temporarily while the consumer works through earlier items — an in-memory queue, a channel, a message broker, a persistent log, a database queue, a time or size window, an event batch.

### Bounded buffers

Prefer explicit limits:

```text
Maximum capacity: 10,000 events
Maximum age:      30 seconds
Maximum size:     100 MB
```

On reaching the limit the system must apply another policy: block the producer, refuse new input, drop old items, drop new items, consolidate similar items, spill to persistent storage, or degrade.

### Unbounded buffers

Avoid them. An unbounded buffer turns a speed problem into an out-of-memory failure.

Never recommend one without stating the expected maximum capacity, the peak duration, the drain rate, the available memory, the behaviour under saturation and the recovery policy.

## 3. Dropping and reducing

When not every item must be processed, part of the flow can be discarded.

### Drop newest

The incoming item is rejected when the buffer is full. Suitable when older items remain relevant.

### Drop oldest

The oldest item is evicted to admit the newest. Suitable for dashboards, real-time metrics, location, telemetry, UI data — anything where the current state matters more than the history.

### Keep latest only

Intermediate values are discarded: cursor position, a displayed price, a progress percentage, a device's current state, a window's current size.

### Sampling

Process a fraction of events.

```text
Received:  100,000 events per second
Processed: 1 in every 100
Result:    1,000 events per second
```

### Aggregation

Combine many events into a summary: mean per second, count per minute, last value in the interval, min and max, sum per batch, histogram, count per category.

### Deduplication

Remove repeated or equivalent events.

### Coalescing

Merge related updates into one operation — updates A, B, C become final state C.

## 4. Rejection and admission control

When the system is saturated, refusing new work in a controlled way may be preferable to accepting it.

Mechanisms: rate limiting, load shedding, concurrency limits, maximum queue depth, circuit breakers, explicit overload responses, per-client quotas, priority by work type, admission control.

An explicit, fast rejection is often safer than accepting an operation that will probably time out.

## 5. Scaling

More capacity can reduce backpressure: more CPU, more memory, faster disks, more bandwidth, more consumers, partitioned work, parallel processing, multi-region distribution, specialised accelerators.

**Do not treat scaling as an automatic solution.** Before recommending it, check:

* can the work be parallelised?
* is there contention on a shared resource?
* must event order be preserved?
* is a single database the limit?
* will the producer also increase its rate?
* is the cost acceptable?
* will the bottleneck simply move somewhere else?

## 6. Concurrency control

Limit how many operations run at once.

```text
Requests received:   10,000
Maximum in flight:   100
Remainder:           queued, rejected or deferred
```

Excess concurrency causes CPU contention, connection saturation, more context switching, database overload, higher latency, more timeouts and lower overall throughput.

More concurrency does not necessarily mean more performance. When throughput falls as concurrency rises, the limit is contention, not capacity — and scaling will not help.

## 7. Degradation

Keep the essential feature working while secondary ones shrink: reduce update frequency, disable non-essential computation, serve cached data, remove visual detail, process only priority operations.

## 8. Batching and microbatching

Grouping items into a single operation improves efficiency: 500 rows per transaction, 100 messages per call, several changes per frame, events written in batches, files processed in blocks.

**Benefits.** Lower per-operation overhead, better network utilisation, fewer calls, higher throughput.

**Costs.** Higher individual latency, more memory, batches that grow too large, larger blast radius per failure, harder error isolation.
