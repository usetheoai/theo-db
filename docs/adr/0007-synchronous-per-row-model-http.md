# ADR 0007 — Synchronous per-row model HTTP calls (embed + ai.*), with batch/async deferred

- Status: Accepted
- Date: 2026-06-29
- Deciders: TheoDB core (CTO sign-off: Opção α, 2026-06-27)
- Tags: data-flow, scaling, ai-surface, alloydb-compat
- Supersedes / relates to: ADR 0006 (own-code Rust/Go), ADR 0005 (unification moat)

Technical Story: durable record for the synchronous-per-row model-call design that is
today documented only in SQL function COMMENTs (`sql/theodb--1.0.sql:89-90`,
`sql/50-theodb-ai.sql:86-90`, `theodb_rs/src/lib.rs:55`). Derived from
`tradeoff_decisions` id 10 (`suggests_adr = 1`) and scaling findings (N+1 critical,
blocking-IO high).

## Context and Problem Statement

`theodb.embed(text)` and every `ai.*` generative function (`ai.generate`, `ai.summarize`,
`ai.if`, `ai.rank`, `ai.analyze_sentiment`) issue **one blocking outbound HTTPS round-trip
per row**. They are marked `VOLATILE` so the planner does not fold a single call across N
rows. A `SELECT ai.generate(col) FROM t` therefore fans out to N sequential 30s-timeout
HTTP calls, each holding one PostgreSQL backend for the full model latency. There is no
ADR recording this as a deliberate architectural choice, its scaling boundary, or the
async/batch deferral — only inline comments, which are not a durable decision record.

## Decision Drivers

- AlloyDB API compatibility — `embedding()` / `ai.generate()` are per-row SQL functions; the
  surface must match for migration parity (the north-star metric in ADR 0005).
- Correctness — `VOLATILE` prevents the planner broadcasting one model result over N rows.
- Honesty (Unbreakable Rule 3) — the known scaling footgun must be a recorded decision, not
  tribal knowledge in a comment.
- Measurement-first (ADR 0002) — async/queue machinery is essential complexity only once a
  measured bottleneck justifies it.

## Considered Options

1. **Synchronous one-call-per-row + `VOLATILE`** (status quo), with `ai.generate_batch`
   (N prompts → 1 round-trip) as the only accelerator; broad batch/async deferred.
2. **Async / queued embedding** via a background worker (de-couple the backend from model
   latency).
3. **Client-side batching by default** (caller chunks input before calling).

## Decision Outcome

Chosen option: **Option 1**, because it is the only one that preserves AlloyDB-compatible
per-row semantics and planner correctness at current (pre-GA, single-node) scale. The
async/queue path (Option 2) is **explicitly deferred** until a measured backend-exhaustion
bottleneck under realistic fan-out justifies it.

Two follow-ups are accepted as part of this decision (tracked separately):

- **DELIVERED (audit-remediation, 2026-06-29):** `theodb.embed_batch(text[]) RETURNS vector[]`
  mirroring `ai.generate_batch` — the embeddings endpoint accepts an input array, so this is a
  low-effort, high-leverage mitigation that closes the embed N+1 (the most common bulk operation)
  with no design change. Shipped with a reproducible N→1 latency benchmark
  (`docs/benchmarks/audit-remediation-embed-batch.md`). A bounded recoverable-class retry was also
  added to the embed client + `ai._chat` (closing the no-retry consequence below).
- Document a recommended `LIMIT` / batch-size in each function COMMENT (the embed warning
  exists; `ai.generate` has none) and consider a server-side concurrency-cap GUC.

### Consequences

- Good: AlloyDB parity preserved; planner correctness guaranteed; no premature async
  infrastructure; blast radius bounded by `REVOKE ... FROM PUBLIC`.
- Bad: a column-wide call ties up one backend per row for the sum of per-row latencies, so
  `max_connections` (not CPU/RAM) is the first vertical wall under fan-out. (The single-transient-
  failure-aborts-the-statement consequence was mitigated in audit-remediation by a bounded
  recoverable-class retry; `embed_batch` collapses the bulk-embed N+1 to one round-trip.)
- Re-open trigger: a measured bulk-embedding / bulk-generate workload that exhausts
  backends, OR a customer running embeds over large corpora in one statement.

## Pros and Cons of the Options

### Option 1 — synchronous per-row
- Good: AlloyDB-compatible; correct under `VOLATILE`; zero new infrastructure.
- Bad: backend-per-row occupation; N+1 on bulk operations without a batch entry point.

### Option 2 — async / queued
- Good: de-couples backend lifetime from model latency; natural home for circuit-breaker.
- Bad: large new component (worker, queue, status surface); not justified pre-measurement;
  breaks the synchronous SQL contract callers expect.

### Option 3 — client-side batching by default
- Good: fewer round-trips.
- Bad: pushes complexity to every caller; diverges from the AlloyDB call shape.

## More Information

- Evidence: `theodb_rs/src/embed.rs:43-58`, `sql/50-theodb-ai.sql:64`,
  `sql/theodb--1.0.sql:89-90`.
- Related findings: scaling `n_plus_one_query` (critical), `blocking_io_in_hot_path`
  (high), data-flow `missing_backpressure` / `missing_retry_policy` (medium).
