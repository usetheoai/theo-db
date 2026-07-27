---
slug: m160-decode-zerocopy
milestone_id: M160
created_at: 2026-07-26
goal: cut the columnar pushdown-path decode cost by decoding fixed-width non-null columns into one typed Vec<T> per column (zero-copy into Arrow) instead of per-cell Vec<u8>, measured by a covered-class ClickBench geomean drop
---

# M160 — decode zero-copy fixed-width → Arrow (eliminate the `Vec<Option<Vec<u8>>>` bridge)

## Goal

Reduce the covered-class ClickBench geomean-vs-ClickHouse (measured 7.54× at M159) by decoding fixed-width non-null
columns directly into one typed `Vec<T>` per column (handed zero-copy to Arrow `PrimitiveArray`) instead of a per-cell
`Vec<Option<Vec<u8>>>` + a re-read in `build_arrow` — with the win proven by a before/after pushdown flamegraph AND a
re-run of the M159 harness showing the covered-class geomean drop, A/B byte-identical.

## Context

Post-M159 deep-dive (`knowledge-base/discoveries/blueprints/columnar-improvement-deepdive-blueprint.md`, council-
performance-simd) found — and an empirical pushdown flamegraph (directional, 318 samples) confirmed — that the covered
32-query pushdown class does NOT run `form_row` (the M148 Volcano bottleneck) but pays a morally identical, never-profiled
cost: `decode_column` allocates every cell into a separate heap `Vec<u8>` (`.to_vec()`, `columnar_codec.rs:293,308`) and
`build_arrow` re-reads each cell (`from_le_bytes`, `df_executor.rs:49-137`). The flamegraph showed `build_arrow` +
`decode_column` + `malloc`/`cfree` + a kernel page-fault storm (`clear_page_erms`) dominating, with DataFusion's
aggregate compute ~absent. This milestone is Layer A of that deep-dive — the highest-ROI, own-code, PG-safety-neutral fix.

## Baseline Context

| File | LoC (approx) | Role | Current cost |
|---|---|---|---|
| `theodb_rs/src/am/columnar_codec.rs` | ~380 | `decode_column` (raw → cells) | per-cell `raw[off..off+len].to_vec()` (`:293,308`) — 1M tiny allocs/column |
| `theodb_rs/src/am/columnar.rs` | ~1300 | `decode_columns` (:824) accumulates cells across chunk-groups/stripes into `Vec<(String,u32,Vec<Option<Vec<u8>>>)>` (:829) | one giant boxed Vec/column |
| `theodb_rs/src/am/df_executor.rs` | ~770 | `build_arrow` (:49-137) cells → Arrow via per-cell `from_le_bytes`; `decode_to_batch` (:238) | second full pass + copy |

Current callers of `decode_columns`/`build_arrow` — the pushdown path only (`decode_to_batch` → `run_columnar_aggs`/
`_grouped_aggs`/`_topk`). The row path (`form_row`/`decode_stripe`) is separate and NOT touched by this milestone.
Supported fixed-width Arrow types in `build_arrow`: 21(int2), 23(int4), 20(int8), 700(f4), 701(f8), 16(bool),
1114/1184(timestamp/tz), 1082(date). Varlena/text (25/1042/1043) stay on the cell path.

Git sha at plan time: v0.150.0 (M159 released). Prior art: M148 (Volcano flamegraph), M158 (Arrow batch), M151 (the
Volcano-path decode work this mirrors for the pushdown path).

## Prior Art & Related Work

- `knowledge-base/discoveries/blueprints/columnar-improvement-deepdive-blueprint.md` — Lever A (this milestone), council-performance-simd finding + empirical flamegraph.
- `references/arrow-rs/` — `PrimitiveArray::from(Vec<T>)` (zero-copy from a typed Vec), `arrow::buffer` (Buffer/ScalarBuffer alignment contract).
- `references/papers/monetdb-x100-boncz-2005.pdf` — vectorized decode; the "don't materialize per-cell" principle.
- M148 flamegraph (`docs/benchmarks/m148-flamegraph-scan.md`) — the Volcano twin of this cost.

## ADRs

### ADR-1 — typed `Vec<T>` decode (not raw `Buffer::from_vec::<u8>`)

**Decision:** for a fixed-width non-null column, decode the contiguous value stream into one pre-sized typed `Vec<T>`
(`for i in 0..n { v.push(T::from_le_bytes(raw[i*w..])) }`) and build the Arrow array via `PrimitiveArray::from(vec)`
(zero-copy: Arrow adopts the Vec's allocation).

**Rationale / alternatives rejected:**
- *Raw `Buffer::from_vec::<u8>(raw)` + reinterpret as `ScalarBuffer<T>`* — REJECTED: a `Vec<u8>` is 1-byte-aligned;
  Arrow's `ScalarBuffer<T>::from(Buffer)` requires T-alignment and would panic or force a realign-copy. A typed `Vec<T>`
  is T-aligned by construction and adopts into Arrow with no copy. (`references/arrow-rs/`.)
- *Keep the per-cell path but skip `build_arrow`'s re-read* — REJECTED: the dominant cost (flamegraph) is the per-cell
  `.to_vec()` alloc storm in `decode_column`, not the re-read; must eliminate the boxing at the source.
- **Endianness:** `from_le_bytes` is endian-safe on any host (it reads little-endian explicitly), so the typed-Vec fill
  is correct on big-endian too — unlike a raw `Buffer::from_vec` reinterpret, which would only be correct on LE. This is
  a second reason to prefer typed-Vec (Rule 8 — no silent wrong result on an exotic host).

### ADR-2 — additive fast path, cell path retained for nullable/varlena/text (fail-safe)

**Decision:** the fast path fires ONLY for `attlen_fixed=Some(w)` + `has_nulls=false` + a `build_arrow`-supported fixed
type. Nullable columns, varlena/text, and unsupported types fall back to the existing cell path unchanged.

**Rationale / alternatives rejected:** *rewrite the whole decode to typed columns incl. nullable* — REJECTED (YAGNI +
risk): nullable needs a null bitmap + typed values (Arrow `PrimitiveArray::new(values, Some(nulls))`), a larger change;
the ClickBench covered class is dominated by fixed-width non-null int/timestamp columns (the measured win). Nullable
fast path is a fast-follow if measured to matter. Fail-safe: any column that does not qualify uses the proven path, so
A/B byte-identity is preserved by construction for the fallback.

## Dependency Graph

Phase 1 (measurement gate) → Phase 2 (decode fast path) → Phase 3 (validation). Phase 1 blocks Phase 2 (measurement-first).

## Phase 1 — measurement gate (confirm the bottleneck before building)

### T1.1 — pushdown flamegraph, pure-int covered query, ≥500 samples

#### Why this step
The deep-dive flamegraph was 318 samples on a text-heavy query (directional). The blueprint's own gate requires a
≥500-sample pure-int covered-query flamegraph to confirm `decode_column`/`build_arrow`/`malloc` self-time BEFORE writing
the fix (measurement-first; do not optimize an unconfirmed hotspot).

#### TDD
- Test (measurement, not unit): run `benchmarks/profile_columnar_scan.sh`-style perf on a pure-int covered query
  (e.g. `SELECT SUM(ResolutionWidth) FROM hits` or a bare int GROUP BY) with `enable_columnar_agg=on`, ≥500 samples.
  ASSERT: `decode_column` + `build_arrow` + malloc/page-fault dominate user-space self-time; DataFusion aggregate is minor.
#### Files to edit
- `benchmarks/profile_pushdown_decode.sh` (NEW) — the pushdown-path profiling harness.
#### Acceptance criteria
- [ ] ≥500 samples; folded output shows decode-bridge self-time > DataFusion-compute self-time (the finding confirmed or refuted).
- [ ] If REFUTED (decode bridge is NOT the hotspot) → STOP, honest-negative, do not build the fix (anti-sunk-cost).

## Phase 2 — the zero-copy fixed-width decode fast path

### T2.1 — decode fixed-width non-null columns into typed Arrow arrays

#### Why this step
Eliminate the per-cell `Vec<u8>` alloc storm (the measured dominant cost). ADR-1/ADR-2: one typed `Vec<T>` per column
→ `PrimitiveArray::from(vec)`, gated to fixed-width non-null supported types, cell path retained otherwise.

#### TDD
- RED: a Rust unit test `decode_fixed_width_int32_matches_cell_path` — decode a known int32 chunk both ways (fast path +
  cell path) and assert the resulting Arrow arrays are element-wise equal (incl. an int16, int64, f8, timestamp, date case).
- RED: `decode_fixed_width_falls_back_when_nullable` — a column with a null bitmap uses the cell path (asserts the fast
  path is NOT taken; result correct incl. the null).
- GREEN: implement the fast path (walk the parsimony ladder — reuse `from_le_bytes`, `PrimitiveArray::from`, no new dep).
#### Files to edit
- `theodb_rs/src/am/columnar_codec.rs` — a fixed-width contiguous decode that returns the typed value stream (or the raw contiguous slice + width) for non-null fixed columns.
- `theodb_rs/src/am/columnar.rs` — `decode_columns` carries the fast-path representation across chunk-groups (concatenate the typed Vec — one bulk extend per chunk-group, O(bytes) not O(cells)).
- `theodb_rs/src/am/df_executor.rs` — `build_arrow`/`decode_to_batch` construct `PrimitiveArray::from(Vec<T>)` for fast-path columns.
#### Concurrency tests
(none — single-threaded main-thread decode; no shared mutable state introduced.)
#### Acceptance criteria
- [ ] Fast path fires for int2/int4/int8/f4/f8/bool/timestamp/timestamptz/date non-null columns; cell path for the rest.
- [ ] No new dependency (Rule 9 — `from_le_bytes` + arrow already present).
- [ ] `cargo build --release` clean; no new `unsafe` (the typed-Vec path is safe Rust).

## Phase 3 — validation (measurement-first, the DoD)

### T3.1 — A/B byte-identical + before/after flamegraph + M159 harness geomean

#### Why this step
The DoD: prove the decode change is (a) correct (A/B byte-identical vs heap — the decoded values unchanged) and (b) a
measured win (flamegraph self-time of decode_column/build_arrow/malloc drops; M159 harness covered-class geomean drops).

#### TDD
- Integration: re-run the M159 ClickBench harness (`run_m128_clickbench.py --agg`) → `result_ab.diverged == 0` (43/43)
  AND the covered-class hot geomean drops vs the M159 baseline (7.54× → measured lower).
- Integration: re-run the T1.1 flamegraph after the fix → `decode_column`/`build_arrow`/malloc self-time measurably down.
#### Files to edit
- `docs/benchmarks/m160-decode-zerocopy-verdict.md` (NEW) — before/after flamegraph + geomean + A/B evidence.
#### Failure scenarios
- Decode of a corrupt/truncated chunk → typed error (not panic across C), same as the current cell path's bounds checks.
#### Acceptance criteria
- [ ] A/B byte-identical 43/43 (the fast path must produce identical values to the cell path).
- [ ] Measured geomean drop on the covered class (or honest-negative documented if marginal).
- [ ] Flamegraph before/after proving the decode-bridge self-time fell.
- [ ] CHANGELOG `[Unreleased]` updated.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Confirm the decode bridge is the covered-class hotspot (≥500 samples) | T1.1 |
| Decode fixed-width non-null columns zero-copy into Arrow | T2.1 |
| Correctness (A/B byte-identical) | T3.1 |
| Measured win (flamegraph + geomean) or honest-negative | T1.1 (gate) + T3.1 |

## Drawbacks & Risks

- **Endianness (severity: low, mitigated):** raw `Buffer::from_vec` would be LE-only; ADR-1 uses `from_le_bytes` typed
  fill (endian-safe). Owner: implementer. Mitigation: no host-endian assumption; unit test on the typed path.
- **Nullable/varlena not accelerated (severity: low, accepted):** fast path skips them (fail-safe cell path). The
  covered class is fixed-width-int/timestamp-dominated; nullable is a fast-follow. Owner: implementer.
- **Win may be smaller than the flamegraph suggests (severity: medium):** the alloc storm is dominant but zstd decode +
  DataFusion still cost; honest-negative accepted if the geomean drop is marginal (anti-sunk-cost). Owner: implementer.

## Unresolved Questions

- Does the concatenation of typed `Vec<T>` across chunk-groups need a single pre-sized alloc (row_count known from the
  directory) to avoid re-alloc growth? (Likely yes — pre-size from the stripe row counts.) Resolved at implementation via the directory row counts.

## Global Definition of Done

- [ ] `/plan-confidence` ≥ SHIPPABLE_WITH_CAVEATS.
- [ ] T1.1 gate PASSED (bottleneck confirmed ≥500 samples) OR honest-negative STOP.
- [ ] A/B byte-identical 43/43 + measured covered-class geomean drop (or documented honest-negative).
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}; `/review` READY_TO_MERGE.
- [ ] Released + M160 checkbox flipped.

## Final Phase: Integration Validation

- [ ] Full ClickBench harness green (43/43 A/B), covered-class geomean measured before/after, flamegraph before/after, all committed to `docs/benchmarks/m160-decode-zerocopy-verdict.md`.
