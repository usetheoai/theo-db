# Dogfood Golden Rule

Locked contract that `/dogfood` reads to evaluate whether a project may legitimately claim `production-ready` / `v1.0`. **This file is a template — each project edits the marked sections to declare its own anchor scenario.**

Without this file, `/dogfood` emits `EVIDENCE_INSUFFICIENT` with flag `golden_rule_missing`.

## § 1 — Anchor scenario (PER-PROJECT — EDIT THIS)

The anchor scenario is the single use case that, if you cannot dogfood it, you cannot claim production-ready. Pick one. Be specific.

**Slug:** `theo-data-capability-on-theodb` (kebab-case identifier referenced in the manifest)

**Description:** A real theo-data capability (e.g. `theo-rag` or `theo-memory`) uses a **self-hosted TheoDB instance the team owns** — NOT pgvector, NOT a managed vector DB — as its live retrieval store, exercised by the team's own product traffic (not synthetic load) for a sustained window (≥ 30 days). Concretely: the capability declares a vectorizer (`theodb.create_vectorizer`) so the background worker keeps an embedding column fresh as content changes, and it serves the capability's real user queries through `ai.hybrid_search_rrf` (BM25 + own-vector + RRF), on infrastructure the team runs, with the team depending on the results being correct and fresh.

**Why this scenario:** TheoDB's primary promise is an **open-source, PostgreSQL-compatible, AI-native** database — embeddings, a declarative vectorizer, and hybrid search **inside SQL**, on your own infra, model-agnostic. This scenario exercises that entire promise end-to-end under real load: the own `vector` type + `ai.hybrid_search_rrf` (the AI-native surface), the vectorizer bgworker (the operability surface M122 hardened), and the crash-safe job queue — all at once, with a team that depends on it. If TheoDB cannot back a theo-data capability's own retrieval on our own infra, the "AI-native OSS DB you can self-host" claim is unproven, no matter how many benchmarks pass. Synthetic recall/QPS numbers (the 109 benchmark artifacts) prove the algorithms; only this proves the product.

## § 2 — Status vocabulary (LOCKED — do not change without ADR)

The `Status` field in `knowledge-base/dogfood/manifest.md` MUST take one of these values:

| Status | Meaning |
|---|---|
| `planned` | Anchor is identified but no implementation work has started. |
| `wired` | Implementation lands; the anchor is invoked at least once in CI or a manual smoke. |
| `running` | The anchor is **actively used by the team on real infrastructure**. This is the bar for v1.0. |
| `paused` | Was `running`; explicitly stopped for a documented reason. NOT a degradation of `running`. |
| `abandoned` | Anchor is no longer pursued. Requires ADR to set. |

`/dogfood` accepts `running` as the only value satisfying hard cap #2 (`anchor_not_running`).

## § 3 — Hard caps (LOCKED)

In order; first failure short-circuits to `EVIDENCE_INSUFFICIENT`.

| # | Check | Flag |
|---|---|---|
| 1 | Manifest contains a section identifiable by `Slug` or anchor header | `anchor_missing` |
| 2 | `Status` matches the running value declared in § 2 | `anchor_not_running` |
| 3 | At least one evidence file under `knowledge-base/dogfood/evidence/` has frontmatter `scenario:` matching the anchor slug | `no_anchor_evidence` |
| 4 | The most recent matching evidence file (by frontmatter `date:`) is within the freshness threshold below | `anchor_evidence_stale` |

**Freshness threshold (PER-PROJECT — EDIT THIS):** `30 days` by default. Reduce for fast-moving products; never raise without ADR.

## § 4 — Soft caps (PER-PROJECT — EDIT OR EXTEND)

Soft caps cap the verdict at `EVIDENCE_WITH_CAVEATS`. They fire when hard caps pass but evidence is thin.

| Soft cap | Default | Rationale |
|---|---|---|
| Total evidence count for the anchor | ≥ 3 | Single evidence point is not a trend. |
| Failure stories present | ≥ 1 | A dogfood without failures is theatre. |
| Evidence from ≥ 2 different operators | recommended | Avoid "the one person who knows how" syndrome. |

## § 5 — Evidence file frontmatter (LOCKED)

Every file under `knowledge-base/dogfood/evidence/` MUST have YAML frontmatter:

```yaml
---
scenario: <slug>        # matches the anchor slug or a declared sibling
date: YYYY-MM-DD        # local date of the dogfood run
operator: <name>        # who ran it
outcome: pass | partial | fail
summary: <one line>
---
```

Missing any field → evidence file ignored by hard cap #3.

## § 6 — When this rule may change

Per `cycle-rule-schema.md § Golden Rule Change Protocol`, plus one extra requirement:
sign-off from at least one operator who has logged anchor evidence. Rule-specific
deviations:

- Changing the anchor slug = abandoning the previous anchor (requires ADR).
- Loosening the freshness threshold = downgrading the gate (requires ADR).
- Adding a new `Status` value or new hard cap = expanding the contract (requires ADR).

## § 7 — Failure modes the rule guards against

- "Production-ready" claim backed only by synthetic benchmarks.
- Silently swapping the anchor when the original becomes inconvenient.
- Aging evidence (dogfood worked 6 months ago; nothing since).
- Single-operator knowledge (only one person can actually run the anchor).
- Dogfood theatre — checking the box without using the product.
