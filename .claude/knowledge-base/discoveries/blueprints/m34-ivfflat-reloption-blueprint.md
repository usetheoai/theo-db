# Blueprint: M34 — theodb_ivfflat configurable lists/probes (reloption + GUC)

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS — the wiring is a well-trodden path fully present in two cloned
> references (pgvectorscale amoptions + pgvector ivfflat reloption/GUC) and the M32 measurement pinpoints the exact
> lever. Discovery method: in-repo AM inventory + reference study (pgvectorscale/pgvector, both cloned).

**Slug:** `m34-ivfflat-reloption` · **Owner:** paulohenriquevn · **Created:** 2026-07-02

## Context

M32 measured `theodb_ivfflat` ~8× behind pgvector on QPS at 1M (30.7 vs 242) — root cause: `DEFAULT_LISTS=100`
(`theodb_rs/src/am/build.rs:14`) + `SCAN_PROBES=10` (`theodb_rs/src/am/scan.rs:13`) are **fixed Rust constants**
with no reloption/GUC (`amroutine.amoptions = None`, `theodb_rs/src/am/mod.rs:94`). At 1M each list holds ~10k
vectors → 10 probes scan ~100k candidates vs pgvector's `lists=1000` → ~10k. M34 makes both configurable
(`lists` at build, `probes` at scan) so theodb_ivfflat reaches p50 ≤ pgvector at 1M. (Lever 2 — structured HNSW
scan — was split to M35 after discovery sized it at ~3-4× M31.)

## Coverage Corner 1 — Integration Tests

Reuse the M32 harness for the 1M DoD gate (`benchmarks/run_m32_sift1m.py` → `docs/benchmarks/`): with theodb_ivfflat
built `WITH (lists=1000)` + `SET theodb_ivfflat.probes`, assert its p50 **≤ pgvector** at 1M (recall ≥ parity). New
extension `#[pg_test]`s: reloption `WITH (lists=N)` round-trips to the build; GUC `theodb_ivfflat.probes` read at
scan changes the candidate count; edge validation (lists/probes out of range → typed error). The M26/M31 index-AM
suites (`benchmarks/tests/test_index_am*.py`) MUST stay green with the DEFAULT (unchanged behavior when no option set).

## Coverage Corner 2 — Dependencies

**No new dependency.** pgrx 0.16.1 already provides `pgrx::GucRegistry`/`GucSetting` (GUC registration) and
`pg_sys` exposes the reloption FFI (`add_reloption_kind`, `add_int_reloption`, `build_reloptions`,
`relopt_parse_elt`). The reference `pgvectorscale` uses the identical pgrx version + these exact APIs.

## Coverage Corner 3 — Tools

- **Build reloption:** `pg_sys::add_reloption_kind()` (once at load) + `pg_sys::add_int_reloption(kind, "lists", …,
  default, min, max, AccessExclusiveLock)`; the `amoptions` callback `unsafe extern "C-unwind" fn(Datum, bool) ->
  *mut pg_sys::bytea` calling `pg_sys::build_reloptions(reloptions, validate, kind, size, tab, ntab)` over a
  `relopt_parse_elt` table; a `#[repr(C)]` options struct (`vl_len_: i32` + `lists: i32`) read from
  `relation.rd_options` at build (null → default).
- **Scan GUC:** `pgrx::GucRegistry::define_int_guc("theodb_ivfflat.probes", …, &GucSetting<i32>, min, max,
  GucContext::Userset, GucFlags::default())`; read at scan via `SETTING.get()`.

## Coverage Corner 4 — Techniques

**T1 — reloption for a BUILD param, GUC for a QUERY param (the pgvector split).** `lists` is fixed at build time
(the index is partitioned once) → a reloption (`WITH (lists=N)`); `probes` is a per-query recall/speed knob → a GUC
(`SET …`). This is exactly pgvector's `ivfflat.c` design (reloption `lists` + `DefineCustomIntVariable
"ivfflat.probes"`) and how M34 mirrors it (`.claude/knowledge-base/references/pgvector/src/ivfflat.c`,
`.claude/knowledge-base/references/pgvector/src/ivfflat.h`).

**T2 — the pgrx amoptions pattern (copy source).** `pgvectorscale`'s
`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/options.rs` shows the whole surface
in the SAME pgrx 0.16.1: the `#[repr(C)]` options struct, `from_relation` (rd_options null-check → defaults), the
`amoptions` callback with `build_reloptions` + a `relopt_parse_elt` table, and `init()` doing `add_reloption_kind`
+ `add_int_reloption`. Its GUC pattern is
`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/guc.rs`
(`GucRegistry::define_int_guc`). theodb's amhandler already mirrors pgvectorscale (`theodb_rs/src/am/mod.rs:9`), so
this is a direct copy + adapt.

**T3 — default preserves behavior (no regression).** When no `WITH (lists=)` is given, `rd_options` is null → the
struct falls back to `lists=100`; when the GUC is unset it defaults to `probes=10`. So every existing M26/M31 test
(built without options) behaves identically. This is the load-bearing safety property.

**T4 — edge validation (fail-fast).** lists/probes below min or above max → the reloption `add_int_reloption`
min/max bounds reject at DDL time (typed error, not a crash); the scan clamps `probes` to the actual list count
(existing `SCAN_PROBES.clamp(1, nlists)` shape) so an over-large probes is a no-op, not an OOB.

## Cross-cutting Comparison

| | theodb today | pgvector | theodb M34 |
|---|---|---|---|
| lists | fixed 100 (const) | `WITH (lists=N)` reloption | `WITH (lists=N)` reloption (default 100) |
| probes | fixed 10 (const) | `SET ivfflat.probes` GUC | `SET theodb_ivfflat.probes` GUC (default 10) |
| amoptions | `None` | `ivfflatoptions` | wired via pgrx `build_reloptions` |

## ADRs

### D1 — reloption for `lists` (build) + GUC for `probes` (scan)
Mirror pgvector/pgvectorscale exactly: build param → reloption, query param → GUC. Rejected: a single mechanism for
both — `probes` must be tunable per-session without rebuilding, so it cannot be a build reloption; `lists` is
baked into the partition at build, so it cannot be a runtime GUC.

### D2 — default MUST preserve current behavior
lists=100 / probes=10 when unset — the M26/M31 gates built without options stay green. Rejected: changing the
default to lists=1000 now — would silently alter every existing index's build + could regress recall on small N.

### D3 — the ivfflat build at large `lists` is single-thread (time, not correctness)
theodb's k-means build is single-thread scalar (M32 finding); a large `lists` at 1M costs build TIME. Honest, not a
defect. Rejected: parallel k-means now — out of M34 scope (a future lever).

## Recommendations

1. `theodb_rs/src/am/options.rs` (NEW): `#[repr(C)] TheodbIvfflatOptions{vl_len_, lists}` + `amoptions` callback +
   `init()` (add_reloption_kind + add_int_reloption) + `from_relation`.
2. `theodb_rs/src/am/guc.rs` (NEW): `PROBES: GucSetting<i32>` + `init()` (`define_int_guc "theodb_ivfflat.probes"`).
3. `mod.rs`: `amroutine.amoptions = Some(options::amoptions)`; call `options::init()` + `guc::init()` in `_PG_init`.
4. `build.rs`: read `lists` from the relation's options (fallback 100) → pass to `IvfflatIndex::build`.
5. `scan.rs`/`index.rs`: read `probes` from the GUC (fallback 10) instead of `SCAN_PROBES`.
6. Re-run the M32 harness at 1M with `WITH (lists=1000)` + tuned probes → assert theodb_ivfflat p50 ≤ pgvector;
   record in `docs/benchmarks/m34-ivfflat-reloption.{md,json}`.
