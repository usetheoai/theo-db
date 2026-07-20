# ADR-0050 — Official benchmark harness: ADOPT-AND-WRAP, not pure replace

- **Status:** Accepted (owner directive "o mais FAANG possível", 2026-07-20)
- **Date:** 2026-07-20
- **Supersedes:** the initial owner preference to *pure-replace* the bespoke harness (2026-07-20, pre-discovery)
- **Evidence:** `knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md` (SHIPPABLE 97.5)

## Context

TheoDB's ~40 bespoke benchmark scripts + `theodb_bench/` are self-authored; `rules/public-copy.md § 4` requires a
third-party-reproducible artifact for any comparative claim. The owner asked to adopt the benchmarks officially
used by DB-engineering teams across all four pillars (vector/columnar/OLTP/HTAP), initially phrased as "replace"
the bespoke harness.

A 4-pillar web-evidenced discovery (2026-07-20, REGRA MÁXIMA) found — **unanimously** — that the official tools
(ann-benchmarks, VectorDBBench, big-ann-benchmarks, ClickBench, TPC-H/DS, pgbench, HammerDB TPROC-C,
CH-benCHmark/BenchBase) are **timing/leaderboard runners**: none provides (a) paired statistical significance
testing, (b) byte-identical result-regression A/B, or (c) result-correctness gating (ClickBench `check` is a
`SELECT 1`; pgbench/HammerDB post throughput with `fsync=off`; BenchBase validates timing, not OLAP results).
TheoDB shipped exactly these capabilities in 2026 (M123/M125 significance; M114/M126 byte-identical A/B; #46/#47
crash gates).

## Decision

**Adopt-and-wrap.** Per pillar, adopt the canonical official benchmark's **driver + real datasets + public
leaderboard entry** (for third-party-reproducible external comparability), AND **retain a thin TheoDB-owned
analysis layer** on top: paired significance (`benchmarks/theodb_bench/significance.py`), byte-identical /
cross-version result-regression A/B, and result-correctness + crash-safety gating. Retire only the ~40 redundant
bespoke *comparative* `run_m*.py` scripts once each pillar's official entry lands; the wrap layer is kept and
generalized into a shared library.

**Rollout: vector-pilot-first.** M127 (vector) is the de-risking vertical slice that establishes the reusable
pattern (official-entry adapter + shared wrap layer); M128 (columnar), M129 (OLTP), M130 (HTAP) then apply the
proven pattern. Each milestone runs the full cycle with measured evidence.

## Rationale (why this is the most FAANG-grade choice)

FAANG database teams run the industry-standard benchmarks for external comparability *and* layer their own
statistical-significance + performance-regression CI on top — they never discard significance/regression. Pure
replace would (i) drop capabilities the official tools do not provide, and (ii) accept a `SELECT 1`-gated harness
where a wrong-but-fast engine could top ClickBench undetected — the opposite of rigorous. Vertical-slice
de-risking before horizontal rollout is standard FAANG infra practice.

## Alternatives rejected

- **Pure replace** (initial preference) — REJECTED by unanimous 4-pillar evidence: drops significance + regression
  + correctness; none replaced by the official tools.
- **Keep bespoke only** — REJECTED: no third-party reproducibility (the exact `public-copy.md § 4` gap).
- **Big-bang all four pillars at once** — REJECTED as the rollout: FAANG de-risks with a proven vertical slice
  (vector) before scaling; the wrap-layer library must be proven once before reuse.

## Consequences

- New roadmap program M127–M130 (one pillar each), sequenced vector → columnar → OLTP → HTAP.
- License guardrails (per D1): ClickBench `hits` (CC-BY-NC-SA) + TEXMEX SIFT/GIST (unconfirmed) are CI-download
  only; HammerDB (GPLv3) is an external out-of-tree driver only; TPC results labeled "TPC-H-derived"; only GloVe
  (PDDL) + pgbench (PostgreSQL License) may be bundled. TEXMEX license is an open MUST-VERIFY.
- Honest positioning preserved: the ScaNN/AlloyDB QPS-gap magnitude cites TheoDB's own
  `docs/benchmarks/m73-headtohead-verdict.md`; public sources only for the gap's direction.

## Cross-references

- Blueprint: `knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md`
- Discovery plan: `knowledge-base/discoveries/plans/official-db-benchmark-harness-plan.md`
- Positioning: `docs/adr/0033`/`0035`/`0036` (vector QPS gap structural), `rules/public-copy.md § 4`
- Retained capabilities: M123/M125 (significance), M114/M126 (byte-identical A/B), #46/#47 (crash gates)
