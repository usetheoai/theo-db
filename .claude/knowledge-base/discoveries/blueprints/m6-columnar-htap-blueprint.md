# Blueprint: Columnar / HTAP analytics for PostgreSQL (pg_mooncake, permissive)

> **Version 1.0** — Synthesizes the permissive columnar/HTAP path for TheoDB M6: how `pg_mooncake` (MIT, a
> DuckDB-powered Apache Iceberg columnstore *mirror*) delivers fast analytics over live transactional Postgres
> tables, how the planner routes row-vs-columnar (`Custom Scan (DuckDBScan)` vs heap `Seq Scan`), the PG-version
> support (risk #1 — resolved: pg14–18, sourced verbatim from the cloned `Makefile`/`README`), the
> build/adoption cost (heavy Rust+pgrx+DuckDB build; no pg17 prebuilt `.so` exists), and the honest D2 framing
> (a DuckDB+Iceberg **lakehouse on disk**, NOT the **in-memory** columnar engine of AlloyDB). Investigated:
> the cloned `pg_mooncake` (README/Makefile/Dockerfile/control/LICENSE) and `duckdb` references, plus allowlisted
> `github.com`/`duckdb.org`/`cloud.google.com`. It informs the M6 deliverable: the columnar capability + the
> row-vs-columnar plan evidence + a **measurement-first adoption gate** (cf. the M7-S2 BM25 precedent).

**Slug:** `m6-columnar-htap`
**Source plan:** `.claude/knowledge-base/discoveries/plans/m6-columnar-htap-plan.md`
**Owner:** paulohenriquevn
**Generated:** 2026-06-28 via `/discover-execute`
**Confidence verdict:** SHIPPABLE_WITH_CAVEATS (placeholder — updated by `/discover-confidence`)

## Context

ROADMAP `### M6` (Analytics colunar / HTAP) wants a "Camada de armazenamento colunar (DuckDB-powered,
`pg_mooncake` MIT) para analytics rápido sobre dados transacionais vivos, com escolha de plano row vs
colunar." The DoD has three parts: (1) `pg_mooncake` enabled for selected tables with an analytical query
**measured** vs row-store; (2) the row-vs-columnar plan choice documented; (3) honesty — it is a columnar
**lakehouse** (DuckDB+Iceberg), NOT in-memory like AlloyDB (PRD D2). The north-star ADR
`docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` already frames the columnar pillar as a *different,
competitive* permissive bet forced by D1 (AlloyDB's in-memory columnar peers — Citus columnar / Hydra — are
AGPL-barred from the distribution). Risk #1 was whether `pg_mooncake` supports the PG major TheoDB ships (17);
this discovery resolves it from the repo, establishes the plan-choice evidence, the build/adoption cost, and the
honest scope, under measurement-first discipline (measure before embedding a heavy dependency).

## Objective

Let the reader decide the M6 deliverable: ship the pg_mooncake columnar/HTAP capability (capability + DuckDBScan-vs-SeqScan
plan evidence + a measured analytical query vs row-store), gate the heavy build's embedding into `theo-db:dev`
(PG17) on that measurement, and state the honest lakehouse-not-in-memory delta (D2).

---

## Coverage Corner 1 — Integration Tests

> The "integration test" for a columnstore mirror is: prove the mirror **stays in sync** with the row table and
> returns the **same answer** — then prove the planner actually routes the analytic query to the columnstore.

### pg_mooncake — columnstore-mirror correctness / freshness (Q1)

The canonical correctness pattern is the README quickstart: create a row table, create a mirror, insert into the
**row** table, then query the **mirror** and confirm it reflects up-to-date state.

- **Pattern — create the mirror**: a stored procedure binds a columnstore mirror to an existing heap table
  (`CALL mooncake.create_table('trades_iceberg', 'trades');` — `pg_mooncake/README.md:77`). The README states the
  mirror "stays in sync with `trades`" (`pg_mooncake/README.md:75`) with "sub-second freshness"
  (`pg_mooncake/README.md:13`).
- **Fixtures — the row table + rows**: a plain heap `trades(id, symbol, time, price)` table
  (`pg_mooncake/README.md:67-73`), then 4 rows inserted into the **row** table only
  (`pg_mooncake/README.md:82-87`).
- **Correctness assertion**: query the **mirror**, not the row table —
  `SELECT avg(price) FROM trades_iceberg WHERE symbol = 'AMZN';` (`pg_mooncake/README.md:91`). The README frames
  this as "query `trades_iceberg` to see that it reflects the up-to-date state of `trades`"
  (`pg_mooncake/README.md:89`). **Live functional evidence (observed on the canonical PG18 distribution):** this
  returns `208.5` — the arithmetic mean of the two AMZN rows (207, 210) inserted at
  `pg_mooncake/README.md:84,86` — i.e. the mirror answer equals the row-store answer. This is the freshness +
  correctness proof: an insert into the row table is visible through the columnstore mirror with the correct
  aggregate.
- **Coverage / what the project's own suite asserts**: the repo runs regression tests via
  `cargo pgrx regress --resetdb` (`pg_mooncake/Makefile:44-45`) — pgrx's `.sql`/`.out` golden-file harness, the
  same mechanism the underlying engine (DuckDB) favors for SQL-level coverage (sqllogictest-style golden files —
  `duckdb/AGENTS.md` "Test File Format"). **TheoDB integration test to author (the eval shape):** insert/update/
  delete on the row table → assert the mirror's aggregate matches the row-store's aggregate (correctness) and
  becomes visible within the freshness window (freshness, `UNBENCHMARKED` until TheoDB measures the lag — E3).

Code example (cited):

```sql
-- .claude/knowledge-base/references/pg_mooncake/README.md:75-91
CALL mooncake.create_table('trades_iceberg', 'trades');   -- mirror, stays in sync
INSERT INTO trades VALUES (2, 'AMZN', '2024-06-05 10:05:00', 207), (4, 'AMZN', '2024-06-05 10:15:00', 210);
SELECT avg(price) FROM trades_iceberg WHERE symbol = 'AMZN';   -- → 208.5 (== row-store)
```

---

## Coverage Corner 2 — Dependencies

> Risk #1 lives here: does the columnar stack support the PG major TheoDB ships (17)? **Resolved: yes (pg14–18),
> sourced verbatim from the cloned repo — never memory (D1 / Rule 3).**

### pg_mooncake + its dependency chain (Q2)

| Dependency | License | PG17? | Requires | Citation |
|---|---|---|---|---|
| `pg_mooncake` | **MIT** | **Yes — pg14–18** | `pg_duckdb` | License: `pg_mooncake/LICENSE:1`; PG matrix: `pg_mooncake/Makefile:20` ("`pg14, pg15, pg16, pg17, or pg18 (default)`") + `pg_mooncake/README.md:42` ("Postgres versions 14-18"); requires: `pg_mooncake/pg_mooncake.control:5` (`requires = 'pg_duckdb'`) |
| `pg_duckdb` | MIT | Yes — pg14–18 | DuckDB (vendored) | Supports "PostgreSQL: 14, 15, 16, 17, 18" — `github.com/duckdb/pg_duckdb` README (allowlisted) |
| `duckdb` (engine) | MIT | n/a (embedded) | — | The columnar-vectorized execution engine — `duckdb/README.md:16-18` |
| `moonlink` | (Mooncake) | n/a | — | streaming/batched CDC into the mirror — `pg_mooncake/README.md:14` |

- **License gate (D1 / CLAUDE.md rule 2):** the whole load-bearing stack is **MIT** — `pg_mooncake`
  (`pg_mooncake/LICENSE:1`, "Copyright (c) 2024-2025 Mooncake Labs"), `pg_duckdb` (MIT,
  `github.com/duckdb/pg_duckdb`), and DuckDB (MIT). No AGPL anywhere in the chain → it **passes** the
  distribution license gate, unlike the in-memory columnar peers (Citus columnar / Hydra) excluded by D1.
- **Risk #1 — RESOLVED:** the cloned `Makefile` help text lists `pg17` explicitly in the supported
  `PG_VERSION` values (`pg_mooncake/Makefile:20`), corroborated by the live README "Postgres versions 14-18"
  (verified via `raw.githubusercontent.com/Mooncake-Labs/pg_mooncake/main/README.md`, allowlisted) and by
  `pg_duckdb`'s own PG14–18 support. **Not asserted from memory.**
- **`superuser = true`** (`pg_mooncake/pg_mooncake.control:6`) and `relocatable = false`
  (`pg_mooncake/pg_mooncake.control:3`) — install/enable is a superuser operation; note for the TheoDB
  provisioning path (a tenant cannot self-install it).

---

## Coverage Corner 3 — Tools

> The adoption decision turns on **build cost**. The capability is provable cheaply on the official PG18 image,
> but **embedding it into the shipped `theo-db:dev` (PG17) requires a source build** — there is no pg17 prebuilt
> `.so`. This is the gate.

### Build / adoption cost (Q3)

| Option | Cost | Ships in `theo-db:dev` (PG17)? | Citation |
|---|---|---|---|
| **A. Official Docker image (PG18)** | Zero build — `docker run … mooncakelabs/pg_mooncake` | No (it is PG18; TheoDB ships PG17) — **use for the capability/measurement** | `pg_mooncake/README.md:24-26` |
| **B. Source build on `pgduckdb:17-main` base (PG17)** | **Heavy**: Rust toolchain + `cargo-pgrx@0.16.1` + `cargo pgrx init` + build DuckDB + `make package` | **Yes — but gated on the measurement (D3)** | Dockerfile build stages: `pg_mooncake/Dockerfile:1-25`; runtime base: `pg_mooncake/Dockerfile:27`; pg17 base tag exists: `github.com/duckdb/pg_duckdb/blob/main/docker/README.md` |
| **C. Prebuilt `.so` for PG17** | n/a — **does not exist** | No | Releases v0.1.0–v0.1.2 ship only `*.tar.gz`/`*.zip` source, no `pg17` artifact — `api.github.com/repos/Mooncake-Labs/pg_mooncake/releases` |

- **The build is heavy (E2).** The official Dockerfile is a two-stage build: stage 1 `FROM postgres:18`
  installs `curl/gcc/make/pkg-config/postgresql-server-dev-18`, installs Rust via rustup, then
  `cargo install --locked cargo-pgrx@0.16.1 && cargo pgrx init --pg18=$(which pg_config)` and `make package`
  (`pg_mooncake/Dockerfile:1-25`); stage 2 is `FROM pgduckdb/pgduckdb:18-main` with the compiled artifact
  copied in (`pg_mooncake/Dockerfile:27-29`). The runtime then sets
  `shared_preload_libraries = 'pg_duckdb,pg_mooncake'`, `duckdb.allow_community_extensions = true`, and
  `wal_level = logical` (`pg_mooncake/Dockerfile:33-37`, mirrored by `pg_mooncake/README.md:51-54`). For a **PG17**
  TheoDB build, substitute `pg18`→`pg17` and the base `pgduckdb/pgduckdb:18-main`→`:17-main` (the pg17 base tag
  exists — `github.com/duckdb/pg_duckdb/blob/main/docker/README.md`; `make install PG_VERSION=pg17` is the
  documented invocation — `pg_mooncake/Makefile:20`, `pg_mooncake/README.md:46`).
- **No prebuilt PG17 binary → a source build is unavoidable for the shipped image.** The GitHub releases attach
  only source archives (`pg_mooncake-0.1.{0,1,2}.tar.gz`/`.zip` —
  `api.github.com/repos/Mooncake-Labs/pg_mooncake/releases`); none target pg17. This is the cost that justifies
  the **measurement-first gate**: prove the win on the zero-build PG18 image first; only then pay the PG17
  build cost (D3, E2).
- **`wal_level = logical` is a prerequisite** (`pg_mooncake/README.md:54`) — the mirror is fed by logical
  replication / CDC (`moonlink`, `pg_mooncake/README.md:14`); note the operational implication for the TheoDB
  HA/replication pillar (M4) — logical decoding must be enabled.

---

## Coverage Corner 4 — Techniques

### Technique 1 — Row-vs-columnar plan choice: `Custom Scan (DuckDBScan)` vs `Seq Scan` (Q4 / DoD-2)

This is the DoD-2 mechanism: how the planner routes an analytic query to the columnstore vs the heap.

| Path | EXPLAIN shape | Engine | Citation |
|---|---|---|---|
| Query the **columnstore mirror** | `Custom Scan (DuckDBScan)` + an embedded "DuckDB Execution Plan:" operator tree | DuckDB (vectorized) | `github.com/duckdb/pg_duckdb` (Discussion #640 — DuckDBScan custom-scan + DuckDB plan) |
| Query the **row table** | `Aggregate -> Seq Scan on trades` | Postgres heap executor | observed live (below) + PostgreSQL custom-scan model |

- **Mechanism.** `pg_duckdb` installs a Postgres **planner hook** that wraps a DuckDB-executed query in a single
  `Custom Scan (DuckDBScan)` node; under it Postgres prints DuckDB's own operator tree as a "DuckDB Execution
  Plan:" block, with `cost=0.00..0.00 rows=0 width=0` because planning happens inside DuckDB, not Postgres's
  cost optimizer (`github.com/duckdb/pg_duckdb`, Discussion #640). `EXPLAIN ANALYZE` additionally embeds DuckDB's
  profiling + a "Total Time" figure (same source).
- **Live plan-choice evidence (observed on the canonical pg_mooncake distribution, PG18):**
  - `EXPLAIN SELECT avg(price) FROM trades_iceberg WHERE symbol='AMZN';` → **`Custom Scan (DuckDBScan)`** + a
    DuckDB Execution Plan (columnstore → DuckDB vectorized path).
  - `EXPLAIN SELECT avg(price) FROM trades WHERE symbol='AMZN';` → **`Aggregate -> Seq Scan on trades`** (row
    table → Postgres heap path).
  - This is the DoD-2 evidence: **the columnstore mirror is planned through DuckDB; the row table through the
    heap.** The branch is the *table you query* (the mirror vs the heap), not a runtime cost flip on one table.
- **Honest contrast with AlloyDB's routing (R1):** AlloyDB chooses row-vs-column per plan node via a **costing
  model** on the *same* logical table — "the AlloyDB query planner uses a costing model to automatically choose
  the best mode of execution for each node" and may pick the row store for small tables (<~5,000 rows)
  (`cloud.google.com/blog/products/databases/alloydb-for-postgresql-columnar-engine`). pg_mooncake's choice is
  coarser and explicit (you address the mirror by name) — a real architectural delta to disclose (E5).

### Technique 2 — Why columnar wins on scan-heavy aggregates + the measurement method (Q5 / DoD-1)

- **The rationale (duckdb.org, R3 source).** DuckDB uses a "columnar-vectorized query execution engine, where
  queries are still interpreted, but a large batch of values (a 'vector') are processed in one operation"
  (`duckdb.org/why_duckdb`). It is built for "analytical query workloads, also known as online analytical
  processing (OLAP)" — "complex … queries that process significant portions of the stored dataset"
  (`duckdb.org/why_duckdb`). Batch/vectorized processing lowers per-value CPU overhead and improves cache
  utilization — exactly the win shape for `avg/group-by/filter` over a wide scan, the DoD-1 workload.
- **The workload to measure (the eval).** A scan-heavy aggregate with a selective filter over a wide-ish table
  (e.g. `SELECT symbol, avg(price), count(*) FROM trades_iceberg WHERE time >= … GROUP BY symbol`), run against
  the **mirror** vs the same query against the **row table**, at a realistic row count (the 4-row README example
  proves correctness, not performance). pg_mooncake's own marketing cites a top-10 ClickBench ranking
  (`pg_mooncake/README.md:15`) — ClickBench is the relevant scan-heavy analytic benchmark family.
- **Measurement method (R3 / Rule 5):** ≥ 3 runs warm-cache, report median wall-clock (and `EXPLAIN ANALYZE`
  DuckDB "Total Time" for the columnstore path), same hardware, same data, columnstore vs row-store. **Status:
  `UNBENCHMARKED`** — no TheoDB number exists yet; the README's "ClickBench top 10" claim
  (`pg_mooncake/README.md:15`) is the *vendor's* claim, not a TheoDB reproduction, and per `public-copy.md` §4
  cannot be restated as a TheoDB performance claim until reproduced under `docs/benchmarks/`.

### Technique 3 — AlloyDB in-memory columnar SOTA vs TheoDB lakehouse — the honest D2 delta (Q6)

| Dimension | AlloyDB columnar (SOTA) | TheoDB via pg_mooncake | Honest delta (D2) |
|---|---|---|---|
| Storage locus | **In-memory** column store ("keeps frequently queried data in an in-memory columnar format") | **On-disk lakehouse** — DuckDB over Apache **Iceberg** files (`pg_mooncake/README.md:13,17`) | Different bet: RAM-resident vs disk lakehouse — NOT a copy |
| Engine | "modern, vectorized query processing engine … optimal use of system caches and vector processing" | DuckDB columnar-vectorized engine (`duckdb/README.md:16-18`, `duckdb.org/why_duckdb`) | Comparable *vectorized* approach; different data residency |
| Population | ML auto-columnarization; auto-refresh | Explicit `mooncake.create_table(mirror, base)` + CDC sync (`pg_mooncake/README.md:14,77`) | AlloyDB automates selection; pg_mooncake is explicit per-table |
| Plan choice | per-node costing model; row store if <~5k rows | address the mirror by name (DuckDBScan) vs heap (SeqScan) | coarser, explicit routing |
| License | proprietary (managed) / AGPL-class OSS peers barred | **MIT** end-to-end | TheoDB wins on openness/portability today |
| Interop | engine-internal | **Iceberg-native**, "readily accessible by other query engines" (`pg_mooncake/README.md:17`) | TheoDB lakehouse is openly queryable; AlloyDB's store is not |

*(All AlloyDB quotes: `cloud.google.com/blog/products/databases/alloydb-for-postgresql-columnar-engine`, allowlisted.
The deeper `docs.cloud.google.com/alloydb/docs/columnar-engine/about` page redirects **off-allowlist** → its
docs-only phrasings are marked `UNVERIFIED`; the in-memory + costing-model framing above is sourced from the
allowlisted blog and corroborated by `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`.)*

**The honest statement (D2 / CLAUDE.md rule 7):** TheoDB's columnar pillar is a DuckDB+Iceberg **lakehouse on
disk**, NOT AlloyDB's **in-memory** columnar engine. It is a *competitive-different* permissive bet (forced by
D1's AGPL bar), winning today on openness/cost/portability/Iceberg-interop — not a claim of in-memory parity.

---

## Cross-cutting Comparison

| Dimension | pg_mooncake columnstore mirror | Postgres row-store (heap) | AlloyDB in-memory columnar (SOTA) |
|---|---|---|---|
| Integration-test style | row-write → mirror-read equality + freshness (`pg_mooncake/README.md:89-91`) | native heap correctness | EXPLAIN verifies columnar usage (blog) |
| Primary deps / license | MIT; requires `pg_duckdb` + DuckDB (`pg_mooncake.control:5`, `LICENSE:1`) | core PG | proprietary / managed |
| Local dev story | zero-build PG18 image (`README.md:24-26`); PG17 = source build | built-in | managed service / Omni |
| Storage locus | **on-disk Iceberg lakehouse** (`README.md:13,17`) | on-disk heap (row) | **in-memory** column store (blog) |
| Plan signature | `Custom Scan (DuckDBScan)` (pg_duckdb Disc. #640) | `Seq Scan` | per-node costing-model choice (blog) |
| Freshness | sub-second, CDC-fed (`README.md:13-14`) — `UNBENCHMARKED` | immediate (same table) | auto-refresh (blog) |

## ADRs

### D1 — Ship the pg_mooncake columnar/HTAP capability; gate the shipped-image embedding on measurement (measurement-first)

**Decision:** Make M6 deliver (a) the **capability**: pg_mooncake columnstore mirror enabled for a selected
table, the DuckDBScan-vs-SeqScan plan evidence captured, and **one analytical query measured** mirror-vs-row;
(b) a **measurement-first adoption gate** before embedding the heavy `pg_duckdb`+DuckDB+pgrx build into the
shipped `theo-db:dev` (PG17). Prove the win on the zero-build official PG18 image first; only on a positive,
reproduced number do we pay the PG17 source-build cost.

**Rationale:** PG17 support is real (pg14–18 — `pg_mooncake/Makefile:20`, `README.md:42`, `pg_duckdb` PG14–18)
and the PG17 base exists (`pgduckdb/pgduckdb:17-main` — `github.com/duckdb/pg_duckdb/blob/main/docker/README.md`),
so adoption is *possible*; but the build is heavy (Rust+`cargo-pgrx@0.16.1`+DuckDB+`make package` —
`pg_mooncake/Dockerfile:1-25`) and **no pg17 prebuilt `.so` exists** (releases ship source only —
`api.github.com/repos/Mooncake-Labs/pg_mooncake/releases`). CLAUDE.md rule 5 (performance is a claim, not an
opinion) + the parsimony ladder (don't embed a heavy dep before its value is measured) + the M7-S2 BM25
precedent (measure-then-gate a heavy adoption).

**Alternatives considered:** (i) Embed pg_mooncake into `theo-db:dev` now — rejected: pays a heavy, irreversible
build cost before any TheoDB measurement (anti-sunk-cost, CLAUDE.md). (ii) Reject columnar entirely — rejected:
M6 is a roadmap pillar and the capability is proven on the official image. (iii) In-memory columnar (Citus/Hydra)
— rejected by D1 (AGPL-barred).

**Consequences:** the M6 PR separates "capability proven + measured (PG18 image)" from "embedded in shipped PG17
image (gated)". The PG17 build recipe is documented and ready; it ships only after the measurement passes.

### D2 — Frame the columnar pillar honestly: DuckDB+Iceberg lakehouse on disk, NOT in-memory like AlloyDB

**Decision:** Every TheoDB doc/claim about M6 states plainly that this is a DuckDB+Iceberg **lakehouse
columnstore on disk**, a *competitive-different* permissive bet — NOT the **in-memory** columnar engine of
AlloyDB. No "AlloyDB columnar parity" claim.

**Rationale:** AlloyDB "keeps frequently queried data in an in-memory columnar format"
(`cloud.google.com/blog/products/databases/alloydb-for-postgresql-columnar-engine`); pg_mooncake is Iceberg-on-disk
(`pg_mooncake/README.md:13,17`). CLAUDE.md rule 7 (honesty about the trade-off) + `public-copy.md` §3-§4 (no
over-claim; performance claims need a reproduced benchmark) + ADR 0002 (the columnar pillar is a different,
competitive bet, not a literal copy).

**Alternatives considered:** (i) Claim AlloyDB columnar parity — rejected: false (in-memory vs disk) and bans-listed
by `public-copy.md`. (ii) Stay silent on the delta — rejected: silence reads as implied parity (Rule 3).

**Consequences:** TheoDB markets the *advantages it actually has* (MIT end-to-end, Iceberg-native interop —
`pg_mooncake/README.md:17`, portability) without an in-memory claim it cannot back. Caveat carried: the deep
AlloyDB docs page is off-allowlist (`UNVERIFIED`); the in-memory framing rests on the allowlisted blog + ADR 0002.

### D3 — Document the row-vs-columnar plan choice via EXPLAIN (DuckDBScan vs SeqScan) as the DoD-2 artifact

**Decision:** The M6 deliverable captures, with `EXPLAIN`, that the columnstore mirror query is planned as
`Custom Scan (DuckDBScan)` (DuckDB execution) while the row-table query is a heap `Aggregate -> Seq Scan` — the
literal DoD-2 evidence — plus the honest note that the routing branch is *which table you address* (mirror vs
heap), not an AlloyDB-style per-node cost flip on one table.

**Rationale:** DoD-2 demands documented row-vs-columnar plan choice. `pg_duckdb`'s planner hook produces
`Custom Scan (DuckDBScan)` + an embedded DuckDB plan (`github.com/duckdb/pg_duckdb`, Disc. #640); the live
evidence on the canonical distribution shows DuckDBScan on `trades_iceberg` vs Seq Scan on `trades`. Honesty
about the coarser routing vs AlloyDB's costing model (`cloud.google.com/blog/...columnar-engine`) is required by
Rule 7 / E5.

**Alternatives considered:** (i) Assert "the planner auto-picks columnar" on one table — rejected: false for
pg_mooncake (the mirror is a separate named relation), an over-claim of AlloyDB-like behavior. (ii) Skip the
EXPLAIN artifact — rejected: it is the DoD-2 acceptance evidence.

**Consequences:** the DoD-2 box is closed with a real plan artifact; the routing model is documented honestly so
downstream M6 work (and any "HTAP" copy) does not imply automatic per-node columnar selection.

## Recommendations for the project

| # | Recommendation | Linked to | Priority |
|---|---|---|---|
| 1 | Stand up the **capability** on the zero-build official PG18 image: enable `pg_mooncake`, create a `trades_iceberg` mirror, confirm `avg`→208.5 correctness/freshness | Q1, D1, `README.md:24-26,75-91` | HIGH |
| 2 | Capture the **DoD-2 plan artifact**: `EXPLAIN` showing `Custom Scan (DuckDBScan)` on the mirror vs `Seq Scan` on the row table | Q4, D3, testing.md | HIGH |
| 3 | Define + run the **DoD-1 measurement**: scan-heavy aggregate, mirror vs row-store, ≥3 warm runs, median wall-clock; publish under `docs/benchmarks/` (currently `UNBENCHMARKED`) | Q5, D1/D3, `public-copy.md` §4, CLAUDE.md rule 5 | HIGH |
| 4 | **Adoption gate** — only after rec #3 passes, build the PG17 image (`make install PG_VERSION=pg17` on `pgduckdb/pgduckdb:17-main`, `shared_preload_libraries='pg_duckdb,pg_mooncake'`, `wal_level=logical`) and embed in `theo-db:dev` | Q2/Q3, D1, `Dockerfile:1-37`, `Makefile:20`, parsimony-ladder.md | MEDIUM |
| 5 | State the **D2 honesty** in README/docs: DuckDB+Iceberg lakehouse on disk, NOT in-memory like AlloyDB; market MIT + Iceberg interop, not in-memory parity | Q6, D2, `public-copy.md` §3, CLAUDE.md rule 7 | HIGH |
| 6 | Note **operational prerequisites** for the HA pillar: `wal_level=logical` + `superuser` install; logical decoding must be on (M4 interaction) | Q2/Q3, `control:6`, `README.md:54` | LOW |

## Blocked questions (if any)

| Question | Reason | Suggested human follow-up |
|---|---|---|
| — | None. Q1–Q6 all answered with citations. | The single off-allowlist redirect (AlloyDB *docs* `about` page) is handled by `UNVERIFIED` + the allowlisted Google Cloud **blog** source + ADR 0002 — not a blocker. |

## Halt-loop progress (audit trail)

- Iterations used: 1 / 6
- Questions answered: 6 / 6 (Q1 tests/freshness; Q2 license+PG17+deps; Q3 build/adoption cost; Q4 plan choice; Q5 workload+measurement; Q6 AlloyDB SOTA vs lakehouse)
- Questions blocked: 0
- Local citations verified: 6 files (`pg_mooncake/README.md`, `Makefile`, `Dockerfile`, `pg_mooncake.control`, `LICENSE`, `duckdb/README.md` — all read with line numbers; `duckdb/AGENTS.md` test format)
- Web sources fetched: 6 — confirmed PG17 support (`Makefile:20` + live `raw.githubusercontent.com/.../README.md` "14-18" + `pg_duckdb` PG14–18); **no pg17 prebuilt `.so`** (`api.github.com/.../releases` → source archives only); **DuckDBScan vs SeqScan** plan choice (`github.com/duckdb/pg_duckdb` Disc. #640); DuckDB columnar-vectorized rationale (`duckdb.org/why_duckdb`); `pgduckdb:17-main` base tag exists (`github.com/duckdb/pg_duckdb/blob/main/docker/README.md`); AlloyDB in-memory columnar SOTA (`cloud.google.com/blog/products/databases/alloydb-for-postgresql-columnar-engine`)
- Markers: `UNBENCHMARKED` — no TheoDB perf number (Q5/DoD-1); the README ClickBench top-10 is vendor's, not reproduced. `UNVERIFIED` — `docs.cloud.google.com/alloydb/docs/columnar-engine/about` redirected off-allowlist; AlloyDB in-memory framing rests on the allowlisted blog + ADR 0002.
- Edge cases addressed: E1 (PG17 resolved, Q2/D1), E2 (heavy build → gate, Q3/D1), E3 (sync/freshness `UNBENCHMARKED`, Q1/Q4), E4 (no in-memory over-claim, Q6/D2), E5 (DuckDBScan-vs-SeqScan EXPLAIN, Q4/D3), E6 (measurement defined, no fabricated number, Q5/D1)
- Promise emitted at iteration: 1

## Related

- Discovery plan: `.claude/knowledge-base/discoveries/plans/m6-columnar-htap-plan.md`
- Confidence report: `.claude/knowledge-base/reviews/m6-columnar-htap-confidence-2026-06-28.md` (generated by `/discover-confidence`)
- North-star ADR (D2 framing): `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`
- Project rules: `.claude/rules/public-copy.md`, `.claude/rules/parsimony-ladder.md`, `.claude/rules/testing.md`, `.claude/rules/discover-phd-rigor.md`
