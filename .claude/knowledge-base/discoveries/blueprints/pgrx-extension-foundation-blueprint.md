# Blueprint: pgrx extension foundation — TheoDB's own Rust extension + theodb.embed (parity + benchmark)

> **Version 1.0** — Synthesizes how to build TheoDB's **own** PostgreSQL extension in Rust (**pgrx**) and
> rewrite `theodb.embed` (today plpython3u) in Rust with **proven parity + a benchmark** — the ROADMAP-v2 /
> ADR 0006 foundation (M17). Reference: **pgvectorscale** (a real pgrx extension, cloned). Honesty (ADR
> 0006/0002): measurement-first — the benchmark is a gate; parity is proven by the existing tests; no perf
> claim without evidence.

**Slug:** `pgrx-extension-foundation`
**Source plan:** `.claude/knowledge-base/discoveries/plans/pgrx-extension-foundation-plan.md`
**Owner:** TheoDB maintainers
**Generated:** 2026-06-29 via `/discover-execute` (inline, citations verified)
**Confidence verdict:** SHIPPABLE_WITH_CAVEATS (89.0 — 0 fabricated citations, 4/4 corners; sole soft cap `soft_floor_citation_density_low`, heuristic, accepted)

## Context

ADR 0006 pivots TheoDB to own code in Rust (pgrx). M17 stands up the own extension and rewrites the simplest
surface (`theodb.embed`, `sql/30-theodb-embed.sql` — plpython3u + urllib) in Rust, proving the
"plpython3u → own Rust extension" pattern with parity + a latency benchmark. Honors `.claude/rules/parsimony-ladder.md`
+ Rule 9 (minimal deps — one audited HTTP crate, not reinventing HTTP), `.claude/rules/testing.md` (parity =
existing tests), `.claude/rules/public-copy.md` (no perf claim), ADR 0002 (measurement-first).

## Objective

Let M17 implement: the pgrx project + Docker build, `theodb.embed` in Rust with a minimal audited HTTP crate +
SSRF/typed-error parity, coexistence with the SQL-only extension, and a reproducible latency benchmark + parity.

---

## Coverage Corner 1 — Integration Tests

### pgvectorscale — how a pgrx extension is tested
- pgrx ships an in-crate test harness: `#[cfg(test)] pub mod pg_test { pub fn setup(...) ... }` at the crate
  root (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/lib.rs:30-40`) + `#[pg_test]`
  functions (present in `src/lib.rs`, `src/util/chain.rs`, `src/access_method/vacuum.rs`). `cargo pgrx test`
  spins a temp PG and runs them in-process.
- **Transfer to M17:** the **authoritative parity gate is the existing Python e2e** `benchmarks/tests/test_embed_sql.py`
  (`:79-82` asserts `len(vec)==384` + not-all-zeros; `:85+` semantic) run against the rebuilt image — same
  contract, now served by the Rust `theodb.embed`. pgrx `#[pg_test]` adds Rust-side unit checks (SSRF reject,
  error mapping) but the cross-language parity proof is the Python suite (no rewrite of the oracle).

---

## Coverage Corner 2 — Dependencies

| Dep | Version | License (D1) | Note | Citation |
|---|---|---|---|---|
| `pgrx` | `=0.16.1` | Apache/MIT | **matches our image** — no toolchain drift | `pgvectorscale/pgvectorscale/Cargo.toml:31`; repo `Dockerfile:14` (`PGRX_VERSION=0.16.1`) |
| HTTP crate: **`minreq`** | latest | **ISC** (permissive — D1 OK) | "simple, minimal-dependency HTTP client"; ~148 KB stripped; HTTPS via `https-rustls` feature; JSON feature | web `github.com/neonmoe/minreq` (README) |

- **minreq vs ureq:** minreq is the more minimal (fewer deps, ISC); `ureq` (MIT/Apache) is the fallback if a
  needed API (e.g. explicit no-redirect) is missing. The README does **not** document POST-body / header /
  timeout / redirect-control explicitly → **M17 must confirm the exact API** (POST + header + timeout + no
  redirect) against the crate source/docs (github) before locking it; if minreq lacks no-redirect control,
  switch to `ureq` (which exposes redirect config). Honest open item, recorded.
- `/deps-audit` runs on the chosen crate (CVE) before merge. All Rust deps come via the pinned `Cargo.lock`
  (reproducible, like pgvectorscale's pinned build — `Dockerfile:24-25`).

---

## Coverage Corner 3 — Tools

### Build/install (reuse the scale-builder pattern)
- pgrx build recipe (from pgvectorscale `Makefile`): `cargo install cargo-pgrx --version <V>` (`Makefile:55-56`),
  `cargo pgrx init --pg<N>=<pg_config>` (`:59-62`), `cargo pgrx install --release --features pg<N>` (`:69-71`).
- Our image already does exactly this for pgvectorscale (`Dockerfile:10-28` scale-builder stage: apt
  `build-essential postgresql-server-dev-17 libssl-dev pkg-config clang`; rustup; `cargo install --locked
  cargo-pgrx 0.16.1`; `cargo pgrx init --pg17`; `cargo pgrx install --release`). → **M17 adds a second builder
  stage** (`theodb-rs-builder`) mirroring it for our crate, then COPYs the `.so` + `.control` + `--N.sql` into
  the runtime (same as the vectorscale artifact COPY).
- **Disk cost (honest, Q6):** `cargo pgrx init` compiles a PostgreSQL (~GB). Resolved: disk freed to ~34 GB
  (was 7 GB) by pruning intermediate images + build cache; build is image-side (reproducible), not
  local-toolchain-dependent.

---

## Coverage Corner 4 — Techniques

### T1 — pgrx project shape + coexistence (Q1)
- **Crate skeleton** (from `pgvectorscale/src/lib.rs:1-27`): `use pgrx::prelude::*;` + `pgrx::pg_module_magic!();`
  + `#[pg_guard] _PG_init()`. Functions exposed with `#[pg_extern]` (e.g.
  `#[pg_extern(immutable, parallel_safe, create_or_replace)] pub fn smallint_array_overlap(...) -> bool`
  — `src/access_method/mod.rs:284-285`; `distance/mod.rs:52-53`). The `.control` + install `--<version>.sql`
  are generated by `cargo pgrx schema` / `install`.
- **Schema:** put the function in the `theodb` schema via pgrx `#[pg_schema] mod theodb { #[pg_extern] fn embed(...) }`
  (or a schema-qualified control). Signature target: `theodb.embed(content text, model text DEFAULT NULL) RETURNS vector`.
- **Coexistence (EC-2) — the decision:** ship the Rust function as a **separate extension `theodb_rs`** during
  the incremental rewrite, and **remove `theodb.embed` from `sql/30-theodb-embed.sql`** (so the SQL `theodb`
  no longer defines it). Both extensions install on the image; `theodb.embed` is now served by `theodb_rs`
  (Rust). No duplicate-definition clash. At the end of the rewrite (M19) the surfaces consolidate. The `vector`
  return type comes from pgvector (still `requires`/present).

### T2 — HTTP in Rust, minimal + SSRF + typed errors (Q2)
- Replace plpython3u `urllib` (`sql/30-theodb-embed.sql:16-46`) with **minreq** (ISC):
  `minreq::post(endpoint).with_header("Content-Type","application/json").with_body(json).with_timeout(secs).send()`.
  Confirm POST/header/timeout/no-redirect against the crate source in M17 (open item); fall back to `ureq` if
  no-redirect control is absent.
- **SSRF parity** (preserve `sql/30:37-39`): reject non-`http(s)://` endpoints → `ereport`/`PgSqlErrorCode`
  mapped to **SQLSTATE 22023** (the plpython3u `plpy.error(..., sqlstate="22023")` equivalent). No redirects
  (avoid SSRF via redirect). Same GUCs (`theodb.embedding_endpoint/model/api_key` via `pgrx` GUC or
  `current_setting`).

### T3 — Benchmark + functional parity (Q3)
- **Functional parity (the gate):** `benchmarks/tests/test_embed_sql.py` (unchanged) run against the rebuilt
  image — the Rust `theodb.embed` must produce a 384-dim non-zero vector (`:79-82`), be semantically
  meaningful (`:85+`), and raise the same typed errors (endpoint unset / non-http → 22023). Same oracle, new
  implementation.
- **Latency benchmark (measurement-first, the CTO requirement):** a reproducible micro-bench — N (e.g. 200)
  `SELECT theodb.embed('…')` calls against the **same deterministic embedding stub** (`tools/embedding_server.py`,
  used by the test via `host.docker.internal`), reporting **mean ± std dev** for Rust vs plpython3u, run ≥ 3
  times. Output to `docs/benchmarks/m17-embed-rust-vs-plpython.md`. **No perf claim in prose** beyond the
  measured numbers + methodology (ADR 0002 / public-copy). Expectation honest: embed is **I/O-bound** (the
  stub/LLM dominates), so the bench documents that the rewrite does not regress latency — not a speed win.

---

## Cross-cutting Comparison

| Dimension | pgvectorscale (pgrx ref) | TheoDB M17 (own) |
|---|---|---|
| Crate | `pgrx =0.16.1`, pgNN features | same pgrx version (no drift) |
| Expose fn | `#[pg_extern]` + `#[pg_guard] _PG_init` | `#[pg_schema] mod theodb { #[pg_extern] fn embed }` |
| HTTP | none (index crate) | **minreq (ISC)** — new, minimal |
| Build | scale-builder stage (cargo pgrx init/install) | second builder stage, same pattern |
| Test | `#[pg_test]` + suite | `#[pg_test]` (SSRF/errors) + Python parity gate (`test_embed_sql.py`) |

## ADRs

### D1 — Separate `theodb_rs` extension during the incremental rewrite (coexistence)
**Decision:** the Rust function ships as a separate extension `theodb_rs`; `theodb.embed` is removed from
`sql/30` (SQL `theodb` no longer defines it); both install on the image; consolidate at M19.
**Rationale:** two extensions cannot both define `theodb.embed` (clash); a separate ext + removing the SQL
definition keeps the rewrite incremental and clash-free (EC-2). The user still calls `theodb.embed`.
**Alternatives:** migrate the whole `theodb` ext to pgrx now (rejected — big-bang, against incremental);
keep both definitions (rejected — duplicate-definition error).
**Consequences:** one transition extension; clean per-feature migration; consolidation tracked for M19.

### D2 — HTTP via `minreq` (ISC), confirm API in M17; `ureq` fallback
**Decision:** use `minreq` (ISC, minimal) for the embed POST; confirm POST/header/timeout/**no-redirect** API
against the crate source during M17; fall back to `ureq` (MIT/Apache) if no-redirect control is missing.
**Rationale:** minimal audited crate (Rule 9 — don't reinvent HTTP; don't pull a heavy `reqwest`); ISC is D1-OK.
**Alternatives:** `reqwest` (rejected — heavy dep tree, async runtime); hand-rolled TCP/TLS (rejected — reinvent
the wheel + security risk).
**Consequences:** small dep; the no-redirect confirmation is an explicit M17 open item (SSRF parity depends on it).

### D3 — Build via a second Docker builder stage (mirror scale-builder)
**Decision:** add a `theodb-rs-builder` stage mirroring `scale-builder`; COPY the `.so`/`.control`/`.sql` into
runtime.
**Rationale:** the pattern already works for pgvectorscale (`Dockerfile:10-28`); reproducible, image-side;
disk freed to ~34 GB.
**Alternatives:** local toolchain build (rejected — non-reproducible, env-dependent).
**Consequences:** longer image build (compiles our crate); pinned `Cargo.lock` for reproducibility.

### D4 — Benchmark is measurement-first, parity via existing tests, no perf claim
**Decision:** prove functional parity with `test_embed_sql.py` (unchanged); measure latency Rust vs plpython3u
(N runs, mean±std, same stub) → `docs/benchmarks/`; assert no regression, claim nothing beyond the numbers.
**Rationale:** ADR 0002 / public-copy; embed is I/O-bound so the honest result is "no regression", not a win.
**Alternatives:** skip the benchmark (rejected — CTO requires data); claim a speedup (rejected — unbenchmarked/
false; I/O-bound).
**Consequences:** the benchmark is a gate + honest evidence.

## Recommendations for the project (M17)

| # | Recommendation | Linked to | Priority |
|---|---|---|---|
| 1 | Create the `theodb_rs` pgrx crate (`pgrx =0.16.1`, pg17), `#[pg_schema] mod theodb` + `#[pg_extern] fn embed`, `.control` generated | Q1, D1 | HIGH |
| 2 | Implement `embed` HTTP via minreq (POST/header/timeout/no-redirect — confirm API; ureq fallback) + SSRF reject + 22023 typed errors (parity with sql/30) | Q2, D2 | HIGH |
| 3 | Remove `theodb.embed` from `sql/30-theodb-embed.sql`; add a `theodb-rs-builder` Docker stage + COPY artifacts; `CREATE EXTENSION theodb_rs` at init | Q1, Q6, D1, D3 | HIGH |
| 4 | Parity: `benchmarks/tests/test_embed_sql.py` green against the rebuilt image (Rust embed) + a pgrx `#[pg_test]` for SSRF/error | Q3, Q4, D4, testing.md | HIGH |
| 5 | Latency benchmark Rust vs plpython3u (N runs, mean±std, same stub) → `docs/benchmarks/m17-embed-rust-vs-plpython.md`; no perf claim | Q3, D4, public-copy.md | HIGH |
| 6 | `/deps-audit` on minreq/ureq (CVE) + record license (ISC/MIT) in the deps gate | Q5 | MEDIUM |

## Blocked questions (if any)

(none — all 6 answered. Open item recorded honestly: the exact minreq POST/no-redirect API must be confirmed
in M17 from the crate source, with ureq as the fallback — this is an implementation detail, not a blocker.)

## Halt-loop progress (audit trail)

- Execution mode: inline (operator-driven).
- Questions answered: 6/6 · Blocked: 0 · Coverage corners: 4/4.
- Citations: pgvectorscale (`src/lib.rs`, `src/access_method/mod.rs`, `distance/mod.rs`, `Cargo.toml`, `Makefile`) verified on disk; minreq via `github.com` (allowlisted); repo `Dockerfile`/`sql/30`/`test_embed_sql.py` cited.

## Related

- Discovery plan: `.claude/knowledge-base/discoveries/plans/pgrx-extension-foundation-plan.md`
- Edge-case review: `.claude/knowledge-base/reviews/pgrx-extension-foundation-edge-cases-2026-06-29.md`
- Strategy anchor: `docs/adr/0006-own-code-postgres-based-rust-go.md`; `ROADMAP-v2.md` (M17)
- Project rules: `.claude/rules/architecture.md`, `.claude/rules/testing.md`, `.claude/rules/parsimony-ladder.md`, `.claude/rules/public-copy.md`
