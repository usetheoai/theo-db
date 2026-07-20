# Dogfood manifest — TheoDB

Source of truth for the production-readiness anchor. `/dogfood` reads this + `rules/dogfood-golden-rule.md` and
emits EVIDENCE_SUFFICIENT / EVIDENCE_WITH_CAVEATS / EVIDENCE_INSUFFICIENT. `running` is the only status that
satisfies the v1.0 / production-ready claim.

## Anchor scenario

**Slug:** `theo-data-capability-on-theodb`

**Status:** `wired`

A real theo-data capability (`theo-rag` or `theo-memory`) uses a self-hosted TheoDB instance the team owns as its
live retrieval store — declarative vectorizer keeping an embedding column fresh + `ai.hybrid_search_rrf` serving
the capability's real queries — on infra the team runs, for a sustained ≥ 30-day window. See
`rules/dogfood-golden-rule.md § 1` for the full anchor definition and the rationale.

## Honest status rationale (why `wired`, not yet `running`)

**M124 (2026-07-20) advanced this anchor from `planned` → `wired`:** the anchor path is now exercised on a
self-hosted TheoDB — `theodb.create_vectorizer` + the vectorizer worker + `ai.hybrid_search_rrf`, driven by
`benchmarks/dogfood_anchor_smoke.sh`, with the QUERY path proven end-to-end using **real** OpenAI embeddings
(`evidence/2026-07-20-anchor-smoke.md`). A reproducible self-host recipe exists at
`docs/ops/self-host-quickstart.md`. This meets the `wired` bar (§ 2: "the anchor is invoked at least once in a
manual smoke").

It is **not** `running`: the rest of the evidence is still **synthetic benchmarks** (109 artifacts under
`docs/benchmarks/`) and this run is a smoke, not sustained real product traffic. The dogfood already earned its
keep — it surfaced two real gaps (`evidence/2026-07-20-anchor-failure-modes.md`): the async vectorizer worker
dead-letters embeds on self-host (issue #132) and `create_vectorizer` does not backfill pre-existing rows.
Setting this to `running` now would be exactly the dogfood-theatre the gate exists to prevent (§ 7).

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

Evidence files under `.claude/knowledge-base/dogfood/evidence/` for this anchor:

- `2026-07-20-anchor-smoke.md` — outcome `pass`: the QUERY path proven on self-hosted TheoDB with real embeddings.
- `2026-07-20-anchor-failure-modes.md` — outcome `partial`: two real failure modes (worker embed → #132; no backfill).

Enabler (M124): `docs/ops/self-host-quickstart.md` + `benchmarks/dogfood_anchor_smoke.sh`. The remaining step to
`running` is operational/cross-repo: a theo-data capability migrating its production retrieval here for ≥30 days.
