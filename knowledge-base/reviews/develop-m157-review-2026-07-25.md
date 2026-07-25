# Review — M157 date_trunc GROUP BY expression pushdown (develop)

**Date:** 2026-07-25
**Slug:** m157-expr-group
**Commits reviewed:** `dc8b697` (feature) → `581bf10`/`f3ab762` (swap fixes) → `8353ddc` (rust-pgrx LOW) → `8ece562` (index-storage CRITICAL) → docs
**Verdict:** READY_TO_MERGE

## Scope

Route `GROUP BY date_trunc('unit', ts::timestamp)` — an EXPRESSION group key — to the vectorized columnar aggregate
CustomScan via a 3rd `custom_private` channel + a DataFusion `date_trunc` group expression. Files:
`theodb_rs/src/am/{columnar_agg.rs, df_executor.rs}`.

## Method — 3 adversarial councils (each proves what the ClickBench A/B does not)

| Council | Lens | Findings |
|---|---|---|
| council-rust-pgrx | unsafe / FFI / panic-across-C | LIMPO + **2 LOW** (both fixed) |
| council-index-storage | correctness vs PG + serialization | **1 CRITICAL** (fixed) — the rest of the guards correct vs PG primary source |
| council-benchmark | number honesty vs raw artifact | LIMPO (KEY-exact A/B; the prior false-green is documented) |

## Findings + resolution

### CRITICAL

- **[CRITICAL → FIXED] `date_trunc('month'/'quarter'/'year')` diverged from PG (calendar epoch mismatch)**
  (council-index-storage). The columnar TAM stores `timestamp` as `int64 µs since 2000-01-01` (PG epoch) but the Arrow
  decode (`df_executor.rs:111`) reads it as `µs since 1970-01-01`. The 10957-day offset is a whole multiple of
  day/hour/minute/second (epoch-invariant → correct) but NOT of month/quarter/year, so `date_trunc`'s calendar
  truncation lands on the wrong absolute date (wrong key AND wrong partition near bucket boundaries). **The first A/B
  false-greened** by comparing count/sum (which survive the bucket collapse) rather than the key column. **Fix
  (`8ece562`):** restrict the whitelist to `{second,minute,hour,day}`; month/quarter/year/week decline fail-closed.
  q42 uses `minute` → coverage +1 preserved. Re-proven with a **symmetric-EXCEPT KEY-exact** A/B (`day_mism=0`,
  `hour_mism=0`, `minute_mism=0`) + month/quarter/year showing Seq Scan.

### LOW (rust-pgrx, both fixed `8353ddc`)

- `df_executor.rs` group-key sort used `[idx]` (panics across C on a corrupt layout) vs the materialization loop's
  `.get(idx).ok_or()?` — matters for a 0-row grouped result. Fixed → `.get(idx)`.
- `encode_group_exprs` used `unwrap_or_default()` (masks an interior NUL) vs M156's `encode_text_preds` which declines.
  Fixed → return `Option`, decline on NUL. Both unreachable today (validated whitelist); defense-in-depth.

### Guards verified correct vs PostgreSQL primary source (council-index-storage)

Timezone (admit `timestamp` 1114, decline `timestamptz` 1184 — PG `timestamptz_trunc` uses `session_timezone`,
diverges under `TimeZone≠UTC`); ns/µs unit preserved (`return_field_from_args`, no off-by-1000);
`deparse_safe_tlist` tag==2 `makeVar` is descriptor-equal + deparse-only (cosmetic base-column name, no correctness
bug); AGG_SORTED order + Rust gk sort (kind==2) correct; NULL group correct.

## Measured evidence (council-benchmark — HONEST vs raw JSON/log)

- Coverage `columnar_customscan_count` **31 → 32** (+1: q42 `date_trunc('minute', EventTime)`) — verified by recounting
  the per-query arrays; the two regimes' routed sets are byte-identical.
- `result_ab.diverged = 0` (43/43 ok, 0 errored) in both regimes (head 100k + systematic 300k), which agree exactly.
- EC harness A/B is **KEY-exact** (symmetric EXCEPT on the full `(date_trunc key, count)` tuple), non-vacuous.
- No unsupported speed claim; non-canonical box declared; no ClickHouse baseline claimed.

## Diagnostic note

The initial date_trunc decline was root-caused via `THEODB_ADMIT_TRACE=1` (behavior-neutral M152 trace): admit
ADMITTED the key, but `try_swap_agg` declined at `deparse_safe_tlist` because it required a `T_FuncExpr` tlist entry —
post-planning the GroupAggregate tlist entry for an expression group key is a bare `Var`. Fixed in `f3ab762`.

## Hard gates (cycle-review)

- No failing tests on the branch (KEY-exact A/B is the executable oracle; `cargo pgrx test` does not link — validated in-PG).
- No new secrets; no direct commit to `main`; no `Co-Authored-By` trailer on any M157 commit.
- CHANGELOG `[Unreleased]` updated (Rule 6).
- `/code-quality`: new symbols (`GroupExprSpec`, `GroupFunc`, `extract`/`encode_group_exprs`, the T_FuncExpr admission)
  all have callers; the build is clean on the new code (BUILD_EXIT:0; the E0133 `unsafe_op_in_unsafe_fn` warnings are
  pre-existing in `am::columnar`, not M157).

## Verdict

**READY_TO_MERGE** — the one CRITICAL and both LOW findings fixed and re-proven on-box; benchmark audited honest with a
KEY-exact A/B that closes the false-green class.
