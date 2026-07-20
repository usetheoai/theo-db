# Dogfood manifest — TheoDB

Source of truth for the production-readiness anchor. `/dogfood` reads this + `rules/dogfood-golden-rule.md` and
emits EVIDENCE_SUFFICIENT / EVIDENCE_WITH_CAVEATS / EVIDENCE_INSUFFICIENT. `running` is the only status that
satisfies the v1.0 / production-ready claim.

## Anchor scenario

**Slug:** `theo-data-capability-on-theodb`

**Status:** `planned`

A real theo-data capability (`theo-rag` or `theo-memory`) uses a self-hosted TheoDB instance the team owns as its
live retrieval store — declarative vectorizer keeping an embedding column fresh + `ai.hybrid_search_rrf` serving
the capability's real queries — on infra the team runs, for a sustained ≥ 30-day window. See
`rules/dogfood-golden-rule.md § 1` for the full anchor definition and the rationale.

## Honest status rationale (why `planned`, not `running`)

As of 2026-07-20 all recorded evidence is **synthetic benchmarks** (109 artifacts under `docs/benchmarks/` —
recall/QPS/latency/significance). There is **no sustained real-use evidence**: no theo-data capability yet runs
its production retrieval on a self-hosted TheoDB the team depends on. Setting this anchor to `running` without
that evidence would be exactly the dogfood-theatre the gate exists to prevent (§ 7). So it is honestly `planned`:
the gate is now IN PLACE and the bar is explicit, but it is NOT yet satisfied.

**Consequence (Unbreakable Rule 3 + `rules/public-copy.md § 3`):** until this anchor reaches `running` with fresh
evidence, TheoDB MUST NOT be described as `production-ready` / `production-grade` / `battle-tested` in any
external copy. Permitted framings: "designed for production HA scenarios", "targeted at <use case>". The 109
benchmarks justify algorithm/feature claims (recall parity, billion-scale memory, measured latency), NOT a
production-ready claim.

## Path to `running` (what would satisfy the gate)

1. Stand up a self-hosted TheoDB the team owns (this repo builds the engine + extension; a deploy target is
   needed — HA/control-plane are out of this repo's scope by design).
2. Point one theo-data capability's retrieval at it (vectorizer + `ai.hybrid_search_rrf`), replacing its current
   store.
3. Run the team's own product traffic against it for ≥ 30 days; log evidence files under
   `knowledge-base/dogfood/evidence/` per the § 5 frontmatter (scenario / date / operator / outcome / summary),
   including at least one **failure story** (a dogfood with no failures is theatre — § 4).
4. Flip status → `running` once the sustained evidence exists; then `/dogfood` can emit EVIDENCE_SUFFICIENT and a
   production-ready claim becomes defensible.

## Evidence

Evidence files: `knowledge-base/dogfood/evidence/*.md` (currently none for this anchor — status `planned`).
