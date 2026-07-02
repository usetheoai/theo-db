---
slug: theodb-ann-qps-scale
generated_by: roadmap-feature
milestone_id: M34
date: 2026-07-02
status: completed
---

# Feature grill — theodb-ann-qps-scale (M34)

**Q1 — What / why now:** M32 measured theodb ~8× behind pgvector on QPS at 1M (theodb_ivfflat 30.7 vs 242; fixed
100-list under-partitioning) and theodb_hnsw impractical (1.6 QPS, O(N) blob scan). The two named levers close that
gap. Why now: it must precede M33 (head-to-head vs AlloyDB) — the superiority claim is only defensible once theodb
reaches QPS parity with pgvector.

**Q2 — Dependencies:** M32 (the measurement that identified the levers). Strategically precedes M33.

**Q3 — DoD (user-confirmed target):** theodb_ivfflat p50 ≤ pgvector at 1M (recall ≥ parity) via configurable
lists/probes; theodb_hnsw structured scan O(probes) not O(N) (QPS ≥ ~50); M20–M22 coexistence green; reproducible
benchmark re-run.

**Q4 — Top NEW risks:** (1) pgrx reloption/amoptions API design + edge validation; (2) structured HNSW graph
persistence is harder than flat ivfflat lists — the graph is not a flat partition, so partial-read needs a new
per-node/per-layer page layout.

**Scope decision:** ONE milestone (both levers) — user choice. **out_of_scope_overlap:** none.
