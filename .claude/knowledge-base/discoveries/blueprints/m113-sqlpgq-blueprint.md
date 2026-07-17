---
slug: m113-sqlpgq-surface
milestone_id: M113
date: 2026-07-17
cycle: discover
verdict: SHIPPABLE_WITH_CAVEATS
---
# M113 Blueprint — SQL/PGQ-subset surface (DuckPGQ UDF-minimal)
Prior art: DuckPGQ (CIDR/VLDB 2023 — SQL/PGQ mapped to logical plan via UDFs, minimal planner intrusion),
SQL/PGQ SQL:2023 (arXiv 2505.07595), ADR-0048. The GraphRAG subset SQL/PGQ needs is `MATCH (a)-[e*min..max]-(b)`
bounded reachability. Full grammar-level conformance (a real `MATCH` clause hooking the PG parser) is the large,
deferrable part the milestone explicitly scopes out ("NÃO exige conformância total ... o mais diferível").
## ADR-1 — UDF-minimal pattern function, not a grammar extension
Ship `theodb.pgq_match(edge_rel, source_ids, pattern, default_max)` — a function that parses the bounded-path
quantifier (`*min..max`) and dispatches to the M108/M109 traversal. *Rejected:* a real SQL/PGQ grammar
extension (PG-parser intrusion — enormous, YAGNI for the GraphRAG subset; DuckPGQ itself is UDF-minimal).
## ADR-2 — composability IS the gate, and it is already met
The `graph_*` SETOF functions (M108-M111) + `pgq_match` compose with `<=>` (vector) and `ai.rerank` in one SQL
statement — the milestone's composability gate. Proven by `m113_pgq_composes_in_one_statement`.
## Coverage corners
Integration Tests: hop-bound parser (subset grammar), bounded-path MATCH semantics, one-statement composition.
Dependencies: none new (reuses graph_expand). Tools: cargo pgrx test. Techniques: DuckPGQ UDF-minimal, SQL/PGQ subset.
## Honest caveat
This is the ergonomic *subset* (bounded reachability), not full SQL/PGQ conformance (path variables, ELEMENT_ID,
WHERE on patterns) — deferred per the milestone. The M110 `graph_*` functions already serve theo-rag.
