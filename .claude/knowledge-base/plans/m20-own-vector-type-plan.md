---
slug: m20-own-vector-type
milestone_id: M20
created_at: 2026-06-30
goal: Implement TheoDB's own Rust f32-parity distance functions over pgvector's binary vector layout, proven byte-identical to pgvector on its regression oracle + a reproducible benchmark.
---

# Plan: M20 — Own `vector` distance operators in Rust/pgrx (pgvector parity, coexistence)

## Goal

Implement TheoDB's own Rust distance functions `theodb.l2_distance` / `theodb.inner_product` /
`theodb.cosine_distance` over pgvector's binary `vector` layout in `theodb_rs`, measured by
`benchmarks/tests/test_vector_ops.py` passing — each function returns **byte-identical text output to
pgvector's** native `l2_distance`/`inner_product`/`cosine_distance` on pgvector's own regression oracle rows,
against the rebuilt container, plus a reproducible parity+perf benchmark in `docs/benchmarks/`.

## Context

M20 (ROADMAP-v2.md:90→104) wants an own `vector` type + 3 distance ops in Rust at numeric parity,
measurement-first ("só substitui pgvector quando a paridade for provada"). The SHIPPABLE discovery blueprint
(`knowledge-base/discoveries/blueprints/m20-own-vector-type-blueprint.md`, /discover-confidence 97.3) resolved
the key decisions: (a) both SOTA pgrx peers (pgvectorscale — our exact pgrx; vectorchord) FFI-wrap pgvector's
`#[repr(C)]` layout rather than fork the type; (b) pgvector accumulates distance sums in **f32** (not f64) —
the bit-parity determinant; (c) the migration decision is **COEXISTENCE** (own Rust ops reading pgvector's
bytes, NOT a competing storage type), because a competing type forks data + breaks HNSW/IVFFlat +
pgvectorscale DiskANN + `theodb.embed`/`hybrid`/`import`. This plan implements that.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last sha | Why it exists / role |
|---|---|---|---|
| `theodb_rs/src/vec.rs` (NEW) | 0 | — | NEW: `#[repr(C)]` pgvector-compatible reader + the 3 f32-parity distance fns (domain) |
| `theodb_rs/src/lib.rs` | 424 | 6f5a01a | api-surface: `#[pg_extern]` entrypoints + `extension_sql!` wrappers (M17-M19); adds the 3 distance fns + `mod vec;` |
| `theodb_rs/src/pg.rs` | 56 | 6f5a01a | pg-glue: typed `err_input` (22023) reused for dim-mismatch |
| `benchmarks/tests/test_vector_ops.py` (NEW) | 0 | — | NEW: pytest parity gate replaying pgvector's oracle rows against `theodb.*` |
| `benchmarks/bench_vector_ops.py` (NEW) | 0 | — | NEW: reproducible numeric-parity + perf benchmark vs pgvector |
| `docs/benchmarks/m20-vector-ops-parity.md` (NEW) | 0 | — | NEW: benchmark report (parity proof + perf delta) |
| `CHANGELOG.md` | — | — | `[Unreleased]` entry (Unbreakable Rule 6) |

### Current callers / dependents

- pgvector's `vector` type is the substrate of: `theodb.embed` (returns `vector`, `lib.rs` extension_sql), `ai.hybrid_search_rrf` (`<=>`, `hybrid.rs`), `theodb.import_pinecone` (`$2::vector`, `migrate.rs`), and HNSW/IVFFlat + pgvectorscale DiskANN indexes. **The own ops MUST NOT redefine pgvector's `<->`/`<#>`/`<=>` operators on the shared `vector` type** (would conflict) — they are exposed as NEW functions in the `theodb` schema (coexistence). Verified: `grep -rn "vector" theodb_rs/src` shows only text-cast usage; no Rust code currently reads the `vector` binary layout.
- pgvector installed in the image at pinned commit `586e7515bafe6912c425164d186d56550657c349` (`Dockerfile:54`). **CK-1:** a Phase-1 task cross-checks this commit's distance formulas + binary layout against the 0.8.3 reference the blueprint read (stable since ≤0.5; the check is the parity guarantee).

### Architecture boundaries affected

- `rules/architecture.md` layering: `vec.rs` = domain (pure distance math + the FFI read), `lib.rs` = api-surface (`#[pg_extern]` + SQL). Same 3-boundary pattern as M17-M19 (`pg.rs`/`embed.rs`/`lib.rs`). DIP: the distance math is pure (`&[f32] → f64/f32`), testable without Postgres.

### Domain glossary

- **varlena**: PostgreSQL's variable-length datum header (`vl_len_`).
- **`#[repr(C)]` FFI read**: a Rust struct laid out byte-identically to pgvector's C `Vector`, read from the datum pointer (no copy, no competing type).
- **f32 accumulation**: summing distance terms in `f32` (matches pgvector's `float` accumulators) — the bit-parity determinant.
- **coexistence**: own ops read pgvector's bytes; pgvector's type/operators/indexes stay untouched.
- **parity oracle**: pgvector's `test/sql/vector_type.sql` + `test/expected/vector_type.out` distance rows.

## Prior Art & Related Work

- **Internal blueprint** `knowledge-base/discoveries/blueprints/m20-own-vector-type-blueprint.md` (SHIPPABLE 97.3) — the primary source: pgvector layout (Q1), distance formulas + f32 accumulation (Q2), pgrx-peer FFI pattern (Q3), oracle (Q4), test shape (Q5), deps (Q6), tools (Q7), + 3 ADRs (coexistence / f32 / SIMD-text-parity).
- **Reference (permissive, implementable)**: pgvectorscale `PgVectorInternal` `#[repr(C)]` + `to_slice()` + f32 distance (`references/pgvectorscale/.../pg_vector.rs`, `.../distance/mod.rs`) — our exact pgrx 0.16.1.
- **Reference (AGPL, technique-only)**: vectorchord `VectorInput`/`VectorHeader` (`references/vectorchord/src/datatype/memory_vector.rs`) — corroborates the FFI pattern; code NOT copied (D1 bars AGPL in-package).
- **SOTA spec**: pgvector `vector.c`/`vector.h` (the parity ground truth).

## Objective

Deliver own Rust f32-parity distance functions over pgvector's binary layout (coexistence), proven by a
container parity suite (byte-identical to pgvector on its oracle) + a reproducible benchmark.

## ADRs

### D1 — Coexistence via binary-compatible FFI read; expose as FUNCTIONS, not redefined operators
**Decision:** implement an own `#[repr(C)]` pgvector-compatible reader + 3 Rust distance functions exposed as
`theodb.l2_distance`/`theodb.inner_product`/`theodb.cosine_distance` (NEW functions in the `theodb` schema),
reading pgvector's `vector` bytes. Do NOT define a competing storage type; do NOT redefine pgvector's
`<->`/`<#>`/`<=>` operators on the shared `vector` type.
**Rationale:** blueprint ADR-1 — both pgrx peers FFI-wrap rather than fork; a competing type or duplicate
operator forks data / breaks indexes / conflicts on the shared type. Functions coexist cleanly and still
"own the distance computation in Rust" (the M20 intent — reduce dependency on pgvector's *op code*). Parsimony
ladder: reuse pgvector's binary layout (rung 4) instead of inventing one.
**Alternatives:** (a) competing `vector` type — REJECTED (forks data + breaks HNSW/IVFFlat/DiskANN +
embed/hybrid/import); (b) redefine `<->`/`<#>`/`<=>` operators — REJECTED (duplicate-operator conflict on the
shared type); (c) own operators with own opclass in a separate schema — DEFERRED to M21 (index AM) — M20 is
type+ops parity, not indexing.

### D2 — f32 accumulation (bit-parity), f64 sqrt/divide, cosine clamp
**Decision:** accumulate L2/IP/cosine sums in **`f32`** (`iter().map(...).sum::<f32>()`); `sqrt`/division in
`f64`; cosine clamps similarity to [-1,1] then returns `1.0 - sim`; negative inner product negates.
**Rationale:** blueprint ADR-2 / Q2 — pgvector accumulates in `float` (`vector.c:557/604/646`); pgvectorscale's
Rust does the same. f64 accumulation would diverge from the oracle.
**Alternatives:** (a) f64 accumulation — REJECTED (diverges from pgvector); (b) hand-rolled SIMD — REJECTED for
M20 (YAGNI; scalar f32 is the parity reference, SIMD is M21+ perf work).

### D3 — Parity asserted to pgvector's TEXT output (SIMD low-bit honesty)
**Decision:** the parity gate asserts `theodb.*` == pgvector's native function text output on identical inputs
(the oracle rows); bit-exactness vs a SIMD pgvector build is best-effort, documented.
**Rationale:** blueprint ADR-3 / Q2 SIMD note — `VECTOR_TARGET_CLONES` reorders f32 sums; asserting on rounded
text output absorbs low-bit noise. Avoids a false "bit-exact" claim (`public-copy.md`).
**Alternatives:** (a) claim bit-exact vs SIMD — REJECTED (dishonest / flaky).

## Dependencies

| Dependency | Version | Status | Rule-9 justification |
|---|---|---|---|
| pgrx | =0.16.1 | already declared (`theodb_rs/Cargo.toml`) | the pgrx framework; the FFI `#[repr(C)]` read needs nothing beyond pgrx + std (blueprint Q6) |
| std (`MaybeUninit`, `__IncompleteArrayField` via pg_sys) | — | stdlib + pgrx | parsimony rung 2/4 — no new crate |

**No new dependency added.** `/deps-audit` should confirm.

## Dependency Graph

```
Phase 1 (vec.rs: #[repr(C)] reader + CK-1 version check) ──▶ Phase 2 (3 f32-parity distance fns + Rust unit tests)
                                                                   │
                                                                   ▼
                                              Phase 3 (#[pg_extern] + extension_sql theodb.* wrappers)
                                                                   │
                                                                   ▼
                                              Phase 4 (parity gate: pytest replaying pgvector oracle)
                                                                   │
                                                                   ▼
                                              Phase 5 (benchmark: parity proof + perf delta)  ──▶ Final: Integration Validation
```

## Phase 1: `vec.rs` — pgvector-compatible `#[repr(C)]` reader + version cross-check

### T1.1 — `#[repr(C)]` reader that exposes pgvector's `vector` payload as `&[f32]`

#### Why this step
**Action:** add `theodb_rs/src/vec.rs` with a `#[repr(C)] struct PgVectorBytes { vl_len_: i32, dim: i16, unused: MaybeUninit<i16>, x: __IncompleteArrayField<f32> }` + `fn as_slice(datum) -> &[f32]` (dim-checked) that **DETOASTS the datum first** (`pg_sys::pg_detoast_datum`, mirroring pgvector's `DatumGetVector(x) = (Vector*) PG_DETOAST_DATUM(x)` — `vector.h:7`) before casting, exactly like pgvectorscale's `PgVector::from_datum`.
**Reasoning:** blueprint Q1/Q3 + ADR D1 — reading pgvector's exact bytes is the coexistence foundation; the struct layout is the parity ground truth (`vector.h:11-17`). **EC-1 (MUST-FIX, edge-case review):** a `vector` value may be TOASTed (compressed/out-of-line) for large dims; casting the RAW datum without detoast reads garbage → segfault/wrong result. The reader MUST detoast (then free if it allocated a copy).

#### Files to edit
- `theodb_rs/src/vec.rs` (NEW, ≤120 LoC)
- `theodb_rs/src/lib.rs` (add `mod vec;`)

#### Deep file dependency analysis
No current Rust reads `vector` bytes (Baseline). `lib.rs` adds one `mod vec;` line. No caller breaks.

#### TDD
- RED: `#[test] fn as_slice_reads_dim_and_payload()` — construct a byte buffer matching pgvector's layout for `[1.0, 2.0, 3.0]` and assert `as_slice` yields `[1.0,2.0,3.0]` and `dim==3`. `test_<behavior>`: `as_slice_reads_payload`. Plus a `#[pg_test]` (EC-1) that round-trips a real `'[...]'::vector` datum through the detoasting reader (proves detoast on a column-stored value, incl. a large/TOASTable dim).
- GREEN: minimal `#[repr(C)]` + detoasting `as_slice`.
- REFACTOR: doc-comment the layout + detoast invariant.

#### Concurrency tests
(none — single-threaded) — pure datum read, no shared state

#### Acceptance criteria
- `cargo test` (or `cargo pgrx test`) green for the reader; `cargo clippy -D warnings` clean; `vec.rs` ≤ 120 LoC.

#### DoD
- `docker build` compiles; the reader round-trips a known buffer.

### T1.2 — CK-1: cross-check the image's pgvector formulas/layout vs the 0.8.3 reference
#### Why this step
**Action:** add a doc note + a Phase-4 assertion that the installed pgvector (commit `586e7515`, `Dockerfile:54`) produces the SAME distance outputs the blueprint recorded from 0.8.3 (the parity is defined against the INSTALLED pgvector, which is also the oracle source at runtime).
**Reasoning:** blueprint CK-1 — parity is only meaningful against the pgvector actually installed. Since the parity suite (Phase 4) computes pgvector's own outputs at runtime and compares to `theodb.*`, this is self-consistent; this task documents that the reference formulas match.

#### Files to edit
- `docs/benchmarks/m20-vector-ops-parity.md` (NEW — note the pgvector commit/version under test)

#### TDD
(verification task — the assertion lives in Phase 4's parity suite, which compares against the live pgvector)

#### Concurrency tests
(none — single-threaded) — documentation/verification task, no code path

#### Acceptance criteria
- The benchmark doc records the installed pgvector commit/version; Phase 4 compares `theodb.*` to the LIVE pgvector functions (not a hardcoded constant), so any version drift is caught.

#### DoD
- Doc note present; Phase-4 suite compares against live pgvector.

## Phase 2: the 3 f32-parity distance functions (pure Rust)

### T2.1 — `l2_distance`, `inner_product` (+negative), `cosine_distance` in `vec.rs`, f32-accumulation
#### Why this step
**Action:** implement `fn l2(a:&[f32],b:&[f32])->f64`, `fn inner_product(a,b)->f64`, `fn cosine(a,b)->f64` matching `vector.c`: L2 = `sqrt((f64)Σ_f32 (a-b)²)`; IP = `(f64)Σ_f32 a*b`; cosine = clamp(`(f64)sim / sqrt((f64)na*(f64)nb)`, -1,1) → `1.0 - sim`; all sums in **f32**. Dim-mismatch → `err_input` (22023, parity with pgvector's CheckDims).
**Reasoning:** blueprint Q2 + ADR D2 — exact accumulation order/width is the parity determinant. Pure fns → unit-testable against oracle values.

#### Files to edit
- `theodb_rs/src/vec.rs` (the 3 fns + a dim-check helper)

#### Deep file dependency analysis
Pure fns over `&[f32]`; called by Phase-3 `#[pg_extern]`. Reuses `crate::pg::err_input` for dim-mismatch.

#### TDD
- RED: `#[test]` per op asserting pgvector's oracle values: `l2([0,0],[3,4])==5.0`, `l2([0,0],[0,1])==1.0`, `inner_product([1,2],[3,4])==11.0`, `cosine([1,0],[1,0])==0.0`, `cosine([1,0],[0,1])==1.0`; zero-norm cosine → NaN/inf parity; dim-mismatch → 22023. `test_<behavior>`: `l2_matches_pgvector_oracle`, `cosine_clamps_and_one_minus_sim`, `dim_mismatch_rejected_22023`.
- GREEN: minimal f32-accumulation impls.
- REFACTOR: extract the shared dim-check.

#### Concurrency tests
(none — single-threaded) — pure distance math, no shared state

#### Acceptance criteria
- All oracle-value unit tests green; f32 accumulation (not f64) verified by a test that would fail under f64 on a crafted input where the widths diverge; clippy clean.

#### DoD
- `cargo pgrx test` / `cargo test` green for `vec.rs`; oracle values match.

## Phase 3: SQL surface — `theodb.*` distance functions (coexistence wiring)

### T3.1 — `#[pg_extern]` entrypoints + `extension_sql!` `theodb.l2_distance`/`inner_product`/`cosine_distance(vector, vector)`
#### Why this step
**Action:** add 3 `#[pg_extern]` fns in `lib.rs` `mod theodb_rs` that receive two `vector` datums (via the `vec.rs` FFI reader) and call the Phase-2 math; `extension_sql!` creating `theodb.l2_distance(vector, vector) RETURNS float8` etc., REVOKE-from-PUBLIC parity. They COEXIST with pgvector's operators (no operator redefinition — D1).
**Reasoning:** blueprint Q3 + ADR D1 — same `#[pg_extern]` + `extension_sql!` idiom as M17-M19; functions (not operators) avoid the shared-type conflict.

#### Files to edit
- `theodb_rs/src/lib.rs` (3 `#[pg_extern]` + 1 `extension_sql!` block)
- `theodb_rs/src/vec.rs` (the datum→`&[f32]` reader used by the externs)

#### Deep file dependency analysis
The externs receive `vector` — declared in the `extension_sql` wrapper as `vector` arg, the Rust fn reads the datum via the FFI reader (pgvectorscale pattern). `theodb_rs requires theodb` → `vector` type exists at CREATE time. No existing function signature changes.

#### TDD
- RED: a `#[pg_test]` calling `theodb.l2_distance('[0,0]'::vector,'[3,4]'::vector)` asserting `5.0` (needs pg; the OBSERVABLE gate is Phase 4's pytest). `test_<behavior>`: `theodb_l2_distance_matches`.
- GREEN: wire the externs + SQL.
- REFACTOR: dedup the datum-read across the 3 externs.

#### Failure scenarios
See `## Failure scenarios` — dim-mismatch (22023), NULL arg, theodb_rs-absent (function gone → 42883, like hybrid).

#### Concurrency tests
(none — single-threaded) — synchronous SQL functions over local datum bytes, no shared mutable state

#### Acceptance criteria
- `docker build` compiles + installs; `theodb.l2_distance`/`inner_product`/`cosine_distance` exist; REVOKE-from-PUBLIC present; clippy clean.

#### DoD
- The 3 functions callable in the container; return float8.

## Phase 4: parity gate — pytest replaying pgvector's oracle (the OBSERVABLE proof)

### T4.1 — `benchmarks/tests/test_vector_ops.py`: byte-identical to pgvector on oracle rows
#### Why this step
**Action:** add a pytest suite that, against the rebuilt container, runs pgvector's oracle inputs (from `vector_type.sql:89-98` + crafted edge rows) through BOTH pgvector's native funcs AND `theodb.*`, asserting identical text output; + dim-mismatch (22023) + NULL + high-dim (e.g. 1536) parity.
**Reasoning:** blueprint Q4/Q5 + ADR D3 — pytest-against-container is TheoDB's observable gate (CI runs it; `#[pg_test]` does not). Comparing to LIVE pgvector also satisfies CK-1.

#### Files to edit
- `benchmarks/tests/test_vector_ops.py` (NEW)

#### Deep file dependency analysis
Mirrors existing `benchmarks/tests/test_*.py` (psycopg2 against PG* env). No production code.

#### TDD
- RED: the suite (fails until Phase 3 wired). Tests: `test_l2_matches_pgvector_on_oracle`, `test_cosine_matches_pgvector`, `test_inner_product_matches_pgvector`, `test_dim_mismatch_22023`, `test_null_arg`, plus boundary rows from the edge-case review: `test_l2_dim1_boundary` (EC-2 — min valid dim=1), `test_highdim_parity` (EC-3 — two rows: 1536 AND near-max 16000), `test_nan_inf_parity` (EC-4 — `'[NaN]'`/`'[3e38]'` overflow→inf match pgvector exactly). Given-When-Then; assert `theodb.fn(a,b) == pgvector.fn(a,b)` for each row (parity-to-live pgvector).
- GREEN: (Phase 3 makes them pass).
- REFACTOR: parametrize over the oracle rows.

#### Failure scenarios
dim-mismatch → both raise (22023 parity); NULL → both NULL/error parity.

#### Concurrency tests
(none — single-threaded) — pytest issues sequential SQL, no shared mutable state

#### Acceptance criteria
- All parity tests green against the rebuilt `theo-db` image; ≥1 high-dim (1536) row; the full existing suite stays green (no regression).

#### DoD
- `python3 -m pytest benchmarks/tests/test_vector_ops.py` green + full suite green.

## Phase 5: benchmark — numeric-parity proof + perf delta (MANDATORY)

### T5.1 — `benchmarks/bench_vector_ops.py` + `docs/benchmarks/m20-vector-ops-parity.md`
#### Why this step
**Action:** a reproducible benchmark: (a) numeric-parity proof (max abs diff = 0 / within text-rounding on N random + oracle vectors, own vs pgvector); (b) perf delta (own f32-scalar vs pgvector SIMD), mean±std over ≥3 runs, on a fixed dim (e.g. 1536) + row count; write the report.
**Reasoning:** M20 DoD + TheoDB rule 5 / `public-copy.md` — no perf claim without a reproducible benchmark; parity must be MEASURED, not asserted.

#### Files to edit
- `benchmarks/bench_vector_ops.py` (NEW)
- `docs/benchmarks/m20-vector-ops-parity.md` (the report)

#### TDD
- RED: a gate in the bench script: parity max-diff within tolerance → exit 0, else exit 1 (like `bench_nl.py`). `test_<behavior>`: bench exits non-zero on parity violation.
- GREEN: implement the harness (warmup + ≥3 runs, mean±std).
- REFACTOR: parametrize dim/rows.

#### Concurrency tests
(none — single-threaded) — sequential timed calls, no shared mutable state

#### Acceptance criteria
- Bench runs against the container, prints mean±std + parity max-diff, writes the doc; parity gate passes (own == pgvector within text-rounding); records the installed pgvector commit (CK-1). Honest perf framing (scalar vs SIMD), `UNBENCHMARKED` markers removed once measured.

#### DoD
- `python3 benchmarks/bench_vector_ops.py --write-doc` exits 0; doc has numbers + methodology + repro command.

## Coverage Matrix

| Goal/DoD claim | Task(s) |
|---|---|
| Own `#[repr(C)]` pgvector-compatible reader (binary-compat, coexistence) | T1.1 |
| Parity defined vs the INSTALLED pgvector (CK-1) | T1.2, T4.1, T5.1 |
| 3 own distance ops in Rust (l2/ip/cosine), f32-parity | T2.1 |
| SQL surface `theodb.*` coexisting with pgvector (no operator clobber) | T3.1 |
| Numeric parity PROVEN by test (byte-identical to pgvector on oracle) | T4.1 |
| Migration decision (coexistence) documented | D1 + T5.1 doc |
| MANDATORY reproducible parity+perf benchmark | T5.1 |
| No new dependency | Dependencies section + deps-audit |
| CHANGELOG updated | Final phase |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| SIMD low-bit divergence vs pgvector makes "bit-exact" flaky | Medium | ADR D3 — assert to TEXT output (rounded); document scalar-vs-SIMD honestly; no bit-exact claim | maintainers |
| Receiving a `vector` datum in a pgrx `#[pg_extern]` (FFI read) is non-trivial / unsafe | High | follow pgvectorscale's proven `#[repr(C)]` + datum-pointer pattern (our exact pgrx 0.16.1); dim-check before slice; `cargo pgrx test` + container parity gate | maintainers |
| "Own type" DoD interpreted as a competing storage type | Medium | ADR D1 — coexistence is the evidence-backed reading; documented; functions not a new type | maintainers |
| pgvector version drift (installed commit vs 0.8.3 ref) | Low | CK-1 — parity computed vs the LIVE installed pgvector at test time | maintainers |

## Unresolved Questions

- Whether to ALSO expose own `<->`/`<#>`/`<=>` operators bound to an own opclass (for an own index AM) — DEFERRED to M21 (index access method); M20 delivers the type-reader + distance functions only. (Resolved-as-deferred, not open.)

## Failure scenarios (external I/O + adversarial input)

- **Dim-mismatch** (`a.dim != b.dim`): both pgvector and `theodb.*` raise — `theodb.*` raises 22023 (`err_input`, parity with pgvector CheckDims). Test: `test_dim_mismatch_22023` (T4.1).
- **NULL argument**: SQL `STRICT`/NULL handling — `theodb.*` returns NULL on NULL input (parity with pgvector). Test: `test_null_arg` (T4.1).
- **theodb_rs dropped** (the function gone): calling `theodb.l2_distance` → 42883 (clean undefined_function, like M19 hybrid). Documented; not a data path.
- **Zero-norm cosine**: division by zero → inf/NaN — parity with pgvector's unguarded core path (T2.1 test).
- (No network/HTTP I/O in M20 — distance math is pure + reads local datum bytes.)

## Global Definition of Done

- [ ] T1.1–T5.1 complete; Coverage Matrix 100%.
- [ ] `theodb.l2_distance`/`inner_product`/`cosine_distance` exist, REVOKE-from-PUBLIC, coexist with pgvector (no operator clobber).
- [ ] `benchmarks/tests/test_vector_ops.py` green (byte-identical to pgvector on oracle + high-dim) AND full existing suite green (no regression) against the rebuilt image.
- [ ] `cargo clippy --release --features pg17 -- -D warnings` CLEAN; `cargo pgrx test` compiles the new unit tests.
- [ ] Reproducible benchmark (`bench_vector_ops.py` + `docs/benchmarks/m20-vector-ops-parity.md`): parity max-diff within tolerance + perf delta (mean±std ≥3 runs); installed pgvector commit recorded.
- [ ] Migration decision (coexistence) documented (ADR D1 + benchmark doc).
- [ ] No new dependency (deps-audit PASS); each changed file ≤ 500 LoC; CHANGELOG `[Unreleased]` updated.
- [ ] File-size budget: `vec.rs` ≤ 120 LoC; `lib.rs` stays cohesive.

## Final Phase: Integration Validation (MANDATORY)

1. `docker build -t theo-db:m20 .` → EXIT 0.
2. Recreate container; run the FULL `benchmarks/tests/` SQL-integration suite (nl/hybrid/unified/embed/ai/import/install/retirement + NEW `test_vector_ops.py`) — all green, zero regression.
3. `cargo clippy -D warnings` CLEAN; `cargo check --tests` compiles.
4. `bench_vector_ops.py --write-doc` exits 0 (parity gate) + doc written.
5. `/code-quality` verdict ∉ {FAIL_HARD, INVALID}; `/deps-audit` PASS (no new dep).
6. CHANGELOG updated. Plan NOT complete until the chain passes (eat-your-own-cooking gate).
