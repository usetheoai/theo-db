---
slug: native-graph-engine
generated_by: roadmap-feature
status: completed
date: 2026-07-16
milestone: M107
---

# Grill — native graph engine pillar Phase 0 (M107)

Interactive grill SKIPPED: rich context from the in-session SOTA deep research + the user's explicit directive
("temos outros sistemas com grafo, uso frequente, banco AI-native, o mais eficiente, NÃO importa o esforço"),
satisfying the 95%-confidence "detailed spec already exists" escape. Derived answers:

- **Why now:** graph is a recurring, cross-system, frequent capability (NOT YAGNI); the user removed the effort
  constraint and asked for the most EFFICIENT AI-native design. SOTA (DuckPGQ/Kùzu) converges on native graph
  execution (CSR + vectorized MS-BFS + WCOJ) fused with columnar+vector — and TheoDB already has 3 of the 4
  ingredients (columnar M99-M103, vector AM + SIMD kernels, ai.* in-SQL). The missing piece is native traversal.
- **Why Phase 0 / measurement-first:** a whole graph engine is a PILLAR (like the vector pillar M75-M82). Per the
  project's D3 / anti-sunk-cost mandate, the first milestone is the measurement-first gate: blueprint + spike proving
  native-graph-over-columnar beats recursive-CTE on the GraphRAG workload, BEFORE building the engine. honest-negative
  is a valid outcome that closes the pillar cheaply.
- **Dependencies:** M104 (the hardened columnar/vector/AI foundation the graph engine fuses with).
- **DoD / risks:** see the ROADMAP M107 block (blueprint + spike benchmark + D3 verdict + ADR + Rule-9 reuse discipline).
- **Architecture verdict (from research):** native CSR+MS-BFS over columnar (DuckPGQ/Kùzu model) > Apache AGE
  (Cypher-on-relational-joins, same per-hop-join tax) > recursive-CTE (baseline). GRFusion: the gap is the EXECUTION
  model, not storage — so build native traversal operators over the existing columnar/vector storage (Rule 9).
