# Blueprint — M120 fail-closed structured filter for `ai.hybrid_search_rrf`

- Slug: `hybrid-fail-closed-filter` · Milestone: M120 · Date: 2026-07-20
- Gap (verified in code): `theodb_rs/src/hybrid.rs::run_rrf` composes `filter_sql` (Option<&str>) as **raw** `%5$s`
  into both legs' `WHERE ... AND (%5$s)`. Guard (L147) rejects only `;`/comment/chaining — **syntactic, not a
  parser** (the module docstring L8-11 + the code L144 both name the structured-filter API as the fail-closed
  follow-up). Safe today under SECURITY INVOKER + read-only SPI + REVOKE-FROM-PUBLIC, but a **latent BLOCKER**
  if `ai.hybrid_search_rrf` is exposed in the multi-tenant data-plane (theo-data tenant model).

## Coverage Corner 4 — Techniques (the safe pattern, SOTA-anchored)

**The injection-safe structured predicate** = allowlisted operator + quoted identifier + quoted literal value.
This is the standard parameterized-filter pattern (pgvector/Qdrant/Weaviate all expose a structured filter DSL,
never raw SQL, for exactly this reason). Applied here **without** threading new SPI binds through the fixed
`$1..$6` template:

- **Identifier:** `pgrx::spi::quote_identifier(col)` (confirmed present in pgrx — a safe wrapper over
  `pg_sys::quote_identifier`). Safely quotes/escapes the column name.
- **Value:** `pgrx` literal quoting (`quote_literal` / `pg_sys::quote_literal_cstr`) → the SQL `%L` equivalent.
  Injection-safe string literal. (Verify the exact pgrx 0.19 helper at plan/impl time; `pg_sys::quote_literal_cstr`
  is the fallback.)
- **Operator:** a fixed **allowlist** — `=`, `<`, `>`, `<=`, `>=`, `<>`, `IN`, `&&` (array overlap for labels).
  Anything else → typed 22023 (fail-closed). No operator ever interpolated from free text.
- Composition: `AND (quote_identifier(col) <op> <quoted_value> AND ...)` → the same `%5$s` slot, now a proven-safe
  string. Empty structured filter ⇒ `true` (byte-identical to no filter).

**Prior art to REUSE (Rule 9 — internal):** `theodb_rs/src/nl.rs` — the L2 fail-closed posture (denylist,
SQLSTATE 22023 verbatim, stdlib scanning no-regex, "fail-closed either way"). M120 mirrors this: an
un-allowlisted operator/shape is a typed 22023, never a silent pass.

## Coverage Corner 1 — Integration tests
- Negative: `hybrid_filter_structured_rejects_bad_op` (op not in allowlist → 22023), `_rejects_subquery_value`
  (a value that is a subquery/SQL fragment is quoted-as-literal, so it matches nothing — NOT executed).
- The payload that passed the raw guard (`filter_sql => '(SELECT count(*) FROM t) >= 0'`) has **no structured
  equivalent** — the structured path cannot express a subquery predicate (that IS the point).
- Positive: structured `[{col:"cat", op:"=", value:1}]` returns the same rows as the equivalent safe `filter_sql`.

## Coverage Corner 2 — Dependencies
- None new. `pgrx::spi::quote_identifier` + pgrx/pg_sys literal quoting (already linked). serde_json already used
  by `run_rrf_json`. Parsimony rung 2/4.

## Coverage Corner 3 — Tools
- Validation: A/B in-PG (the project convention — `cargo pgrx test` doesn't link on the droplet). Build+install,
  create a table + hybrid config with a structured `filter`, assert correct rows + the negative 22023 cases.

## ADRs
1. **Structured filter over hardening the raw guard.** Chosen: add a structured `filter` (col/op/value) path.
   Rejected: extend the blacklist on `filter_sql` — rejected because a blacklist can never be complete for raw
   caller SQL (the council-security F1 finding). The structured path is the only *fail-closed* option.
2. **Keep `filter_sql` as opt-in, documented caller-privilege.** Chosen: retain `filter_sql` (backward-compat)
   but the COMMENT/doc drops any "injection-safe" implication and names it raw caller-privilege; the structured
   `filter` is the recommended path for untrusted/multi-tenant callers. Rejected: hard-remove `filter_sql`
   (breaks existing callers).
3. **Quote-literal the value, don't thread new binds.** Chosen: `quote_literal(value)` into `%5$s` (the template's
   `$1..$6` binds are fixed; adding `$7+` for filter values would rewrite the template + arg plumbing for no safety
   gain — quote_literal is already injection-safe). Rejected: parameterized binds (larger change, same safety).

## Open questions (impl-time)
- The exact pgrx 0.19 literal-quoting helper (`quote_literal` vs `pg_sys::quote_literal_cstr`) — verify at impl.
- Value typing: keep values as JSON scalars (string/number/bool) → quote_literal handles the string form; numbers
  emit bare numeric (safe). Arrays (for `IN`/`&&`) → quote each element.

## Prior art cited (resolve on disk)
- `theodb_rs/src/hybrid.rs:92-151` (run_rrf + filter_sql guard), `:270-306` (run_rrf_json config)
- `theodb_rs/src/nl.rs:8-135` (the fail-closed L2 posture to mirror)
- `.claude/knowledge-base/backlog.md` (M53 council-security F1)
- `pgrx-0.19` `spi::quote_identifier`
