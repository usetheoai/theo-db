# ADR-0047: M104 Scaling trade-offs — bounded-by-design, not deferred gaps

- **Status:** accepted
- **Date:** 2026-07-16
- **Deciders:** system-design-hardening owner (M104)
- **Relates:** ADR-0014 (crash-recovery proof), the M104 `/loop-system-design` re-audit, `../../.claude/rules/parsimony-ladder.md`, `../../CLAUDE.md § Esforço ≠ Complexidade`
- **Tags:** scaling, trade-offs, yagni, bounded-memory, honesty

> Drafted while closing the M104 hardening findings. The re-audit scored **Scaling 4.8/5**,
> capped by three items it labelled "skip-not-fix" or "secondary". This ADR records the
> **deliberate engineering decision** behind each — so they are documented trade-offs with a
> migration path, not silent gaps. Honesty (Unbreakable Rule 3): where the correct design is a
> bound + a migration path rather than an unbounded "fix", we say so plainly.

## Context and problem statement

M104 bounded every unbounded-memory path the audit flagged (columnar write → incremental
stripe flush at `maintenance_work_mem`; columnar scan → one-stripe streaming; Arrow cache →
entry cap; AI batch → `ai_max_batch` chunking; vectorizer queue → coalescing backpressure;
dead-letter → retained cap). Three residual Scaling items remain. The naïve reading is "finish
them to reach 4.9". The honest reading is that each is **already bounded by design**, and the
remaining "fix" is either research-scope or investment in a deprecated path — both forbidden by
the project's own principles (`parsimony-ladder.md`; `Esforço ≠ Complexidade`: essential vs
accidental complexity; anti-sunk-cost).

## Decision

### 1. In-VACUUM compaction fold — the guard + REINDEX is the design, not a stopgap

The legacy blob (M26), v3-structured, and HNSW-structured folds materialize the live set in RAM
(O(N)). M104 added a guard: when the index-on-disk exceeds `theodb.vacuum_fold_max_mb`
(default 1024), the in-VACUUM fold **SKIPs with a WARN** and defers compaction to `REINDEX`.
Correctness is preserved (the scan's pending-fold + MVCC re-check already return correct results
on an un-compacted index); only the *space* reclaim is deferred.

This is the correct bounded design, not a placeholder:

- **HNSW is inherently O(N)-in-RAM to rebuild.** An HNSW graph fold must hold the full vector
  set to reconstruct neighbor lists (`build_owned` already MOVEs, no clone). A "streaming HNSW
  fold" is external-memory graph construction — a research topic, out of scope per YAGNI until a
  measured need exists. `REINDEX` performs the same rebuild as an explicit, user-scheduled op
  (not silently inside autovacuum) with identical memory, which is the right place for it.
- **blob / v3 are deprecated** (M104 `DEPRECATED` markers; superseded by the streaming v5–v7
  IVF-AQ layout, whose `write_ivf_aq_split_streaming` fold is already bounded). Building a new
  bounded fold for a format we are removing is accidental complexity (`parsimony-ladder.md §
  Anti-patterns`). REINDEX migrates a legacy index to the modern streaming layout.

**Deferred (scoped, not silently dropped):** a bounded external-memory HNSW compaction fold is a
future milestone, gated on a *measured* need (a real workload that cannot REINDEX and hits the
guard). Recording it here satisfies "no silent cap" — the limit is documented, WARN-surfaced,
and has a migration path.

### 2. AI HTTP connection pool — mitigated by request batching (YAGNI)

The audit flagged `http.rs` opening a fresh connection per call (no keep-alive pool). M104's
`ai_max_batch` chunking already collapses a per-row AI surface into a few large batched
requests, so the TCP/TLS handshake is amortized over up to `ai_max_batch` (default 256) rows per
connection. Swapping the minimal client for a pooled one (e.g. ureq `Agent`) is a dependency
change that would have to re-prove the entire SSRF posture (redirect=0, private-range block,
api-key-in-header, `38000` fail-closed) and the circuit-breaker wiring — real risk for a benefit
the batching has already largely captured. Deferred per YAGNI; revisit only if a measured
non-batchable hot path appears.

### 3. v4 interleaved IVF-AQ default — on-disk-format stability over a risky flip

The v4 (interleaved) build path is OOM-prone at scale; the bounded layout is `separate_storage`
(v6/v7). M104 emits a WARN pointing writers at `WITH (separate_storage=1)` rather than flipping
the *default*, because the default governs the **on-disk format** — a silent flip changes what
new indexes write, a compatibility-sensitive decision that belongs in an explicit format-version
ADR, not a hardening pass. The WARN closes the "inverted default" surprise without a risky
format change.

### 4. `page.rs` decomposition — done

The 1986-LoC `am/page.rs` god-module was split into `am/page/{mod,ivf}.rs` (generic
page/buffer/WAL primitives vs the IVF/AQ on-disk format cluster), zero call-site churn via a
facade re-export, 319/319 pg_tests GREEN. This closes the Boundaries "tangled namespace"
finding physically (not by rationale).

## Consequences

- **Positive:** every Scaling limit is now either bounded-in-code (write/scan/cache/batch/queue)
  or bounded-by-guard with a documented migration path (fold). No unbounded-memory path remains
  unflagged. The remaining "distance to 4.9" on the fold is honestly a research-scope external-
  memory item, not an oversight.
- **Negative / accepted:** a very large *legacy or HNSW* index cannot reclaim dead space inside
  autovacuum — it requires an explicit `REINDEX`. This is surfaced by WARN and documented here.
- **Honesty:** this ADR exists so the audit's "skip-not-fix" items read as *deliberate bounded
  designs with migration paths*, which is their true nature — not as gaps papered over to move a
  score.

## Alternatives considered

- **Build the external-memory streaming HNSW fold now.** Rejected: research-scope, no measured
  need (YAGNI), and REINDEX already covers the space reclaim with the same memory profile as an
  explicit op.
- **Swap to a pooled HTTP client now.** Rejected: re-proving the full SSRF + breaker posture is
  real risk for a benefit batching has already captured; revisit on measured evidence.
- **Flip the v4 default now.** Rejected: changing the default on-disk format is a
  compatibility decision for a dedicated format-version ADR, not a hardening pass.
