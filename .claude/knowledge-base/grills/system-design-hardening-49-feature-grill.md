---
slug: system-design-hardening-49
generated_by: roadmap-feature
milestone_id: M104
date: 2026-07-16
status: completed
---

# Feature grill — system-design hardening (health 4.2 → ≥4.9/5)

## Q1 — What is this feature and why NOW?

Close the findings from the `/loop-system-design` Staff-level audit (2026-07-16,
`system-design-output/final_report.md`, overall **4.2/5**) to raise the health score to **≥4.9/5**.
**Why now:** the audit just surfaced a **CRITICAL production risk** — `columnar WRITE_STATES` buffers a whole
transaction's rows in RAM (issue #99 → OOM on a large `INSERT...SELECT`) — plus 4 HIGH scaling/resilience gaps and a
governance contradiction (the LOCKED North-Star mandate contradicts the team's own measured verdicts). Crucially, the
building blocks to fix the scaling findings (streaming build M89/M96, tuplesort spill) **already exist in-tree**, so
the remediation is low-risk application of proven patterns, not new invention.

## Q2 — Dependencies

All of the columnar/AI pillar the findings live in is `[x]` (M98–M103). **Depends on M103** (the last pillar
milestone) + the completed audit. No new milestone is a prerequisite.

## Q3 — Definition of Done (verifiable)

1. **CRITICAL closed:** columnar write path is bounded-memory (streaming flush at a stripe/row-count boundary, reusing the M89/M96 spill pattern) — no O(rows-in-xact) RAM (#99). A measured test proves a large `INSERT...SELECT` stays within a bounded memory envelope.
2. **All 4 HIGH closed:** (a) columnar seq-scan streams instead of full-materializing; (b) VACUUM fold bounded (or a documented, benchmarked cap) — M55 window; (c) M101 Arrow cache has an eviction/size bound; (d) AI HTTP client gains a circuit breaker + connection reuse (batch-size cap).
3. **Deletion/boundary wins:** the inert `rabitq/vendor/` tree deleted OR `#[cfg(feature)]`-gated with a corrected `VENDORED.md` (ADR); the `vec/ah.rs → am::aq` layering inversion fixed (relocate `AqQuantizer`); `columnar::decode_columns` gains a typed projection accessor (kills the `vindex → am::columnar` internals leak); legacy blob/v4 paths get `#[deprecated]`/WARN + the v4 OOM-default flipped.
4. **Data-flow win:** vectorizer producer backpressure + a dead-letter retention/purge bound.
5. **Governance (owner):** ADR-0033 signed (or a supersede note on the LOCKED ADR-0002 pointing at the measured verdicts 0035/0036) — closes the sole rationale-invalid trade-off. Non-code; owner decision.
6. **Verified:** a re-run of `/loop-system-design --mode=full` scores **≥4.9/5** overall, with the CRITICAL and all HIGH findings resolved.

## Q4 — Top 2 NEW risks

1. **Scope creep** — bundling all 5 dimensions into one milestone is large. Mitigation: each finding-closure is independently verifiable (DoD is a checklist), and the re-audit is the single acceptance gate; medium/low findings may be explicitly deferred with a note if they don't move the score.
2. **Refactoring the MVCC-load-bearing columnar path** (streaming write/scan) risks new MVCC / crash-safety regressions in code that was just proven correct. Mitigation: preserve the M99 heap-catalog visibility invariant + re-run the crash proofs (`make -C theodb_rs/isolation check-crash`) and the isolation permutations after the refactor.
