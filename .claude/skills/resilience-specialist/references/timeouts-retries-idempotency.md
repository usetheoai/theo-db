# Timeouts, retries and idempotency

## Contents

1. [Timeouts](#timeouts)
2. [Latency budget and deadlines](#latency-budget-and-deadlines)
3. [The timeout that does not stop the server](#the-timeout-that-does-not-stop-the-server)
4. [Retry](#retry)
5. [Backoff and jitter](#backoff-and-jitter)
6. [Retry storms and retry budgets](#retry-storms-and-retry-budgets)
7. [Idempotency](#idempotency)

---

## Timeouts

A timeout is the maximum time an operation may wait. Every remote call needs one, and it must be coherent with the operation's total budget.

Avoid: *the client waits indefinitely.*
Prefer: *the client waits up to a defined limit → the operation is cancelled or marked expired → resources are released.*

### Types

| Type | Bounds |
|---|---|
| **Connection** | Establishing the connection |
| **Read** | Waiting for data after the connection exists |
| **Write** | Sending data |
| **Request** | The whole request |
| **Idle** | Time allowed with no activity |
| **Deadline** | The absolute instant by which the whole operation must finish |

## Latency budget and deadlines

A dependency's timeout must respect the total time available.

```text
Total budget: 2 seconds

Gateway:     100 ms
Service A:   300 ms
Service B:   500 ms
Database:    700 ms
Safety margin: 400 ms
```

**Do not give every dependency a 2-second timeout** — the full chain would then exceed the total budget.

Prefer propagating deadlines:

```text
Original deadline: 2 seconds
Already consumed:  800 ms
Remaining:         1.2 seconds
```

The next dependency receives only the remaining time.

## The timeout that does not stop the server

A client-side timeout does **not** guarantee the server stopped working.

```text
Client sends payment
↓
Server processes payment
↓
Response is slow
↓
Client hits its timeout
↓
Client does not know whether the payment happened
```

Retry without idempotency here duplicates the operation. Always analyse cancellation, idempotency, the unknown-state case, later confirmation, status queries and compensation.

## Retry

Use retry only when the failure is probably temporary, the operation can be safely repeated, there is a bound on attempts, there is time left in the deadline, the dependency has a real chance of recovery, and the retry will not cause dangerous duplication.

**Appropriate for retry:** transient network failure, connection reset, an overload response indicating a later attempt, brief unavailability, temporary leader election, transient conflict, an occasional timeout — depending on the operation.

**Not appropriate:** authentication error, permission denied, invalid payload, permanently missing resource, rejected business rule, validation failure, non-idempotent operation without protection, permanent lack of capacity, misconfiguration.

## Backoff and jitter

Increase the interval between attempts:

```text
Attempt 1: immediate
Attempt 2: after 200 ms
Attempt 3: after 400 ms
Attempt 4: after 800 ms
Attempt 5: after 1,600 ms

delay = base × 2^attempt      (with a maximum)
```

**Jitter** adds randomness. Without it:

```text
10,000 clients fail at the same moment
↓
all wait 1 second
↓
all retry at the same moment
```

With jitter, clients spread their retries across an interval, which removes the synchronised spike.

## Retry storms and retry budgets

A retry storm happens when new attempts add load to an already degraded system:

```text
Original load: 10,000 rps
First retry:  +10,000 rps
Second retry: +10,000 rps
Potential:     30,000 rps
```

Prevent with exponential backoff, jitter, an attempt cap, a retry budget, a circuit breaker, rate limiting, deadlines and load shedding.

A **retry budget** caps the share of traffic spent on retries:

```text
Original requests: 10,000
Retry allowance:   10%
Maximum extra:     1,000
```

**Retry at every layer multiplies.** Gateway 3 × Service A 3 × Service B 3 = up to 27 executions of one request. Decide which layer owns the retry.

## Idempotency

An idempotent operation can be repeated without additional unwanted effects.

* Idempotent: *set order status to CANCELLED*.
* Potentially not: *charge R$ 100*.

To protect non-idempotent operations: idempotency key, unique operation identifier, deduplication, result recording, transactional outbox, inbox pattern, version control, status query.
