---
slug: docs-features-reality-reconciliation
generated_by: roadmap-feature
status: completed
date: 2026-07-16
milestones: [M105, M106]
---

# Grill — docs/features reconciliation + API hygiene (M105, M106)

Interactive grill SKIPPED: the user supplied a detailed spec (the 3-agent feature audit + explicit corrections),
satisfying the 95%-confidence "detailed spec already written" escape. Derived answers:

- **Why now:** pre-launch, zero users → cheapest window. `docs/features/*.md` promise an AlloyDB-mirror API-alvo; a dev
  copying examples hits `42883 undefined_function` in 04/05/08/09 and much of 12. For an OSS product, docs that lie kill
  first impressions. Low risk (docs), objective (concrete wrong symbols).
- **Split:** M105 = docs-only reconciliation (safe, always-do); M106 = optional code enrichment (canonical ai.rank/rerank
  name + honor `weight`) — non-overlapping (M105 makes docs honest; M106 optionally makes the API richer to match nicer docs).
- **Dependencies:** M105 gated M104 (latest shipped state the audit reflects); M106 gated M105 (docs decision drives the code).
- **DoD:** see the ROADMAP M105/M106 blocks (per-file checklist; grep-verified runnable examples; labeled target-API sections).
- **New risks:** M105 — claiming shipped what isn't (honesty) → grep-verify per example; scope creep → correct+label, don't rewrite.
  M106 — duplicate API surface (2 names) → pick one canonical, deprecate other; weighted-RRF ranking change → weight=1 default + no users yet.
