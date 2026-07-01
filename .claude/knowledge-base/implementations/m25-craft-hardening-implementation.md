# M25 craft hardening — implementation summary

**Slug:** m25-craft-hardening · **milestone_id:** M25 · **Date:** 2026-07-01
**Plan:** `.claude/knowledge-base/plans/m25-craft-hardening-plan.md`
**Verdict:** IMPLEMENTATION_COMPLETE → READY_TO_MERGE

Behavior-preserving craft hardening of the `theodb_rs` engine extension, closing the MEDIUM/LOW findings of
the architecture audit (`.claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md`).

## Commits

| SHA | Task | Summary |
|---|---|---|
| `b167dc5` | T1.1 + T2.1 | DRY (`rerank_dist` deleted, `Metric::dist` `pub(crate)`), decompose `nl_to_sql`/`run_rrf`, `SbqParams`, named consts, unit tests, fix latent sbq test |
| `3daad59` | T3.1 | split `lib.rs` (721→92 LoC) → new `api.rs` (extern surface + 8 `extension_sql!`, verbatim) |
| `0585abe` | T4.1 | benchmark evidence doc + CHANGELOG |
| `0e412ab` | review-fix | ADR 0009 (amends ADR-2), stronger security tests, doc/citation fixes |

## Wiring triad (per task)

Behavior-preserving refactor — no new runtime feature, so the "runtime metric" pillar is N/A (no new
observable behavior). Callers + tests:
- **T1.1** DRY: caller = `sbq::knn` rerank uses `metric.dist()`; test = `test_sbq_index.py` recall parity (green).
- **T2.1** decomposition: callers = `nl_to_sql` → `l2_validate`/`l4_validate_relations`; `run_rrf` →
  `resolve_query_vector`; tests = new `#[pg_test]` units + `test_nl_sql.py` (35 green) + `test_ai_sql.py`.
- **T3.1** split: caller = `lib.rs` declares `mod api`; test = image rebuild (SQL regen) + `CREATE EXTENSION` +
  72 integration tests green (schema byte-identical).

## Goal metric — met

- Every extracted function CCN < 10: `l2_validate` 7, `relation_allowed` 5, `l4_validate_relations` 4,
  `resolve_query_vector` 4, `nl_to_sql` 8, `run_rrf` 9 (lizard). ✓
- `lib.rs` < 200 LoC: 92. ✓
- Suites pass at parity in Docker: 72 (sbq/ann/ai_sql) + 35 (nl_sql) integration tests green; clippy -D clean. ✓
- Evidence recorded: `docs/benchmarks/m25-craft-hardening.md`. ✓

## Deviations (documented)

- Plan ADR-2 (per-feature externs split) → single `api.rs` facade. Amended by **ADR 0009**; the heuristic
  500-LoC budget is formally waived for the declarative facade (`api.rs` = 640 LoC). Essential metric
  (`lib.rs < 200`) met.
- `#[pg_test]` unit harness does not run as root (pgrx `initdb` guard); runtime covered by the Python
  integration suite. Documented in the benchmark doc.

## Out-of-scope (honest, not regressed)

Pre-existing CCN ≥ 11 functions NOT flagged by the audit + NOT touched by M25 (identical before/after):
`ann_query::knn` 15, `chat::first_number` 14, `nl::strip_sql_comments` 13, `ann/hnsw::search_layer` 12,
`ann/ivf::kmeanspp` 11, `embed::run_batch` 11. Candidates for a future scoped milestone.
