---
slug: audit-remediation
milestone_id: none
date: 2026-06-29
verdict: IMPLEMENTATION_COMPLETE
plan: .claude/knowledge-base/plans/audit-remediation-plan.md
---

# Implementation summary — System-Design Audit Remediation

Closes all 5 actionable "Top Refactor Priorities" of the `loop-system-design` audit
(`.claude/knowledge-base/audits/system-design-audit-2026-06-29.md`, overall 3.2/5) + promotes 2 ADRs.
Off-roadmap (no `milestone_id` → `cycle-release` skips the checkbox flip with WARN).

## Evidence (against the rebuilt image `theo-db:audit-rem`)

- **Fresh install lands clean at theodb 1.1** (default_version bump + 1.0→1.1 chain): `pg_extension` →
  `theodb|1.1`, `theodb_rs|1.0.0`; `theodb.embed_batch(text[],text)` + `theodb.import_pinecone_chunked`
  both present.
- **New tests: 18/18 PASS** — `test_embed_batch.py` (7), `test_hybrid_guard.py` (2), `test_retry.py` (3),
  `test_retirement_migration.py` (3), `test_import_chunked.py` (3).
- **Regression oracle: 13/13 PASS UNCHANGED** — `test_embed_sql.py` (10) + `test_embed_failure_scenarios.py` (3).
- **Total: 41/41 tests green** (13 oracle + 18 new + 9 extension-install + 1 added real-upgrade-path retirement test) — re-verified after the /review fixes.
- **Benchmark (CTO data, `docs/benchmarks/audit-remediation-embed-batch.md`, mean±std, 5 runs):** the N→1
  collapse is real and grows with N — N=8 → **2.93×**, N=32 → **7.49×**, N=128 → **7.99×** (per-row issues
  N HTTP round-trips; `embed_batch` issues 1).
- **`cargo clippy --features pg17 -- -D warnings`: clean.** **`ruff check`: clean.** **`cargo pgrx install
  --release` (image build): compiles.**
- **`cargo pgrx test` (the Rust #[pg_test]s):** blocked in the build container by `initdb: cannot be run as
  root` (a PostgreSQL safety constraint of the pgrx-managed test PG, NOT a code defect). The behaviors those
  tests assert (NULL-element → 22023, empty → no call, unset/scheme/unreachable typed errors) are fully
  covered by the Python oracle against the REAL rebuilt image — the project's documented gate (`lib.rs`).

## Tasks + wiring triad

| Task | Change | Caller (pillar a) | Integration test (pillar b) | Observability (pillar c) |
|---|---|---|---|---|
| T0.1 | ADR 0007 + 0008 (Accepted) | docs (referenced by CHANGELOG + ADR 0007↔embed_batch) | n/a (docs) | n/a |
| T1.1 | `theodb.embed_batch` + Rust `run_batch`/`_embed_batch_text` | `theodb.embed_batch` SQL wrapper → `theodb_rs._embed_batch_text` → `embed::run_batch` | `test_embed_batch.py` (parity/N-in-out/empty/null/order/dups) + `bench_embed_batch.py` | typed 22023/38000 errors; benchmark report |
| T2.1 | hybrid seam guard | `ai.hybrid_search_rrf` (the guard runs before the embed call) | `test_hybrid_guard.py` | typed `0A000` + clear message |
| T3.1 | recoverable-class retry | `embed::post_json` (run + run_batch inherit) + `ai._chat` (all ai.* inherit) | `test_retry.py` | bounded retries; api_key never in error |
| T4.1 | 1.0→1.1 retirement migration + default_version 1.1 | `ALTER EXTENSION theodb UPDATE` / fresh `CREATE EXTENSION theodb` | `test_retirement_migration.py` | `RAISE NOTICE` on drop |
| T5.1 | `theodb.import_pinecone_chunked` PROCEDURE | `CALL theodb.import_pinecone_chunked(...)` (documented in migrate guide) | `test_import_chunked.py` | `RAISE NOTICE` row count |

## Coverage Matrix (audit findings → resolution)

All 11 actionable findings closed (the 4 INFO are positive baselines): #1 CRITICAL embed N+1 → T1.1; #2 HIGH
blocking I/O → T1.1 (batch); #3/#8 seam silent break → T2.1; #4 backpressure → T1.1+T3.1; #5 retry → T3.1;
#6/#7 unbounded import → T5.1; #9 deprecation → T4.1; #10/#11 undocumented decisions → T0.1 (ADR 0007/0008).

## Commits (one per task, atomic, no Co-Authored-By)

`163640f` PLAN · `6b082ad` T0.1 · `86ab578` T1.1 · `64a4249` T2.1 · `17c2a92` T3.1 · `f4cb304` T4.1 ·
`3091dcf` T5.1 · `b0c740d` clippy doc fix.

## Backward compatibility

`theodb.embed`, `theodb.import_pinecone` (FUNCTION), `ai._chat` contracts unchanged — every addition is
additive; the 13-test oracle is green unchanged.
