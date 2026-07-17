---
slug: m113-sqlpgq-surface
milestone_id: M113
created_at: 2026-07-17
goal: Ship theodb.pgq_match — a SQL/PGQ-subset bounded-path MATCH function that parses *min..max and dispatches to the M108 traversal, composing with vector/ai.rerank in one SQL statement, proven by tests.
---
# M113 — SQL/PGQ-subset surface
## Goal
Ship `theodb.pgq_match(edge_rel, source_ids, pattern, default_max)` — SQL/PGQ-subset bounded-path MATCH
(`*min..max` parser → M108 traversal), composing with the rest of SQL in one statement, proven by tests.
## ADRs (from blueprint)
ADR-1 UDF-minimal pattern function (rejected: full grammar extension — deferrable). ADR-2 composability = the gate,
already met by SETOF functions.
## Coverage Matrix
| claim | task |
| pattern parser (subset grammar) | T1.1 |
| bounded-path MATCH semantics | T1.1 |
| composes in one SQL statement | T1.1 |
## Drawbacks & Risks
| Risk | Sev | Mitigation |
| Not full SQL/PGQ conformance | LOW (accepted) | milestone explicitly scopes to subset; documented |
| min-hop via subtract-<min set (2 traversals) | LOW | correct + cheap for bounded paths; documented |
## Unresolved Questions
Full grammar-level MATCH (parser hook) deferred — honest, per milestone "o mais diferível".
## Global DoD
TDD; parser + semantics + composition tests; no new crate; full suite GREEN 0 regression; CHANGELOG; develop.
