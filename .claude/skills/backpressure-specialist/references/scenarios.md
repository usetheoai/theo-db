# Worked scenarios

Six recurring shapes, each with the arithmetic that makes the deficit visible.

## Contents

1. [File read and write](#file-read-and-write)
2. [Service to service](#service-to-service)
3. [Many producers, one consumer](#many-producers-one-consumer)
4. [User interface](#user-interface)
5. [Keyboard input](#keyboard-input)
6. [Telemetry](#telemetry)

---

## File read and write

```text
Read:    150 MB/s
Write:   100 MB/s
Deficit:  50 MB/s
```

Processing a 6 GB file:

```text
Approximate read time: 40 seconds
Accumulation:          50 MB/s × 40
Approximate buffer:    2 GB
```

**Problem.** Reading the whole file before writes catch up consumes excessive memory.

**Preferred solution.** Process in chunks; pause reading when writing saturates; use streams; wait for drain before continuing; set buffer limits.

## Service to service

```text
Service A sends:    100 requests per second
Service B processes: 75 requests per second
Deficit:             25 requests per second
```

**Options.** Reduce A's rate; limit concurrency; respond with an overload signal; use a bounded queue; add consumers; apply delayed retry; reject low-priority work.

Avoid immediate, unbounded retries — they add load precisely when the system has none to spare.

## Many producers, one consumer

```text
Service A ─┐
Service B ─┼─→ Service Z
Service C ─┘
```

If A generates excessive load, Z must stop a single producer from consuming the whole capacity.

**Apply.** Per-producer quotas; separate queues; concurrency limits; fair scheduling; priorities; resource isolation; bulkheads.

The goal is to prevent one producer causing unavailability for everyone else.

## User interface

**Scenario.** A WebSocket delivers thousands of events per second, but the interface can only update tens of times per second.

**Options.** Render at the browser's rhythm; group events per frame; show only the latest value; aggregate; use virtualisation; cap update frequency; separate ingestion from visualisation; store data for later querying.

Do not attempt to render every event where it brings the user no benefit.

**Concrete shape.** Given 20,000 updates per second into a dashboard:

```text
Ingestion:   20,000 events per second
Aggregation: continuous
Interface:   update every 500 ms
```

Keep the latest value per entity, batch updates, update on the render cycle, virtualise, cap visual frequency, and move full history to separate storage. The system preserves the useful information without trying to render each event.

## Keyboard input

Keystrokes can generate excessive work, such as search queries.

**Apply.** Debounce; cancel superseded queries; cache; limit concurrency; process only the latest value.

## Telemetry

Sensors can generate more data than the system can store.

**Apply.** Sampling; aggregation; compression; partitioning; retention by priority; dropping redundant events.
