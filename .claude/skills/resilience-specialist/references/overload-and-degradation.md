# Overload control and degradation

## Contents

1. [Rate limiting](#rate-limiting)
2. [Load shedding](#load-shedding)
3. [Admission control](#admission-control)
4. [Fallback](#fallback)
5. [Cache as a resilience mechanism](#cache-as-a-resilience-mechanism)
6. [Priority](#priority)
7. [Graceful degradation](#graceful-degradation)
8. [Partial responses](#partial-responses)

---

## Rate limiting

Limits how many operations are allowed in a period. Apply per user, client, token, IP, service, tenant, endpoint, region or operation type.

Common algorithms: fixed window, sliding window, token bucket, leaky bucket.

**Goals.** Protect capacity, ensure fairness, prevent abuse, avoid saturation, reserve resources, control cost, reduce cascading failure.

**Responses** should carry, where appropriate: an indication that the limit was reached, the recommended retry delay, a correlation id, a pointer to the policy, the current limit and the remaining capacity. Clients must respect the indicated interval and add jitter.

## Load shedding

The deliberate rejection of work to keep the system operational.

> It is better to reject some requests quickly than to accept all of them and fail slowly for everyone.

Apply when capacity is exhausted, the queue hit its limit, latency exceeded the objective, critical resources are saturated, or the work will lose its value before it can be processed.

**Strategies.** Reject low-priority traffic; refuse new sessions; limit non-essential features; drop old events; refuse operations already past their deadline; reduce result quality; sample telemetry; serve cached data; serve priority users only.

## Admission control

Before accepting work, check whether there is capacity to finish it.

```text
Is there capacity?
├── Yes → accept
└── No  → reject fast
```

Criteria: current concurrency, queue size, queue age, CPU, memory, available connections, recent latency, error rate, remaining deadline.

## Fallback

An alternative response when the main operation fails: cached data, a default value, omitting the recommendation, a simplified version, a secondary dependency, moving the work to async processing, or reporting partial unavailability.

**A fallback must not** hide critical errors, return dangerously wrong data, violate business rules, turn an explicit failure into silent corruption, overload another dependency, or produce surprising behaviour.

Always signal when the fallback carries stale data, partial information, an estimate, or reduced functionality. A fallback nobody can see is indistinguishable from a healthy system — see the anti-pattern in `observability-and-anti-patterns.md`.

## Cache as a resilience mechanism

| Strategy | Behaviour |
|---|---|
| **Cache-aside** | The application checks the cache and falls back to the origin |
| **Stale-while-revalidate** | Serve stale data while refreshing in the background |
| **Stale-if-error** | Serve stale data when the origin fails |
| **Read-through** | The cache itself loads from the origin |

**Risks.** Stale data, cache stampede, wrong invalidation, over-reliance on the cache, no data during cold start, memory growth, cross-region inconsistency.

### Cache stampede

Many requests try to rebuild the same entry at once. Prevent with single-flight, distributed locks, TTL jitter, early refresh, stale-while-revalidate, concurrency limits, or pre-warming.

## Priority

Classify work by importance — financial operations, authentication, critical reads, profile updates, recommendations, reports, admin tasks, telemetry — and preserve the highest priority under overload.

| Criticality | Prefer |
|---|---|
| **Critical** | Consistency · idempotency · confirmation · controlled timeout · bounded retry · isolation · audit |
| **Important** | Fallback · cache · moderate retry · async processing · persistent queue |
| **Optional** | Drop · reduced quality · temporary disablement · sampling · partial response |

## Graceful degradation

Keep the essential, shrink the secondary.

```text
1. Payments        → preserved
2. Authentication  → preserved
3. Order lookup    → cached data
4. Recommendations → disabled
5. Personalisation → reduced
```

Other shapes: lower-resolution images, search without advanced ranking, a feed without personalisation, a report with delayed data, a dashboard refreshing less often, an operation moved to a queue, a partial response.

## Partial responses

An operation does not always have to fail completely.

```text
Product:         available
Price:           available
Reviews:         unavailable
Recommendations: unavailable
```

Return product and price, omit the rest — and state clearly which data is missing.
