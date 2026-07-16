---
slug: graph-surface-graphexpand
generated_by: roadmap-feature
status: completed
date: 2026-07-16
milestone: M110
---

# Grill — M110 (theodb.graph_expand + ai.extract_graph surface (theo-rag adopts))

Interactive grill SKIPPED: derived from ADR-0048 (native graph pillar follow-on milestones) + the
M107 SOTA blueprint + the measured spike (D3=GO), satisfying the 95%-confidence "detailed spec exists" escape.
- **Why now:** ADR-0048 authorized the pillar; M110 is a follow-on gated on M109. Each milestone carries
  its own measurement gate and can honest-negative to STOP the chain (measurement-first / anti-sunk-cost).
- **Dependency:** M109.
- **DoD / gate / risks:** see the ROADMAP M110 block.
- **Reuse (Rule 9):** builds on the existing index-AM/WAL/VACUUM, SIMD kernels, vector AM, columnar M99-M103, ai.* — no reinvention.
