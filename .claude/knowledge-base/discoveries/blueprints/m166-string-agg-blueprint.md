# Discovery blueprint — M166 string aggregates + wide SUM(expr) (q21, q22, q27, q29)

**Date:** 2026-07-27 · **Cycle:** discover (for M166) · council-index-storage + web SOTA.

## Per-query verdict (ranked by leverage × safety × surface)

| Query | Leverage | Blocker (file:line) | Safe? | Verdict |
|---|---|---|---|---|
| **q29** | 567× | `agg_over_expression` `columnar_agg.rs:935` (OpExpr agg arg) | **YES** (int2-col class) | **ROUTE** |
| q27 | 817× | `agg_over_expression` `columnar_agg.rs:935` (FuncExpr agg arg) | conditional (UTF-8) | STRETCH / evaluate |
| q21 | 300× | `minmax_over_unordered_text` `columnar_agg.rs:652` | NO (collation) | HONEST-NEGATIVE |
| q22 | 260× | `minmax_over_unordered_text` `columnar_agg.rs:652` | NO (collation) | HONEST-NEGATIVE |

## q29 — SUM(int2_col ± const) — the clean win (ROUTE)

`SUM(ResolutionWidth + k)` declines because the arg is a `T_OpExpr` (`col + const`), not a bare `Var` (`:935`).
**`ResolutionWidth` is `SMALLINT` (int2)** (confirmed in `benchmarks/clickbench/theodb/create.sql`), so `int2 + int4_const`
has an int4 result whose per-row value (max 32767 + const) **cannot overflow int4** — the M161 per-row-22003 concern
does NOT arise. The Int64 SUM is exact; `SUM(int)→int8` is the existing `SumInt`→Int64 path. **Fail-closed:** admit only
`Var(int2) ± Const(int)` with int4 result; decline int4-col / int8-result (per-row overflow reachable in general).
Surface: mirror the GROUP BY `IntAddConst` OpExpr branch (`columnar_agg.rs:785-860`) into the agg-arg path at `:934`;
new `AggSpec::SumIntAddConst(col, delta)` pushing `sum(cast(col→Int64) + lit(delta))`; decode via the existing SumInt path.

## q27 — AVG(length(URL)) — stretch (evaluate during implement)

`length(URL)` is a `T_FuncExpr` agg arg → `agg_over_expression` `:935`. Routable ONLY under **UTF-8 server encoding**
(PG `length`=char count = DataFusion `char_length` for valid UTF-8; NOT `octet_length`). Needs a NEW scalar-func-in-agg
mechanism + `GetDatabaseEncoding()==PG_UTF8` gate + verify the `HAVING COUNT(*) > 100000` survives the Agg-swap. Highest
leverage but new mechanism + encoding correctness — evaluate; defer if risky.

## q21/q22 — MIN(text) — honest-negative (collation)

DataFusion `MIN(Utf8)` = byte-minimum (memcmp); PG `min(text)` = **collation-minimum** (`varstr_cmp`). A deterministic
collation constrains *equality*, not *order* — so even `en_US` (deterministic) orders differently from byte order →
routing gives a WRONG min, A/B-visible. Safe ONLY under `C`/`POSIX` collation (`varcollid ∈ {950,951}`), and ClickBench
columns carry `DEFAULT_COLLATION_OID (100)` → a C-only gate declines them in practice. So implementing the gate would
NOT route q21/q22 on the benchmark (YAGNI). **Honest-negative** — the correct fix (collation-aware executor min/max) is
a separate deferred capability, same class as the M165 q17 honest-negative.

## SOTA (R0)

PG `min/max(text)` is collation-ordered (C/POSIX = byte order); non-C = locale order
([PG collation docs](https://www.postgresql.org/docs/current/collation.html)). DataFusion string min/max = Arrow
`StringArray` min = Rust `str` Ord = memcmp. They coincide only for C/POSIX + valid UTF-8. Confirms the q21/q22 honest-negative.

## Invariants
- Byte-identical A/B (the wrong-MIN / wrong-SUM cases are A/B-visible — the oracle catches them).
- q29 fail-closed: int2-col ± int-const with int4 result only; decline otherwise.
- No page-format/WAL/crash-safety surface (read-path admit routing only).

## References (resolve on disk)
- `theodb_rs/src/am/columnar_agg.rs` (`parse_agg_kind:627`, agg-arg check `:935`, `minmax_over_unordered_text:652`, IntAddConst `:785-860`)
- `theodb_rs/src/am/df_executor.rs` (`AggSpec`/`MinCol`/`MaxCol:266`, `push_agg_exprs:318`, `agg_datum:837`)
- `theodb_rs/src/am/columnar.rs` (`minmax_kind_of:530`)
- `benchmarks/clickbench/theodb/create.sql` (ResolutionWidth SMALLINT)
- `docs/benchmarks/clickbench-fresh-vs-clickhouse-2026-07-27.md` (q21=300×, q22=260×, q27=817×, q29=567×)
