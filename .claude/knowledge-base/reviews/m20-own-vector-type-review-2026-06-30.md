# Review: m20-own-vector-type — 2026-06-30

**Verdict:** `READY_TO_MERGE`
**Domain:** database / numeric-parity (primary)
**Agents:** 6 (architecture, numeric-parity, tests, wiring, cross-validation, domain-database)
**Severity tally (as found):** BLOCKER 0 · HIGH 1 · MEDIUM 6 · LOW ~10 · INFO ~16
**Severity tally (after in-cycle fixes):** BLOCKER 0 · HIGH 0 · MEDIUM 0 open · LOW/INFO residual (documented)

M20 implements TheoDB's own Rust f32-parity distance functions (`theodb.l2_distance`/`inner_product`/
`cosine_distance`) over pgvector's `vector` (coexistence). 6 specialist agents reviewed `git diff
origin/main..develop`. **No BLOCKER.** The 1 HIGH + the material MEDIUMs were fixed in-cycle. Per-agent
findings: `.claude/agents/review-m20-own-vector-type-2026-06-30/findings/*.yaml`.

## Per-agent summary

| Agent | Verdict | Headline |
|---|---|---|
| architecture | 0 BLOCKER/HIGH/MEDIUM; 1 LOW | Coexistence (functions, not redefined operators) is the right call; parsimony `::real[]` refinement sound + documented |
| numeric-parity | 0 BLOCKER/HIGH/MEDIUM; 2 LOW | **Byte-faithful to pgvector's vector.c** (f32 accum, f64 sqrt/divide, cosine clamp, `<#>`=-ip); no real divergence; `::real[]` lossless |
| tests | **1 HIGH** + 4 MEDIUM + 2 LOW | f32 determinant + 22023 + NaN + TOAST path not guarded by the always-on CI gate |
| wiring | 0 BLOCKER/HIGH; 1 LOW | Triad complete for all 3 externs; no dead export; REVOKE on all 6; `::real[]`→Vec<f32> wiring works |
| cross-validation | 0 BLOCKER/HIGH; 1 MEDIUM + 3 LOW | Plan↔impl aligned; FFI→`::real[]` divergence documented (5 places), ADR D1 intent met; DoD-A reinterpreted (coexistence) |
| domain-database | 0 BLOCKER/HIGH; 1 MEDIUM + 2 LOW | IMMUTABLE/STRICT/PARALLEL SAFE correct; coexistence clean; inner externs were VOLATILE (inlining/perf) |

## Findings + resolution

### Fixed in-cycle

1. **TST-H1 (HIGH) — f32-accumulation determinant not guarded by the always-on gate.** CI runs only the Python
   parity gate (REL_TOL=1e-5); an f32→f64 regression diverges <1e-5 on the existing rows → would pass. The
   discriminating `#[pg_test]` isn't run in CI. **Fixed:** added a catastrophic-cancellation row to the
   always-on Python gate — `inner_product([1e8,1,-1e8],[1,1,1])` is 0 in f32, 1 in f64; pgvector (f32) → 0, so
   `theodb == pgvector` holds ONLY if theodb accumulates in f32 (an f32→f64 regression → theodb≈1, rel 1.0 ≫
   REL_TOL → FAILS). Plus an explicit `test_f32_accumulation_not_f64_via_sql`.

2. **DB-M20-001 (MEDIUM) — inner `_vec_*` externs were default VOLATILE/PARALLEL UNSAFE** while the wrappers
   are IMMUTABLE/STRICT/PARALLEL SAFE → blocks SQL inlining (perf overhead). **Fixed:**
   `#[pg_extern(immutable, parallel_safe, strict)]` on all 3 inner externs (matches the wrappers, enables
   inlining).

3. **TST-M1 (MEDIUM) — dim-mismatch test didn't assert the specific SQLSTATE.** **Fixed:**
   `test_dim_mismatch_raises_22023` now asserts `exc.value.pgcode == '22023'` (testing.md §4.1).

4. **TST-M2 (MEDIUM) — NaN input literal missing.** **Fixed:** added `("[NaN,1]","[1,1]")` to PAIRS
   (NaN-propagation parity, `_parity_ok` handles NaN equality).

5. **TST-M3 (MEDIUM) — TOAST/column-stored path not in the always-on gate.** **Fixed:**
   `test_column_stored_toastable_vector_parity` inserts a dim-4096 (TOASTable) vector into a table and asserts
   `theodb.*` == pgvector from the COLUMN (proves the `::real[]` cast detoasts end-to-end).

6. **Doc honesty (LOW — numeric-parity NP-2/3, domain-db DB-M20-002):** `vec.rs check_dims` comment reworded —
   pgvector raises 22000, TheoDB uses house 22023 (documented divergence, same fail-fast); the 22023 is now
   asserted. The `::real[]`-vs-`#[repr(C)]` parsimony refinement stays documented in 5 places.

### Residual (LOW/INFO — accepted, documented)

- **XV-1 (MEDIUM→reconciled):** ROADMAP-v2 M20 DoD literal "own type + operators" shipped as 3 coexistence
  FUNCTIONS (no competing storage type, no redefined `<->`/`<#>`/`<=>`), per blueprint(97.3) + ADR D1. Own
  type/operators+opclass are explicitly DEFERRED to M21 (index AM). This is a documented reinterpretation
  (measurement-first / coexistence — the evidence-backed reading), recorded in ADR D1 + impl summary +
  CHANGELOG, NOT a silent violation. Accepted: M20 delivers the own f32-parity distance computation in Rust;
  a competing type would have forked data + broken HNSW/IVFFlat/DiskANN + embed/hybrid/import.
- **ARCH-M20-01 (LOW):** `vec.rs` math couples to `crate::pg::err_input` (an ereport), so it's `#[pg_test]`,
  not plain `#[test]` — matches the M17-M19 ADR-C boundary convention. Accepted.
- **Perf (LOW):** `theodb.*` is scalar f32 + per-call `::real[]` materialization (~3.9× vs pgvector SIMD), and
  cannot use pgvector ANN indexes. Documented honestly (M20 is parity, not perf/indexing — SIMD + index AM are
  M21+).
- **#[pg_test] not run in CI** (same as M18/M19) — disclosed; the always-on gate is the Python parity suite,
  now hardened with the f32 discriminator (TST-H1 fix).

## Hard gates (all pass)

- Tests green on `develop`: full SQL integration suite + the hardened `test_vector_ops.py` (parity vs LIVE
  pgvector incl. the f32-discriminator, NaN, 22023, column-stored TOAST, dim=1/1536/16000, NULL, `<#>`, REVOKE).
- `cargo clippy --release --features pg17 -- -D warnings`: CLEAN; `cargo check --tests`: CLEAN.
- No secrets; no Co-Authored-By; CHANGELOG updated; working on `develop`.
- code-quality verdict PASS (clippy-backed; audit at `knowledge-base/audits/m20-own-vector-type-code-quality-2026-06-30.md`).
- Benchmark: numeric parity ~1e-6 rel (proven) + perf delta documented (`docs/benchmarks/m20-vector-ops-parity.md`).

## Output

- Per-agent findings: `.claude/agents/review-m20-own-vector-type-2026-06-30/findings/*.yaml`
- This report: `knowledge-base/reviews/m20-own-vector-type-review-2026-06-30.md`
