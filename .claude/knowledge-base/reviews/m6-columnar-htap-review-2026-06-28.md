# Review — M6 Columnar / HTAP (pg_mooncake)

**Slug:** m6-columnar-htap
**Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m6-columnar-htap-plan.md` (SHIPPABLE 96.8)
**Discovery:** `.claude/knowledge-base/discoveries/blueprints/m6-columnar-htap-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89)
**Code-quality:** `.claude/knowledge-base/audits/m6-columnar-htap-code-quality-2026-06-28.md` (PASS)
**Report:** `docs/benchmarks/m6-columnar-vs-row.md` · **Doc:** `docs/analytics/columnar-htap.md`
**Commits:** `981ee9e` (impl) · `16421b2` (review fixes)

## Process

5 specialist agents in parallel (measurement-honesty · cross-validation · test-auditor · license · architecture).
Tally: measurement-honesty + cross-validation + architecture = READY_TO_MERGE; test+license = NEEDS_FIXES.
**Zero BLOCKER on shipped artifacts; the 3 ROADMAP M6 DoDs honestly met.** All HIGH/MEDIUM/LOW addressed + re-verified live.

## ROADMAP M6 DoDs — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| 1 — columnar enabled + analytical query measured vs row-store | ✅ (letter, honest) | `run_columnar_vs_row`: 100k rows, columnstore mirror, aggregate measured row **10.9 ms** vs columnar **44.3 ms**, `match=True` group-for-group. Columnar *speed win* NOT claimed (row faster at this scale; large-scale win `UNBENCHMARKED`) — disclosed everywhere (Rule 5). |
| 2 — row vs columnar plan choice documented | ✅ | `test_columnar_uses_duckdb_plan`: mirror = `Custom Scan (DuckDBScan)`, row = `Seq Scan`; doc `columnar-htap.md §` + report. |
| 3 — honesty: lakehouse DuckDB+Iceberg, NOT in-memory (D2) | ✅ | `columnar-htap.md`, report, CHANGELOG, `columnar.py` docstring. No AlloyDB in-memory parity claimed. |

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH (test) | Mirror created AFTER seeding the base (vs README's create-then-insert); no sync barrier → mirror could be empty/unsynced at query time → `match` flake/fail | Added `_wait_mirror_synced` (fail-fast barrier: block until mirror count == base count, bounded retry; clear error on timeout) before comparing | 4 columnar tests green; `rows_synced` in the result |
| 2 | MEDIUM (test) | `pg_mooncake_available` checked only `pg_available_extensions`, not the preload (regression from the BM25 honesty pattern) → skip gate could false-negative | Now checks BOTH the control file AND `shared_preload_libraries` (parity with `pg_textsearch_available`) | shipped-image skip-path green (4 skipped) |
| 3 | MEDIUM (test) | Cross-engine exact numeric equality (`round(avg,4)`) PG-vs-DuckDB → flake risk | `_results_match`: exact group + exact count + avg within `eps=1e-3` | `test_columnar_mirror_matches_row` green |
| 4 | MEDIUM (test) | Missing negative test (create mirror over non-existent base) | Added `test_columnar_create_mirror_nonexistent_base_raises` (typed `DBUnavailableError`) | green |
| 5 | LOW (test) | skip-test asserted only the exception type, not the message | Added `assert "pg_mooncake" in str(exc.value)` | green |
| 6 | MEDIUM (license) | §(e) fetched LICENSE from the moving `main` ref (non-reproducible) | Pinned to tags: pg_mooncake `v0.1.2`, pg_duckdb `v1.0.0` | sweep verdicts green |
| 7 | MEDIUM (license) | Doc asserted "DuckDB MIT/D1-clean" without fetching DuckDB's license / transitive scan | Added a `duckdb/duckdb v1.1.3` LICENSE fetch to §(e); softened the audit claim to "top-level LICENSEs only; a §(b)-equivalent transitive scan MUST run before the shipped-image D1 claim holds" | all 3 verdicts permissive (live) |
| 8 | MEDIUM (cross-val) | Plan Coverage row 5 / Objective said "Dockerfile.columnar builds pg17 from source" (stale — it's FROM canonical PG18; pg17 is the failed probe) | Corrected the plan row 5 + Objective to describe the canonical-PG18 substrate + the gated pg17 probe | plan re-scored SHIPPABLE 96.8 |
| 9 | LOW (cross-val) | CHANGELOG didn't name the PG18 substrate | Added "(PG18)" + "medir no PG17 exige o build gated" | CHANGELOG diff |

Reviewer-confirmed (no action): correctness is real (type-safe result-set equality + sync barrier); the
DuckDBScan-vs-SeqScan plan evidence is load-bearing + real; timing is symmetric + honest (no columnar-speedup
overclaim); substrate honesty (canonical PG18, PG17-build-blocked recorded); measurement-first gate consistent
with the BM25 S2 precedent; shipped `Dockerfile` verified unchanged; additive backward-compat; the kept
`Dockerfile.columnar-pg17probe` is the right honest adoption-path artifact (CI never references it). License
ordering is fail-closed (AGPL matched before the permissive MIT-body branch).

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 4 columnar + 69 unit; shipped skip-path clean |
| No secrets committed | PASS — `sk-proj` staged = 0 |
| No direct commit to `main` | PASS — develop |
| No authorship trailer (user policy) | PASS |
| CHANGELOG updated | PASS — `[Unreleased]` M6 (PG18 substrate + honest measured numbers) |
| No unbenchmarked perf claim | PASS — report states measured numbers only; columnar win `UNBENCHMARKED`; D2 honesty explicit |
| D1 (no AGPL) | PASS — pg_mooncake/pg_duckdb/DuckDB MIT (pinned); not in the shipped image; transitive-scan caveat documented |

## Verdict

READY_TO_MERGE. M6's 3 DoDs are honestly met: the pg_mooncake columnar/HTAP capability is **proven + measured**
(mirror == row, with a sync barrier), the **row-vs-columnar plan choice** is captured (DuckDBScan vs Seq Scan),
and the **D2 honesty** (lakehouse on disk, not in-memory) is documented. Per measurement-first (ADR 0002 + the
BM25 S2 precedent), the heavy PG17 source-build embedding into the shipped image is the gated adoption step —
attempted (`Dockerfile.columnar-pg17probe`), blocked on a rustc/MSRV pin at upstream HEAD (recorded honestly,
resolvable). No columnar speed win is claimed (row-store faster at 100k; large-scale win `UNBENCHMARKED`).
Licenses MIT (pinned, reproducible). HIGH sync-barrier + all MEDIUM/LOW fixed and re-verified live.
