---
slug: m113-sqlpgq-surface
milestone_id: M113
date: 2026-07-17
cycle: review
---
# /review — M113 SQL/PGQ-subset surface
**Verdict:** READY_TO_MERGE
- `theodb.pgq_match` parses the `*min..max` quantifier + dispatches to M108 traversal (DuckPGQ UDF-minimal). REVOKE'd.
- Bounded-path semantics correct (min-hop shell via subtract-`<min`); parser handles `*`, `*N`, `*N..M`, `*..M`, bare edge; rejects min>max.
- Composability gate met (one-statement MATCH + aggregate; `<=>`/`ai.rerank` compose as plain SQL over bindings).
- Honest scope: subset, not full conformance (deferred per milestone). No new crate. edge_rel is `%s`-spliced but
  the caller already needs the relation; consistent with M108 `graph_expand` (identifier via regclass at call).
## Hard gates
no BLOCKER; no secrets; no main commit; CHANGELOG updated; full suite GREEN 0 regression.
