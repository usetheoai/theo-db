# M120 — Fail-closed structured filter for `ai.hybrid_search` (security evidence)

Date: 2026-07-20 · Validated in-PG (pgrx-managed PG17, theodb_rs installed).

## What changed

`ai.hybrid_search` (jsonb surface) accepts a new **structured `filter`** key — `[{col, op, value}]` — as the
**fail-closed** alternative to the raw caller-privilege `filter_sql`. Composed in Rust
(`theodb_rs/src/hybrid.rs::compose_structured_filter`) with:

- **Identifier** → `pgrx::spi::quote_identifier` (`%I`).
- **Value** → `pgrx::spi::quote_literal` (`%L`) for strings, bare numeric for numbers, `true`/`false` for bools.
- **Operator** → a fixed allowlist `= < > <= >= <> IN &&`; anything else is a typed **SQLSTATE 22023** (fail-closed).
- `filter` and `filter_sql` are **mutually exclusive** (both set → 22023). `filter_sql` is retained as an opt-in,
  documented **raw caller-privilege** escape hatch (no "injection-safe" claim — it never was).

Closes the council-security F1 finding (backlog / M53): the raw `filter_sql` guard was a syntactic blacklist, not
a parser. The structured path is the only fail-closed option for untrusted / multi-tenant callers.

## Evidence (A/B in-PG — reproducible)

Script: 200-row table `h(id, cat, tsv, v)`, vector leg only (`query_vector`, no embed).

| # | Test | Result |
|---|---|---|
| 1 | **Parity**: structured `[{col:cat, op:=, value:1}]` vs `filter_sql:"cat = 1"` | `structured_rows=20`, `filter_sql_rows=20`, `common=20`, **`parity_ok = t`** ✅ |
| 2 | **Un-allowlisted operator** (`op: "; DROP TABLE h; --"`) | **`SQLSTATE 22023`** (rejected) ✅ |
| 3 | **`filter` + `filter_sql` together** | **`SQLSTATE 22023`** (mutual exclusion) ✅ |
| 4 | **Injection value** (`value: "1); DROP TABLE h; --"`) | quoted-as-literal → **table `h` SURVIVES** (the DROP never executed) ✅ |

Reproduction: `scratchpad/m120_ab.sql` against the installed extension (positive parity + the three fail-closed
assertions). All four pass; no regression to the existing `filter_sql` path (parity is byte-for-byte on the id-set).

## Honest boundary

- The structured filter is **less expressive** than raw SQL (no subqueries/functions) — that is the fail-closed
  point. Advanced raw predicates remain available via `filter_sql` at caller privilege.
- Operators covered: `= < > <= >= <> IN &&`. Extending the allowlist is a one-line change + a test.
- This hardens the **relational filter composition** only; the embed/HTTP leg's SSRF guards are unchanged (M65).
