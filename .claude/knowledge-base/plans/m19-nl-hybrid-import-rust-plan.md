---
slug: m19-nl-hybrid-import-rust
milestone_id: M19
created_at: 2026-06-30
goal: Port NL→SQL (anti-injection L1–L4), hybrid search (RRF) and Pinecone import to Rust/pgrx at proven parity, removing the last plpython3u so the theodb extension is 100% Rust, measured by the full nl/hybrid/unified suites green against the rebuilt image plus a nl_to_sql Rust-vs-plpython3u benchmark.
---

# Plan: M19 — NL→SQL + Hybrid + Import → Rust (end of plpython3u)

## Goal

> "Port `ai.nl_to_sql` (the last plpython3u, anti-injection L1/L2/L4), `ai.nl_query` (L3 sandbox),
> `ai.hybrid_search(_rrf)` and `theodb.import_pinecone(_chunked)` to Rust/pgrx at proven parity, and remove
> `plpython3u` from `theodb.control` requires, measured by (a) `benchmarks/tests/test_nl_sql.py` (35) +
> `test_unified.py` (11) + `test_hybrid.py` (14) green against the rebuilt image, every injection case still
> rejected with the same SQLSTATE, AND (b) `docs/benchmarks/m19-nl-rust-vs-plpython.md` (no-regression,
> mean±std, ≥3 runs), with **zero `LANGUAGE plpython3u` anywhere in `sql/`**."

## Context

ROADMAP-v2 M19 (depends on M18, done). User decision (2026-06-30): **DoD-2 literal — all of nl/hybrid/import
in Rust**. The inventory (blueprint `.claude/knowledge-base/discoveries/blueprints/m19-nl-hybrid-import-rust-blueprint.md`)
found ONLY `ai.nl_to_sql` is plpython3u; the rest are plpgsql. Per blueprint ADR-A/C, the inherently-relational
operations (L4 relation enumeration, RRF RANK/JOIN, jsonb→INSERT) stay SQL executed via `Spi` — the Rust
functions own the orchestration + validation; NO `sqlparser` crate (it would reopen the comma-join/quoted-ident
vulnerability the EXPLAIN approach closed). Reuses M17/M18 (`theodb_rs/src/{lib,http,chat,embed,pg}.rs`).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit | Why it exists | Invariants to preserve |
|---|---|---|---|---|
| `sql/60-theodb-nl.sql` | ~179 | M12 | `ai.nl_to_sql` (plpython3u, L1/L2/L4) + `ai.nl_query` (plpgsql, L3) | REMOVE the plpython3u `nl_to_sql`; both become Rust-backed; L3 sandbox semantics preserved |
| `sql/40-theodb-hybrid.sql` | ~154 | audit-rem | `ai.hybrid_search_rrf` + `ai.hybrid_search` (plpgsql) | port to Rust orchestrating the SAME RRF SQL via SPI; embed-seam guard + `'english'` pin + tie-break preserved |
| `sql/80-theodb-migrate.sql` | ~131 | audit-rem | `theodb.import_pinecone` (FUNCTION) + `import_pinecone_chunked` (PROCEDURE) | FUNCTION → Rust (jsonb loop + INSERT via SPI); PROCEDURE per-batch COMMIT decided in T-import |
| `theodb_rs/src/nl.rs` (NEW) | 0 | — | domain: nl_to_sql L1/L2/L4 + nl_query L3 orchestration | byte-faithful L2 denylist + L4 EXPLAIN; typed 22023/25006 |
| `theodb_rs/src/hybrid.rs` (NEW) | 0 | — | domain: build+run the RRF SQL via SPI | identical fusion SQL; %I-safe |
| `theodb_rs/src/migrate.rs` (NEW) | 0 | — | domain: import_pinecone jsonb→INSERT via SPI | %I/regclass-safe; 22023 validation |
| `theodb_rs/src/lib.rs` | ~250 | M18 | api-surface | add nl/hybrid/migrate externs + `extension_sql!` wrappers (REVOKE); embed/chat/ai unchanged |
| `theodb_rs/src/pg.rs` | ~43 | M18 | glue | reuse err_input/err_external/guc/warn; add a Spi helper if needed |
| `sql/theodb--1.2--1.3.sql` (NEW) | 0 | — | retirement migration: conditional DROP of plpython3u `ai.nl_to_sql` | mirror M18 1.1→1.2 guard |
| `theodb.control` | 6 | M18 | umbrella control | `requires = 'vector, vectorscale'` (drop plpython3u); `default_version` 1.2 → 1.3 |
| `README.md` | — | — | public docs | remove the "Limitação honesta (plpython3u)" note + the CREATE EXTENSION plpython3u comment |
| `benchmarks/tests/{test_nl_sql,test_unified,test_hybrid}.py` | — | M12/M16 | the parity oracles | stay green UNCHANGED |
| `benchmarks/bench_nl.py` (NEW) + `docs/benchmarks/m19-nl-rust-vs-plpython.md` (NEW) | 0 | — | nl Rust-vs-plpython3u benchmark | — |

### Current callers / dependents

- `ai.nl_to_sql(text,text[],text)` — called by `ai.nl_query` (sql/60, plpgsql) + the M12 config layer (`sql/61`). Must stay SQL-callable as `ai.nl_to_sql(text, text[], text) RETURNS text`.
- `ai.nl_query` — calls `ai.nl_to_sql` + `ai._chat` indirectly. `sql/61` (nl-config) calls `ai.nl_query`. If nl_query becomes Rust, `sql/61` must keep resolving it by name.
- `ai.hybrid_search_rrf` — called by `ai.hybrid_search(jsonb)` + users; calls `theodb.embed` (Rust, M17) via the seam guard.
- `theodb.import_pinecone` — user-facing (docs/migrate-from-pinecone.md).
- `ai.nl_to_sql` is the ONLY plpython3u left → removing it lets plpython3u leave `requires`.

### Domain glossary

- **L1/L2/L3/L4** — prompt constraint / static validation (single-stmt, SELECT-WITH-only, 29-keyword denylist) / read-only sandbox (transaction_read_only + statement_timeout → 25006) / parser-grade relation allowlist via `EXPLAIN (FORMAT JSON)`.
- **EXPLAIN-based allowlist** — enumerate every planned relation from the EXPLAIN JSON tree (`"Relation Name"`/`"Schema"`); a Rust SQL parser would diverge from the planner (search_path/views) → security regression (blueprint ADR-A).
- **RRF** — Reciprocal Rank Fusion: `score = Σ 1/(k+rank_leg)`, RANK per leg, FULL OUTER JOIN (sql/40).

### Architecture boundaries affected

Per `.claude/rules/architecture.md`: new domain modules `nl.rs`/`hybrid.rs`/`migrate.rs` (portable orchestration) + `pg.rs` glue (typed errors, GUC, SPI) + `lib.rs` api-surface (`#[pg_extern]` + `extension_sql!`). The relational primitives (EXPLAIN, RANK/JOIN, INSERT) execute via `Spi` — the boundary stays: domain orchestrates, Postgres executes. No new dependency-direction violation; NO new crate.

## Prior Art & Related Work

- Blueprint: `.claude/knowledge-base/discoveries/blueprints/m19-nl-hybrid-import-rust-blueprint.md` (port map + ADR seeds A–E).
- M17/M18 reference impls: `theodb_rs/src/{embed,chat,http}.rs` (the pgrx + SPI + stdlib-regex pattern; `chat.rs::first_token/first_number/strip_fence` = the house style for porting `re.*` without a crate).
- Behavioral spec: `sql/60` (nl), `sql/40` (hybrid), `sql/80` (import) — verbatim in the blueprint.
- Parity oracles: `test_nl_sql.py` (35, incl. 12 injection cases), `test_unified.py` (11), `test_hybrid.py` (14 offline RRF twin).

## Objective

- [ ] `ai.nl_to_sql` is Rust (theodb_rs), L1 prompt byte-identical, L2 denylist + single-stmt + SELECT/WITH verbatim, L4 via EXPLAIN-over-SPI; every injection case rejected with the same SQLSTATE (22023).
- [ ] `ai.nl_query` is Rust orchestrating the L3 read-only sandbox (write → 25006) at parity.
- [ ] `ai.hybrid_search(_rrf)` is Rust building+running the identical RRF SQL via SPI (embed-seam guard preserved).
- [ ] `theodb.import_pinecone` is Rust (jsonb→INSERT via SPI, %I/regclass-safe, 22023 validation); the chunked PROCEDURE handled per T-import ADR.
- [ ] `plpython3u` removed from `theodb.control requires`; zero `LANGUAGE plpython3u` in `sql/`; README plpython3u note removed.
- [ ] 1.2→1.3 retirement migration; `default_version` 1.3.
- [ ] `docs/benchmarks/m19-nl-rust-vs-plpython.md` shows nl_to_sql Rust at no-regression vs plpython3u.

## ADRs

### D1 — nl_to_sql L4 stays `EXPLAIN (FORMAT JSON)` via SPI; NO Rust SQL parser
**Decision:** the relation allowlist enumerates relations via `Spi`-run `EXPLAIN (FORMAT JSON, VERBOSE false) <sql>` + a `serde_json` tree-walk for `"Relation Name"`/`"Schema"`, matching the plpython3u logic exactly.
**Rationale:** the EXPLAIN approach is WHY comma-joins/quoted-idents/CTEs are caught (regex lexing was a review BLOCKER, sql/60:4-8). A `sqlparser` crate would diverge from the planner (search_path, view→base-rel) and reopen that class — tests 14/15/17/26 at risk.
**Alternatives:** (a) `sqlparser` crate — rejected (security regression + new dep); (b) regex lexing — rejected (the original BLOCKER).
**Consequences:** L4 needs `Spi` + `serde_json` (both present). Fail-closed on plan error (22023).

### D2 — L2 static validation ports to Rust stdlib (no regex crate)
**Decision:** port the comment-strip + single-statement + `^(select|with)` + 29-keyword denylist + `do $$|call` guard with stdlib char-scanning (house style: chat.rs hand-rolls `re.*`).
**Rationale:** L2 is regex/keyword heuristics; a regex crate is unjustified (parsimony rung 5; precedent chat.rs/http.rs). Byte-faithful caveats: `\b` treats `_` as word-char; block-comment DOTALL; the nl fence-strip variant (`^```[a-zA-Z]*\n?`) differs from chat.rs::strip_fence.
**Alternatives:** (a) `regex` crate — rejected (parsimony + the patterns are simple/fixed); (b) reuse chat.rs::strip_fence — rejected (different fence regex).
**Consequences:** a Rust unit test per L2 rule + the 12 Python injection tests as the cross-language oracle.

### D3 — hybrid + import: Rust #[pg_extern] orchestrating the SAME SQL via SPI
**Decision:** `ai.hybrid_search_rrf`/`ai.hybrid_search` and `theodb.import_pinecone` become Rust `#[pg_extern]`s that build the identical parameterized `%I`-quoted dynamic SQL and run it via `Spi` (RRF fusion query; jsonb→INSERT). Preserve the embed-seam guard, `'english'` FTS pin, `score DESC, id ASC` tie-break, regclass/`%I` injection-safety, and the exact RETURNS/validation.
**Rationale:** the relational operations (RANK/FULL-OUTER-JOIN, INSERT) are inherently SQL — reimplementing them in Rust would reinvent the engine (anti-objetivo). "In Rust" = Rust owns the orchestration/validation; Postgres executes the SQL. Satisfies the user's "tudo em Rust" without the anti-pattern of a Rust query engine.
**Alternatives:** (a) fuse RRF in Rust (fetch both legs, sort/join in Rust) — rejected (slower, regression risk, reinvents SQL); (b) keep plpgsql — rejected (user chose Rust).
**Consequences:** thin Rust shells generating the same SQL; parity proven by test_hybrid/test_unified.

### D4 — import_pinecone_chunked PROCEDURE stays plpgsql (COMMIT-per-batch); FUNCTION → Rust
**Decision:** the per-batch-COMMIT PROCEDURE stays plpgsql (a pgrx `#[pg_extern]` runs in the caller's transaction and cannot COMMIT cleanly; pgrx procedure-COMMIT support is fragile). The atomic FUNCTION `import_pinecone` → Rust.
**Rationale:** COMMIT-mid-function is a transaction-control concern PostgreSQL exposes only to PROCEDUREs; forcing it through pgrx is accidental complexity + risk. The PROCEDURE is already own-code (plpgsql, no external dep) — keeping it is parsimony, not a plpython3u concern.
**Alternatives:** (a) pgrx procedure with Spi COMMIT — rejected (fragile/unsupported); (b) port FUNCTION too only — this IS the decision.
**Consequences:** `import_pinecone` Rust; `import_pinecone_chunked` plpgsql (documented in an ADR; not a plpython3u dependency so DoD-3 unaffected).

### D5 — Retirement migration 1.2→1.3 + remove plpython3u from requires + README
**Decision:** `theodb--1.2--1.3.sql` conditionally DROPs the legacy plpython3u `ai.nl_to_sql` (guarded: plpython3u AND not theodb_rs member); `theodb.control requires = 'vector, vectorscale'`; `default_version` 1.3; README plpython3u limitation note + CREATE EXTENSION comment updated; keep `superuser=true` (vectorscale/DiskANN may need it — not proven removable).
**Rationale:** mirrors M17/M18 retirement (proven). Removing plpython3u from requires is the headline DoD-3 (managed-PG limitation gone).
**Alternatives:** (a) leave plpython3u in requires — rejected (defeats M19); (b) drop superuser too — rejected (unverified; out of scope).
**Consequences:** existing installs UPDATE then add/refresh theodb_rs; fresh installs at 1.3 plpython3u-free.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| L2 denylist port not byte-faithful → weakened anti-injection | High | Per-rule Rust unit tests + the 12 Python injection tests (drop/multi/exfil/relation/comma-join/quoted/stat_file) as cross-language oracle; `\b`-as-`[A-Za-z0-9_]`, DOTALL block-comment, nl-fence variant explicitly tested | maintainers |
| L4 EXPLAIN tree-walk misses a relation shape the planner nests differently | High | Recursive walk over all dict/list nodes (mirror plpython3u); tests 5/14/15/17/26 (non-allowlisted, comma-join, quoted, end-to-end) | maintainers |
| nl_query read-only sandbox (SET LOCAL) semantics differ in Rust SPI | Medium | Keep the exact `set_config('transaction_read_only','on',true)` + statement_timeout via Spi; test_readonly_sandbox + 25006 cases | maintainers |
| hybrid/import Rust shells diverge from the plpgsql SQL (subtle %I/param drift) | Medium | Build byte-identical SQL strings; test_hybrid/test_unified parity; injection-safety tests (test 7 hostile identifiers) | maintainers |
| Removing plpython3u from requires breaks an unforeseen dependent (sql/70 ml?) | Medium | Verified sql/70 plpython3u-free; full suite + test_extension_install vs rebuilt image before claiming done | maintainers |
| chunked PROCEDURE staying plpgsql contradicts "tudo em Rust" literally | Low | ADR D4 documents why (COMMIT-in-pgrx infeasible); it carries no plpython3u/external dep | maintainers |

## Unresolved Questions

- Q1 — Can a pgrx `#[pg_extern]` reliably issue `COMMIT` per batch (for import_pinecone_chunked)? (Resolved at D4: no — keep the PROCEDURE in plpgsql; pgrx functions run in the caller's transaction.)
- Q2 — Does removing `plpython3u` from `requires` require also dropping `superuser = true`? (Resolved at D5: no — vectorscale/DiskANN may need superuser; keep unless proven removable, out of M19 scope.)

## Dependencies

(none — NO new dependency. nl/hybrid/import reuse `pgrx` (Spi), `serde_json` (EXPLAIN JSON walk), already in `theodb_rs/Cargo.toml`. NO `sqlparser`/`regex` crate (ADR D1/D2 — stdlib scanning + Postgres EXPLAIN). `/deps-audit` has no new declared dep to scan.)

## Dependency Graph

```
Phase 1 (nl.rs: nl_to_sql L1/L2/L4 + nl_query L3) ──┐
Phase 2 (hybrid.rs: RRF via SPI) ───────────────────┤
Phase 3 (migrate.rs: import_pinecone via SPI) ──────┤──▶ Phase 5 (Integration Validation + benchmark)
Phase 4 (sql side: remove plpython3u, 1.2->1.3 migration, control, README) ─┘
```
Phase 1 is the critical path (the only plpython3u + the anti-injection). Phases 2/3 independent. Phase 4 (SQL/migration/control/README) depends on 1–3's Rust functions existing. Phase 5 validates everything.

## Phase 1: `nl.rs` — nl_to_sql (L1/L2/L4) + nl_query (L3) in Rust

### T1.1 — `nl_to_sql` Rust port (L1 prompt, L2 denylist, L4 EXPLAIN allowlist)

#### Objective
Port the anti-injection NL→SQL validator to Rust at byte-faithful parity; the last plpython3u eliminated.

#### Why this step (action + reasoning)
1. **What:** `theodb_rs/src/nl.rs` — `nl_to_sql(question, allowed: &[Option<&str>], model) -> String`: input guards (22023), L1 system prompt (verbatim), `ai._chat` via SPI, nl-fence strip, L2 (comment-strip + single-stmt + `^(select|with)` + 29-keyword denylist + `do $$|call`), L4 (`EXPLAIN (FORMAT JSON)` via Spi + serde_json walk + allowlist match). `lib.rs`: `#[pg_extern] _nl_to_sql` + `extension_sql!` `ai.nl_to_sql(text,text[],text)` (REVOKE).
2. **Why now:** the milestone's headline + the only plpython3u. Cites blueprint ADR-A/B + sql/60 verbatim.

#### Deep Dives
- **L1 prompt (verbatim, byte-identical — stub routes on "read-only postgresql select"):** `"You translate a question into exactly ONE read-only PostgreSQL SELECT query. You may reference ONLY these relations: " + sorted(allowed).join(", ") + ". Output ONLY the SQL — no prose, no markdown, no trailing semicolon. Use SELECT or WITH only. Never modify data."`
- **fence strip (nl variant):** strip leading ` ``` ` + `[a-zA-Z]*` + optional `\n`, trailing optional `\n` + ` ``` ` (NOT chat.rs::strip_fence).
- **L2 denylist (29 keywords, verbatim, `\b`-bounded with `_` as word char, case-insensitive):** drop/insert/update/delete/alter/truncate/grant/revoke/create/copy/merge/reindex/vacuum/pg_read_file/pg_read_binary_file/pg_stat_file/pg_ls_dir/pg_ls_waldir/pg_ls_logdir/pg_ls_tmpdir/pg_ls_archive_statusdir/pg_ls_/lo_import/lo_export/lo_get/lo_put/dblink/pg_sleep/set_config/current_setting/pg_terminate_backend/pg_cancel_backend/pg_read_server_files. Plus `\bdo\b\s*\$\$|\bcall\b`.
- **L4:** `Spi::get_one::<JsonB|String>("EXPLAIN (FORMAT JSON, VERBOSE false) " + sql)`; on Err → 22023 "query did not plan (rejected)"; serde_json recursive collect (schema default "public"); match `schema.name in allowed OR (name in allowed AND schema==public)`; else 22023 "relation '..' is not in the allowlist".
- **Errors:** all 22023, verbatim messages (blueprint § 1 table).

#### Files to edit
```
theodb_rs/src/nl.rs (NEW) — nl_to_sql + helpers (l2 scan, explain allowlist)
theodb_rs/src/lib.rs — #[pg_extern] _nl_to_sql + extension_sql! ai.nl_to_sql wrapper (REVOKE)
```

#### TDD
```
RED:    (parity, py) test_nl_sql.py benign + 12 injection cases (drop/multi/exfil/relation/comma-join/quoted/stat_file/...) all 22023 against the rebuilt image
RED:    #[pg_test] / unit (no network): l2 banned-token catches 'pg_ls_dir(' ; single-statement rejects interior ';' ; select/with anchor
GREEN:  implement nl.rs + wrapper
VERIFY: python3 -m pytest benchmarks/tests/test_nl_sql.py -v (35, 1 real-skip)
```

#### Concurrency tests (only)
(none — single-threaded) — one synchronous validation per call.

#### Acceptance Criteria
- [ ] `ai.nl_to_sql` is Rust + every injection case rejected with 22023 — asserted by `pytest benchmarks/tests/test_nl_sql.py` (exit 0) against the rebuilt image.
- [ ] L1 prompt byte-identical — verified by `grep -q "read-only PostgreSQL SELECT" theodb_rs/src/nl.rs` and the stub routing benign queries green.
- [ ] `ai.nl_to_sql` SQL-callable `(text,text[],text)` + REVOKE FROM PUBLIC — `psql` `to_regprocedure('ai.nl_to_sql(text,text[],text)') IS NOT NULL` true + `test_nl_to_sql_revoked_from_public` green.
- [ ] Quality gates — `cargo clippy --features pg17 -- -D warnings` exit 0; no new crate (`git diff theodb_rs/Cargo.toml` empty).

#### DoD
- [ ] nl_to_sql Rust; test_nl_sql.py green; clippy clean; no sqlparser/regex crate.

### T1.2 — `nl_query` Rust port (L3 read-only sandbox)

#### Objective
Port the validate-then-execute read-only sandbox to Rust at parity (write → 25006).

#### Why this step
1. **What:** `nl.rs` `nl_query(question, allowed, model, max_rows) -> JsonB`: max_rows guard (22023); call `ai.nl_to_sql` via SPI; `set_config('transaction_read_only','on',true)` + `set_config('statement_timeout','5000',true)` via Spi; EXECUTE `SELECT coalesce(jsonb_agg(row_to_json(t)),'[]') FROM (<validated>) t LIMIT max_rows` via Spi. `lib.rs` extern + wrapper.
2. **Why now:** L3 completes the L1–L4 DoD. Cites sql/60:124-160.

#### Files to edit
```
theodb_rs/src/nl.rs — nl_query (SPI sandbox + execute)
theodb_rs/src/lib.rs — #[pg_extern] _nl_query + ai.nl_query wrapper (REVOKE)
```

#### TDD
```
RED:    test_nl_sql.py nl_query benign (rows==2), inject write blocked (25006 / 22023), max_rows<=0 -> 22023, CTE, generated-LIMIT no-syntax-error, read-only fail-safe
GREEN:  implement nl_query
VERIFY: python3 -m pytest benchmarks/tests/test_nl_sql.py -v
```

#### Concurrency tests (only)
(none — single-threaded)

#### Failure-scenario note
The read-only sandbox + a write attempt is the negative case (25006) — covered in `## Failure scenarios`.

#### Acceptance Criteria
- [ ] `ai.nl_query` is Rust; benign returns rows, a write reaching execution raises `25006`, max_rows≤0 → 22023 — asserted by `pytest benchmarks/tests/test_nl_sql.py -k nl_query` (exit 0).
- [ ] `sql/61` (config layer) still resolves `ai.nl_query` — `pytest benchmarks/tests/test_nl_sql.py -k cfg` (exit 0).

#### DoD
- [ ] nl_query Rust; test_nl_sql.py green incl. sandbox + config layer.

## Phase 2: `hybrid.rs` — RRF via SPI

### T2.1 — `ai.hybrid_search_rrf` + `ai.hybrid_search` Rust (orchestrate the RRF SQL via SPI)

#### Objective
Port hybrid search to Rust building+running the identical RRF dynamic SQL.

#### Why this step
1. **What:** `theodb_rs/src/hybrid.rs` — build the `%I`-quoted RRF SQL (RANK per leg, FULL OUTER JOIN, `1/(k+rank)`, `'english'` pin, `score DESC, id ASC`), preserve the embed-seam guard (`to_regprocedure('theodb.embed(text,text)')` → 0A000) + validation (k/per_leg_limit/result_limit>0, ≥1 query input → 22023); run via Spi returning the rows. `lib.rs` externs + `ai.hybrid_search_rrf`/`ai.hybrid_search(jsonb)` wrappers (RETURNS TABLE(id text, score real); REVOKE).
2. **Why now:** DoD-2 hybrid. Cites sql/40 verbatim + blueprint ADR-C.

#### Files to edit
```
theodb_rs/src/hybrid.rs (NEW) — RRF SQL builder + SPI exec + validation + seam guard
theodb_rs/src/lib.rs — externs + ai.hybrid_search_rrf / ai.hybrid_search(jsonb) wrappers (REVOKE)
```

#### TDD
```
RED:    test_unified.py + test_hybrid (RRF twin) — fused order/scores, empty leg, tie-break id ASC; validation 22023; embed-seam absent -> 0A000
GREEN:  implement hybrid.rs
VERIFY: python3 -m pytest benchmarks/tests/test_unified.py -v ; existing hybrid behavior green
```

#### Concurrency tests (only)
(none — single-threaded)

#### Acceptance Criteria
- [ ] `ai.hybrid_search_rrf`/`ai.hybrid_search` are Rust; fused results match the RRF math (tie-break id ASC) + validation 22023 + embed-seam 0A000 — asserted by `pytest benchmarks/tests/test_unified.py` (exit 0) and the hybrid behavior preserved.
- [ ] Injection-safety: `%I`/regclass identifiers — hostile column/table names rejected/quoted (no injection).

#### DoD
- [ ] hybrid.rs Rust; test_unified.py green; RRF parity.

## Phase 3: `migrate.rs` — import_pinecone via SPI

### T3.1 — `theodb.import_pinecone` Rust (jsonb→INSERT via SPI)

#### Objective
Port the atomic Pinecone import to Rust at parity; keep the chunked PROCEDURE in plpgsql (ADR D4).

#### Why this step
1. **What:** `theodb_rs/src/migrate.rs` — `import_pinecone(target regclass, export JsonB, id_col, embedding_col, metadata_col) -> i32`: validate array + per-record id/values (22023); build `%I`-safe `INSERT INTO %s (%I,%I,%I) VALUES ($1,$2::vector,$3)`; loop records (serde_json) + INSERT via Spi with bound params; return count. `lib.rs` extern + `theodb.import_pinecone` wrapper (REVOKE). The chunked PROCEDURE stays plpgsql (sql/80).
2. **Why now:** DoD-2 import. Cites sql/80 + ADR D4.

#### Files to edit
```
theodb_rs/src/migrate.rs (NEW) — import_pinecone jsonb loop + INSERT via SPI
theodb_rs/src/lib.rs — #[pg_extern] _import_pinecone + theodb.import_pinecone wrapper (REVOKE)
sql/80-theodb-migrate.sql — remove the plpgsql FUNCTION import_pinecone (now Rust); keep import_pinecone_chunked PROCEDURE (D4)
```

#### TDD
```
RED:    test_unified.py import cases — maps 2 records (metadata+embedding), non-array/missing-values -> 22023 no partial insert, hostile identifiers safe (%I), dim mismatch -> 22xxx no corrupt insert
GREEN:  implement migrate.rs + remove plpgsql FUNCTION
VERIFY: python3 -m pytest benchmarks/tests/test_unified.py -k import -v
```

#### Concurrency tests (only)
(none — single-threaded)

#### Acceptance Criteria
- [ ] `theodb.import_pinecone` is Rust; maps records, rejects malformed (22023, no partial insert), %I/regclass-safe, dim-mismatch fails clean — asserted by `pytest benchmarks/tests/test_unified.py -k import` (exit 0).
- [ ] `import_pinecone_chunked` PROCEDURE still works (plpgsql, D4) — its existing test (if any) green.

#### DoD
- [ ] import_pinecone Rust; test_unified.py import cases green.

## Phase 4: SQL side — remove plpython3u, 1.2→1.3 migration, control, README

### T4.1 — Retire plpython3u nl_to_sql, drop from requires, README, default_version 1.3

#### Objective
Make the extension plpython3u-free + the managed-PG limitation gone.

#### Why this step
1. **What:** remove the plpython3u `ai.nl_to_sql` from sql/60 (now Rust); `sql/theodb--1.2--1.3.sql` conditional DROP of legacy plpython3u nl_to_sql (guarded); `theodb.control requires = 'vector, vectorscale'` + `default_version 1.3`; README plpython3u note + CREATE EXTENSION comment removed; Dockerfile ships the new delta.
2. **Why now:** DoD-3. Cites blueprint ADR-D/E + M18 retirement precedent.

#### Files to edit
```
sql/60-theodb-nl.sql — remove the plpython3u nl_to_sql def (keep nl_query? -> nl_query is now Rust too, T1.2; sql/60 keeps only what stays SQL, or is emptied to schema + comments)
sql/theodb--1.2--1.3.sql (NEW) — conditional DROP of plpython3u ai.nl_to_sql (lanname=plpython3u AND not theodb_rs member)
theodb.control — requires = 'vector, vectorscale' ; default_version = '1.3'
Dockerfile — install sql/theodb--1.2--1.3.sql
README.md — remove the "Limitação honesta (plpython3u)" note + drop plpython3u from the CREATE EXTENSION comment
benchmarks/tests/test_ai_retirement.py OR test_nl_retirement.py (NEW) — real 1.2->1.3 upgrade: seed plpython3u nl_to_sql member -> UPDATE TO '1.3' -> dropped -> theodb_rs no clash; requires has no plpython3u
```

#### TDD
```
RED:    test_nl_retirement.py — fresh DB CREATE theodb VERSION '1.2' + seed plpython3u ai.nl_to_sql member + ALTER EXTENSION theodb UPDATE TO '1.3' -> dropped -> CREATE EXTENSION theodb_rs no clash; assert pg_extension.extversion theodb=1.3 ; assert requires excludes plpython3u
RED:    grep -c 'LANGUAGE plpython3u' sql/*.sql (excluding generated 1.0 base) == 0
GREEN:  remove plpython3u, write migration, bump control, update README
VERIFY: python3 -m pytest benchmarks/tests/test_nl_retirement.py benchmarks/tests/test_extension_install.py -v
```

#### Concurrency tests (only)
(none — single-threaded) — DDL migration.

#### Acceptance Criteria
- [ ] Zero `LANGUAGE plpython3u` in the modular sql sources — `grep -rc 'LANGUAGE plpython3u' sql/60-theodb-nl.sql sql/40 sql/50 sql/30 sql/61 sql/70 sql/80` all 0.
- [ ] `theodb.control` `requires` excludes plpython3u + `default_version='1.3'` — `grep -q "requires = 'vector, vectorscale'" theodb.control` and `grep -q "default_version = '1.3'" theodb.control`.
- [ ] Fresh install + upgrade land plpython3u-free — asserted by `pytest benchmarks/tests/test_nl_retirement.py benchmarks/tests/test_extension_install.py` (exit 0); `CREATE EXTENSION theodb` no longer pulls plpython3u.
- [ ] README plpython3u limitation note removed — `grep -c 'plpython3u' README.md` reflects only historical/none in the limitation section.

#### DoD
- [ ] plpython3u-free; requires updated; migration + README; tests green.

## Phase 5: benchmark + Integration Validation

### T5.1 — `bench_nl.py` + report (no-regression)

#### Objective
Measure nl_to_sql Rust vs plpython3u (no-regression, I/O-bound).

#### Files to edit
```
benchmarks/bench_nl.py (NEW) — bench nl_to_sql Rust vs a plpython3u ai.nl_to_sql_py baseline (same chat stub)
docs/benchmarks/m19-nl-rust-vs-plpython.md (NEW) — no-regression report (mean±std, >=3 runs)
```

#### TDD
```
RED:    bench produces finite mean/std for both arms
GREEN:  implement bench_nl.py + write report from a real run vs the rebuilt image
VERIFY: python3 benchmarks/bench_nl.py --endpoint ... --report docs/benchmarks/m19-nl-rust-vs-plpython.md
```

#### Concurrency tests (only)
(none — single-threaded) — serial latency measurement; the read-only sandbox (transaction_read_only) and the chunked PROCEDURE's per-batch COMMIT are single-threaded transaction-control concerns, not concurrent-race paths.

#### Acceptance Criteria
- [ ] `docs/benchmarks/m19-nl-rust-vs-plpython.md` shows nl_to_sql Rust mean±std (≥3 runs) vs plpython3u, honest no-regression framing — produced by `python3 benchmarks/bench_nl.py ... --report ...`.

#### DoD
- [ ] Benchmark report committed; real numbers vs the rebuilt image.

## Coverage Matrix

| # | M19 DoD / requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | nl_to_sql anti-injection (L1/L2/L4) in Rust, parity, injection→22023 | T1.1 | nl.rs + EXPLAIN allowlist; test_nl_sql 12 injection cases |
| 2 | nl_query L3 read-only sandbox in Rust, parity (write→25006) | T1.2 | nl.rs nl_query; test_nl_sql sandbox cases |
| 3 | hybrid (RRF) in Rust, parity | T2.1 | hybrid.rs RRF via SPI; test_unified |
| 4 | import_pinecone in Rust, parity | T3.1 | migrate.rs jsonb→INSERT via SPI; test_unified import |
| 5 | chunked PROCEDURE handled (plpgsql, D4) | T3.1 | kept plpgsql (no plpython3u/dep) |
| 6 | zero plpython3u in sql/ | T4.1 | remove nl_to_sql plpython3u; grep 0 |
| 7 | plpython3u removed from requires | T4.1 | control requires = vector, vectorscale |
| 8 | 1.2→1.3 retirement migration + default_version 1.3 | T4.1 | theodb--1.2--1.3.sql + control |
| 9 | README plpython3u limitation removed | T4.1 | README edit |
| 10 | benchmark (nl Rust vs plpython3u) | T5.1 | bench_nl.py + report |

**Coverage: 10/10 requirements covered (100%).**

## Global Definition of Done

- [ ] All phases complete.
- [ ] `test_nl_sql.py` (35) + `test_unified.py` (11) + `test_hybrid.py` (14) green vs the rebuilt image (UNCHANGED test files); every injection case rejected with the same SQLSTATE.
- [ ] The embed/ai oracle (test_embed_*, test_ai_sql, test_ai_edge) green UNCHANGED (no regression).
- [ ] `test_nl_retirement.py` + `test_extension_install.py` green (fresh + upgrade; extversion 1.3; requires sans plpython3u).
- [ ] `cargo clippy --features pg17 -- -D warnings` clean; `ruff` clean; NO new crate (no sqlparser/regex).
- [ ] Zero `LANGUAGE plpython3u` in the modular `sql/` sources; `plpython3u` not in `theodb.control requires`.
- [ ] `docs/benchmarks/m19-nl-rust-vs-plpython.md` present (no-regression, mean±std, ≥3 runs).
- [ ] CHANGELOG `[Unreleased]` updated; README plpython3u note removed.
- [ ] Backward compat: `ai.nl_to_sql`/`ai.nl_query`/`ai.hybrid_search*`/`theodb.import_pinecone` signatures/RETURNS unchanged; SQL-callable; `import_pinecone_chunked` PROCEDURE intact.
- [ ] File-size budget ≤ 500 lines per changed file.

## Failure scenarios (external I/O + adversarial input)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| chat endpoint (nl_to_sql L1) | injection in model output (drop/multi/exfil/relation/comma-join/quoted/stat_file) | `__nlinject_*` stub seams | 22023 reject, DB unmodified (test_nl_sql) |
| chat endpoint | unreachable / 5xx | (reuses embed/chat retry via ai._chat) | typed 38000 fail-fast |
| read-only sandbox (nl_query L3) | a write reaching execution | `__nlinject_funcwrite__` (nextval) | 25006 read_only_sql_transaction; no mutation |
| EXPLAIN (L4) | query does not plan (unknown relation/syntax) | crafted SQL | fail-closed 22023 "query did not plan" |
| import endpoint (jsonb) | non-array / missing values / dim mismatch / hostile identifiers | test_unified import cases | 22023 / 22xxx, no partial/corrupt insert, %I-safe |

## Final Phase: Integration Validation (MANDATORY)

> Runs after Phases 1–5. NOT done until the full chain + benchmark pass.

### Execution
```
docker build -t theo-db:m19 .
docker run -d --add-host=host.docker.internal:host-gateway ... theo-db:m19
python3 -m pytest benchmarks/tests/test_nl_sql.py benchmarks/tests/test_unified.py benchmarks/tests/test_hybrid.py \
  benchmarks/tests/test_ai_sql.py benchmarks/tests/test_ai_edge.py benchmarks/tests/test_embed_sql.py \
  benchmarks/tests/test_embed_failure_scenarios.py benchmarks/tests/test_embed_batch.py \
  benchmarks/tests/test_extension_install.py benchmarks/tests/test_nl_retirement.py -v
cargo clippy --features pg17 -- -D warnings ; ruff check benchmarks/
python3 benchmarks/bench_nl.py ... --report docs/benchmarks/m19-nl-rust-vs-plpython.md
```

### Acceptance Criteria
- [ ] All nl/hybrid/unified + embed/ai + install + retirement tests green vs `theo-db:m19`; every injection case rejected with the same SQLSTATE.
- [ ] `cargo clippy -- -D warnings` + `ruff` clean; no new crate.
- [ ] Benchmark report shows no-regression nl numbers (mean±std, ≥3 runs).
- [ ] `grep -rc 'LANGUAGE plpython3u'` over modular sql sources == 0; `theodb.control requires` has no plpython3u.

### If Validation Fails
1. Separate plan-caused from pre-existing failures.
2. Fix all plan-caused failures before declaring complete.
3. Re-run the chain.
