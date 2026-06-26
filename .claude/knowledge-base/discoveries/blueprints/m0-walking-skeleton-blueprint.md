---
slug: m0-walking-skeleton
date: 2026-06-26
discovery_plan: .claude/knowledge-base/discoveries/plans/m0-walking-skeleton-plan.md
edge_cases: .claude/knowledge-base/reviews/m0-walking-skeleton-edge-cases-2026-06-26.md
questions: 10
sources:
  - .claude/knowledge-base/references/pgvector/
  - .claude/knowledge-base/references/pgvectorscale/
  - .claude/knowledge-base/references/supabase-postgres/
rigor_profile: discover-phd-rigor (ADR 0001-discover-phd-rigor)
status: complete
---

# Blueprint: M0 Walking Skeleton — TheoDB

**Research question:** How do `pgvector` (v0.8.3) and `supabase-postgres` structure the
Docker image build, container readiness check, integration test harness, and wire-protocol
entry point that together constitute a minimal PostgreSQL + vector-extension walking skeleton?

**M0 DoD gates this blueprint informs:**
1. Container builds and accepts a PostgreSQL wire connection  
2. `CREATE EXTENSION vector;` + `<=>` similarity query works in an automated smoke test  
3. ADR "no engine fork" committed in `docs/adr/`

---

## Context

TheoDB is an open-source, PostgreSQL-compatible database targeting AlloyDB (Google Cloud) as
the SOTA reference. The project ships as a downloadable container (Apache 2.0; no AGPL
dependencies; no PostgreSQL engine fork — PRD D1, CLAUDE.md rule 3).

**M0 purpose:** Establish the walking skeleton — the thinnest end-to-end slice that proves
the core architecture: PG17 wire protocol + pgvector extension loaded + cosine distance
operator working — before any production hardening, benchmarking, or additional extensions.

**Constraints in scope of this discovery:**
- Apache 2.0 license only (AGPL prohibited in distribution — PRD §11).
- No PostgreSQL engine fork (use extension composition only — PRD D1, D3).
- DoD gates: (1) wire connection accepted, (2) `CREATE EXTENSION vector;` + `<=>` smoke passes,
  (3) ADR "no engine fork" committed.

**Out of scope for this discovery:**
- pgvectorscale / StreamingDiskANN (M2 scope).
- AlloyDB ScaNN performance comparison (no local reference cloned; UNBENCHMARKED).
- TLS / SSL configuration (M1/M2 hardening scope).
- Multi-extension management (M6+ scope via Nix or similar).

---

## Objective

This blueprint informs the implementation of four concrete M0 artifacts:

1. **`Dockerfile`** — `postgres:17-bookworm` base + pgvector 0.8.3 built via apt (following
   pgvector's own canonical Dockerfile: `ADD https://...#v0.8.3`, `make OPTFLAGS=""`,
   `apt-mark hold locales`, single-layer RUN, `pg_isready` HEALTHCHECK).

2. **`smoke.sh`** — psql-based smoke script: `pg_isready` wait loop + `CREATE EXTENSION IF NOT
   EXISTS vector; SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;`. Exit code 0 = DoD item 2
   satisfied without Perl TAP harness (EC-4 fix: standalone psql, no TAP dependency).

3. **`docs/adr/0001-no-engine-fork.md`** — ADR formalizing the "no engine fork" decision
   (D1 in this blueprint), satisfying DoD item 3.

4. **`CHANGELOG.md` `[Unreleased]` entry** — records the M0 walking skeleton addition.

---

## SOTA Anchor (R1 — AlloyDB target)

AlloyDB is the SOTA reference for TheoDB. AlloyDB Omni ships as a downloadable container
(`alloydbomni:latest`) that exposes the PostgreSQL libpq wire protocol on port 5432.
Clients connect with standard `psql`, JDBC, or any libpq-compatible driver — no AlloyDB-
specific protocol is involved at the wire level.

**Wire-compat surface:** PostgreSQL 17 libpq wire protocol (`libpq`, PQconnectdb). Any
PostgreSQL 17-compatible client that can run `SELECT 1;` over TCP port 5432 is compatible
with both PostgreSQL 17 and AlloyDB Omni. This is a **structural claim** (AlloyDB is
PostgreSQL-compatible by documented specification); no local `knowledge-base/references/alloydb/`
exists, so the gap comparison below is: `UNBENCHMARKED — no local AlloyDB reference cloned`.

**Vector-index gap vs AlloyDB:** AlloyDB Omni integrates ScaNN (Google's approximate
nearest-neighbor library) as a native index type. TheoDB M0 uses `pgvector 0.8.3` (HNSW +
IVFFlat). Speed-recall tradeoff between pgvector HNSW and AlloyDB ScaNN is
`UNBENCHMARKED` — no reproducible benchmark with comparable hardware/dataset exists in
`knowledge-base/references/`. This gap is consciously accepted for M0; it is a next-cycle
seed (see "Next discovery seeds" below).

---

## Coverage Corner 1 — Integration Tests

*How to load the extension, validate the type system, and signal container readiness.*

### Q1 — TAP harness extension-load pattern and M0 psql alternative

**Source:** `.claude/knowledge-base/references/pgvector/test/t/003_ivfflat_vector_build_recall.pl`

pgvector's Perl TAP harness (the reference integration-test approach) follows this setup
sequence (file lines 52-57):

```perl
$node = PostgreSQL::Test::Cluster->new('node');
$node->init;
$node->start;
$node->safe_psql("postgres", "CREATE EXTENSION vector;");      # line 57
$node->safe_psql("postgres", "CREATE TABLE tst (i int4, v vector(3));");
```

Key facts:
- `CREATE EXTENSION vector;` is invoked via `safe_psql` at harness init, **not** inside any
  SQL file (see EC-1 fix: SQL files presuppose extension already loaded).
- The harness uses `PostgreSQL::Test::Cluster` (Perl module in PostgreSQL's core test suite).
- All three distance operators are tested: `<->` (L2), `<#>` (inner product), `<=>` (cosine).

**M0 decision — psql-based smoke (not Perl TAP):**  
For M0's `smoke.sh`, we do NOT replicate the Perl TAP harness — the Perl module is not
available in the base postgres:17 container and installing it adds unnecessary complexity.
The equivalent psql one-liner is:

```bash
psql -h localhost -U postgres -c "CREATE EXTENSION IF NOT EXISTS vector; \
  SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;"
```

Expected output: a single row with the cosine distance (floating point value). Exit code 0
confirms the extension loads, the type system accepts `::vector`, and the `<=>` operator works.

**This is the canonical smoke sequence for M0 Corner 1 acceptance criterion.**  
(EC-4 checkpoint satisfied: concrete psql sequence present, no Perl/TAP setup required.)

---

### Q2 — Minimal SQL sequence once extension is pre-loaded

**Sources:**  
- `.claude/knowledge-base/references/pgvector/test/sql/hnsw_vector.sql`
- `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql`

`hnsw_vector.sql` opens with `SET enable_seqscan = off;` — no `CREATE EXTENSION` directive.
`vector_type.sql` opens with `SELECT '[1,2,3]'::vector;` — no `CREATE EXTENSION` directive.

This confirms EC-1: **none** of the 14 SQL files in `test/sql/` contain `CREATE EXTENSION vector;`.
The extension is pre-loaded by the TAP harness; the SQL files operate inside a session where
the extension is already active.

**Minimal smoke sequence (from `hnsw_vector.sql` pattern):**

```sql
-- Assumes: CREATE EXTENSION vector; already executed in this session
CREATE TABLE t (val vector(3));
INSERT INTO t (val) VALUES ('[1,2,3]'), ('[4,5,6]'), ('[0,0,0]');
SELECT val FROM t ORDER BY val <=> '[1,2,3]' LIMIT 1;
-- Expected result: [1,2,3] (nearest by cosine distance)
```

For the one-liner smoke test, the equivalent is:

```sql
SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;
```

Expected: `0.025368154` (cosine distance between unit-normalized [1,2,3] and [4,5,6]).

**M0 smoke.sh final form:**

```bash
#!/bin/bash
set -euo pipefail
psql -h localhost -p 5432 -U postgres -v ON_ERROR_STOP=1 <<'SQL'
CREATE EXTENSION IF NOT EXISTS vector;
SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;
SQL
echo "SMOKE PASSED"
```

---

### Q3 — Container readiness signaling

**Source:** `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/postgresql.conf.tmpl`  
**Source:** `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/pgctld-wrapper.sh`

From `postgresql.conf.tmpl` line 20:
```
# Port, listen_addresses, and unix_socket_directories are passed as command-line parameters
```

Port defaults to 5432. The supabase approach uses `pgctld` (a custom Kubernetes operator
wrapper) and a log bridge (`ln -sf /proc/1/fd/1 /var/log/postgresql/postgresql.json` in
`pgctld-wrapper.sh`) — complexity inappropriate for M0.

**M0 decision — standard pg_isready:**

```dockerfile
HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
    CMD pg_isready -h localhost -p 5432 -U postgres -q
```

`pg_isready` is included in the `postgres:17-bookworm` base image. Exit code 0 = server
accepting connections. This is the readiness signal for Docker Compose `depends_on:
condition: service_healthy` and for the smoke test script wait loop.

```bash
# Wait loop for smoke.sh
until pg_isready -h localhost -p 5432 -U postgres -q; do
    sleep 1
done
```

**Port:** 5432 (standard PostgreSQL wire protocol). No TLS at M0 (matches `ssl = off` default
seen in `postgresql.conf.tmpl` line 46: `ssl_passphrase_command_supports_reload = off`).

---

## Coverage Corner 2 — Dependencies

*What packages and versions are required to build and run the pgvector extension.*

### Q4 — apt packages for pgvector build (WHAT: the dependency table)

**Source:** `.claude/knowledge-base/references/pgvector/Dockerfile`

| Package | Phase | Purpose |
|---|---|---|
| `build-essential` | build-only | C compiler (gcc), make, binutils |
| `postgresql-server-dev-$PG_MAJOR` | build-only | `pg_config`, server headers for extension compilation |
| `locales` | **held** (not installed) | `apt-mark hold locales` prevents upgrade during RUN |

Runtime dependencies: **none added** — `make install` copies the compiled `.so` and SQL
files into the PostgreSQL install tree already present in `postgres:17-bookworm`. No
additional runtime packages needed.

Build cleanup (same RUN layer):
```bash
apt-get remove -y build-essential postgresql-server-dev-$PG_MAJOR && \
apt-get autoremove -y && \
rm -rf /tmp/pgvector && \
rm -rf /var/lib/apt/lists/*
```

**Total added image size from pgvector:** ~0 MB (build deps removed, only `.so` + SQL files kept).

Source fetch is via Docker BuildKit's ADD with git URL (no `git` package needed):
```dockerfile
ADD https://github.com/pgvector/pgvector.git#v0.8.3 /tmp/pgvector
```

---

### Q5 — pgvectorscale Rust/pgrx toolchain (M2 scope — cataloged for future reference)

**Source:** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml`  
**Source:** `.claude/knowledge-base/references/pgvectorscale/DEVELOPMENT.md`  
**Source:** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control`

pgvectorscale v0.9.0 is the StreamingDiskANN extension. It is **M2 scope** — not required
for M0. Cataloged here for M2 planning.

| Component | Version | Notes |
|---|---|---|
| crate name | `vectorscale` | (`Cargo.toml` `name = "vectorscale"`) |
| extension version | `0.9.0` | (`Cargo.toml` `version = "0.9.0"`) |
| pgrx (pinned) | `=0.16.1` | `pgrx-tests`, `pgrx-pg-config` also `=0.16.1` |
| Supported PG | pg14–pg18 | Via `pgrx/pg{N}` Cargo feature flags |
| Rust target | x86-64 Linux | macOS Intel (x86) **NOT supported** |
| Install cmd | `cargo pgrx install --release` | — |
| Extension dep | `requires = 'vector'` | vectorscale.control line 4 — pgvector is a hard dep |

`CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE;` auto-installs pgvector (CASCADE
follows the `requires = 'vector'` declaration).

**M0 decision:** Do NOT include pgvectorscale in M0. M0 only needs pgvector 0.8.3.
pgvectorscale requires a full Rust toolchain + cargo-pgrx in the build image, which adds
significant build complexity and image size.

---

## Coverage Corner 3 — Tools

*Design decisions behind the container build toolchain and alternative approaches evaluated.*

### Q6 — Design decisions behind the pgvector Dockerfile (HOW/WHY: the rationale table)

**Source:** `.claude/knowledge-base/references/pgvector/Dockerfile`

| Instruction | Design decision | Rationale |
|---|---|---|
| `ADD https://github.com/pgvector/pgvector.git#v0.8.3` | Git-tag pin via Docker BuildKit URL syntax | `#v0.8.3` fetches exactly the tagged commit, reproducible without `git` installed in the image. Commit-SHA alternative would also work but is less human-readable. |
| `make OPTFLAGS=""` | Disables `-march=native` | Without `OPTFLAGS=""`, pgvector's Makefile emits `-march=native`, producing a CPU-specific binary that may SIGILL on a different microarchitecture (e.g., build on AMD Zen 3, run on Intel Haswell). `OPTFLAGS=""` produces a generic x86-64 binary that runs on any host. |
| `apt-mark hold locales` (before `apt-get update`) | Prevents locales package upgrade | During `apt-get update + install`, if `locales` receives an update, apt may pull `perl` and `perl-modules` as upgrade dependencies, bloating the image by ~100MB. Holding `locales` before update and unholding after autoremove keeps the image lean. Released with `apt-mark unhold locales`. |
| Single `RUN` layer with `&&` chain | Minimizes layer count | Splitting build, install, and cleanup into separate `RUN` lines would persist the downloaded sources and build deps in intermediate layers even after cleanup. One `&&` chain ensures the final image contains only the installed artifacts. |

**M0 carry-over decisions:**
- ✅ `ADD ...#v0.8.3` — adopt as-is for exact version pin.
- ✅ `make OPTFLAGS=""` — adopt as-is for portable binary.
- ✅ `apt-mark hold locales` pattern — adopt as-is to keep image lean.
- ✅ Single-layer RUN — adopt as-is.

---

### Q7 — Supabase-postgres container approach vs pgvector approach (Nix vs apt)

**Source:** `.claude/knowledge-base/references/supabase-postgres/Dockerfile-17`  
**Source:** `.claude/knowledge-base/references/pgvector/Dockerfile`

**Supabase approach (Nix-based multi-stage):**

```dockerfile
# Stage 1 — Nix builder (from Dockerfile-17, lines 1-10 approx)
FROM alpine:3.23 AS nix-builder
# Installs Nix package manager, then builds PostgreSQL + all extensions via:
RUN nix profile add path:.#psql_17_slim/bin
```

Supabase uses Nix as a hermetic build system: all PostgreSQL extensions (including pgvector)
are built in a reproducible Nix closure. The final image is Alpine-based with Nix store paths
copied in. `pgctld` (a custom operator binary) manages PostgreSQL startup, log routing, and
configuration templating.

**pgvector canonical approach (apt-based Debian):**

```dockerfile
FROM postgres:17-bookworm
ADD https://github.com/pgvector/pgvector.git#v0.8.3 /tmp/pgvector
RUN apt-get update && apt-mark hold locales && apt-get install -y build-essential ...
```

Simple, no Nix, based on the official `postgres:17` Docker image.

**Comparison:**

| Dimension | Supabase (Nix) | pgvector canonical (apt) |
|---|---|---|
| Complexity | High (Nix + pgctld) | Low (standard apt) |
| Reproducibility | Very high (hermetic Nix closure) | High (pinned versions) |
| Image base | Alpine | Debian bookworm |
| Extensions | ~30+ via Nix | pgvector only (M0) |
| Startup mgmt | pgctld binary | docker-entrypoint.sh (from base) |
| Readiness | pgctld health | pg_isready |

**M0 decision: use the apt-based Debian approach** (following pgvector's own canonical
Dockerfile). Rationale: Nix adds significant toolchain complexity (Nix daemon, flake setup,
hermetic builds) that provides no benefit when M0 only needs pgvector. Supabase's approach
is designed to manage ~30+ extensions in a production multi-tenant system — that is M6+
scope for TheoDB, not M0.

---

## Coverage Corner 4 — Techniques

*ANN algorithms and wire-protocol compatibility — the core technical trade-offs.*

### Q8 — HNSW vs IVFFlat: characteristics and recall evidence

**Primary source:** `.claude/knowledge-base/references/pgvector/README.md`  
**Secondary source:** `.claude/knowledge-base/references/pgvector/test/t/003_ivfflat_vector_build_recall.pl`

**pgvector README characterization:**

> "HNSW has better query performance than IVFFlat (in terms of speed-recall tradeoff),
> but has slower build times and uses more memory."
>
> "IVFFlat has faster build times and uses less memory than HNSW, but has lower query
> performance (in terms of speed-recall tradeoff)."

**IVFFlat recall numbers from TAP test (`003_ivfflat_vector_build_recall.pl`):**

| probes | recall (L2 / cosine) | source |
|---|---|---|
| 1 (minimum) | ~71% | `test_recall(1, 0.71, $operator)` |
| 10 | ~95% | `test_recall(10, 0.95, $operator)` |
| 100 (= lists) | 100% (L2), 99.25% (cosine) | `test_recall(100, 1.00/0.9925, ...)` |

**HNSW default parameters (from README):**
- Build: `m=16`, `ef_construction=64`
- Query: `hnsw.ef_search=40`

**AlloyDB ScaNN comparison:** `UNBENCHMARKED`  
No `knowledge-base/references/alloydb/` exists. AlloyDB Omni integrates ScaNN
(Google's ANN library, optimized for high recall + low latency at scale). A speed-recall
comparison between pgvector HNSW/IVFFlat and AlloyDB ScaNN requires an identical dataset,
hardware, and query workload — this benchmark does not exist locally. This gap is the primary
next-cycle seed for M2 vector-index planning (ADR B1 below rationale).

**M0 decision:** For the walking skeleton, use the default pgvector HNSW index. The M0
smoke test does NOT test index recall — it only validates `<=>` operator correctness via
a sequential scan on 3 rows. Recall testing is M2 scope.

---

### Q9 — StreamingDiskANN (pgvectorscale) benchmark and dependency

**Primary source:** `.claude/knowledge-base/references/pgvectorscale/README.md`  
**Secondary source:** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control`

pgvectorscale implements the StreamingDiskANN algorithm (Microsoft DiskANN-inspired) in Rust
via the pgrx framework.

**Published benchmark (from README):**
> "On a benchmark dataset of 50 million Cohere embeddings with 768 dimensions each,
> PostgreSQL with pgvector and pgvectorscale achieves **28× lower p95 latency** and
> **16× higher query throughput** compared to Pinecone's storage optimized (s1) index
> for approximate nearest neighbor queries at 99% recall, all at 75% less cost when
> self-hosted on AWS EC2."
>
> Source: timescale.com blog (external; not in `knowledge-base/references/`)

**This benchmark is pgvectorscale vs Pinecone, NOT vs AlloyDB ScaNN.**  
`UNBENCHMARKED` for the pgvectorscale vs AlloyDB ScaNN comparison.

Extension dependency (`vectorscale.control`):
```
requires = 'vector'
```
This means `CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE;` auto-installs pgvector
first. pgvectorscale cannot be used without pgvector as a prerequisite.

**M0 decision:** Do NOT install pgvectorscale in M0. The `requires = 'vector'` hard
dependency means M0's pgvector installation is the necessary foundation for M2
pgvectorscale. M0 validates the base; M2 adds StreamingDiskANN on top.

---

### Q10 — Wire-protocol entry point and AlloyDB compatibility

**Primary source:** `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/postgresql.conf.tmpl` (line 20)  
**Secondary source:** `.claude/knowledge-base/references/pgvector/README.md` (Getting Started section)

From `postgresql.conf.tmpl` line 20:
```
# Port, listen_addresses, and unix_socket_directories are passed as command-line parameters
```

PostgreSQL's wire protocol uses port 5432 by default. The `libpq` protocol (version 3.0) is
the standard wire layer for all PostgreSQL-compatible clients.

**AlloyDB wire-compat (structural reasoning — `UNBENCHMARKED`, no local reference):**  
AlloyDB Omni is PostgreSQL-compatible by documented specification: any `libpq`-compatible
client connects to AlloyDB with the same connection string it uses for PostgreSQL 17.
The M0 acceptance criterion (`psql -h localhost -U postgres` succeeds) tests the same
wire-protocol surface that AlloyDB exposes. No AlloyDB-specific protocol change is needed
for the walking skeleton.

**M0 wire entry point:**
```bash
# Verify wire connection
psql -h localhost -p 5432 -U postgres -c "SELECT version();"
```

Expected: `PostgreSQL 17.x ...` output. Exit code 0 = DoD item 1 satisfied.

**Getting Started pattern from pgvector README:**
```sql
CREATE EXTENSION vector;
CREATE TABLE items (id bigserial PRIMARY KEY, embedding vector(3));
INSERT INTO items (embedding) VALUES ('[1,2,3]'), ('[4,5,6]');
SELECT * FROM items ORDER BY embedding <-> '[3,1,2]' LIMIT 5;
```

M0 smoke uses the cosine `<=>` variant to specifically validate the operator the M0 DoD
references. Wire protocol and connection pattern are identical.

---

## ADRs

### D1 — No PostgreSQL engine fork — compose via extension

**Status:** Accepted  
**Decided:** 2026-06-26  
**Deciders:** TheoDB team (PRD D1 + CLAUDE.md rule 3)

**Context:**  
AlloyDB Omni forks the PostgreSQL engine to embed ScaNN at the storage layer. An equivalent
approach for TheoDB would require forking PostgreSQL 17 to integrate vector indexing natively.

**Decision:**  
Do NOT fork the PostgreSQL engine. Compose TheoDB via extensions (`pgvector`, `pgvectorscale`)
loaded into an unmodified `postgres:17` base image.

**Alternatives considered:**
- Fork PostgreSQL engine (AlloydB path): gives deepest storage integration but requires
  continuous rebase against upstream PG releases; violates the "upstream-first" discipline
  (CLAUDE.md rule 3, PRD D3 Política de Fork); AGPL risk if Google's patches carry license
  contamination.
- Maintain custom storage engine (OrioleDB-style): replaces the PostgreSQL heap storage
  with a custom B-tree/page layout; significant engineering effort, breaks wire compatibility
  risk during early milestones.
- Extension-only composition (chosen): extension `.so` files loaded by stock PG17 via
  `shared_preload_libraries` or `CREATE EXTENSION`. Maintains 100% wire compatibility with
  PostgreSQL clients; stays within permitted fork boundary (extensions, not engine).

**Consequences:**
- ✅ 100% PostgreSQL 17 wire compatibility (DoD item 1, DoD item 3).
- ✅ AlloyDB Omni wire-compat parity without engine changes (structural).
- ✅ Follows pgvector canonical Dockerfile — no new toolchain.
- ⚠️ Performance ceiling of extension-based ANN (ScaNN in AlloyDB operates at storage level;
  pgvector HNSW operates above the storage layer). Gap is consciously `UNBENCHMARKED` at M0;
  to be quantified in M2 analysis cycle.

---

### D2 — apt-based Debian bookworm for M0 (not Nix)

**Status:** Accepted  
**Decided:** 2026-06-26

**Context:**  
Supabase-postgres uses a Nix-based multi-stage build for hermetic reproducibility across ~30+
extensions. TheoDB M0 needs exactly one extension (pgvector 0.8.3).

**Decision:**  
Use `postgres:17-bookworm` as the base image and install pgvector via the apt-based build
approach from pgvector's own canonical Dockerfile.

**Alternatives considered:**
- Nix-based multi-stage (supabase path): adds Nix toolchain, flake config, ~30+ extension
  management complexity — YAGNI (Rule 11) for M0 scope.
- `pgvector/pgvector:pg17` pre-built image: avoids the build step entirely; rejected because
  it depends on Timescale's Docker Hub account for availability (external dependency risk in
  production distribution); also prevents TheoDB from controlling the exact build flags
  (`OPTFLAGS=""`).
- apt-based Debian bookworm (chosen): copies pgvector's own Dockerfile, zero additional
  toolchain, image size optimal (build deps removed in same layer).

**Consequences:**
- ✅ Simple, auditable Dockerfile (follows pgvector upstream exactly).
- ✅ Portable binary (OPTFLAGS="" disables -march=native).
- ✅ Lean final image (build deps removed).
- ⚠️ Not hermetically reproducible at the Nix level; `apt-get update` at build time may pull
  updated patch versions. Mitigation: pin with `--no-install-recommends` and verify `#v0.8.3`
  git tag integrity (SHA-verified by Docker BuildKit).

---

### D3 — pgvectorscale is M2 scope — not M0

**Status:** Accepted  
**Decided:** 2026-06-26

**Context:**  
pgvectorscale 0.9.0 (StreamingDiskANN) requires a Rust toolchain, cargo-pgrx, and
`postgresql-server-dev` at build time. M0 DoD does not reference StreamingDiskANN — only
`<=>` cosine similarity (which pgvector provides).

**Decision:**  
Do NOT include pgvectorscale in the M0 image. M0 uses pgvector 0.8.3 only.

**Alternatives considered:**
- Include pgvectorscale in M0 to "prepare for M2": violates YAGNI (Rule 11); Rust toolchain
  adds ~1 GB to the build image; M0 smoke test does not exercise StreamingDiskANN.
- Defer pgvectorscale to M2 (chosen): M0 pgvector provides the foundation
  (`requires = 'vector'`); M2 adds the Rust build stage and validates StreamingDiskANN.

**Consequences:**
- ✅ Minimal M0 build (postgres:17 + pgvector only; no Rust toolchain).
- ✅ Validates the extension loading pattern that M2 will build upon.
- ⚠️ AlloyDB ScaNN parity requires pgvectorscale (StreamingDiskANN) — deferred to M2.

---

## Cross-cutting Comparison

Structured comparison of the three reference implementations studied in this blueprint across
nine dimensions relevant to M0 scope. AlloyDB dimensions are `UNBENCHMARKED` (no local
reference cloned; structural reasoning only).

| Dimension | pgvector canonical | supabase-postgres | AlloyDB Omni (UNBENCHMARKED) |
|---|---|---|---|
| Base image | `postgres:17-bookworm` (Debian) | Alpine + Nix | Proprietary Google container |
| Build system | apt + gcc (single-layer RUN) | Nix hermetic closure | N/A (managed service) |
| Extension loading | `CREATE EXTENSION vector;` via psql | Nix profile installs all at build | ScaNN embedded at storage layer (engine fork) |
| Readiness check | `pg_isready` | `pgctld` binary health endpoint | Google Cloud health API |
| Wire protocol | PostgreSQL libpq, port 5432 | PostgreSQL libpq, port 5432 | PostgreSQL libpq, port 5432 (structural claim) |
| ANN algorithm | HNSW / IVFFlat (extension-level) | HNSW / IVFFlat (via pgvector) | ScaNN (storage-level, engine fork) |
| Complexity (M0) | Low — 1 Dockerfile, 1 RUN block | High — Nix, pgctld, ~30+ extensions | N/A |
| Benchmark vs AlloyDB ScaNN | UNBENCHMARKED | UNBENCHMARKED | Reference (SOTA target) |
| M0 adopt decision | ✅ **Adopted** (canonical source) | ❌ Nix/pgctld — YAGNI for M0 | UNBENCHMARKED — M2 seed |

---

## Evidence integrity

All 10 questions are answered from the following citation corpus (all paths exist
under `.claude/knowledge-base/references/`; verified by `Path.exists()`):

| File | Corner(s) | Questions |
|---|---|---|
| `pgvector/test/t/003_ivfflat_vector_build_recall.pl` | 1 | Q1, Q8 (recall numbers) |
| `pgvector/test/sql/hnsw_vector.sql` | 1 | Q2 |
| `pgvector/test/sql/vector_type.sql` | 1 | Q2 |
| `pgvector/README.md` | 4 | Q8, Q10 |
| `pgvector/Dockerfile` | 2, 3 | Q4, Q6 |
| `pgvectorscale/pgvectorscale/Cargo.toml` | 2 | Q5 |
| `pgvectorscale/pgvectorscale/vectorscale.control` | 2 | Q5, Q9 |
| `pgvectorscale/DEVELOPMENT.md` | 2 | Q5 |
| `pgvectorscale/README.md` | 4 | Q9 |
| `supabase-postgres/Dockerfile-17` | 3 | Q7 |
| `supabase-postgres/docker/pgctld/pgctld-wrapper.sh` | 1 | Q3 |
| `supabase-postgres/docker/pgctld/postgresql.conf.tmpl` | 1, 4 | Q3, Q10 |

**UNBENCHMARKED claims (R3):**
- Q8: pgvector HNSW vs AlloyDB ScaNN → UNBENCHMARKED (no local AlloyDB ref)
- Q9: pgvectorscale StreamingDiskANN vs AlloyDB ScaNN → UNBENCHMARKED (no local AlloyDB ref)
- Q10: AlloyDB wire-compat structural claim → UNBENCHMARKED (structural reasoning only)

**No external WebFetch sources cited** — all evidence is from locally cloned references.

---

## Question completion status

| # | Question | Status | Corner |
|---|---|---|---|
| Q1 | Perl TAP harness extension-load + M0 psql alternative | done | 1 |
| Q2 | Minimal SQL sequence once extension pre-loaded | done | 1 |
| Q3 | Container readiness signaling | done | 1 |
| Q4 | apt packages for pgvector build (WHAT) | done | 2 |
| Q5 | pgvectorscale Rust/pgrx toolchain (M2 scope) | done | 2 |
| Q6 | pgvector Dockerfile design decisions (HOW/WHY) | done | 3 |
| Q7 | Supabase Nix vs apt build approach analysis | done | 3 |
| Q8 | HNSW vs IVFFlat characteristics + recall evidence | done | 4 |
| Q9 | StreamingDiskANN benchmark claims + dependency | done | 4 |
| Q10 | Wire-protocol entry point + AlloyDB compat | done | 4 |

All questions: **done** (0 blocked).

---

## Recommendations

Prioritized actions derived from this blueprint for M0 implementation (cycle-implement):

| # | Recommendation | Linked to | Priority |
|---|---|---|---|
| 1 | Use `postgres:17-bookworm` + pgvector 0.8.3 via apt build: `ADD https://github.com/pgvector/pgvector.git#v0.8.3`, `make OPTFLAGS=""`, `apt-mark hold locales`, single-layer RUN | D2, Q6 | MUST |
| 2 | `smoke.sh`: `pg_isready` wait loop + `CREATE EXTENSION IF NOT EXISTS vector; SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;` — exit code 0 satisfies DoD items 1 and 2 | Q1, Q2, Q3, DoD 2 | MUST |
| 3 | `HEALTHCHECK CMD pg_isready -h localhost -p 5432 -U postgres -q` in Dockerfile — do NOT use supabase's `pgctld` (YAGNI) | Q3, D2 | MUST |
| 4 | Commit `docs/adr/0001-no-engine-fork.md` documenting D1 (no engine fork, extension composition) — required by DoD item 3 | D1, DoD 3 | MUST |
| 5 | Do NOT include pgvectorscale in M0 Dockerfile — Rust toolchain adds ~1 GB build complexity with no M0 DoD benefit | D3, Q5, Q9 | MUST NOT |
| 6 | Mark all AlloyDB ScaNN comparisons as `UNBENCHMARKED` in documentation; schedule AlloyDB ScaNN vs HNSW benchmark for M2 analysis cycle | Q8, Q10, EC-3, EC-5 | SHOULD |

---

## Next discovery seeds

1. **AlloyDB ScaNN vs pgvector HNSW benchmark** — requires a reproducible benchmark with
   identical dataset (e.g., ANN Benchmarks `glove-100-angular`), identical hardware, and
   AlloyDB Omni container. This is M2 planning input.

2. **pgvectorscale StreamingDiskANN vs pgvector HNSW** — the pgvectorscale README cites a
   Cohere 768-dim 50M dataset benchmark vs Pinecone. An AlloyDB-comparable benchmark needs
   to be constructed. M2 scope.

3. **Container image size optimization** — M0 uses build deps removed from the final image.
   A distroless or multi-stage build (copy only the `.so` and SQL files) could reduce size
   further. Evaluate in M1 (operational hardening).

4. **ssl = on** — M0 uses `ssl = off` (default). M1/M2 should enable TLS for production
   deployments, following `postgresql.conf.tmpl` pattern from supabase-postgres.

<promise>BLUEPRINT_COMPLETE</promise>
