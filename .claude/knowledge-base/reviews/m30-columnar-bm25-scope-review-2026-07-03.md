# Review — M30 v1-legacy scope decision (columnar + BM25): KEEP, benchmark-validated

**Date:** 2026-07-03
**Slug:** m30-columnar-bm25-scope
**Verdict:** READY_TO_MERGE
**Scope:** decision milestone — ADR 0013 (KEEP columnar + BM25 as permissive Rule-9 exceptions) + a
columnar-at-scale benchmark (the validating data) + ROADMAP note + CHANGELOG. **Zero product code** (theodb_rs
/ sql / shipped-Dockerfile untouched — the shipped product is provably unchanged).

## Decision

CTO steer: columnar is a general analytics/HTAP capability (AlloyDB parity; observability is one workload among
many), not a niche. Evidence-based per-pillar KEEP: columnar (measured win at scale) + BM25 (measured lexical
win). Neither deprecated; both recorded as permissive exceptions, gated for adoption. The shipped hybrid FTS leg
stays native `ts_rank_cd` (untouched).

## Agents & findings

Three independent specialist agents (benchmark-rigor, research/ADR, cross-validation). All converged on the
KEEP being sound + one shared HIGH (the M6↔M30 100k contradiction) — fixed in commit `9b79215` before this verdict.

| Agent | Verdict | Findings |
|---|---|---|
| council-benchmark | PASS + 1 HIGH | The ≥1M win (8.65×/14.94×) is robust; HIGH = 100k reversal vs M6 unreconciled + single-run over-read; MEDIUM = no mean±std; LOW = unpinned image, "byte-correto", query drift → **all FIXED**. |
| council-research-adr | PASS + 1 HIGH | ADR is proper MADR; **licensing VERIFIED accurate** (pg_mooncake/duckdb MIT; citus/hydra columnar AGPL — resolved on disk); no shipping overclaim; renumber 0007→0013 clean. HIGH = same 100k contradiction; LOW = "in-memory" mislabel, pg_duckdb citation → **all FIXED**. |
| cross-validation | DoD met + 1 HIGH | All 4 ROADMAP DoD checkboxes truthful; zero product code; no Co-Authored-By; Coverage 4/4. HIGH = same 100k; LOW = plan path → **all FIXED**. |

### Resolved (commit `9b79215`)

- **HIGH (all 3) — M6↔M30 100k reversal unreconciled + single-run over-read.** M6 measured row winning at 100k
  (columnar 44.3 ms); M30 measured columnar winning (single run). FIXED: (a) benchmark re-run with **mean±std
  over 3 runs** + warmup + an **effect>variance** gate — result 2.99× / 8.89× / 13.87×, effect>variance TRUE at
  every point (bands separated), closing the single-run gap; (b) a **"Reconciliation with M6"** section in the
  doc AND the ADR — the 100k columnar timing swung ~11× between runs (unpinned `:latest` image / DuckDB drift,
  ADR-0012 class), so the 100k point is treated as near-parity and **NOT load-bearing**; the KEEP decision is
  anchored on the **image-robust ≥1M win** (~9×/~14×). The image digest is now pinned in the artifact.
- **LOW — correctness wording:** "byte-correto" → "correto ±1e-3 (cross-engine, não byte-idêntico)".
- **LOW — query drift:** the artifact now records the real `_AGG` (round/::numeric/ORDER BY).
- **LOW — licensing mislabel:** "Citus/Hydra in-memory" → "on-disk comprimidos" (AGPL — verified on disk);
  pg_duckdb MIT noted via the mooncake bundle.
- **LOW — plan path:** `scripts/check_xrefs.py` → `.claude/scripts/check_xrefs.py`.

## Hard gates

- Tests green (ADR-doc + integration structure, mean±std keys) on the mooncake substrate; `check_xrefs` PASS;
  pyflakes clean. No secrets. On `develop`. **No `Co-Authored-By`** in any M30 commit. CHANGELOG `[Unreleased]`
  updated. **Zero product code touched** — `git show --stat` confirms only docs/benchmark/ADR/ROADMAP/CHANGELOG;
  the shipped product is unchanged (so nothing could break).

## Verdict rationale

0 BLOCKER, 0 unresolved HIGH. The consensus HIGH (100k) is fixed with real rigor (mean±std + effect>variance +
an honest M6 reconciliation that anchors the decision on the image-robust ≥1M win). Licensing — the load-bearing
Rule-9/D1 justification — was independently verified accurate. The decision is evidence-grounded (columnar wins
2.99×→8.89×→13.87× at scale; BM25 nDCG 0.95 vs 0.51), honest (M6 contradiction acknowledged, not papered over;
columnar explicitly NOT shipped — gated adoption), and touches zero product code. **READY_TO_MERGE.**

## Release note

A decision + evidence milestone (ADR + benchmark) with zero product change — a legitimate release candidate.
The ROADMAP M30 `[ ]`→`[x]` flip happens post-merge (cycle-release). Human decides.
