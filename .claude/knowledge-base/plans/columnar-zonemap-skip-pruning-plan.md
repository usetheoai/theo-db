# Plan: theodb_columnar zone-map skip-pruning (predicate pushdown consumer)

> **Version 1.1** (2026-07-18 — absorbed EC-1/EC-2 MUST-FIX from `reviews/columnar-zonemap-skip-pruning-edge-cases-2026-07-18.md`: ADR D5 same-domain-or-fallback push gate + T2.2 bounds guard + partial-chunk-group test) — The `theodb_columnar` TAM already **writes** a min/max zone-map per `(chunk_group, column)` but **never reads it to skip anything** (a write-only feature; the comment says "consumed in C2" — C2 was never built). This slice builds the missing consumer: widen the M100 planner CustomScan to accept a simple `WHERE`, carry the predicate to the columnar decode leaf, and skip decoding chunk groups whose zone-map cannot satisfy the predicate — the DataFusion predicate stays the final authority so the result is byte-identical, only the work drops. Measured by an in-PG A/B on a clustered 1M-row table.

## Goal

> "Enable a `WHERE`-filtered aggregate over a `theodb_columnar` table to skip decoding chunk groups whose min/max zone-map cannot satisfy the predicate, measured by the in-PG A/B (`benchmarks/columnar_zonemap_ab.py`) returning a **byte-identical** filtered-aggregate result while the skip path decodes **≤ 25% of the chunk groups** the skip-off baseline decodes, on a clustered column with a ~10%-selective range predicate."

Correctness is a HARD gate: the filtered-aggregate result with skip MUST be byte-identical to skip-off (the skip only avoids work, never changes the answer — the row-level predicate remains the final authority). The ≤25%-chunk-groups metric is measurement-first: the skip ratio tracks selectivity × clustering; the A/B records the measured number.

## Context

The columnar/HTAP pillar has **projection pushdown** (M100: decode only needed columns — measured 9.89× vs seqscan) but **no predicate pushdown**: a `WHERE`-filtered scan decodes every chunk group and the executor discards rows above. The M99/M100/M103 verdicts name this exact gap:

- M99 (`docs/benchmarks/m99-columnar-tam.md`): "does NOT yet have … min/max skip-pruning consumption … M100, where the DataFusion CustomScan … skip chunk groups via the min/max directory this milestone already writes."
- M100 (`docs/benchmarks/m100-datafusion-executor.md`): "Slice 1 covers `count(*)`/`sum(float8)` **without GROUP BY / WHERE**."
- M103 (`docs/benchmarks/m103-vector-columnar.md`): the pruning value "where pruning avoids reading whole segments … is **unproven until measured**."

The zone-map (min/max per chunk group) is the defining columnar advantage on selection — without the consumer, the columnar store only wins on projection, never on filtering. This slice closes that half.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/am/columnar_codec.rs` | 483 | `213138e` (2026-07-16) | On-disk TCS1 format: `ChunkDirEntry` (min/max at offsets 28-44), `MinMaxKind` (I2/I4/I8/F4/F8/Bool), `compute_minmax`, `CHUNK_GROUP_ROWS=10_000` | The 44-byte `ChunkDirEntry` wire layout MUST stay unchanged (read/write compat); `has_minmax=false` MUST remain the un-skippable fail-safe sentinel |
| `theodb_rs/src/am/columnar.rs` | 1599 | `f088e64` (2026-07-16) | The columnar TableAM: `decode_columns` (667-740, the DataFusion feed), `decode_stripe` (599, seqscan), `flush_pending` (write), `read_visible_stripes` (MVCC gate) | `read_visible_stripes` visibility gate MUST run before any skip; the seqscan path (`decode_stripe`) is OUT of this slice and unchanged; file is 3.2× over the 500-LoC budget — new code goes in a sibling module, not here |
| `theodb_rs/src/am/columnar_agg.rs` | 420 | `b66689d` (2026-07-16) | M100 planner CustomScan admission: `admit` (98-200) currently REJECTS any `WHERE` (returns None → native plan) | The `count(*)`/`sum` no-WHERE path MUST keep working unchanged; the CustomScan `custom_private` carry pattern is the plumbing seam |
| `theodb_rs/src/am/df_executor.rs` | 294 | `330b6b8` (2026-07-16) | DataFusion executor: `run_columnar_aggs`/`decode_to_batch` → calls `decode_columns(rel, Some(&proj))` (projection-only) | DataFusion's own predicate evaluation MUST remain the final authority over surviving rows |
| `theodb_rs/src/am/guc.rs` | 379 | `4f9c38c` (2026-07-18) | GUC registry (pgvectorscale pattern); has `EF_SEARCH`, `SYMQG_FASTSCAN`, etc. | Existing GUCs unchanged; new bool GUC added the same way |
| `theodb_rs/src/am/zonemap.rs` (NEW) | 0 | — | (NEW) the pure predicate-descriptor + min/max test | — |
| `benchmarks/columnar_zonemap_ab.py` (NEW) | 0 | — | (NEW) in-PG A/B: skip on/off on a clustered table | — |

Every file in any task's `#### Files to edit` appears here.

### Current callers / dependents

- **Symbol:** `decode_columns(rel, projection)` in `columnar.rs:667`
  - **Callers (production):** `df_executor.rs` (`decode_to_batch`/`run_columnar_aggs`)
  - **Callers (tests):** `df_executor.rs` tests (`m100_projection_decodes_only_aggregated_column`)
  - **External:** no — `pub(crate)`.
- **Symbol:** `admit(...)` in `columnar_agg.rs:98`
  - **Callers (production):** the `upper_paths_hook` at `UPPERREL_GROUP_AGG` (`columnar_agg.rs`)
  - **External:** no.
- **Symbol:** `ChunkDirEntry` / `compute_minmax` / `MinMaxKind` in `columnar_codec.rs`
  - **Callers (production):** `columnar.rs::flush_pending` (write), `decode_columns`/`decode_stripe` (read directory), `deserialize_directory`
  - **Callers (tests):** `columnar.rs` (`m99_stripe_is_column_major`, `theodb_columnar_test_stripe_info`).

### Domain glossary

- **stripe** — the flush unit (one `flush_pending`); its MVCC visibility is a `columnar.stripe` heap-catalog row resolved by `read_visible_stripes`.
- **chunk group** — 10 000-row pruning granule (`CHUNK_GROUP_ROWS`); the unit at which min/max is stored and a scan can skip.
- **zone-map** — per `(chunk_group, column)` `(has_minmax, min_bits, max_bits)` in the `ChunkDirEntry` — already written by `compute_minmax`.
- **`MinMaxKind`** — the comparison domain of a column's zone-map: `I2/I4/I8/F4/F8/Bool` (native cheap order) or `None` (text/varlena/other → un-skippable).
- **admission filter** — a conservative pre-filter that may over-admit but never under-admits; the real predicate downstream is the final authority (the pattern `customscan.rs:29-37` documents for the vector filter).
- **ZonePredicate** — (this slice) a decoded `col <op> const` clause reduced to the column's min/max-bits domain, testable against a chunk group's `(min_bits, max_bits)`.

### Architecture boundaries affected

- `am/columnar_codec.rs` (on-disk format, pure) → the min/max test is pure logic; goes in a NEW pure module `am/zonemap.rs` (no `pg_sys`) so it is standalone-testable and keeps `columnar.rs` from growing (already 3.2× over budget). Preserves `architecture.md § 3` module cohesion.
- `am/columnar_agg.rs` (planner, PG boundary) extracts the `RestrictInfo` into `ZonePredicate`s and carries them via `custom_private`; `am/columnar.rs::decode_columns` (scan, PG boundary) consumes them. `am` may depend on the pure `zonemap` module inward.

## Dependencies

**No new external dependency (Rule 9 — reuse before add).**

| Dependency | Version | Status | Rule 9 justification |
|---|---|---|---|
| `theodb_rs::am::columnar_codec` (in-crate) | current | existing | `ChunkDirEntry` min/max + `MinMaxKind` already exist — the write side is done; this slice reuses them read-side. |
| `pgrx` `GucRegistry` (installed) | 0.19.0 | existing | The kill-switch GUC uses the same `define_bool_guc` already used by `SYMQG_FASTSCAN`. |
| `zstd` (installed) | current | existing | Only relevant because skipping AVOIDS a `zstd::decode_all` call — no new use. |

No `Cargo.toml` change. `/deps-audit` verdict: PASS (no declared dep added).

## Prior Art & Related Work

- **In-repo (the storage this slice consumes):** `theodb_rs/src/am/columnar_codec.rs:100-158` (`ChunkDirEntry` min/max) + `columnar_codec.rs:293-340` (`compute_minmax`) — written by `columnar.rs::flush_pending`. The comment `columnar_codec.rs:101` ("consumed in C2") names the exact missing consumer.
- **In-repo (the plumbing pattern to reuse):** `theodb_rs/src/am/customscan.rs:38-101` — the backend-local side-channel that carries planner state (vector-filter membership) into the exec node; `customscan.rs:29-37` documents the "admission filter, never the final authority" discipline this slice mirrors.
- **In-repo verdicts (the declared gap):** `docs/benchmarks/m99-columnar-tam.md`, `docs/benchmarks/m100-datafusion-executor.md`, `docs/benchmarks/m103-vector-columnar.md`.
- **External literature (min/max skip-pruning, textbook — cited in the code):** Hydra/Citus columnar 10 000-row chunk-group min/max skip (`columnar_codec.rs:23-24`); Parquet row-group statistics + predicate pushdown; DuckDB zone maps. Own-code clean-room; Apache-permissive (D1).

## Objective

- [ ] `am/zonemap.rs::chunk_can_match` exists + the 5 pure `#[test]`s pass via standalone `rustc --test` (fail-safe: `has_minmax=false` → true asserted).
- [ ] `admit` extracts `Vec<ZonePredicate>` from `WHERE col BETWEEN a AND b`; `test_admit_*` green (droplet) incl. cross-type + var-op-var fallback; the M100 no-WHERE tests still green.
- [ ] `decode_columns(rel, projection, predicates, skip)` skips proven-non-matching chunk groups; `test_decode_columns_skips_nonmatching_cg` green (skip-ratio metric shows 2/3 skipped).
- [ ] `SHOW theodb.columnar_zonemap_skip` returns `on`; under `THEODB_SCAN_PROFILE=1` the scan logs `skipped N/M chunk groups` with N>0 in the A/B.
- [ ] `benchmarks/columnar_zonemap_ab.py` asserts `sum_skip_on == sum_skip_off` (byte-identical) AND skip-on decodes ≤ 25% of skip-off's chunk groups (clustered 10%-selective range).

## ADRs

### D1 — The min/max test lives in a NEW pure module `am/zonemap.rs`, not in `columnar.rs`

- **Decision:** Put `ZonePredicate` + `chunk_can_match` in a new `pg_sys`-free module `am/zonemap.rs`, unit-tested standalone (no pgrx link).
- **Rationale:** `columnar.rs` is already 1599 LoC (3.2× over the 500-LoC budget in `architecture.md`); the skip-decision is pure arithmetic on min/max bits (SRP — one reason to change). A pure module is standalone-`rustc`-testable (the box has no pgrx), isolating the correctness-critical fail-safe logic from the PG plumbing.
- **Alternatives considered:** (a) inline the test in `decode_columns` — rejected: grows an over-budget file + couples pure logic to the PG scan, un-testable without a droplet. (b) put it in `columnar_codec.rs` — rejected: the codec is the on-disk *format*; the *predicate test* is a different concern (DRY: don't merge format + query logic).
- **Consequences:** correctness-critical logic is proven off-PG before any droplet build; `columnar.rs` does not grow.

### D2 — Push only a conjunction of `Var(col) <op> Const` on min/max-able columns; everything else falls back

- **Decision:** `admit` extracts ONLY implicit-AND clauses shaped `Var(col) OP Const` where `OP ∈ {<,<=,=,>,>=}` and `col`'s `MinMaxKind` is native (I2/I4/I8/F4/F8/Bool). Any other qual shape (OR, function, non-supported type, join qual) → the clause is ignored for pruning (still applied by the executor); if NO clause is pushable, fall back to the current native plan.
- **Rationale:** Parsimony/YAGNI — the measured win comes from range/equality predicates on ordered columns (the 90% case); OR/complex predicates + text zone-maps are separate scope. A conjunction is sound: a chunk group can be skipped if ANY conjunct excludes it (AND semantics).
- **Alternatives considered:** (a) full expression-tree pushdown (OR, IN, functions) — rejected: large surface, most of it low-value, high bug risk in a correctness-critical path. (b) push text/date via extended `MinMaxKind` — rejected: prerequisite scope (write-side change), deferred.
- **Consequences:** un-pushable predicates are still CORRECT (executor filters them) just not pruned; the slice's value is scoped to ordered-column filters, honestly.

### D3 — The skip is an admission filter; the DataFusion predicate is the final authority (byte-identical gate)

- **Decision:** Skipping a chunk group only avoids `read_chunked` + `zstd` + decode for chunk groups whose min/max PROVE no row can match. Surviving (over-admitted) chunk groups' rows are still filtered by the real predicate in the executor. Result MUST be byte-identical to skip-off.
- **Rationale:** Correctness (Rule 8) + the `customscan.rs:29-37` discipline. A zone-map skip that changed the answer would be a silent-corruption bug (the worst kind). MVCC: prune only WITHIN stripes `read_visible_stripes` already admitted; `has_minmax=false` (unsupported type / all-NULL / all-NaN) → never skip; chunk groups with NULLs pass (min/max is over present values); pending same-xact rows always scanned.
- **Alternatives considered:** trust the zone-map as the final filter (skip the executor re-check) — rejected: unsound (over-admitted rows would leak; NULL semantics break).
- **Consequences:** the correctness gate (byte-identical) is testable and non-negotiable; the win is bounded by clustering, never by cutting the real filter.

### D4 — Goal metric is chunk-groups-decoded ratio + a byte-identical gate; ship the measured number (measurement-first)

- **Decision:** The A/B measures the skip ratio (chunk-groups-decoded skip-on vs skip-off) + wall-clock, with byte-identical result as the hard gate. Ship the measured number (D4 measurement-first, as the E1/E2 verdicts did).
- **Rationale:** `public-copy.md` rule 5 — no performance claim without the benchmark. The skip ratio depends on data clustering (a data-layout property, honestly flagged), so the metric is "≤25% on a clustered 10%-selective range", not an unconditional speedup.
- **Alternatives considered:** claim an unconditional Nx speedup — rejected: dishonest (unsorted data prunes little).
- **Consequences:** the verdict states the measured ratio + the clustering caveat; no over-claim.

### D5 — Same-domain-or-fallback push gate (absorbs EC-1/EC-2)

- **Decision:** `admit` pushes a clause ONLY when the operator is the column-type-NATIVE btree comparison (the operator's input types == the column type) AND the const is in that same type; the `ZoneOp` is resolved from the **btree strategy number** (`BTLessStrategyNumber`…`BTGreaterStrategyNumber` via the operator's opfamily), never a hardcoded per-type OID list; `const_bits` is encoded in the column's `MinMaxKind` domain. Any type/operator/opfamily mismatch → **do not push** (the clause is left to the executor — correct, just unpruned). The consumer additionally guards `p.col < natts` (fail-safe must-scan on any out-of-range predicate).
- **Rationale:** Correctness (Rule 8) is the whole point of a skip-pruner. `compute_minmax` writes min/max in the COLUMN's domain (`columnar_codec.rs:293-340`); comparing a const encoded in a DIFFERENT type's domain (cross-type operator `int48gt`, an int8/numeric literal vs an int4 column) silently mis-orders the exclusion test → skips a matching chunk → wrong `sum` with no error (EC-1, the worst failure class). Resolving via btree strategy number (not OID lists) is robust across all native types and cannot mis-map an operator.
- **Alternatives considered:** (a) coerce the const to the column type at plan time — rejected: coercion can round/truncate (numeric→int4) and re-introduce the domain error; refusing to push is simpler and always correct. (b) hardcode per-type operator OIDs — rejected: fragile, easy to mis-map, misses types.
- **Consequences:** the pushable surface is narrower (only same-type native comparisons) but every push is domain-correct; everything else is still correct via the executor. The byte-identical A/B gate (D3) + the partial-chunk-group test (EC-6) prove it.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| A wrong min/max test skips a chunk group that HAD matching rows → silent wrong aggregate | **High** | D3: byte-identical A/B gate + exhaustive `chunk_can_match` unit tests (boundary: const == min, == max, just outside; all 5 ops; NULL/NaN fail-safe). The executor re-check is the backstop | impl |
| Predicate extraction from the PG expression tree is fiddly (Var/Const/OpExpr, operator OID → op) | Medium | D2 scopes to `Var OP Const` only; anything unrecognised falls back (correct, unpruned). Unit-test the extractor against fixture quals | impl |
| Skip ratio is low on unsorted/unclustered columns (10k-row granule) | Medium (honest) | D4: the A/B uses a clustered column + states the clustering dependency as a caveat; no unconditional claim | impl |
| `columnar.rs` already 3.2× over the LoC budget; adding skip risks growing it | Low | D1: the pure test lives in `am/zonemap.rs`; `decode_columns` gains only a small skip guard | impl |
| Widening `admit` could mis-route a query the native planner handled better | Medium | Only route to the CustomScan when ≥1 pushable predicate exists AND the aggregate shape already qualified; GUC kill-switch; the no-WHERE path is byte-unchanged | impl |

## Unresolved Questions

- Q1 — For `=` on a float column, does the min/max test need epsilon tolerance, or is exact-bit compare correct? (Resolved: min/max skip uses the ordered domain — `const < min || const > max` excludes; `=` excludes iff `const < min || const > max`; exact, no epsilon — the executor does the real equality.)
- Q2 — Multiple predicates on the SAME column (`y > a AND y < b` = BETWEEN): does the conjunction handle it? (Resolved: yes — two `ZonePredicate`s on the same col; a chunk skips if EITHER excludes it.)
- Q3 — Does the seqscan path (non-aggregate `SELECT * WHERE`) benefit? (Answer: NO in this slice — D2 scopes to the M100 CustomScan aggregate path; the seqscan predicate plumbing is deferred, documented in the verdict.)

## Dependency Graph

```
Phase 1 (am/zonemap.rs — pure predicate test, off-PG proven)
   │
   ▼
Phase 2 (admit extraction + carry + decode_columns skip + GUC + metric)
   │
   ▼
Phase 3 (Integration Validation — in-PG A/B, byte-identical + skip ratio)
```

Sequential — Phase 2 consumes Phase 1's `chunk_can_match`; Phase 3 measures the wired result.

---

## Phase 1: Pure zone-map predicate test (`am/zonemap.rs`)

**Objective:** A pure, standalone-tested `ZonePredicate` + `chunk_can_match` with fail-safe correctness.

### T1.1 — `ZonePredicate` + `chunk_can_match` (pure, off-PG)

#### Objective
Define the predicate descriptor and the min/max exclusion test, fail-safe by construction.

#### Why this step (action + reasoning)

**What this step does:** Adds `am/zonemap.rs` with `enum ZoneOp {Lt,Le,Eq,Ge,Gt}`, `struct ZonePredicate {col: usize, op: ZoneOp, const_bits: u64}`, and `fn chunk_can_match(has_minmax: bool, min_bits: u64, max_bits: u64, kind: MinMaxKind, pred: &ZonePredicate) -> bool` returning `false` ONLY when the zone-map proves no row can match.

**Why it is necessary now:** This is the correctness heart (D1/D3). A wrong test silently corrupts aggregates (High risk). Isolating it as pure logic lets me prove it standalone (`rustc --test`) before any droplet build — the box has no pgrx. The value column exists (`compute_minmax` already produces `min_bits`/`max_bits` in the same domain), so the test consumes what the write side wrote.

#### Evidence
`columnar_codec.rs:37-46` (`MinMaxKind`), `columnar_codec.rs:293-340` (`compute_minmax` — the bit domains this test compares in: i64 two's-complement for ints, f64 bits for floats), `columnar_codec.rs:34-35` (the `has_minmax=false` fail-safe contract).

#### Files to edit
```
theodb_rs/src/am/zonemap.rs (NEW) — ZonePredicate + ZoneOp + chunk_can_match + #[test]s
theodb_rs/src/am/mod.rs — add `mod zonemap; pub(crate) use zonemap::*;`
```

#### Deep file dependency analysis
- `am/zonemap.rs` (NEW): pure, depends only on `MinMaxKind` (from `columnar_codec`). No `pg_sys`. Consumed by `columnar_agg.rs` (extract) + `columnar.rs::decode_columns` (skip) in Phase 2.
- `am/mod.rs`: registers the module (like the existing `mod symqg`).

#### Deep Dives
- **Comparison domains:** ints stored as i64 two's-complement bits (`compute_minmax` `columnar_codec.rs`); to compare, decode `min_bits`/`max_bits`/`const_bits` per `MinMaxKind` into the ordered native type (i16/i32/i64/f32/f64/bool) and compare there. Two's-complement bit order ≠ numeric order for signed ints, so MUST decode to the typed value, not compare raw u64.
- **Exclusion logic** (a chunk CAN match unless proven otherwise): for `pred.op`/`const c` vs chunk `[min,max]`:
  - `Lt` (col < c): excluded iff `min >= c`.
  - `Le` (col <= c): excluded iff `min > c`.
  - `Gt` (col > c): excluded iff `max <= c`.
  - `Ge` (col >= c): excluded iff `max < c`.
  - `Eq` (col = c): excluded iff `c < min || c > max`.
  - Return `!excluded`.
- **Fail-safe:** `has_minmax == false` OR `kind == None` → return `true` (must scan). NaN const → return `true` (can't order).
- **Edge cases:** const exactly == min or == max (boundary — MUST NOT wrongly exclude); single-value chunk (min==max); float −0.0/+0.0 (compare as f64 values, not bits).

#### Pseudo-code / Signatures
```pseudocode
fn chunk_can_match(has_minmax, min_bits, max_bits, kind, pred) -> bool
  if !has_minmax || kind == None: return true            // fail-safe: must scan
  (min, max, c) = decode3(kind, min_bits, max_bits, pred.const_bits)  // typed, ordered
  if any is NaN: return true
  excluded = match pred.op {
    Lt => min >= c,  Le => min > c,
    Gt => max <= c,  Ge => max < c,
    Eq => c < min || c > max,
  }
  return !excluded

# Example (kind=I4, chunk [100,200]): pred y<50 → min(100)>=50 → excluded → false (skip)
#                                     pred y=150 → 150 in [100,200] → not excluded → true (scan)
```

#### Tasks
1. Add `am/zonemap.rs` with the enum/struct + `chunk_can_match` + a typed `decode3` per `MinMaxKind`.
2. Register `mod zonemap` in `am/mod.rs`.
3. Write RED `#[test]`s; implement to GREEN.

#### TDD
```
RED: test_zonemap_excludes_below_range() — I4 chunk [100,200], y<50 and y<=99 → false (skip); y<101 → true.
RED: test_zonemap_excludes_above_range() — y>200 and y>=201 → false; y>=200, y>199 → true.
RED: test_zonemap_eq_in_and_out() — y=150 → true; y=99, y=201 → false; y=100 and y=200 (boundary) → true.
RED: test_zonemap_failsafe() — has_minmax=false → true; kind=None → true; NaN const (F8) → true.
RED: test_zonemap_signed_and_float() [EC-3] — negative I8 chunk [-50,-10]: y<-100 → skip, y>-30 → scan, y=-20 → scan (assert the TYPED decode, contrasting that a raw-u64 compare would order -1 as u64::MAX and wrongly decide); F8 chunk [-0.0, 5.0] compared as f64 values.
RED: test_zonemap_chunk_with_nulls_and_nan() [EC-4] — a float chunk whose present non-NaN values are [10,20] (but rows also hold NULL/NaN): y>100 → skip is SAFE (NULL/NaN never match); y>15 → scan; all-NULL/all-NaN chunk → has_minmax=false → never skipped.
GREEN: implement chunk_can_match + decode3.
REFACTOR: None expected (keep flat).
VERIFY: standalone `rustc --test` extraction of am/zonemap.rs (pure — no pgrx link) + cargo pgrx test zonemap (droplet).
```

#### Concurrency tests

(none — single-threaded)

`chunk_can_match` is a pure function over owned scalars; no shared state.

#### Acceptance Criteria
- [ ] `chunk_can_match` + the 5 RED tests pass GREEN (standalone rustc).
- [ ] All 5 operators + fail-safe (has_minmax=false, kind=None, NaN) covered; boundary (c==min, c==max) proven not-excluded.
- [ ] Pass: size — `am/zonemap.rs` ≤ 200 lines.
- [ ] Pass: lint — `cargo clippy` clean on `am/zonemap.rs`.

#### DoD
- [ ] `rustc --test` on the extracted `am/zonemap.rs` exits 0 (all 5 tests); `grep -c 'mod zonemap' theodb_rs/src/am/mod.rs` == 1.
- [ ] `cargo clippy` zero warnings on `am/zonemap.rs`; `wc -l am/zonemap.rs` ≤ 200.

---

## Phase 2: Predicate extraction + carry + chunk-group skip

**Objective:** Extract a pushable predicate at plan time, carry it to the exec leaf, and skip chunk groups in `decode_columns`.

### T2.1 — Widen `admit` to extract `Vec<ZonePredicate>` from the WHERE

#### Objective
Change `columnar_agg.rs::admit` to accept an implicit-AND `WHERE` of `Var(col) OP Const` clauses (on min/max-able columns) instead of rejecting any WHERE.

#### Why this step (action + reasoning)

**What this step does:** Where `admit` currently returns `None` on a non-null `jointree.quals`, it instead walks the implicit-AND list; each `OpExpr(Var, Const)` with a supported btree operator + native-`MinMaxKind` column becomes a `ZonePredicate`. Un-pushable clauses are ignored (executor still applies them). If ≥1 pushable predicate results, the CustomScan is admitted with the predicates in `custom_private`; if none, keep the current fallback.

**Why it is necessary now:** This is the plumbing seam (agent map Q3): the predicate only exists at plan time (`RestrictInfo`); `admit` runs there. Widening it is the ONLY place the WHERE becomes available to carry down (D2). Without it the predicate never reaches `decode_columns`.

#### Evidence
`columnar_agg.rs:98-200` (`admit`, the WHERE-rejection at 104-112), `columnar.rs:474-482` (`MinMaxKind` derivation from `atttypid`), `customscan.rs:38-101` (the `custom_private`/side-channel carry pattern).

#### Files to edit
```
theodb_rs/src/am/columnar_agg.rs — admit: extract Vec<ZonePredicate>; stash in custom_private / side-channel
```

#### Deep file dependency analysis
- `columnar_agg.rs` (Baseline row 3): `admit` today rejects WHERE. This task adds a qual-walk (`OpExpr` → `ZonePredicate`) + carries the result. The no-WHERE `count(*)`/`sum` path is unchanged (empty predicate vec). The `upper_paths_hook` caller is unchanged.
- Depends on `am/zonemap.rs::ZonePredicate` (Phase 1) + `MinMaxKind` mapping (`columnar.rs:474-482`).

#### Deep Dives
- **Operator → ZoneOp via btree STRATEGY NUMBER (D5, not OID lists):** for an `OpExpr`, look up the operator's btree opfamily + strategy number (`BTLessStrategyNumber=1 … BTGreaterStrategyNumber=5`) via `get_op_opfamily_properties` / the `amop` catalog. Push ONLY when (a) the operator's input types == the column type (column-type-native comparison — no cross-type `int48gt`), and (b) the strategy ∈ {1,2,3,4,5} → `{Lt,Le,Eq,Ge,Gt}`. Any other operator/opfamily → ignore the clause (D5 fallback).
- **Var/Const extraction:** an `OpExpr` with `args = [Var(varattno=col), Const(constvalue, consttype)]` (or flipped — normalise `Const OP Var` to `Var OP' Const`, inverting the strategy: `<`↔`>`, `<=`↔`>=`, `=`↔`=`). `Const.constisnull` → ignore. `consttype != column type` → ignore (D5). Map `constvalue` (Datum) → `const_bits` in the COLUMN's `MinMaxKind` domain (same encoding as `compute_minmax`).
- **Invariants (D5):** the extraction is CONSERVATIVE — any doubt (cross-type op, unknown strategy, NULL/non-Const, `MinMaxKind::None` column, two-Var qual) → **don't push** (executor still filters → correct, unpruned). A pushed clause is ALWAYS domain-correct.
- **Edge cases:** `Const OP Var` (flip the op), NULL const (ignore), `Var OP Var` two-column qual (ignore — EC-5), cross-type operator (ignore — EC-1), same-column multi-clause (BETWEEN → two predicates).

#### Tasks
1. Replace the WHERE-rejection with a qual-walk producing `Vec<ZonePredicate>`.
2. Map operator OID → `ZoneOp`; Datum const → `const_bits` per `MinMaxKind`.
3. Stash the predicates in `custom_private` / the backend-local side channel; admit iff ≥1 pushable.

#### TDD
```
RED: test_admit_extracts_range_predicate() — a `WHERE y BETWEEN 10 AND 20` (implicit AND of >=,<=) on an int col yields 2 ZonePredicates (Ge 10, Le 20) on col y. (#[pg_test] — builds a fixture qual list.)
RED: test_admit_ignores_unpushable() — `WHERE f(y) > 0` OR a text-column predicate yields 0 pushable predicates → native fallback (no CustomScan).
RED: test_admit_flips_const_op_var() — `WHERE 10 < y` normalises to `y > 10` (Gt).
RED: test_admit_ignores_cross_type_op() [EC-1/D5] — `WHERE int4col > 5000000000` (int8 literal / cross-type op) yields 0 pushable predicates (same-domain-or-fallback gate); the executor filters it (result still correct).
RED: test_admit_ignores_var_op_var() [EC-5] — `WHERE y > z` (two columns) yields 0 pushable predicates; the second Var is never treated as a const.
GREEN: implement the qual-walk (btree strategy number + same-type gate + carry).
REFACTOR: extract the OpExpr→ZonePredicate mapping into a helper if it exceeds ~30 lines.
VERIFY: cargo pgrx test admit_ (droplet).
```

#### Concurrency tests

(none — single-threaded)

Planning runs in the backend's single planner pass; the side-channel is backend-local (the `customscan.rs:38-101` pattern is already backend-local, no cross-backend state).

#### Acceptance Criteria
- [ ] `cargo pgrx test admit_` exits 0: BETWEEN → 2 predicates; cross-type/var-op-var/text → 0 pushable (native fallback).
- [ ] `cargo pgrx test m100_` (the existing no-WHERE aggregate tests) still exits 0 — byte-unchanged.
- [ ] Pass: size — `columnar_agg.rs` ≤ 500 lines after the change (currently 420).
- [ ] Pass: lint — clippy clean.

#### DoD
- [ ] `cargo pgrx test admit_` exits 0 AND `cargo pgrx test m100_` exits 0 (no-WHERE path byte-unchanged).
- [ ] `cargo clippy` zero warnings on `columnar_agg.rs`; `wc -l columnar_agg.rs` ≤ 500.

### T2.2 — Skip chunk groups in `decode_columns` + GUC + metric

#### Objective
Give `decode_columns` a `predicates` parameter; skip a chunk group when any predicate's `chunk_can_match` is false; add the `theodb.columnar_zonemap_skip` GUC + a skip-ratio metric.

#### Why this step (action + reasoning)

**What this step does:** `decode_columns(rel, projection, predicates, skip_enabled)` — in the `for cg` loop (`columnar.rs:702-712`), before decoding a chunk group, for each predicate consult `entries[cg*natts + pred.col]`'s `(has_minmax,min_bits,max_bits)` + the column's `MinMaxKind`; if any predicate excludes the chunk group AND the GUC is on, `continue` (skip decoding ALL wanted columns for that cg — alignment preserved). Count skipped/total for the metric.

**Why it is necessary now:** This is the consumer the whole slice exists to build (agent map Q6: `decode_columns` chunk-group loop is the skip site). The GUC isolates the effect for the A/B (D4, like `theodb.symqg_fastscan`). The metric is wiring pillar (c) — observability of the skip.

#### Evidence
`columnar.rs:667-740` (`decode_columns`), `columnar.rs:702-712` (the chunk-group loop where skip goes), `df_executor.rs` (the caller to update), `guc.rs:83-100` (the `SYMQG_FASTSCAN` GUC pattern to mirror), `columnar.rs:474-482` (`MinMaxKind` per column).

#### Files to edit
```
theodb_rs/src/am/columnar.rs — decode_columns: +predicates param; skip guard in the cg loop; skip-ratio log under THEODB_SCAN_PROFILE
theodb_rs/src/am/df_executor.rs — pass the carried predicates + GUC into decode_columns
theodb_rs/src/am/guc.rs — add COLUMNAR_ZONEMAP_SKIP bool GUC (default true) + getter + register in init()
```

#### Deep file dependency analysis
- `columnar.rs::decode_columns` (Baseline row 2): gains a `predicates: &[ZonePredicate]` + `skip: bool` param; the `for cg` loop gets a skip guard consulting the already-read `entries` directory (no new read — the directory is already deserialized at `columnar.rs:701`). Skipping a cg appends nothing for all wanted columns → alignment preserved (they skip together).
- `df_executor.rs` (Baseline row 4): the `decode_columns(rel, Some(&proj))` call site passes the carried predicates + `guc::columnar_zonemap_skip()`.
- `guc.rs` (Baseline row 5): new GUC mirrors `SYMQG_FASTSCAN`.

#### Deep Dives
- **Skip guard (with the EC-2 bounds guard):** `let skip_cg = skip && predicates.iter().any(|p| p.col < natts && { let e = &entries[cg*natts + p.col]; !zonemap::chunk_can_match(e.has_minmax, e.min_bits, e.max_bits, kind_of[p.col], p) }); if skip_cg { continue; }`. The `p.col < natts` guard makes an out-of-range predicate fail-safe (never skip, never index OOB — EC-2). `continue` skips the inner `for wi` decode loop → no `read_chunked`/`zstd` for that cg.
- **Correctness (D3):** the skip only drops chunk groups PROVEN non-matching. Surviving chunk groups' rows still flow to DataFusion, which applies the real predicate (final authority). Pending same-xact rows (`columnar.rs:717-733`) are NEVER skipped (no min/max). `read_visible_stripes` still gates stripe visibility (unchanged).
- **Metric:** count `skipped_cg` / `total_cg`; under `THEODB_SCAN_PROFILE=1` log `columnar zonemap: skipped {skipped}/{total} chunk groups`. This is the A/B's skip-ratio evidence (pillar c).
- **Edge cases:** empty predicates (no-WHERE) → skip nothing (byte-identical to today); GUC off → skip nothing; a predicate on an un-projected column still works (the directory entry exists for every column regardless of projection).

#### Tasks
1. Add the `predicates`/`skip` params + the skip guard in the cg loop; count skipped/total.
2. Add the `THEODB_SCAN_PROFILE` skip-ratio log.
3. Update the `df_executor.rs` call site to pass predicates + GUC.
4. Add the `COLUMNAR_ZONEMAP_SKIP` GUC (default true) + getter + register.

#### TDD
```
RED: test_decode_columns_skips_nonmatching_cg() — #[pg_test]: a 3-chunk-group columnar table where a range predicate matches only cg#1; decode_columns(preds) returns ONLY cg#1's rows for the wanted columns, and the skip-ratio metric shows 2/3 skipped. MUST fail before the skip guard.
RED: test_decode_columns_skip_off_is_full() — GUC off (or empty preds) → decodes ALL chunk groups (byte-identical to pre-change).
RED: test_decode_columns_result_identical_on_off() [EC-6] — the FINAL filtered aggregate (through df_executor) is byte-identical skip-on vs skip-off on a mixed table that MUST contain all three chunk-group kinds: (a) fully inside the range, (b) fully outside (skipped), and (c) PARTIALLY overlapping (min/max intersects but some rows are outside — proves the skip is an admission filter and the executor re-check drops the non-matching survivors).
RED: test_decode_columns_oob_predicate_failsafe() [EC-2] — a predicate with col >= natts does NOT panic and does NOT skip (fail-safe must-scan).
GREEN: implement the skip guard (with the p.col<natts bounds guard) + GUC + metric + call-site wiring.
REFACTOR: None expected.
VERIFY: cargo pgrx test decode_columns_ (droplet) + the psql smoke (filtered aggregate skip on/off equal).
```

#### Concurrency tests

(none — single-threaded)

`decode_columns` reads pages under the standard buffer share-lock discipline (unchanged); no new shared mutable state. The skip counter is a local.

#### Acceptance Criteria
- [ ] `test_decode_columns_result_identical_on_off` green: scalar aggregate byte-identical skip-on vs skip-off on a fully-in/partial/fully-out mixed table.
- [ ] `theodb.columnar_zonemap_skip` GUC toggles it; skip-ratio metric logged under `THEODB_SCAN_PROFILE`.
- [ ] Pillar (a): `grep -c chunk_can_match theodb_rs/src/am/columnar.rs` ≥ 1 AND `check_wiring.py --symbol chunk_can_match` PASS.
- [ ] Pass: size — `columnar.rs` MUST NOT grow materially (skip guard is ~10 lines; the pure test is in `zonemap.rs`); clippy clean on changed regions.

#### DoD
- [ ] `cargo pgrx test decode_columns_` exits 0; the psql smoke `sum WHERE` returns identical value skip-on vs skip-off.
- [ ] `cargo clippy` clean on changed regions; `SHOW theodb.columnar_zonemap_skip` == on; `THEODB_SCAN_PROFILE=1` log shows skipped N>0.

---

## Phase 3: Integration Validation — in-PG A/B (MANDATORY)

**Objective:** Prove byte-identical correctness + measure the skip ratio on a clustered table.

### T3.1 — In-PG A/B: skip on/off, clustered 1M-row table

#### Objective
Build a `theodb_columnar` table with a clustered column, run a selective filtered aggregate skip-on vs skip-off, record byte-identical result + chunk-groups-decoded ratio.

#### Why this step (action + reasoning)

**What this step does:** Provisions a droplet, `cargo pgrx install --release`, loads a 1M-row table with a column CLUSTERED (sorted on load) so a ~10%-selective range matches few chunk groups, runs `SELECT sum(x) FROM t WHERE y BETWEEN a AND b` with `theodb.columnar_zonemap_skip` on then off, asserts equal results, records the skip ratio + latency, writes the verdict.

**Why it is necessary now:** The Goal metric IS this A/B (D4). Measurement-first: no skip-ratio/latency claim without the artifact (`public-copy.md`). The byte-identical assertion is the D3 correctness gate at scale.

#### Evidence
`benchmarks/e2_symqg_inpg.py` (the harness pattern to mirror), the `THEODB_SCAN_PROFILE` metric (T2.2), `docs/benchmarks/m100-datafusion-executor.md` (the projection-pushdown baseline this composes with).

#### Files to edit
```
benchmarks/columnar_zonemap_ab.py (NEW) — load clustered table, filtered aggregate skip on/off, skip ratio + latency + equality
docs/benchmarks/columnar-zonemap-verdict.md (NEW) — measured verdict
docs/benchmarks/columnar-zonemap-verdict.json (NEW) — raw
CHANGELOG.md — [Unreleased] entry
```

#### Deep file dependency analysis
- `benchmarks/columnar_zonemap_ab.py` (NEW): mirrors `e2_symqg_inpg.py`; no production caller.
- The verdict docs are new artifacts.

#### Deep Dives
- **Clustering:** load rows sorted on `y` (or `CLUSTER`/ordered INSERT) so each 10k-row chunk group has a tight `y` range → a 10%-selective range predicate matches ~10% of chunk groups (skip ~90%).
- **Correctness assertion:** the same query skip-on and skip-off MUST return the identical scalar (sum) — byte-identical (D3 gate). If they differ → the slice FAILs (loop back).
- **Metric:** `THEODB_SCAN_PROFILE=1` logs skipped/total; the bench parses it → chunk-groups-decoded ratio.

#### Tasks
1. Provision droplet, rsync, `cargo pgrx install --release`.
2. Load a clustered 1M-row `theodb_columnar` table; run the filtered aggregate skip on/off (best-of-3).
3. Assert equal results; record skip ratio + latency; write the verdict; update CHANGELOG; destroy droplet.

#### TDD
```
RED: test_ab_byte_identical() — assert sum_skip_on == sum_skip_off (the correctness gate; the A/B harness is the oracle). If they differ, the slice FAILs.
GREEN: verdict artifact written with the measured skip ratio + latency + the equality proof.
REFACTOR: None.
VERIFY: benchmarks/columnar_zonemap_ab.py emits the ratio; skip-on decodes ≤ 25% of the chunk groups skip-off decodes; results equal.
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `columnar_zonemap_ab.py` asserts `sum_skip_on == sum_skip_off` exactly (D3 gate); FAILs the run on any diff.
- [ ] The parsed `THEODB_SCAN_PROFILE` ratio: skip-on decoded chunk groups ≤ 0.25 × skip-off's, on the clustered 10%-range.
- [ ] `docs/benchmarks/columnar-zonemap-verdict.md` states the measured skip ratio + latency as numbers sourced from the `.json`; the clustering-dependency caveat is written.
- [ ] CHANGELOG `[Unreleased]` updated (Rule 6).

#### DoD
- [ ] `git show --stat HEAD` lists `docs/benchmarks/columnar-zonemap-verdict.{md,json}`; `doctl compute droplet list` shows no columnar droplet.
- [ ] `grep -c columnar.zonemap CHANGELOG.md` ≥ 1 under `[Unreleased]`.

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Pure, fail-safe min/max exclusion test | T1.1 | `am/zonemap.rs::chunk_can_match` + 5 unit tests (ops + fail-safe + boundary) |
| 2 | Extract a pushable `WHERE` at plan time | T2.1 | `admit` walks implicit-AND `Var OP Const` on native-`MinMaxKind` columns → `Vec<ZonePredicate>` |
| 3 | Carry the predicate plan-time → exec | T2.1 | `custom_private` / backend-local side channel (`customscan.rs` pattern) |
| 4 | Skip non-matching chunk groups in decode | T2.2 | skip guard in `decode_columns` cg loop; `continue` avoids read_chunked+zstd |
| 5 | A/B kill-switch + skip-ratio observability | T2.2 | `theodb.columnar_zonemap_skip` GUC + `THEODB_SCAN_PROFILE` metric |
| 6 | Byte-identical correctness (skip ≠ answer change) | T2.2 (test) + T3.1 | on/off equality unit test + the in-PG A/B byte-identical gate |
| 7 | Measured skip ratio + latency (Goal) | T3.1 | in-PG A/B verdict, clustered table, ≤25% chunk groups decoded |
| 8 | Un-pushable predicates stay correct (fallback) | T1.1 (fail-safe) + T2.1 (ignore) | `has_minmax=false`/`MinMaxKind::None` → must-scan; unrecognised qual → executor filters |
| 9 | Const/operator type-domain mismatch → no wrong skip (EC-1) | D5, T2.1 | same-domain-or-fallback: push only column-type-native btree ops (strategy number); const in the column's domain; `test_admit_ignores_cross_type_op` |
| 10 | Out-of-range predicate col → no panic (EC-2) | T2.2 | `p.col < natts` fail-safe bounds guard; `test_decode_columns_oob_predicate_failsafe` |
| 11 | Skip sound only for position-independent aggregate (EC-7) | DOCUMENT | scoped to the M100 aggregate path (D2); a future row-returning consumer needs TID-aware pruning — noted, not reused naively |

**Coverage: 11/11 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed.
- [ ] All tests passing — `cargo pgrx test` (droplet) + standalone `rustc --test` (pure `zonemap`) green.
- [ ] Zero clippy warnings on changed files — `cargo clippy`.
- [ ] File-size budget — `am/zonemap.rs` ≤ 200; `columnar_agg.rs` ≤ 500; `columnar.rs` MUST NOT grow materially (per `architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6).
- [ ] Backward compatibility: the `ChunkDirEntry` wire format is UNCHANGED (read-only consumer); existing columnar tables work unchanged; the no-WHERE `count(*)`/`sum` path is byte-identical.
- [ ] Plan-specific: filtered aggregate skip-on == skip-off (byte-identical, D3 hard gate); skip ratio + latency measured + recorded honestly (D4).
- [ ] Runtime-metric proof — the skip-ratio metric is observed non-zero in the A/B, not compiled-only.
- [ ] Plan archived to `knowledge-base/plans/completed/` after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (when I/O external)

```
(none — no external I/O touched)
```
The change is pure predicate logic + an in-process scan skip guard + a benchmark. Page reads use the existing buffer/WAL path (unchanged — the change AVOIDS reads); no HTTP/DB-driver/queue/socket added.

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Prove correctness + measure the skip on a clustered table.

### Execution
```
rustc --test (pure am/zonemap.rs)   # off-PG fail-safe correctness of chunk_can_match
cargo pgrx test                     # (droplet) admit_ + decode_columns_ + zonemap
cargo clippy                        # zero warnings on changed files
benchmarks/columnar_zonemap_ab.py   # (droplet) skip on/off: byte-identical + skip ratio + latency
```

### Acceptance Criteria
- [ ] All `#[pg_test]` + standalone tests green.
- [ ] Zero clippy warnings on changed files.
- [ ] Filtered aggregate skip-on == skip-off (byte-identical) — the correctness gate.
- [ ] Skip path decodes ≤ 25% of the baseline's chunk groups on the clustered 10%-selective range.
- [ ] Runtime-metric proof — the skip-ratio metric fires in the A/B.
- [ ] Failure scenarios green — N/A (none — no external I/O touched).

### If Validation Fails
1. Skip-on ≠ skip-off (result changed) → the `chunk_can_match` test or the extraction is wrong (a chunk with matching rows was skipped) → HALT, fix Phase 1/2 BEFORE any claim (this is the silent-corruption risk — highest priority).
2. Skip ratio ~0 on a clustered table → the predicate isn't reaching `decode_columns` (plumbing bug) → debug the carry; the metric localizes it.
3. Pre-existing failures (unrelated) logged, not blocking.
