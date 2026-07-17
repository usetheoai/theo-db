---
slug: graph-persisted-csr-am
generated_by: roadmap-feature
status: completed
date: 2026-07-16
milestone: M108
---

# Grill — M108 (persisted-CSR index-AM (build 1x + incremental maintenance))

Interactive grill SKIPPED: derived from ADR-0048 (native graph pillar follow-on milestones) + the
M107 SOTA blueprint + the measured spike (D3=GO), satisfying the 95%-confidence "detailed spec exists" escape.
- **Why now:** ADR-0048 authorized the pillar; M108 is a follow-on gated on M107. Each milestone carries
  its own measurement gate and can honest-negative to STOP the chain (measurement-first / anti-sunk-cost).
- **Dependency:** M107.
- **DoD / gate / risks:** see the ROADMAP M108 block.
- **Reuse (Rule 9):** builds on the existing index-AM/WAL/VACUUM, SIMD kernels, vector AM, columnar M99-M103, ai.* — no reinvention.
