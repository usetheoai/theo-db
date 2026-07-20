---
scenario: theo-data-capability-on-theodb
date: 2026-07-20
operator: paulohenriquevn
outcome: partial
summary: Two real failure modes surfaced running the anchor on self-hosted TheoDB — the async vectorizer worker dead-letters all embed jobs (session embed works; issue #132), and create_vectorizer does not backfill pre-existing rows.
---

# Anchor failure modes — self-hosted TheoDB (the mandatory failure story, § 4)

A dogfood with no failures is theatre (`rules/dogfood-golden-rule.md § 4`). Running the M124 anchor smoke on a
self-hosted TheoDB surfaced two real operational gaps — exactly what dogfooding is for.

## Failure mode 1 (MEDIUM) — the async vectorizer worker cannot embed on self-host → issue #132

**Observed:** after `create_vectorizer` + INSERT, all 5 embed jobs dead-lettered to `state = 'failed'`
(attempts = 5, `last_error = 'embed/upsert failed'`) after ~a minute. The embedding column stayed NULL.

**Discriminator (isolates the worker path):** with the SAME instance-level GUCs
(`length(current_setting('theodb.embedding_api_key'))` = 164 in a fresh session), both `theodb.embed('…')` and
`theodb.embed_batch(ARRAY[…])` **succeed** in a normal session (return real 1536-d vectors), but the **background
worker's** embed step **fails** every time. So the key/endpoint/model/TLS/both embed paths are correct — the
delta is the bgworker execution context (prime suspect: the worker's view of the placeholder `theodb.embedding_*`
GUCs, or a worker-side HTTP init; `last_error` is a generic wrapper that hides the root cause).

**Impact:** the "vectorizer keeps embeddings fresh" half of the anchor is blocked on self-host until fixed. The
**query** path is unaffected — a session `theodb.embed` backfill populates the column and `ai.hybrid_search_rrf`
works (see `2026-07-20-anchor-smoke.md`).

**Filed:** [#132](https://github.com/usetheodev/theo-db/issues/132) with the full repro + the discriminator.
**Workaround (documented in the quickstart Troubleshooting):** session backfill
`UPDATE docs SET embedding = theodb.embed(body)::vector WHERE embedding IS NULL;`.

## Failure mode 2 (LOW) — create_vectorizer does not backfill pre-existing rows

**Observed:** loading rows BEFORE `create_vectorizer` left the queue empty (the ON-DML trigger only fires on
INSERT/UPDATE/DELETE *after* the vectorizer exists) — so pre-existing rows are never embedded and the column stays
NULL forever, silently.

**Impact:** an operator who loads content then adds a vectorizer gets an all-NULL embedding column with no error.

**Mitigation (documented in the quickstart):** create the vectorizer **before** loading, or re-touch existing
rows (`UPDATE … SET body = body`) to enqueue them. This is arguably a docs/UX gap, not a code bug (the trigger
semantics are correct); left as a quickstart note rather than a separate issue.

## Why this is honest `wired`, not `running`

The anchor path is exercised on self-hosted infra and the query path is proven with real embeddings — but two
operational gaps mean a capability could not yet depend on the async freshness in production. Recording these (and
filing #132) is the dogfood working as intended: surfacing what synthetic benchmarks never would. The
production-ready claim stays unmade until `running` (sustained real use) with these resolved.
