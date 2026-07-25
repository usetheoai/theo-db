# Review — M156 text-WHERE predicate pushdown (develop)

**Date:** 2026-07-25
**Slug:** m156-text-where
**Commits reviewed:** `d360413` (feature) → `258026a` (review fixes) → `77b9596`/`b2b0b6c`/`d668060` (docs)
**Verdict:** READY_TO_MERGE

## Scope

Route text WHERE predicates (`=`, `<>`, `~~`/LIKE, `!~~`/NOT LIKE) to the vectorized columnar aggregate CustomScan
via a 2nd `custom_private` channel (`Integer`/`String` nodes) + a DataFusion Utf8 filter arm. Files:
`theodb_rs/src/am/{zonemap.rs, columnar_agg.rs, df_executor.rs}`.

## Method — 3 adversarial councils (each proves the shape space the ClickBench A/B does not exercise)

| Council | Lens | Findings |
|---|---|---|
| council-rust-pgrx | unsafe / FFI / panic-across-C | **1 HIGH** (fixed), 1 LOW comment (fixed); rest LIMPO |
| council-index-storage | correctness vs PG + serialization | **1 MEDIUM** (fixed); all other guards correct vs PG primary source |
| council-benchmark | number honesty vs raw artifact | HONEST on all 7 headline points; 1 doc blemish (EC-7) → fixed |

## Findings + resolution

### BLOCKER / HIGH

- **[HIGH → FIXED] `String::from_datum` panics in the planner under non-UTF-8 server encoding** (council-rust-pgrx).
  `columnar_agg.rs:363` — `String::from_datum`/`<&str>::from_datum` assert UTF-8 and PANIC on a non-ASCII byte under
  LATIN1/WIN1252 (Ascii policy) or SQL_ASCII (strict), turning a valid query into a planner ERROR inside the
  upper-paths hook. **Fix (`258026a`):** read via `text_to_cstring` (raw payload, no assertion) + decline fail-closed
  on invalid UTF-8. Re-proven on-box by **EC-6** (LATIN1 `= chr(233)` → Seq Scan, 100=100, no panic).

### MEDIUM

- **[MEDIUM → FIXED] LIKE/NOT LIKE dangling-escape divergence** (council-index-storage). A pattern ending in an odd
  number of `\`: PG rejects with ERROR 22025 (`like_match.c`, data-dependent) while arrow treats the trailing `\` as
  a literal and returns rows. The columnar path can never replicate PG either way. **Fix (`258026a`):** decline such
  patterns at plan time (`=`/`<>` unaffected). Re-proven by **EC-7** (Seq Scan decline; the PG-error is data-dependent
  and cited, not claimed-from-log — corrected per council-benchmark).

### LOW / informational (addressed or accepted)

- **[LOW → FIXED]** misleading comment attributing `varattno ≤ 0` rejection to `checked_sub`; replaced with an explicit
  `varattno >= 1` guard (`258026a`).
- **[LOW → FIXED]** doc EC-7 characterization ("native ERROR 22025") not shown in the log; corrected to state the
  proven decline + data-dependency of the PG error (`d668060`).
- **[INFO — accepted]** `varchar` via `RelabelType` and system-col Vars decline (fail-closed, correct). Non-UTF-8
  *column* decode assumption is pre-existing (M153), not introduced here.

## Guards verified correct vs PostgreSQL primary source (council-index-storage)

Collation-determinism guard (necessary + sufficient for `=`/`<>` AND LIKE — PG also errors on LIKE under
non-deterministic collation, so declining is consistent); bpchar (1042) exclusion (`bpchareq` trims trailing blanks,
M153); operator whitelist via `FirstNormalObjectId` + `get_opname` (no cross-type risk — both operands already
text/varchar); default `\` escape (DataFusion planner `escape_char.unwrap_or('\\')` = PG default); NULL 3-valued;
min/max fast-path disabled when text predicates present.

## Measured evidence (council-benchmark — HONEST vs raw JSON/log)

- Coverage `columnar_customscan_count` **21 → 31** (+10: q10,11,12,13,14,20,30,31,36,37) — verified by independent set
  difference against the M153 baseline JSON.
- `result_ab.diverged = 0` (byte-identical, 43/43 ok, 0 errored) in **both** regimes (head 100k + systematic 300k),
  which agree exactly (anti-sampling-bias control per the M155 lesson).
- No unsupported speed claim; non-canonical box declared; no ClickHouse baseline claimed. A/B non-vacuous.

## Hard gates (cycle-review)

- No failing tests on the branch (A/B byte-identical is the executable oracle; `cargo pgrx test` does not link — validated in-PG).
- No new secrets committed. No direct commit to `main`. No `Co-Authored-By` trailer on any M156 commit.
- CHANGELOG `[Unreleased]` updated (Rule 6).
- `/code-quality`: no `symbol_fabrication` / `dead_code` — the new symbols (`TextOp`, `TextPredicate`,
  `extract_text_predicate`, `classify_text_op`, `encode_text_preds`) all have callers/tests; clippy clean on the
  release build (rebuild `REBUILD_EXIT:0`, no warnings on the new code).

## Verdict

**READY_TO_MERGE** — no BLOCKER; the one HIGH and one MEDIUM both fixed and re-proven on-box; benchmark audited honest.
Ephemeral droplet destroyed.
