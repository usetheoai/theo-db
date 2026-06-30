# Review: m19-nl-hybrid-import-rust — 2026-06-30

**Verdict:** `READY_TO_MERGE`
**Domain:** database (primary) · api-design, security (secondary)
**Agents:** 7 (architecture, tests, wiring, cross-validation, domain-database, domain-security, domain-api-design)
**Severity tally (as found):** BLOCKER 0 · HIGH 0 · MEDIUM 6 · LOW ~8 · INFO ~12
**Severity tally (after fixes in this cycle):** BLOCKER 0 · HIGH 0 · MEDIUM 0 open (all addressed) · LOW residual (documented)

The slice ports `ai.nl_to_sql` (the last plpython3u), `ai.hybrid_search(_rrf)` and `theodb.import_pinecone`
to Rust (theodb_rs). 7 specialist agents reviewed `git diff main..develop`. **No agent found a BLOCKER or
HIGH.** The 6 MEDIUMs were addressed in-cycle (see § Findings + resolution). Per-agent finding files:
`.claude/agents/review-m19-nl-hybrid-import-rust-2026-06-30/findings/*.yaml`.

## Per-agent summary

| Agent | Verdict | Headline |
|---|---|---|
| architecture | no BLOCKER/HIGH; 2 MEDIUM | ADR-C boundary sound; extern-placement inconsistency + stale layering docs |
| tests | no BLOCKER/HIGH; 1 MEDIUM | parity oracles genuinely exercise the Rust paths; missing Rust unit tests for L2 helpers |
| wiring | WIRING_SOLID | 4/4 triads honest; `err_unsupported` called; 1.2→1.3 migration safe (no Rust-fn drop) |
| cross-validation | no BLOCKER/HIGH; 2 MEDIUM | 3 DoDs MET; 2 documented divergences (nl_query plpgsql, hybrid scope pivot) |
| domain-database | no BLOCKER/HIGH; 1 MEDIUM | DB semantics preserved; L4 unplannable-query lost 22023 parity |
| domain-security | no BLOCKER (no missed injection vector); 1 MEDIUM | L2/L4 byte-faithful to plpython3u; original read-exfil BLOCKER stays closed; no Rust unit tests |
| domain-api-design | no BLOCKER/HIGH; 1 LOW | all 4 signatures + named-args + error contract at exact parity |

## Findings + resolution

### Fixed in-cycle (MEDIUM)

1. **L4 unplannable query lost 22023 parity (domain-database F-db-1 / security F-sec-2).** A generated SELECT
   over a NON-EXISTENT relation made the planner's native SQLSTATE (42P01) longjmp past the `Err` arm instead
   of the contracted 22023. **Fixed:** wrapped the EXPLAIN-over-SPI in `PgTryBuilder::new(...).catch_others(|_| None)`
   → re-raises 22023 "did not plan (rejected)" (parity with the plpython3u `try/except`). The old code comment
   warned PgTryBuilder breaks nested EXPLAIN — empirically FALSE (that was the Json-type bug); verified by
   `test_nl_sql.py` 35/35 incl. the nested `nl_query` cases. New regression test
   `test_nl_to_sql_unplannable_query_rejected_22023` (+ stub token `__nlinject_noplan__`).

2. **No Rust unit tests for the L2 helpers (tests F-tests-1 / security F-sec-1).** Plan T1.1 promised "a Rust
   unit test per L2 rule"; only the Python oracle covered them. **Fixed:** added `#[pg_test]` module `nl_tests`
   in `nl.rs` — `first_banned_token` (whole-token + pg_ls_ prefix + embedded-word negatives), `has_do_block`/
   `has_word`, `starts_with_keyword`, `strip_sql_comments`, `nl_fence_strip`, `collect_relations` (nested tree).

3. **nl_query plpgsql vs planned Rust (cross-validation).** **Fixed (doc):** added plan ADR D6 reconciling
   ADR-F (L3 sandbox stays plpgsql — transaction-control + nested-EXPLAIN-SPI cleanliness).

4. **Hybrid scope pivot not in plan (cross-validation).** **Fixed (doc):** added plan ADR D7 recording the
   AskUserQuestion co-residency decision + the 0A000→42883 guard-test consequence.

5. **Extern-placement inconsistency (architecture F-arch-1).** hybrid/migrate put `#[pg_extern]` in an in-module
   `mod theodb_rs` while nl/embed/ai use lib.rs. **Fixed:** moved `_hybrid_search_rrf`/`_hybrid_search_json`/
   `_import_pinecone` into lib.rs's `mod theodb_rs` (run_rrf/run_rrf_json/import are now `pub(crate)`). One
   convention across the crate.

6. **Stale layering docs + label drift (architecture F-arch-2 / security F-sec-4).** **Fixed (doc):** relabeled
   nl/hybrid/migrate headers as "SPI-orchestration adapter"; corrected pg.rs "only module that talks to the ABI";
   "29 keywords" → 33 (CHANGELOG + plan note); stale plpython3u-in-requires comments in sql/30, sql/50, sql/60.

### Residual (LOW/INFO — accepted, documented)

- The defensive 0A000 `err_unsupported` guard in `hybrid.rs` is retained but no longer covered by a test (the
  co-residence repointed the guard test to 42883). Documented in the implementation summary + ADR D7.
- `ai.hybrid_search(jsonb)` reads `k`/limits via `as_i64()` (type-strict) vs the plpgsql `->>'k'::int` (which
  also coerced JSON-string numbers). The tested/documented convention (JSON numbers) is unaffected; non-canonical
  `{"k":"30"}` silently falls to the default. Documented; not a contract break.

## Hard gates (all pass)

- Tests green on `develop`: **168 passed, 7 skipped** (full SQL integration suite incl. the new regression test).
- `cargo clippy --release --features pg17 -- -D warnings`: CLEAN.
- `cargo check --features pg17 --tests`: CLEAN — the new `nl_tests` `#[pg_test]` module compiles. NOTE: `#[pg_test]`
  EXECUTION needs a pgrx-managed Postgres; the builder uses the system pg, so `cargo pgrx test` cannot spin the
  harness here (same pre-existing limitation as the lib.rs embed `#[pg_test]` — project CI runs `install`, not
  `test`). The L2 helper behavior is validated end-to-end by the Python oracle (`test_nl_sql.py` 35/35).
- No secrets committed; no Co-Authored-By trailer; CHANGELOG updated; working on `develop` (not `main`).
- code-quality audit: PASS (clippy-backed; see `knowledge-base/audits/m19-nl-hybrid-import-rust-code-quality-2026-06-30.md`).
- Benchmark: nl_to_sql Rust no-regression vs plpython3u (ratio ~0.88).

## Output

- Per-agent findings: `.claude/agents/review-m19-nl-hybrid-import-rust-2026-06-30/findings/*.yaml`
- This report: `knowledge-base/reviews/m19-nl-hybrid-import-rust-review-2026-06-30.md`
