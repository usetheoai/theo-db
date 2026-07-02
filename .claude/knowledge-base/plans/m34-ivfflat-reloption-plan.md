---
slug: m34-ivfflat-reloption
milestone_id: M34
created_at: 2026-07-02
goal: Make theodb_ivfflat lists (build reloption) + probes (scan GUC) configurable so its Index Scan p50 reaches <= pgvector at 1M, measured by a re-run of the M32 harness (docs/benchmarks/m34-ivfflat-reloption.{md,json}) and the M26/M31/M20-M22 suites staying green.
---

# M34 — theodb_ivfflat configurable lists/probes (reloption + GUC)

## Goal

Make `theodb_ivfflat`'s `lists` (build) and `probes` (scan) **configurable** — `lists` via a `WITH (lists=N)`
reloption (pgrx `amoptions`), `probes` via a `SET theodb_ivfflat.probes` GUC (pgrx `GucRegistry`) — so that with
`lists=1000` + tuned probes its Index Scan p50 reaches **≤ pgvector** at 1M×128 (recall ≥ parity), **measured by**
a re-run of the M32 harness written to `docs/benchmarks/m34-ivfflat-reloption.{md,json}`, with the M26/M31 index-AM
suites and the M20–M22 coexistence suites staying green (default preserves current behavior).

## Context

M32 measured `theodb_ivfflat` ~8× behind pgvector on QPS at 1M — root cause: fixed `DEFAULT_LISTS=100`
(`am/build.rs:14`) + `SCAN_PROBES=10` (`am/scan.rs:13`), no reloption/GUC (`amoptions = None`, `am/mod.rs:94`). At
1M, 100 lists → ~10k/list → 10 probes scan ~100k candidates vs pgvector's `lists=1000` → ~10k. The blueprint
(`.claude/knowledge-base/discoveries/blueprints/m34-ivfflat-reloption-blueprint.md`) confirms the fix is the
pgvector/pgvectorscale reloption+GUC pattern (both cloned references). Lever 2 (structured HNSW scan) was split to
M35 after discovery sized it at ~3-4× M31.

## Baseline Context

### Files that will be touched

| File | LoC | git sha (last) | Why |
|---|---|---|---|
| `theodb_rs/src/am/options.rs` | (NEW) | — | `#[repr(C)] TheodbIvfflatOptions{vl_len_, lists}` + `amoptions` callback + `init()` (reloption kind) + `lists_from_relation` |
| `theodb_rs/src/am/guc.rs` | (NEW) | — | `PROBES: GucSetting<i32>` + `init()` (`define_int_guc theodb_ivfflat.probes`) |
| `theodb_rs/src/am/mod.rs` | ~100 | `61e64db` | `amroutine.amoptions = Some(options::amoptions)` (line 94); add `mod options; mod guc;` |
| `theodb_rs/src/lib.rs` | 95 | `61e64db` | add `_PG_init` calling `am::options::init()` + `am::guc::init()` |
| `theodb_rs/src/am/build.rs` | ~240 | `61e64db` | `ambuild` reads `lists` from `indexrel` rd_options (fallback 100) → `IvfflatIndex::build` |
| `theodb_rs/src/am/scan.rs` | ~200 | `61e64db` | `scan_ivf_structured` reads `probes` from the GUC (fallback 10) instead of `SCAN_PROBES` |
| `benchmarks/run_m32_sift1m.py` | 84 | — | (reuse) parametrize theodb_ivfflat build `WITH (lists=…)` + `SET theodb_ivfflat.probes` for the M34 gate |
| `docs/benchmarks/m34-ivfflat-reloption.{md,json}` | (NEW) | — | the 1M evidence artifact |

### Current callers / dependents

- `ambuild` (`am/build.rs:57`) — receives `indexrel: pg_sys::Relation`; today hardcodes `DEFAULT_LISTS`. Reads rd_options via a `PgRelation` wrapper (pattern: pgvectorscale `options.rs:31` `from_relation`).
- `scan_ivf_structured` (`am/scan.rs:78`) — computes `probes = SCAN_PROBES.clamp(1, meta.centroids.len().max(1))` (`am/scan.rs:93`); the clamp stays (a large probes is a safe no-op).
- `make_amroutine` (`am/mod.rs:65`) sets `amroutine.amoptions = None` (`am/mod.rs:94`) — the single line to flip.
- `IvfflatIndex::build(corpus, lists, metric, seed)` (`ann/ivf.rs:19`) — already takes `lists` as a param; M34 just feeds it a non-constant value. NO change to the k-means itself.
- `pg_module_magic!()` in `lib.rs:17`; NO `_PG_init` today — must add one (pgrx honors a user `_PG_init`).

### Domain glossary

- **reloption** — a `WITH (key=value)` index storage option; parsed by an AM's `amoptions` callback into `rd_options` (a `bytea`), read at build. `lists` is a build param → reloption.
- **GUC** — a `SET key=value` runtime setting (pgrx `GucSetting`/`GucRegistry`); read at scan. `probes` is a per-query knob → GUC.
- **rd_options** — the relation's parsed reloption struct pointer (null when no `WITH` given → defaults).
- **amoptions** — the IndexAmRoutine callback `fn(Datum, bool) -> *mut bytea` that parses reloptions via `build_reloptions`.

### Architecture boundaries affected

The AM interface layer (`am/`) gains an options/GUC surface; the domain (`ann/ivf.rs` k-means) is UNCHANGED (it already accepts `lists`). DIP preserved: `am/build.rs` reads the option and passes a plain `usize` to the domain. `_PG_init` is the composition root for registration (lib.rs).

## Prior Art & Related Work

- Blueprint (this cycle): `.claude/knowledge-base/discoveries/blueprints/m34-ivfflat-reloption-blueprint.md`.
- **Reloption + GUC copy source (cloned, same pgrx 0.16.1):** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/options.rs` (amoptions + `from_relation` + `add_int_reloption`) and `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/guc.rs` (`GucRegistry::define_int_guc`).
- **Semantics reference (cloned):** `.claude/knowledge-base/references/pgvector/src/ivfflat.c` + `.claude/knowledge-base/references/pgvector/src/ivfflat.h` — `lists` reloption + `ivfflat.probes` GUC (the build-param-reloption / query-param-GUC split).
- In-repo AM: `theodb_rs/src/am/{mod,build,scan}.rs` (M26/M31); harness: `benchmarks/run_m32_sift1m.py` (M32).

## ADRs

### ADR-1 — reloption for `lists` (build), GUC for `probes` (scan)
**Decision:** `lists` → `WITH (lists=N)` reloption via `amoptions`; `probes` → `SET theodb_ivfflat.probes` GUC.
**Rationale:** mirror pgvector/pgvectorscale exactly (`ivfflat.c`): a build param is baked into the partition at
build (reloption); a query knob must be tunable per session without rebuild (GUC). Unbreakable Rule 9 (don't
reinvent — copy the proven AM pattern).
**Alternatives rejected:** (a) one mechanism for both — `probes` can't be a build reloption (needs per-session
tuning), `lists` can't be a runtime GUC (baked at build); (b) an env var like `THEODB_SCAN_PROFILE` — not
SQL-standard, invisible to `SET`/`EXPLAIN`, wrong UX for a query knob.

### ADR-2 — default MUST preserve current behavior (no regression)
**Decision:** when `WITH (lists=)` is absent, `rd_options` null → `lists=100`; when the GUC is unset → `probes=10`.
**Rationale:** every existing M26/M31 index-AM test builds without options and MUST behave identically. The default
sentinel is the load-bearing safety property.
**Alternatives rejected:** changing the default to `lists=1000` now — would silently alter every existing index +
could regress recall on small N (fewer points/list than lists).

### ADR-3 — edge validation via reloption min/max + scan clamp (fail-fast, no crash)
**Decision:** `add_int_reloption` enforces `lists` ∈ [1, 32768] (rejects at DDL with a typed error); the GUC
enforces `probes` ∈ [1, 32768]; the scan keeps `probes.clamp(1, nlists)` so an over-large probes is a no-op.
**Rationale:** Error Handling (Rule 8) — invalid options fail-fast at the boundary (DDL/SET), never an OOB at scan.
**Alternatives rejected:** silently clamping `lists` at build (hides the user error).

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `0.16.1` | Rust | already provides `GucRegistry`/`GucSetting` + `pg_sys` reloption FFI (`add_reloption_kind`, `add_int_reloption`, `build_reloptions`, `relopt_parse_elt`) — same version pgvectorscale uses |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | | | pgrx + pg_sys already expose the whole reloption/GUC surface | no new dep needed |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency graph

```
Phase 1 (lists reloption: options.rs + amoptions wired + _PG_init + build reads it)
        │
        ▼
Phase 2 (probes GUC: guc.rs + _PG_init + scan reads it)   [independent of P1 code-wise; shares _PG_init]
        │
        ▼
Phase 3 (1M benchmark: theodb_ivfflat WITH lists=1000 + tuned probes -> p50 <= pgvector; coexistence green)
```

## Phase 1 — `lists` build reloption

### T1.1 — reloption `WITH (lists=N)` wired into ambuild

#### Why this step
The ~8× QPS gap is dominated by theodb building only 100 lists at 1M. Making `lists` a reloption (the pgvector
pattern) lets a 1M index be built `WITH (lists=1000)` so each probed list is ~1k not ~10k — the core DoD lever.
Copying the pgvectorscale `amoptions` surface (same pgrx) is the KISS, Rule-9 path.

#### Files to edit
- `theodb_rs/src/am/options.rs` (NEW), `theodb_rs/src/am/mod.rs`, `theodb_rs/src/lib.rs`, `theodb_rs/src/am/build.rs`

#### TDD
- RED: `#[pg_test] reloption_lists_roundtrips_to_meta` — `CREATE INDEX … USING theodb_ivfflat (embedding
  theodb_ivfflat_l2_ops) WITH (lists=7)` on a small table; read back the persisted meta (`page::read_ivf_meta`) and
  assert `meta.centroids.len() == 7` (the build used 7 lists, not the default 100). Given-When-Then: given `WITH
  (lists=7)`, when the index builds, then the structured meta has 7 centroids.
- RED: `#[pg_test] reloption_absent_defaults_to_100` — `CREATE INDEX …` with NO `WITH` on a ≥100-row table; assert
  `meta.centroids.len() == 100` (default preserved — ADR-2).
- RED: `#[pg_test(error = …)] reloption_lists_out_of_range_rejected` — `WITH (lists=0)` raises at DDL (ADR-3).
- GREEN: add `TheodbIvfflatOptions` + `amoptions` + `init()` (`add_reloption_kind` + `add_int_reloption("lists",100,1,32768,AccessExclusiveLock)`); `mod.rs` sets `amroutine.amoptions = Some(options::amoptions)`; `lib.rs` `_PG_init` calls `options::init()`; `build.rs` reads `lists` from `indexrel` rd_options (fallback 100) → `IvfflatIndex::build`.
- REFACTOR: a `lists_from_relation(indexrel) -> usize` helper (single source; mirrors pgvectorscale `from_relation`).

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- `WITH (lists=<bad>)` (0, negative, > 32768) → typed DDL error via `add_int_reloption` bounds (ADR-3), asserted by `reloption_lists_out_of_range_rejected`.

#### Acceptance criteria
- `cargo pgrx test` (in Docker build) green for the three reloption tests.
- `amroutine.amoptions` is `Some` (grep) and `_PG_init` registers the reloption kind.
- Building WITHOUT options yields exactly 100 lists (byte-identical to today's behavior).

#### DoD
- `PGPORT=<c> python3 -m pytest benchmarks/tests/test_index_am.py benchmarks/tests/test_index_am_latency.py -q` exits 0 (M26/M31 default path unchanged).
- `grep -ciE '^#[0-9]+ .*warning:' <docker-build-log>` returns `0` (release build clean — no dead code / no fabricated symbol).

## Phase 2 — `probes` scan GUC

### T2.1 — GUC `theodb_ivfflat.probes` read at scan

#### Why this step
`probes` is the per-query recall/speed knob; a well-tuned pgvector uses `SET ivfflat.probes`. theodb must expose the
same so an operator tunes recall vs QPS without rebuilding. Reading a `GucSetting` at scan is one `.get()` (pgrx).

#### Files to edit
- `theodb_rs/src/am/guc.rs` (NEW), `theodb_rs/src/lib.rs` (`_PG_init` += `guc::init()`), `theodb_rs/src/am/scan.rs`

#### TDD
- RED: `#[pg_test] guc_probes_changes_candidate_count` — build a small structured index (e.g. 5 lists); `SET
  theodb_ivfflat.probes = 1` then `= 5`; assert the scan with probes=5 returns ≥ as many correct neighbors as
  probes=1 (more probes → ≥ recall), and that probes is honored (via the `THEODB_SCAN_PROFILE` `nonempty_lists`/
  `cand` line or a returned-count assertion). Given-When-Then: given `SET …probes=P`, when a scan runs, then it
  probes P lists (clamped to nlists).
- RED: `#[pg_test] guc_probes_default_is_10` — unset GUC → scan uses 10 (ADR-2; assert via profile or behavior on a >10-list index).
- GREEN: `guc.rs` `PROBES: GucSetting<i32> = GucSetting::new(10)` + `define_int_guc("theodb_ivfflat.probes", …, 1, 32768, Userset)`; `_PG_init` calls `guc::init()`; `scan.rs` reads `let probes = guc::PROBES.get().max(1) as usize;` then keeps `.clamp(1, nlists)`.
- REFACTOR: replace the `SCAN_PROBES` const usage in `scan_ivf_structured` with the GUC read (leave the const only if still referenced by the blob/Persisted path, else remove — no dead code).

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- `SET theodb_ivfflat.probes = 0` → the GUC min (1) rejects (or the scan `.max(1)` guards); never a zero-probe empty scan.

#### Acceptance criteria
- `cargo pgrx test` green for the two GUC tests.
- `SHOW theodb_ivfflat.probes` returns 10 by default; `SET` changes it; the scan honors it.
- Default (unset) scan behaves identically to today (probes=10).

#### DoD
- No dead code (`SCAN_PROBES` fully replaced in the structured path or kept only where still used).
- M20–M22 + M26/M31 suites green.

## Phase 3 — 1M benchmark validation

### T3.1 — theodb_ivfflat p50 ≤ pgvector at 1M (the DoD gate)

#### Why this step
The DoD is measured, not asserted: with `lists=1000` + tuned probes, theodb_ivfflat must reach p50 ≤ pgvector at
1M (recall ≥ parity). Re-run the M32 harness with the new options; honest artifact.

#### Files to edit
- `benchmarks/run_m32_sift1m.py` (parametrize theodb_ivfflat `WITH (lists=…)` + `SET theodb_ivfflat.probes`), `docs/benchmarks/m34-ivfflat-reloption.{md,json}` (NEW)

#### TDD
- Not a unit test — the measured artifact IS the gate. The `theodb_bench` harness already validates recall/QPS/p50.
  RED: `test_scale_benchmark.py::test_ivfflat_reloption_changes_operating_point` asserts (CI-safe real N) that a
  theodb_ivfflat built `WITH (lists=200)` scores strictly FEWER candidates per query (via the `THEODB_SCAN_PROFILE`
  `cand=` line) than one built `WITH (lists=20)`, and that `th_p50 <= pv_p50 * 1.10` at the tuned point — proving
  the reloption/GUC plumbing changes the measured operating point.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- If p50 ≤ pgvector is NOT reached even with tuning → report the honest residual + the exact tuning tried (Rule 3, ADR-1 of M32 lineage); never fake the number. (Expected reachable: with lists=1000 theodb scans ~10k like pgvector; the SIMD (M31b) already ≤ pgvector at 100k.)

#### Acceptance criteria
- `docs/benchmarks/m34-ivfflat-reloption.json` shows `theodb_ivfflat` p50 ≤ pgvector ivfflat (probes matched) at n=1M, recall ≥ parity, mean±std ≥3 runs, hardware + repro command.
- `benchmarks/tests/test_scale_benchmark.py` new assertion green against the container.

#### DoD
- `python3 -c "import json;d=json.load(open('docs/benchmarks/m34-ivfflat-reloption.json'));..."` asserts `theodb_ivfflat` p50 ≤ pgvector ivfflat p50 (matched probes) at `n==1000000` with `recall ≥ pgvector - 0.01`.
- `docs/benchmarks/m34-ivfflat-reloption.md` carries the reproduction command + honest per-knob note; CHANGELOG `[Unreleased]` has the M34 entry.

## Coverage Matrix

| Goal / DoD item | Task(s) |
|---|---|
| `lists` configurable via `WITH (lists=N)` reloption; default preserves behavior | T1.1 |
| `probes` configurable via `SET theodb_ivfflat.probes` GUC; default preserves behavior | T2.1 |
| theodb_ivfflat p50 ≤ pgvector at 1M (recall ≥ parity), benchmark-validated | T3.1 |
| Edge validation (lists/probes range → typed error, no crash) | T1.1 (reloption bounds), T2.1 (GUC bounds + clamp) |
| M20–M22 + M26/M31 coexistence green (no regression) | T1.1, T2.1 DoD |
| No new dependency (Rule 9) | Dependencies (none) |
| CHANGELOG (Rule 6) | T3.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| `_PG_init` / GUC registration wrong → extension fails to load | HIGH | copy the exact pgvectorscale `guc.rs`/`options.rs::init` shape; the Docker build + `CREATE EXTENSION` smoke catches a bad `_PG_init` immediately | paulohenriquevn |
| reloption struct layout (`vl_len_` varlena) mishandled → corrupt rd_options | HIGH | copy pgvectorscale `#[repr(C)]` + `set_varsize_4b` / `build_reloptions` verbatim; a round-trip test (T1.1) proves the read matches the written `lists` | paulohenriquevn |
| ivfflat build at `lists=1000` on 1M is slow (single-thread k-means) | MEDIUM | time cost not correctness; documented (ADR-3 of blueprint); operator artifact, not CI | paulohenriquevn |
| p50 ≤ pgvector not reached even tuned | MEDIUM | honest residual report (Rule 3); expected reachable given lists=1000 → ~10k scanned + M31b SIMD | paulohenriquevn |

## Unresolved Questions

- Does pgrx 0.16.1 auto-provide a `_PG_init` hook, or must it be a `#[pg_guard] pub extern "C" fn _PG_init()`? — resolved in T1.1/T2.1 by copying pgvectorscale's `lib.rs` init pattern (the same pgrx version); the Docker `CREATE EXTENSION` smoke is the fail-fast check.
- Should `SCAN_PROBES` in `am/index.rs` (the blob/Persisted HNSW path) also read the GUC? — No: that path is the legacy blob ivfflat / HNSW; M34 scopes only the structured `theodb_ivfflat` scan (`am/scan.rs`). HNSW ef is M35.

## Failure scenarios

- **Bad reloption/GUC value** → typed error at DDL/SET (bounds), never a scan crash. (T1.1, T2.1)
- **Extension load failure from a bad `_PG_init`** → caught by the Docker `CREATE EXTENSION` smoke before any test. (T1.1)
- **p50 not ≤ pgvector** → honest residual artifact, no fabricated number. (T3.1)

## Final Phase — Integration Validation

- Docker build (release, `cargo pgrx test` for the reloption/GUC pg_tests) + 0 warnings.
- `CREATE EXTENSION theodb_rs` smoke (proves `_PG_init` + amoptions registration).
- M26/M31 index-AM + M20–M22 suites green (default path unchanged).
- 1M benchmark artifact: theodb_ivfflat p50 ≤ pgvector (tuned), committed with repro + honest note.
