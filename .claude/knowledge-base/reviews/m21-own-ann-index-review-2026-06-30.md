# Review: m21-own-ann-index — 2026-06-30

**Verdict:** `READY_TO_MERGE`
**Domain:** database / ANN-algorithm (primary)
**Agents:** 6 (architecture, algorithm-correctness, tests, wiring, cross-validation, domain-database/security)
**Severity tally (as found):** BLOCKER 0 · HIGH 3 · MEDIUM 4 · LOW ~5 · INFO ~12
**Severity tally (after in-cycle fixes):** BLOCKER 0 · HIGH 0 · MEDIUM 0 open · LOW/INFO residual (documented)

M21 implements TheoDB's own HNSW + IVFFlat ANN search in Rust (`theodb.hnsw_knn`/`ivfflat_knn`), recall-gated vs
pgvector (coexistence, measurement-first SQL-callable scope). 6 specialist agents reviewed the M21 diff
(`272cd68..HEAD`). **No BLOCKER.** The 3 HIGH + the material MEDIUMs were fixed in-cycle.

## Per-agent summary

| Agent | Verdict (as found) | Headline |
|---|---|---|
| architecture | 0 BLOCKER; 1 HIGH; 1 MEDIUM; 1 LOW | Clean DIP (ann pure / ann_query IO / lib api), coexistence airtight; `ann.rs` 580 > 500 budget (HIGH) |
| algorithm-correctness | 0 BLOCKER/HIGH | Level formula, two-heap search, NaN ordering, k-means++, `<#>` negation all sound; recall claims plausible |
| tests | 0 BLOCKER; 2 HIGH; 3 MEDIUM; 1 LOW | Pyramid solid + 22023 asserted exactly; missing tests for inconsistent-dims, empty-table, lower-bounds, injection, all-identical-IVF |
| wiring | 0 BLOCKER/HIGH/MEDIUM; 1 LOW | Triad complete; REVOKE signatures match; flatten query-major correct; private-extern REVOKE untested (LOW) |
| cross-validation | 0 BLOCKER; 1 HIGH; 1 LOW | DoD met, parity gate passing, scope honesty clear, anti-sunk-cost implemented; file-size criterion (HIGH) |
| domain-database/security | 0 BLOCKER/HIGH; 1 MEDIUM; 1 LOW | No injection/SQLSTATE vuln; implicit volatility (MEDIUM); unbounded-memory undocumented (LOW) |

## Findings + resolution

### Fixed in-cycle

1. **ARCH-H1 / XV-H1 (HIGH) — `ann.rs` 580 LoC exceeds the 500 budget.** Plan DoD prescribed "split into
   `ann/hnsw.rs` + `ann/ivf.rs` if exceeded". **Fixed:** split `ann.rs` → `ann/mod.rs` (246: shared `Metric`/
   `Rng`/`Cand` + tests), `ann/hnsw.rs` (212), `ann/ivf.rs` (139) — each < 500.

2. **TST-H1 (HIGH) — inconsistent corpus dimensions not tested.** Code catches it (`ann_query.rs`), no test.
   **Fixed:** `test_knn_inconsistent_vector_dims_raises_22023` (mixed-dim `vector` column → 22023).

3. **TST-H2 (HIGH) — empty source table not tested.** **Fixed:** `test_knn_empty_table_returns_zero_rows`.

4. **TST-M1 (MEDIUM) — parameter lower bounds not tested.** **Fixed:** `test_knn_param_lower_bounds_raise_22023`
   parametrized over k<1, m<2, m>100, ef_construction<k, ef_search<k, lists<1, probes<1 (all 22023).

5. **TST-M2 (MEDIUM) — SQL injection via column name not tested.** **Fixed:**
   `test_knn_injection_in_column_name_rejected` (3 hostile `embed_col` payloads → 22023 AND the corpus table
   survives — proves the malicious DDL never executed).

6. **TST-M3 (MEDIUM) — all-identical-vector IVFFlat corpus (k-means++ zero-sum path) not tested.** **Fixed:**
   `ivfflat_all_identical_corpus_no_panic` `#[pg_test]`.

7. **DB-M1 (MEDIUM) — implicit volatility on the table-reading externs.** **Fixed:** `#[pg_extern(volatile)]`
   on `_hnsw_knn`/`_ivfflat_knn` (explicit; never IMMUTABLE).

8. **WIRE-L1 (LOW) — private-extern REVOKE untested.** **Fixed:** extended `test_knn_revoked_from_public` to
   also assert `theodb_rs._hnsw_knn`/`._ivfflat_knn` are REVOKEd from PUBLIC.

9. **DB-L1 (LOW) — unbounded-memory read undocumented.** **Fixed:** doc comment on `read_corpus` (measurement-
   first scope; pre-filter large tables; streaming on-disk AM is M21b).

10. **DB-INFO — allowlist vs `%I`-quoting choice.** **Fixed:** rationale comment on `valid_ident` (strict
    allowlist is injection-proof; keyword/special-char columns out of scope for an embedding column).

### Residual (LOW/INFO — accepted, documented)

- **ARCH-M1 (MEDIUM→accepted):** `Params` carries per-algorithm-unused fields (validation ignores them per algo).
  A pragmatic, type-unsafe-but-simple choice (KISS); not a bug. An `enum Params { Hnsw{..}, Ivfflat{..} }`
  refactor is deferred as optional polish.
- **#[pg_test] not run in CI** (same as M18-M20) — the always-on proof is the Python container suite (now 26
  tests) + the standalone prototype (10/10). Disclosed.
- **Latency not compared to pgvector** — own rebuilds per call vs pgvector's persisted index; recall is the gate.
  Documented in the benchmark doc; latency parity awaits M21b.
- **Scope** — SQL-callable (not a planner `CREATE INDEX … USING` AM). The user-chosen measurement-first scope;
  M21b deferral stated in plan/CHANGELOG/impl-summary/benchmark-doc.

## Hard gates (all pass)

- Tests green on `develop`: `pytest benchmarks/tests/test_ann_index.py` → **26 passed** (recall, parity gate,
  22023 negatives incl. lower-bounds/injection/inconsistent-dims, NULL-skip, empty-table/queries, REVOKE incl.
  private externs).
- `cargo clippy --release --features pg17 -- -D warnings`: CLEAN; `cargo pgrx install`: succeeds (symbols resolve).
- No secrets; no Co-Authored-By; CHANGELOG updated; working on `develop`.
- code-quality verdict PASS (clippy-backed; `.claude/knowledge-base/audits/m21-own-ann-index-code-quality-2026-06-30.md`).
- Benchmark: recall@k **PARITY_REACHED** at every swept point (`docs/benchmarks/m21-ann-index-parity.md`, mean±std ≥3 runs).
- ANN algorithm correctness independently reviewed (level formula, search heaps, k-means++, `<#>` negation) — sound.

## Output

- This report: `.claude/knowledge-base/reviews/m21-own-ann-index-review-2026-06-30.md`
- Per-agent findings: `.claude/agents/review-m21-own-ann-index-2026-06-30/` (consolidated above)
