# Streams, queues and retries

## Contents

1. [Pull streams](#pull-streams)
2. [Push streams](#push-streams)
3. [Push-pull hybrids](#push-pull-hybrids)
4. [Reactive Streams](#reactive-streams)
5. [Queues and brokers](#queues-and-brokers)
6. [Retries](#retries)

---

## Pull streams

The consumer requests data according to its capacity.

```text
Consumer → requests an item
Producer → delivers it
Consumer → processes it
Consumer → requests the next
```

Batched demand also exists: `request(100)` authorises up to 100 items.

**Advantages.** Natural demand control, predictable memory use, fewer buffers, good alignment between production and consumption.

**Limits.** Not every source can be controlled; latency can rise; it requires a demand protocol; coordination adds overhead.

## Push streams

The producer sends data as it becomes available, and the consumer does not directly control production.

In these cases you may need buffering, dropping, sampling, aggregation, subscription cancellation, source limiting, or moving processing to another component.

## Push-pull hybrids

Some systems combine asynchronous delivery with credit control, demand windows, processing acknowledgement and batched requests.

**Do not classify a technology merely because it is asynchronous.** Ask who controls how much data is in flight.

## Reactive Streams

The model, when it is relevant:

* a consumer subscribes to a producer;
* the consumer declares demand;
* the producer must not send more items than requested;
* cancellation and errors are part of the protocol;
* demand is usually expressed as a number of items.

```text
Subscriber: request(10)
Publisher:  sends up to 10 items
Subscriber: processes
Subscriber: request(10)
```

**Do not claim every reactive library provides backpressure.** Some reactive abstractions are push-only and depend on operators instead: debounce, throttle, buffer, sample, audit, window, drop.

## Queues and brokers

A queue does not eliminate backpressure. It moves where the accumulation happens.

When analysing a queue, check: maximum size, persistence, retention, publication rate, consumption rate, number of consumers, maximum wait time, drop policy, dead-letter queue, reprocessing, ordering, duplication, and storage cost.

A queue can be healthy for temporary peaks and still grow without bound when demand permanently exceeds capacity.

## Retries

Badly configured retries amplify backpressure.

**Avoid:** immediate retry; infinite retry; every client retrying at the same moment; retrying non-idempotent operations; retry with no time limit; retrying permanent errors.

**Prefer:** exponential backoff; jitter; a retry cap; a retry budget; circuit breaker; timeouts; idempotency; a dead-letter queue; retrying only transient failures.
