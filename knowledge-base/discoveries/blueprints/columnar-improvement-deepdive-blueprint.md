# Deep-dive: columnar-analytics improvement opportunities (system vs references)

**Date:** 2026-07-26 · **Trigger:** post-M159 (measured 19.4× geomean vs ClickHouse; covered class 7.54×, ≈ pg_mooncake).
**Method:** deep-dive of our columnar path (`theodb_rs/src/am/{columnar,columnar_codec,columnar_agg,df_executor,zonemap}.rs`)
vs the acervo (cstore paper, MonetDB/X100, Morsel, DuckDB, cstore_fdw, Citus columnar, DataFusion, pg_clickhouse `deparse.c`,
Parquet), plus a ground-truth `THEODB_ADMIT_TRACE` run and an empirical pushdown-path flamegraph. Three council agents
(performance-simd, index-storage, research-adr) read the real code + references. **Honest stance: findings are
evidence-backed or marked as inference; "nothing to borrow" and "honest-negative" are reported as such.**

## Executive summary — two independent, real levers + one unmeasured frontier

| Lever | Attacks | Evidence | Verdict |
|---|---|---|---|
| **A — decode→Arrow bridge fix** | the **covered class** residual (7.54× → toward 2-3×) | flamegraph (empirical, directional) + code file:line | **Highest ROI, lowest risk, own-code.** One structural fix improves ALL 32 covered queries at once. |
| **B — expression-routing coverage** | the **11 non-pushdown** (303× drag) | ground-truth admit_trace + DataFusion capability cited | Real but **compound-blocker-limited**: flips FAR fewer queries than an SQL count implies; each slice has a correctness gauntlet. |
| **C — larger-than-RAM (100M)** | unmeasured `[NEEDS-100M]` regime | none yet | **Measure first.** Type-specific encodings are the reference lever (Parquet/ORC), NOT cstore. |

---

## Lever A — the decode→Arrow bridge (the covered-class residual) — HIGHEST ROI

**Finding (council-performance-simd, empirically confirmed).** The covered 32 pushdown queries do NOT run `form_row`
(the M148 Volcano bottleneck) — but they pay a **morally identical, never-before-profiled cost**: the
`Vec<Option<Vec<u8>>>` decode bridge.
- `decode_column` (`columnar_codec.rs:293,308`) allocates **every cell** into a separate heap `Vec<u8>` (`.to_vec()`) →
  millions of tiny allocations per column.
- `build_arrow` (`df_executor.rs:49-137`) then **re-reads every cell** (`from_le_bytes` / `from_utf8_lossy`) and copies it
  into an Arrow array — a **second full pass + copy**.

**Empirical confirmation** (flamegraph, pushdown path, q23 covered GROUP BY, `enable_columnar_agg=on`, 318 samples —
DIRECTIONAL, below the 500-sample bar, one text-heavy query): top user-space self-time is `build_arrow` (1.92B),
`decode_column` (1.31B), `malloc`+`cfree` (2.4B combined), and a **kernel page-fault storm** (`clear_page_erms` 3.18B +
`asm_exc_page_fault` + folio/lru/mmap) from that allocation churn; DataFusion's `GroupedHashAggregate` compute is **~absent**.
→ **the decode bridge, not compute, is the covered-class bottleneck.**

**Fix (own-code, PG-safety-neutral, A/B byte-identical).** For fixed-width non-null columns, the zstd-decompressed `raw`
buffer (`columnar.rs:894`) is **already** `[val0_LE][val1_LE]…` = the exact Arrow `Int32Array`/`Int64Array`/
`TimestampMicrosecondArray` data-buffer layout. Convert via one `arrow::buffer::Buffer::from_vec(raw)` (zero per-cell,
one memcpy / zero-copy) instead of 1M `.to_vec()` + 1M `from_le_bytes`. Runs on the main thread, touches no `pg_sys`.
(Text/varlena + nullable keep a copy path; ClickBench's covered class is mostly fixed-width ints/timestamps.)

**Secondary (sequenced after A):** (A2) stream ~L2-resident batches (~8192 rows, MonetDB/X100) instead of one 370 MB
batch (M158) → cache-residency + decode↔aggregate pipelining; (A3) multi-core compute — tokio `rt-multi-thread` is **not
even compiled** (`Cargo.toml:57` `features=["rt"]`) + `with_target_partitions(1)` (`df_executor.rs:429`); PG-safety
allows it (decode stays main-thread PG-bound; pure-Arrow compute parallelizes) but **Amdahl-capped ~2×** by serial decode,
so only worth it AFTER A/A2.

**Why highest ROI:** ONE structural fix improves all 32 covered queries; own-code; low risk; measurement-validated.
**Measurement-first gate (Phase 1):** re-run the pushdown flamegraph on a **pure-int** covered query at **≥500 samples**
to confirm `decode_column`/`build_arrow` self-time before building (this run was text-heavy + 318 samples — directional).

**Irreducible floor (honest):** decode must read pages via PG's buffer manager on the single backend thread under an MVCC
snapshot, and results re-materialize to Datums on that thread. That is a **fraction** of 7.54×, not the whole — but it is
why no PG extension reaches DuckDB's 1.8× (DuckDB owns its storage + threads; pg_mooncake only gets 6.2× by embedding DuckDB).

---

## Lever B — expression-routing coverage (the 11 non-pushdown) — real but compound-limited

**Finding (council-research-adr, ground-truth admit_trace).** For every decline class, the limitation is **TheoDB's
routing/serialization, not DataFusion** — our own `run_columnar_grouped_aggs` already hands arbitrary `Vec<Expr>` to
`.aggregate()` (`df_executor.rs:498`; M157's `date_trunc` proves it, q42 at 2.99×). **Corrections to the naive thesis:**
1. **Multi-key GROUP BY is NOT a blocker** — bare-Var multi-key already routes (q31/32/33 `pushdown=yes`). REFUTED.
2. **`HAVING` and text-key `AGG_SORTED`-under-`LIMIT` are real blockers** (q27/q28; q17).
3. **Blockers COMPOUND** (M152's measured lesson): q28 has 4 blockers; routing one class flips far fewer queries than an
   SQL count implies. The "each class adds K queries" model is optimistic and already falsified.

**Ranked slices (marginal coverage = queries where the class is the SOLE remaining blocker; correctness gauntlet per Q2):**

| Rank | Slice | Flips (sole) | Safety | Cost | Correctness gauntlet |
|---|---|---|---|---|---|
| 1 | Expression GROUP BY keys — **safe subset** (const `1`; integer `col±k`; `extract(epoch-invariant unit)`) | q34, q35, q18 = **3** | High | Med (reuses M157 group-expr channel) | int is collation-free; `extract` reuses the M157 minute/hour/second/day epoch whitelist; guard int overflow (PG errors 22003, Arrow wraps) |
| 2 | Integer **IN-list** WHERE (`ScalarArrayOpExpr`, never inspected today) | q40 = **1** | **Highest** | **Low** | int equality = OR of shipped `=`; decline `IN (NULL,…)` (pg_clickhouse `deparse.c:1096`) |
| 3 | Computed-expr aggregate, integer (`SUM(col+k)`) | q29 = **1** | Med | Med | int4 overflow gauntlet |
| 4 | `HAVING` absorption | 0 alone (unlocks q27 with #3) | Med | High (Agg-swap absorbs a filter node) | filter on agg output |
| 5 | Text `MIN`/`MAX` | q21, q22 = 2 | **Low — TRAP** | Med + C/POSIX gate | M158 lesson: DataFusion byte-order ≠ PG collation order; safe only under C/POSIX |
| — | `regexp_replace` group key (q28) | — | **Unroutable** | — | **Honest-negative**: Rust/RE2 ≠ PG POSIX ERE (M156 precedent) — leave declined |

**Realistic coverage gain:** ~5-6 of 11 flippable via the SAFE slices (#1+#2+#3); text MIN/MAX is a trap, regexp is a
declared honest-negative, HAVING-stacked queries need multiple slices. **NOT "+11".** Honest upper bound ~3 from the top
slice alone.

**Mechanism (research-adr):** the canonical model is pg_clickhouse's `foreign_expr_walker` (`deparse.c` — allowlist +
collation-as-first-class-state + recurse-all-shippable). But a FULL walker needs a real `custom_private` serializer
(today's 3-channel carries only `Integer|String` leaves; `nodeToString`/`stringToNode` is the idiomatic upgrade).
**Recommendation: BOUNDED generalization** — extend the M157 `GroupFunc` enum + a small allowlist, each cleared through
the gauntlet — NOT a full walker (YAGNI + serialization tax).

---

## Lever C — larger-than-RAM (100M) — MEASURE FIRST

M159 is 1M (fits in RAM → `shared_blks_read ≈ 0`, so I/O is not today's signal). At 100M the non-pushdown cliff likely
widens and I/O/decode becomes the lever. **cstore_fdw is a dead reference** here (pglz-only, worse than our zstd, no
encodings). The real lever is **type-specific lightweight encodings BEFORE the general zstd** — delta (sorted/temporal),
dictionary/RLE (low-cardinality), frame-of-reference (integers) — for which **Parquet/ORC/ClickHouse are the references**
(`references/parquet-format/`), not cstore. **But do NOT build before measuring at 100M** (the M159 `[NEEDS-100M]` run on
a c6a.4xlarge) — the gap shape at scale is currently unknown and building on a guess violates rule 5.

---

## Smaller levers (honest, lower priority)

- **`chunk_group_rows` as a per-table reloption** (cstore has it; we hardcode `const 10000`, `columnar_codec.rs:23`).
  Cheap knob: smaller chunks = finer zone-map skip → helps the selective-WHERE minority (q19 point-filter, 148×). Low
  priority (M148: skip is not the dominant cost).
- **Collation-aware text/date zone-maps** (cstore technique). Real capability gap (we're numeric-only, `zonemap.rs:42`)
  but needs a **format bump** (fixed 8-byte min/max, `columnar_codec.rs:118`) and high-card text ranges rarely skip. Low ROI.
- **Per-chunk bloom filter** for equality point-lookups (q19) — Parquet/DuckDB technique; a real lever for `col = const`
  when the column isn't sorted (zone-map [min,max] can't skip). Candidate for the point-lookup class only.

## What this deep-dive did NOT find (honest negatives)

- **Nothing borrowable as CODE** from cstore_fdw/Citus (Rule 9): same row-at-a-time bug, worse compression, no crash-safety.
- **No columnar paradigm gap** worth chasing beyond the decode bridge — the covered-class 7.54× is mostly the fixable
  bridge, not an intrinsic wall (unlike the vector-QPS gap of ADR-0033/0035, which stays out of scope).
- **regexp group keys** are an honest-negative (dialect mismatch) — will not route.

## Recommended next milestone (measurement-first, five-question ready)

**M160 candidate — "zero-copy fixed-width decode into Arrow" (Lever A):** highest ROI, lowest risk, own-code, one fix for
32 queries. Gate: (Phase 1) confirm the bridge self-time with a ≥500-sample pure-int pushdown flamegraph; (Phase 6) A/B
byte-identical + the M159 harness re-run showing the covered-class geomean drop. **Lever B (bounded expression routing,
slices #1+#2)** is the natural follow-on for coverage, ADR-gated per M151/M153/M157/M158, with realistic ~+3-5 query
expectation (not +11). Lever C waits on a 100M measurement.

## Cross-references
- Measured baseline: `docs/benchmarks/m159-clickhouse-gap-verdict.md` + `m159-artifacts/`
- Bottleneck lineage: `docs/benchmarks/m148-flamegraph-scan.md` (Volcano) + this doc's pushdown flamegraph (318 samples, directional)
- Routing precedents: M151 (cross-type), M153 (text-group collation), M156 (text WHERE + regex decline), M157 (date_trunc epoch whitelist), M158 (text sort-key C/POSIX)
- References: `references/papers/{cstore-stonebraker-2005,monetdb-x100-boncz-2005,morsel-parallelism-leis-2014}.pdf`, `references/{datafusion,duckdb,cstore_fdw,citus,parquet-format}/`, `references/pg_clickhouse/src/deparse.c`
- Method: `.claude/skills/theodb-evolution` (five-question gate, invariant catalog, measurement-first)
