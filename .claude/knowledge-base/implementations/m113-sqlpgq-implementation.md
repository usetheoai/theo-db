---
slug: m113-sqlpgq-surface
milestone_id: M113
date: 2026-07-17
cycle: implement
---
# M113 — SQL/PGQ-subset surface — implementation summary
## Outcome
`theodb.pgq_match(edge_rel, source_ids, pattern, default_max)` in new module `graph_pgq.rs`: parses the
bounded-path quantifier `*min..max` (subset SQL/PGQ grammar) and returns the node bindings reachable in that hop
range (dispatch to M108 `graph_expand`; min-hop shell via subtracting the `<min` reachable set). DuckPGQ
UDF-minimal — no PG-parser intrusion.
## DoD (ROADMAP M113)
(1) parser-extension SQL/PGQ subset ✅ (pattern quantifier parser → M108/M109 operators, UDF-minimal per DuckPGQ)
(2) subset conformance tested ✅ (`m113_parse_hop_bounds`, `m113_pgq_match_bounded_path`)
(3) end-to-end example in ONE SQL statement ✅ (`m113_pgq_composes_in_one_statement`; `<=>`/`ai.rerank` compose
identically as plain SQL over the bindings)
## Tests (3 M113, GREEN)
m113_parse_hop_bounds, m113_pgq_match_bounded_path, m113_pgq_composes_in_one_statement.
## Honest boundary
Ergonomic SUBSET (bounded reachability), not full SQL/PGQ conformance (path vars, ELEMENT_ID, pattern-WHERE) —
deferred per the milestone ("o mais diferível"). The M110 graph_* functions already serve theo-rag.
