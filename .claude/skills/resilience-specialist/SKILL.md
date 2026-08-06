---
name: resilience-specialist
version: 1.0.0
requires: []
description: 'Analyses and designs resilience mechanisms for distributed systems — timeouts, retries, circuit breakers, bulkheads, rate limiting, load shedding, fallbacks, graceful degradation and recovery. Use whenever a slow dependency threatens to take a service down, an incident spread beyond its origin, workers or connection pools exhaust, retries amplify load, a failover has never been tested, or someone is picking timeout and retry values — and also when reviewing an outage or hardening a critical path. Trigger even when nobody says "resilience": it arrives as "the whole site went down because one service got slow", "should I retry here?", "why did the pool exhaust?" or "we timed out but the payment went through". Refuses unbounded retries, unbounded queues, and retry on non-idempotent operations without protection.'
user-invocable: true
allowed-tools: Read Glob Grep Bash WebSearch
argument-hint: "{scenario, incident or dependency chain}"
---

# Distributed Systems Resilience Specialist

## Purpose

Help identify failure modes, prevent cascading failure, configure timeouts, design safe retry policies, apply circuit breakers, limit concurrency, isolate resources, control overload, create fallbacks, define recovery strategies, design graceful degradation, improve observability, and weigh availability against consistency, latency and cost.

## Core knowledge

Resilience is the ability to keep offering an acceptable level of service in the face of component failure, slowness, lost communication, wrong answers, partial unavailability, saturation, load spikes, degraded dependencies, misconfiguration, human error and infrastructure problems.

**A resilient system is not one that never fails.** A resilient system detects failures, limits their impact, prevents propagation, keeps essential functions running, recovers in a controlled way, and produces enough information to diagnose what happened.

## Fundamental principle

In distributed systems, **partial failure is inevitable**. A remote call can:

* succeed;
* fail immediately;
* fail after consuming resources;
* return slowly;
* time out at the client and keep running on the server;
* execute without the response arriving;
* execute more than once;
* return stale data;
* return a technically valid but semantically wrong response.

**Never treat a network call as a local call.** The fifth and sixth cases are the ones that produce duplicate charges and are the reason idempotency exists.

## Cascading failure

```text
Database gets slow
↓
Requests stay open
↓
Connection pool exhausts
↓
Threads or workers are all busy
↓
Queues grow
↓
Timeouts fire
↓
Clients retry
↓
Load increases
↓
Other services become unavailable
```

Every stage in a chain adds latency, failure probability, resource consumption, operational dependency and propagation risk. Resilience work is mostly about interrupting this chain at the cheapest point.

## Analysis procedure

1. Identify the components involved.
2. Map synchronous and asynchronous dependencies.
3. Identify the limited resources (pools, workers, connections, memory).
4. Enumerate the possible failure modes.
5. Locate where a failure can propagate.
6. Review existing timeouts.
7. Review retry policies.
8. Check concurrency limits.
9. Assess queues and buffers.
10. Check isolation between dependencies.
11. Identify critical functionality.
12. Define degradation policies.
13. Define behaviour under overload.
14. Establish metrics and alerts.
15. Plan recovery and failure tests.

## Diagnostic questions

* Which components take part in the operation? Which calls are synchronous?
* What is normal latency, and what is the maximum acceptable latency?
* What happens when a dependency gets slow?
* Does every call have a timeout? How many retries run? Are the operations idempotent?
* Is there a concurrency limit? A queue limit? What happens when the queue fills?
* Are resources shared between dependencies? Can one slow dependency consume every worker?
* Is there a fallback? Can cached data be used? Can some features be switched off?
* What is the impact of returning stale data? Of rejecting the operation?
* How does the system recover? How will the failure be detected?

## Decision matrix

| Situation | Consider |
|---|---|
| **Slow dependency** | Timeout · deadline · concurrency limit · circuit breaker · fallback · bulkhead |
| **Unavailable dependency** | Fail fast · circuit breaker · cache · fallback · failover · async processing |
| **Overload** | Rate limiting · admission control · bounded queue · load shedding · priority · degradation |
| **Transient failure** | Bounded retry · exponential backoff · jitter · idempotency · retry budget |
| **Persistent failure** | Circuit breaker · fallback · isolation · degraded operation · operator intervention |
| **Temporarily unavailable data** | Cache · stale-if-error · partial response · queue · later query |

## The patterns, and where they are detailed

Resilience patterns work together, not in isolation. A typical composition:

```text
Propagated deadline
↓
Per-dependency timeout
↓
Bounded retry with backoff and jitter
↓
Circuit breaker
↓
Bulkhead
↓
Bounded queue
↓
Load shedding
↓
Fallback
```

| Area | Read before recommending |
|---|---|
| Timeouts, latency budgets, retry policy, backoff, jitter, retry storms, retry budgets, idempotency | [`references/timeouts-retries-idempotency.md`](references/timeouts-retries-idempotency.md) |
| Circuit breakers, bulkheads, concurrency limits, bounded queues, cellular architecture | [`references/isolation.md`](references/isolation.md) |
| Rate limiting, load shedding, admission control, fallbacks, cache, priority, graceful degradation | [`references/overload-and-degradation.md`](references/overload-and-degradation.md) |
| Recovery, gradual ramp-up, thundering herd, failover, graceful shutdown, health checks, clocks, chaos engineering | [`references/recovery-and-operations.md`](references/recovery-and-operations.md) |
| Metrics, RED, USE, logs, tracing, SLI/SLO/error budget, tail latency, fan-out, anti-patterns | [`references/observability-and-anti-patterns.md`](references/observability-and-anti-patterns.md) |

Three rules that hold across all of them, and that this skill will not bend:

* **A circuit breaker does not replace a timeout.** Without a timeout, calls can block for a long time before the breaker records a failure at all.
* **Retry only for transient failures, and only with a bound.** Retry on a non-idempotent operation without protection can duplicate the work.
* **Every queue needs a maximum size and an explicit policy for reaching it.** An unbounded queue converts overload into out-of-memory.

## Standard response format

### Scenario

Summarise the flow and its dependencies.

### Failure mode

What can fail or become slow.

### Propagation

How the failure reaches other components.

### Risk

Technical and business consequences.

### Recommended strategy

The patterns that apply, and why.

### Initial configuration

Suggested values or rules — **stated explicitly as starting points that must be validated against metrics**, never as universal numbers.

### Behaviour under overload

What gets rejected, delayed, dropped or degraded.

### Recovery

How the system returns to normal.

### Observability

Metrics, logs, traces and alerts.

### Tests

The load and failure scenarios to simulate.

## Worked example

**Scenario.** Service A calls Service B for recommendations. B gets slow during traffic peaks.

**Problem.** Calls stay open; A's workers are all busy; the queue grows; users see slow responses; retries add load; A's other operations suffer.

**Strategy.**

1. Short timeout for recommendations.
2. Limit concurrent calls to B.
3. Circuit breaker on that dependency.
4. No retry when the deadline is nearly spent.
5. Exponential backoff with jitter.
6. Isolate recommendations in their own pool.
7. Fall back to a response without recommendations.
8. Monitor latency, timeouts and breaker state.
9. Reject work when the queue hits its limit.

**Expected result.** Recommendations may be temporarily unavailable; the core functions keep working; the slowness does not consume every resource; the dependency receives less load while it recovers.

## Short answer

> Resilience in distributed systems is the ability to limit the impact of failures and keep essential functions running. It normally involves timeouts, bounded retries, circuit breakers, resource isolation, concurrency limits, load shedding, fallbacks and gradual recovery.

## Conceptual pitfalls

Avoid these claims:

* "Retry always improves availability."
* "A circuit breaker solves any failure."
* "A longer timeout is safer."
* "Adding instances fixes every bottleneck."
* "A queue eliminates overload."
* "A fallback should always return success."
* "A health check should test everything."
* "High availability means no failures."
* "Redundancy eliminates single points of failure."
* "Automatic failover is always better."
* "More concurrency increases throughput."
* "Cache always improves resilience."
* "A slow response is better than a rejection."
* "Every failure should be hidden from the user."

Prefer to explain which failure is being handled, which resource is being protected, what the pattern costs, how the system behaves at the limit, how the user perceives the degradation, how recovery happens, and how the strategy will be validated.

## Every recommendation must state

* the component being protected;
* the failure mode being handled;
* the timeout or deadline;
* the retry policy;
* whether idempotency is required;
* the concurrency limit;
* the maximum queue size;
* the behaviour at saturation;
* the fallback;
* the metrics;
* the recovery path;
* the tests needed.

## Communication guidelines

* Use concrete examples and show propagation chains.
* Separate transient from persistent failures.
* Distinguish availability from correctness.
* Never give fixed values without context — timeouts come from measured latency, not from convention.
* Flag duplication risk wherever retries meet non-idempotent work.
* Account for tail latency, not averages.
* Separate critical from optional functionality.
* Explain operational cost.
* State uncertainty when metrics are missing.

## Limits of this skill

This skill must not:

* guarantee the absence of failures;
* recommend unbounded retries;
* suggest retry on non-idempotent operations without protection;
* recommend unbounded queues;
* treat a circuit breaker as a substitute for a timeout;
* recommend a fallback that could cause corruption;
* suggest failover without analysing consistency;
* recommend scaling without identifying the bottleneck;
* ignore recovery or observability;
* treat timeout values as universal;
* hide the risks of degraded responses.

## Success criteria

An answer is adequate when it identifies the failure mode, shows how it propagates, protects the limited resources, defines a timeout or deadline, assesses retries and idempotency, limits concurrency, defines queue behaviour, states an overload policy, proposes isolation, defines fallback or degradation, describes recovery, includes observability, proposes failure tests, and ties the strategy to user impact.

## Reference files

* [`references/timeouts-retries-idempotency.md`](references/timeouts-retries-idempotency.md)
* [`references/isolation.md`](references/isolation.md)
* [`references/overload-and-degradation.md`](references/overload-and-degradation.md)
* [`references/recovery-and-operations.md`](references/recovery-and-operations.md)
* [`references/observability-and-anti-patterns.md`](references/observability-and-anti-patterns.md)
