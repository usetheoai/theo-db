# Implementation summary: m19-nl-hybrid-import-rust

**Plan:** `knowledge-base/plans/m19-nl-hybrid-import-rust-plan.md` (verdict SHIPPABLE 96.8)
**milestone_id:** M19
**Branch:** develop
**Commits:** b4fbdb9, aa60756, e00a9a6, 0af2018, e53f8f0
**Status:** IMPLEMENTATION_COMPLETE

## Goal (from plan)

Port NL→SQL (anti-injection L1–L4), hybrid search (RRF) and Pinecone import to Rust/pgrx at proven parity,
removing the last `plpython3u` so the `theodb` extension is **100% Rust** — measured by the full nl/hybrid/
unified suites green against the rebuilt image plus a `nl_to_sql` Rust-vs-plpython3u benchmark.

## What shipped, per task

### Phase 1 — `ai.nl_to_sql` → Rust (`theodb_rs/src/nl.rs`)

- The LAST `plpython3u` function, ported with layered anti-injection preserved:
  - **L1** prompt constraint (system prompt byte-identical to sql/60:44-49).
  - **L2** static validation on a comment-stripped/lowercased copy: single-statement, SELECT/WITH-only,
    29-keyword denylist, no `DO $$`/`CALL` — stdlib token-scan (no regex crate; ADR-B).
  - **L4** parser-grade relation allowlist via `EXPLAIN (FORMAT JSON)` over SPI (no Rust SQL parser; ADR-A).
    Fix: read EXPLAIN's `json` column as `pgrx::Json` (not `String` — TEXTOID type-mismatch surfaced as a
    spurious "did not plan" on nested calls). Works at any nesting depth.
- `ai.nl_query` stays a thin plpgsql L3 read-only-sandbox keeper (ADR-F) calling `ai.nl_to_sql` at SQL level.
- **Wiring triad:** caller = `ai.nl_to_sql` SQL wrapper (extension_sql) + `ai.nl_query` (sql/60) + the M12
  config layer (sql/61); integration test = `benchmarks/tests/test_nl_sql.py` (35); observability = typed
  SQLSTATE 22023 errors with verbatim messages (server log on every rejection).

### Phase 2 — `ai.hybrid_search_rrf` + `ai.hybrid_search(jsonb)` → Rust (`theodb_rs/src/hybrid.rs`)

- Rust entrypoints orchestrate ONE RRF fusion SQL via SPI (one fusion source of truth — RANK per leg →
  FULL OUTER JOIN → summed `COALESCE(1/(k+rank))`; `%I` quoting via Postgres `format()`, not hand-rolled;
  `score DESC, id ASC` tie-break preserved). `query_vector` crosses the Rust boundary as text (→`::vector`).
- Validation 22023 (k/per_leg_limit/result_limit > 0; query_text and/or query_vector required); seam guard
  0A000 (`err_unsupported`) when `theodb.embed` is individually absent.
- **Scope decision (user, AskUserQuestion):** hybrid moves fully into `theodb_rs`, co-resident with
  `theodb.embed`. The former cross-extension seam scenario no longer applies — dropping `theodb_rs` removes
  both, so `test_hybrid_guard::test_absent_raises_undefined` asserts a clean 42883 (the defensive 0A000 guard
  remains in code for an individually-dropped embed).
- **Wiring triad:** caller = `ai.hybrid_search_rrf` + `ai.hybrid_search` SQL wrappers (regclass/vector
  types bridged to the text-typed Rust fn); integration test = `test_integration.py` (rrf + JSON parity +
  empty-leg) + `test_hybrid_guard.py`; observability = typed errors (22023/0A000) + server-side HTTP on embed.

### Phase 3 — `theodb.import_pinecone` (FUNCTION) → Rust (`theodb_rs/src/migrate.rs`)

- Native serde jsonb loop + `%I`-quoted INSERT via SPI; dim-mismatch `$2::vector` cast error propagates
  faithfully as the original 22xxx (SPI_execute is not PG_TRY-wrapped → original ereport longjmps out).
- `theodb.import_pinecone_chunked` (PROCEDURE) STAYS plpgsql (ADR-D — only a PROCEDURE can COMMIT per batch).
- **Wiring triad:** caller = `theodb.import_pinecone` SQL wrapper; integration test = `test_unified.py`
  (maps records / rejects malformed / safe identifiers / dim-mismatch) + `test_import_chunked.py`;
  observability = typed 22023 + the returned inserted-row count.

### Phase 4 — Benchmark (`benchmarks/bench_nl.py` + `docs/benchmarks/m19-nl-rust-vs-plpython.md`)

- Head-to-head `ai.nl_to_sql` Rust vs the exact retired plpython3u body (git 6c1dddb, renamed) against the
  SAME chat stub. Result: Rust 0.663 ms vs plpython3u 0.752 ms (ratio 0.883) — **NO-REGRESSION** (Rust faster).

### Retirement / 100%-Rust

- `theodb.control`: `default_version = '1.3'`, `requires = 'vector, vectorscale'` (plpython3u dropped).
- `sql/theodb--1.2--1.3.sql`: conditional DROP of the legacy theodb-owned `ai.nl_to_sql` + `ai.hybrid_search_rrf`
  + `ai.hybrid_search` + `theodb.import_pinecone` (guard: member-of-theodb AND NOT member-of-theodb_rs).
- sql/40, sql/60, sql/80: function defs removed (schema bootstrap + plpgsql keepers retained).
- README: plpython3u managed-PG limitation note removed.
- **0 `plpython3u` functions in any namespace** in the built image (verified).

## Validation evidence

- **167 passed, 7 skipped** across the full SQL integration suite (nl 35, hybrid, hybrid_guard, unified,
  integration, embed, embed_batch, embed_failure, ai_sql, ai_edge, ai_retirement, import_chunked,
  retirement_migration, extension_install, bm25, retry, metrics).
- **`cargo clippy --release --features pg17 -- -D warnings`**: CLEAN (exit 0).
- **`cargo pgrx install --release`**: succeeds (no symbol fabrication; compiles).
- Benchmark: nl_to_sql Rust no-regression vs plpython3u (0.883x).
- 0 plpython3u functions in the extension; `theodb` 1.3, `requires` sans plpython3u.

## Test-maintenance (M19 reality, committed)

- `test_unified.py::_fresh_db_with_ext` installs `theodb_rs CASCADE` (latent gap since M18 — ai._chat moved).
- version asserts 1.1/1.2 → 1.3; `requires` sans plpython3u; retirement-simulation tests `CREATE EXTENSION
  plpython3u` explicitly (theodb no longer pulls it via CASCADE); install-surface markers updated.
