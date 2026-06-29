# ADR 0008 — No embedding/chat result cache in v1 (YAGNI deferral)

- Status: Accepted
- Date: 2026-06-29
- Deciders: TheoDB core (CTO sign-off: Opção α, 2026-06-27)
- Tags: data-flow, cost, ai-surface, yagni
- Relates to: ADR 0002 (measurement-first), ADR 0007 (synchronous per-row model HTTP)

Technical Story: convert a **decision-by-omission** into an explicit recorded YAGNI.
Derived from `tradeoff_decisions` id 11 (`suggests_adr = 1`). A grep for
`cache`/`memoize`/`materialize` across `sql/` and `theodb_rs/src/` returns nothing, and no
PRD/ADR/feature note mentions caching — so the absence is currently undocumented.

## Context and Problem Statement

`theodb.embed` and the `ai.*` functions re-hit the model endpoint on every call, even for
identical `(content, model)` inputs. Embeddings for a fixed model are deterministic, so a
`(content, model) -> vector` cache would be a legitimate cost/latency optimization. None
exists, and the deferral is recorded nowhere. The risk is not correctness (the stateless
per-call design is sound) but that an intentional trade-off looks like an oversight.

## Decision Drivers

- YAGNI / KISS (parsimony-ladder; CLAUDE.md "Esforço ≠ Complexidade").
- Measurement-first (ADR 0002) — add a cache only when a measured re-embedding cost justifies it.
- Consistency with the `VOLATILE` per-call model (ADR 0007): LLM/chat calls are treated as
  non-deterministic and side-effecting, so caching chat results is semantically wrong;
  caching deterministic embeddings is the only legitimate candidate.
- Honesty (Unbreakable Rule 3) — name the YAGNI so it is a decision, not an accident.

## Considered Options

1. **No cache (stateless per-call)** — status quo; rely on `VOLATILE` semantics.
2. **`(content, model) -> vector` cache table** for deterministic embeddings only.
3. **Memoized chat-result store** for `ai.*` generative calls.

## Decision Outcome

Chosen option: **Option 1 (no cache) for v1**, explicitly. Embeddings and chat results are
treated as `VOLATILE` external calls. Option 2 is the sanctioned future optimization,
gated on a measured re-embedding cost; Option 3 is rejected on semantics (chat is
non-deterministic — caching would silently change behavior).

### Consequences

- Good: no cache-invalidation surface, no stale-vector risk, no new state to manage,
  no premature complexity; fully aligned with the per-call `VOLATILE` contract.
- Bad: repeated identical embeds pay full latency + endpoint cost every time; a
  re-embedding-heavy workload has no relief until the cache lands.
- Re-open trigger: a measured re-embedding cost (duplicate `(content, model)` rate × paid
  endpoint cost/latency) that justifies a `(content, model) -> vector` cache table.

## Pros and Cons of the Options

### Option 1 — no cache
- Good: simplest; no invalidation; no staleness; matches `VOLATILE` design.
- Bad: pays full cost for duplicate deterministic embeds.

### Option 2 — embedding cache table
- Good: cuts cost/latency for duplicate embeds; safe because deterministic.
- Bad: invalidation on model change; extra storage + lookup path; unjustified pre-measurement.

### Option 3 — chat memoization
- Good: would cut duplicate chat spend.
- Bad: chat is non-deterministic/side-effecting — caching changes semantics. Rejected.

## More Information

- Evidence: `sql/30-theodb-embed.sql:1` (schema only post-M17); no cache reference anywhere
  in product source or docs.
- Related findings: tradeoff `undocumented_decision` (medium) — "No embedding/chat cache and
  no record of the deferral".
