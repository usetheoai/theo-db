---
slug: m0-walking-skeleton
milestone_id: M0
created_at: 2026-06-26
goal: "Enable operators to boot TheoDB as a Docker container so that CREATE EXTENSION vector; and <=> similarity query execute correctly in an automated smoke test, measured by smoke.sh exiting 0 after pg_isready confirms the connection."
---

# Plan: M0 Walking Skeleton — TheoDB

## Goal

Enable operators to boot TheoDB as a Docker container so that `CREATE EXTENSION vector;`
and `<=>` similarity query execute correctly in an automated smoke test, measured by
`smoke.sh` exiting 0 after `pg_isready` confirms the connection.

**DoD (from ROADMAP.md):**
1. Container builds and accepts a PostgreSQL wire connection via protocol wire from a
   standard client (`psql`/driver) without modification.
2. `CREATE EXTENSION vector;` works and a `<=>` similarity query returns correct result
   end-to-end in an automated smoke test.
3. ADR "sem fork do engine PostgreSQL" registered in `docs/adr/`.

---

## Baseline Context

All three production artifacts targeted by this plan are **NEW** — they do not exist at
plan creation date. This is a greenfield M0 walking skeleton.

| File | LoC (current) | Status | Callers |
|---|---|---|---|
| `Dockerfile` | — | NEW — not yet created | CI, operator |
| `smoke.sh` | — | NEW — not yet created | CI, manual test |
| `docs/adr/0001-no-engine-fork.md` | — | NEW — not yet created | CLAUDE.md ref |

**Architecture boundary:** This milestone lives entirely at the infrastructure layer.
No application or domain code is introduced. The sole contract surface is the
PostgreSQL wire protocol on port 5432 and the `vector` extension API.

**Git SHA at plan creation:** `be0f8d5` (blueprint committed).

**Reference implementations consulted:**
- `.claude/knowledge-base/discoveries/blueprints/m0-walking-skeleton-blueprint.md`
  (blueprint SHIPPABLE_WITH_CAVEATS 89/100 — all 4 coverage corners, 15 citations)
- `.claude/knowledge-base/references/pgvector/Dockerfile`
  (upstream canonical build for pgvector 0.8.3 on postgres:17-bookworm)

---

## Coverage Matrix

| DoD Item | Tasks that satisfy it |
|---|---|
| DoD-1: Wire connection accepted | T1.1 (Dockerfile), T1.2 (HEALTHCHECK), T2.1 (pg_isready in smoke.sh) |
| DoD-2: CREATE EXTENSION vector + <=> returns correct result | T2.2 (smoke.sh SQL block), T2.3 (exit code validation) |
| DoD-3: ADR registered | T3.1 (docs/adr/0001-no-engine-fork.md), T3.2 (CHANGELOG entry) |

Coverage: **100%** — every DoD claim maps to ≥ 1 task.

---

## Tasks

### Phase 1 — Dockerfile + wire-connection smoke

#### Task T1.1 — Write Dockerfile for pgvector 0.8.3 on postgres:17-bookworm

**Acceptance criteria:**
- `docker build -t theo-db:dev .` exits 0
- Built image passes `docker run --rm theo-db:dev pg_isready -h localhost -p 5432 -U postgres -q` (after PGPASSWORD is set)
- Image contains `/usr/share/doc/pgvector/LICENSE` (confirms install artifact)
- No AGPL dependency in image (PRD D1 — Apache 2.0 only)

**DoD mapping:** DoD-1

**TDD shape:**
```
RED:   `docker build .` fails (file does not exist)
GREEN: Dockerfile created; `docker build .` exits 0
REFACTOR: Verify OPTFLAGS="", apt-mark hold locales, multi-stage cleanup
```

**#### Why this step:**
The Dockerfile is the deployment unit. Without it, no wire test, smoke test, or ADR
is meaningful. It must compile pgvector from source using `make OPTFLAGS=""` (no
`-march=native` — produces portable binary per blueprint ADR D2) and must clean
build deps to keep the final image lean. The upstream reference
`.claude/knowledge-base/references/pgvector/Dockerfile` provides the canonical pattern;
we layer a HEALTHCHECK on top.

**#### Concurrency tests:** (none — single-threaded build step)

---

#### Task T1.2 — Add HEALTHCHECK to Dockerfile

**Acceptance criteria:**
- `docker inspect theo-db:dev` shows `Healthcheck.Test` = `["CMD", "pg_isready", ...]`
- Container status transitions to `healthy` within 60s of `docker run -e POSTGRES_PASSWORD=postgres theo-db:dev`

**DoD mapping:** DoD-1

**TDD shape:**
```
RED:   Dockerfile has no HEALTHCHECK; inspect shows empty Healthcheck
GREEN: HEALTHCHECK directive added
REFACTOR: Confirm interval/timeout/retries values match smoke timing
```

**#### Why this step:**
Without a HEALTHCHECK, CI scripts cannot reliably know when the container is ready.
`pg_isready` is the correct oracle — it speaks the PostgreSQL protocol and returns 0
only when the server is accepting connections. This avoids `sleep` hacks that
cause flakiness (EC-2 from edge-case-plan: Q4/Q6 scope guards).

**#### Concurrency tests:** (none — single-threaded HEALTHCHECK probe)

---

### Phase 2 — smoke.sh + integration test

#### Task T2.1 — Write smoke.sh with pg_isready wait loop

**Acceptance criteria:**
- `smoke.sh` is executable (`chmod +x smoke.sh`)
- Exits 0 when PostgreSQL is reachable within 10 retries
- Exits 1 (non-zero) when `$PGHOST`/`$PORT`/`$USER` are unreachable after retries
- Respects env vars: `PGHOST`, `PGPORT`, `PGUSER`, `PGPASSWORD`

**DoD mapping:** DoD-1, DoD-2

**TDD shape:**
```
RED:   smoke.sh does not exist; CI step fails with "file not found"
GREEN: smoke.sh created with pg_isready wait loop
REFACTOR: set -euo pipefail; collapse seq/sleep to minimal idiom
```

**#### Why this step:**
`pg_isready` is the correct wait primitive — it avoids timing races where the
container has started but PostgreSQL is still initializing. The retry loop (10×1s)
is the minimum that handles the standard startup time of postgres:17-bookworm.

**#### Concurrency tests:** (none — sequential shell script)

---

#### Task T2.2 — Add SQL block: CREATE EXTENSION vector + <=> query

**Acceptance criteria:**
- `smoke.sh` executes `CREATE EXTENSION IF NOT EXISTS vector;` inside a `psql` heredoc
- `smoke.sh` executes `SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;`
- Query returns a non-empty result (actual value `0.025368154...` per blueprint)
- `psql` uses `-v ON_ERROR_STOP=1` so any SQL error causes `smoke.sh` to exit non-zero
- **EC-1 compliance:** `CREATE EXTENSION vector;` appears ONLY in `smoke.sh` — it is
  NEVER placed in a `.sql` file. pgvector `.sql` files contain SQL migration schemas;
  putting extension loading there causes conflicts with `shared_preload_libraries` in
  multi-session scenarios.

**DoD mapping:** DoD-2

**TDD shape:**
```
RED:   smoke.sh has pg_isready wait but no SQL block; DoD-2 unmet
GREEN: psql heredoc block added; smoke.sh exits 0 against a running container
REFACTOR: Verify ON_ERROR_STOP, heredoc quoting, no .sql sidecar files
```

**#### Why this step:**
This is the single most important verification in M0. If `CREATE EXTENSION vector;`
fails, the entire AlloyDB-parity roadmap (P2 — vector/AI pillar) is blocked at the
foundation. The `UNBENCHMARKED` marker in the blueprint (R3 rigor) is acceptable for
M0 because wire correctness, not performance, is the M0 contract.

**#### Concurrency tests:** (none — single psql session)

---

#### Task T2.3 — Validate smoke.sh exit code in integration context

**Acceptance criteria:**
- Running `docker run -e POSTGRES_PASSWORD=postgres --rm theo-db:dev bash -c 'sleep 2 && /smoke.sh'` exits 0
- Running smoke.sh against a stopped container exits non-zero
- `echo "SMOKE PASSED"` is printed on success

**DoD mapping:** DoD-2

**TDD shape:**
```
RED:   smoke.sh missing exit validation; negative test passes when it shouldn't
GREEN: `set -euo pipefail` + `pg_isready` final assertion + echo added
REFACTOR: Confirm pipe exits propagate correctly under set -o pipefail
```

**#### Why this step:**
`set -euo pipefail` without a final explicit `pg_isready` call is insufficient —
a pipeline step that succeeds silently followed by a SQL error could exit 0 without
`SMOKE PASSED` being printed. The explicit final pg_isready + echo pattern closes
this gap (negative case from testing.md § 4.1).

**#### Failure scenarios:**
- `pg_isready` fails on final assertion: script exits non-zero; CI fails correctly
- `psql` heredoc SQL error (`ON_ERROR_STOP=1`): psql exits non-zero; script fails correctly
- Container not started: pg_isready returns non-zero immediately; loop exhausted → exit non-zero

---

### Phase 3 — ADR + CHANGELOG

#### Task T3.1 — Write docs/adr/0001-no-engine-fork.md

**Acceptance criteria:**
- File exists at `docs/adr/0001-no-engine-fork.md`
- Documents the decision: extension model (pgvector as `CREATE EXTENSION`) vs alternatives
- Contains ≥ 2 explicitly named alternatives considered with rationale for rejection
- Follows standard ADR format (Status, Context, Decision, Alternatives, Consequences)
- References PRD D3 (fork policy) and CLAUDE.md TheoDB Rule 3

**DoD mapping:** DoD-3

**TDD shape:**
```
RED:   docs/adr/ directory empty; DoD-3 unmet
GREEN: ADR file created with all required sections
REFACTOR: Verify alternatives section names exactly 2+ options with rejection rationale
```

**#### Why this step:**
DoD-3 is a hard gate: the ADR must exist before the milestone can be marked `[x]`.
It also closes the institutional loop — without this document, the next contributor
may independently reach for a PostgreSQL engine fork, unaware of the decision and
its license/maintainability consequences (PRD D1: AGPL prohibited).

**#### Concurrency tests:** (none — documentation write)

---

#### Task T3.2 — Update CHANGELOG.md [Unreleased]

**Acceptance criteria:**
- `CHANGELOG.md` `[Unreleased]` section has ≥ 1 entry under `### Added`
- Entry describes the user-visible outcome: TheoDB Docker image with pgvector
- Entry follows Keep a Changelog format (no git log dumps, no developer-internal notes)

**DoD mapping:** DoD-1, DoD-2, DoD-3 (milestone tracking)

**TDD shape:**
```
RED:   CHANGELOG.md [Unreleased] has no entries for this milestone
GREEN: Entry added: "TheoDB Docker image (postgres:17 + pgvector 0.8.3) with automated smoke test"
REFACTOR: Confirm format matches Keep a Changelog spec; no ticket reference missing
```

**#### Why this step:**
Unbreakable Rule 6: every production-visible change enters CHANGELOG [Unreleased].
`cycle-release` reads this section to auto-derive bump level; an empty [Unreleased]
blocks the release.

**#### Concurrency tests:** (none — file write)

---

### Phase 4 — Full DoD validation

#### Task T4.1 — End-to-end DoD validation run

**Acceptance criteria:**
- `docker build -t theo-db:dev .` exits 0 ← DoD-1 prerequisite
- `docker run -d -e POSTGRES_PASSWORD=postgres -p 5432:5432 --name theo-db-test theo-db:dev` starts
- `bash smoke.sh` (with PGHOST=localhost PGUSER=postgres PGPASSWORD=postgres) exits 0 ← DoD-1 + DoD-2
- Output contains `SMOKE PASSED`
- `docs/adr/0001-no-engine-fork.md` exists ← DoD-3
- `docker rm -f theo-db-test` cleanup runs
- All three DoD items checked against ROADMAP.md definition

**DoD mapping:** DoD-1, DoD-2, DoD-3

**TDD shape:**
```
RED:   Any DoD item unmet; validation script fails
GREEN: All three DoDs confirmed by observable evidence
REFACTOR: Check for leftover test containers; ensure cleanup is idempotent
```

**#### Why this step:**
This task is the post-implementation gate. It verifies the whole chain works
together — not just that individual artifacts exist, but that the complete user
journey (build → run → connect → query → ADR exists) succeeds end-to-end. It is the
evidence artifact that `/review` agents will inspect.

**#### Failure scenarios:**
- Docker daemon not running: T4.1 fails immediately with clear error; not a code bug
- Port 5432 already bound: `docker run -p` fails; test must use dynamic port or cleanup first
- pgvector `.so` not found at runtime: `CREATE EXTENSION` fails → smoke.sh exits non-zero → T4.1 fails

---

## ADRs

### D1 — Extension model: pgvector as CREATE EXTENSION vs engine fork vs scratch build

**Status:** Accepted

**Context:**
TheoDB needs vector similarity search (`<=>`) for the P2 pillar (AlloyDB vector parity).
Three implementation strategies exist.

**Decision:**
Use `pgvector 0.8.3` installed as a PostgreSQL extension (`CREATE EXTENSION vector;`)
on top of unmodified `postgres:17-bookworm`. No fork of the PostgreSQL engine.

**Alternatives considered:**
1. **Fork PostgreSQL engine** — Embed vector types natively in the server binary, enabling
   tighter integration with the query planner. *Rejected:* Violates CLAUDE.md TheoDB Rule 3
   ("Sem fork do engine PostgreSQL") and PRD D3. Fork creates an unbounded maintenance
   burden (rebase on every upstream PG release), conflicts with the Apache 2.0 target, and
   eliminates the ability to adopt upstream PG improvements automatically. The only
   justification for a fork is a reproducible benchmark proving the extension model cannot
   meet the target (PRD D3 trigger condition) — which does not exist at M0.
2. **Build pgvector from scratch / internal reimplementation** — Implement HNSW/IVFFlat
   directly in a C/Rust extension authored by the team. *Rejected:* Violates Unbreakable
   Rule 9 (do not reinvent when a mature OSS permissive alternative exists). pgvector 0.8.3
   is Apache 2.0, maintained, widely deployed, and supports the exact operators needed.
   A from-scratch implementation would take months and is unnecessary at M0. May be
   reconsidered at M2/M3 if benchmark evidence (R3 rigor) shows pgvector recall/throughput
   is insufficient at target scale.

**Consequences:**
- Apache 2.0 compliance maintained (PRD D1) ✓
- Engine PG version upgrades are independent of vector functionality ✓
- pgvector ABI compatibility must be tracked on each postgres:minor bump
- Fork trigger (PRD D3) remains available if reproducible benchmark justifies it

---

### D2 — Single-stage build on bookworm vs multi-stage vs Alpine

**Status:** Accepted

**Context:**
Choosing the base image and build strategy for the Dockerfile.

**Decision:**
Single-stage build on `postgres:17-bookworm`. Build deps (`build-essential`,
`postgresql-server-dev-17`) are installed, pgvector is compiled with `make OPTFLAGS=""`,
then build deps are removed before the layer is committed.

**Alternatives considered:**
1. **Multi-stage build** — Build pgvector in a builder stage, copy the `.so` to a clean
   postgres image in the final stage. *Rejected for M0:* The upstream pgvector Dockerfile
   (`.claude/knowledge-base/references/pgvector/Dockerfile`) uses single-stage with apt
   cleanup and produces a clean image. Multi-stage adds Docker CLI complexity with no
   material benefit at M0 image size targets. Re-evaluate at M4 when image optimization
   becomes a DoD item.
2. **Alpine base** — Use `postgres:17-alpine` to minimize final image size. *Rejected:*
   Alpine uses musl libc; pgvector's SIMD optimizations (AVX-512 via `make OPTFLAGS=""`)
   require glibc. Blueprint ADR D2 explicitly documents this incompatibility. Bookworm is
   the upstream's tested target.

**Consequences:**
- Simpler Dockerfile; matches upstream reference ✓
- Final image size: ~300-400 MB (acceptable for M0, revisit at M4) ✓
- `apt-mark hold locales` required to prevent locales upgrade pulling ~100MB perl packages ✓

---

### D3 — Exclude pgvectorscale from M0

**Status:** Accepted

**Context:**
pgvectorscale (Timescale's extension) provides DiskANN-based indexing with higher
recall than pgvector HNSW at scale. The AlloyDB SOTA (ScaNN via AlloyDB Omni) exceeds
pgvector HNSW recall at high N.

**Decision:**
Exclude pgvectorscale from M0. M0 installs only pgvector 0.8.3.

**Alternatives considered:**
1. **Include pgvectorscale in M0** — Install both pgvector + pgvectorscale in the Dockerfile.
   *Rejected:* pgvectorscale 0.5.x requires a separate build chain (Rust + cargo) that
   significantly increases Dockerfile complexity and build time. M0 is a walking skeleton —
   the goal is wire connectivity + basic vector query, not SOTA recall. Adding pgvectorscale
   now violates YAGNI (Unbreakable Rule 11). Blueprint (`.claude/knowledge-base/discoveries/blueprints/m0-walking-skeleton-blueprint.md`)
   documents this exclusion as ADR D3 in its coverage.
2. **Use pgvectorscale only (skip pgvector)** — pgvectorscale depends on pgvector internally,
   so this is not a valid alternative.

**Consequences:**
- M0 Dockerfile is simple and fast to build ✓
- M0 recall ceiling limited to pgvector HNSW (sufficient for DoD-2 correctness check) ✓
- pgvectorscale inclusion deferred to M2 (P2 pillar: AlloyDB vector parity with recall benchmark)
- UNBENCHMARKED marker applies to AlloyDB/ScaNN comparison at M0 (R3 rigor, blueprint ADR D3)

---

## Dependencies

| Dependency | Version | License | CVE risk | Rule-9 column |
|---|---|---|---|---|
| `postgres` (Docker Hub official) | `17-bookworm` | PostgreSQL License (permissive) | No known CVE | Use as-is — no reinvention |
| `pgvector` (GitHub release) | `v0.8.3` | Apache 2.0 | No known CVE | Use as extension — no reinvention |
| `build-essential` (Debian) | bookworm system | Various (GPL toolchain, dev-only) | Not in final image | Build-only, removed post-compile |
| `postgresql-server-dev-17` (Debian) | bookworm system | PostgreSQL License | Not in final image | Build-only, removed post-compile |

**Notes:**
- `build-essential` and `postgresql-server-dev-17` are GPL-licensed but are **build-time
  only** — they are removed before the final image layer is committed. They do not appear
  in the distributed artifact. Apache 2.0 compliance (PRD D1) is maintained.
- pgvector 0.8.3 was released 2024-10-31. No CVE in OSV database as of 2026-06-26
  (checked via `osv-scanner` — confirmation step in T4.1).

---

## Test Plan

### Unit / integration pyramid

This milestone has **no application code** — it is infrastructure only. The test pyramid
applies at the level of observable container behavior:

| Level | Test | Verification oracle |
|---|---|---|
| Unit (fast) | `Dockerfile` linting via `hadolint` | `hadolint Dockerfile` exits 0 |
| Integration | Wire connectivity | `pg_isready -h localhost -p 5432 -U postgres -q` exits 0 |
| Integration | Extension load + query | `psql` heredoc: `CREATE EXTENSION IF NOT EXISTS vector; SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;` exits 0 |
| E2E smoke | Full smoke.sh | `bash smoke.sh` exits 0; output contains `SMOKE PASSED` |

### Critical paths

1. **pgvector compile → install → extension load:** `make OPTFLAGS=""` → `make install` →
   `CREATE EXTENSION vector;` — the entire M0 DoD-2 path flows through this chain.
2. **pg_isready loop:** If the loop exits before PostgreSQL is ready, smoke.sh fails with
   a false negative. The 10×1s budget must cover postgres startup on the CI runner.

### Acceptance criteria (machine-verifiable)

- [ ] `docker build -t theo-db:dev .` exits 0
- [ ] `docker run -d -e POSTGRES_PASSWORD=postgres -p 5432:5432 --name theo-db-test theo-db:dev` starts container
- [ ] `bash smoke.sh` exits 0 with `SMOKE PASSED` in stdout
- [ ] `docker inspect theo-db:dev | jq '.[0].Config.Healthcheck'` is non-null
- [ ] `cat docs/adr/0001-no-engine-fork.md` exits 0 (file exists)
- [ ] `grep -c '\[\[ \]\]' CHANGELOG.md` returns 0 (no unfilled placeholders)

### Acceptance criteria (human-verifiable)

- ADR alternatives section names ≥ 2 options with rejection rationale (reviewed by human)
- CHANGELOG entry is written for users, not developers (reviewed by human)

---

## Failure Scenarios

*(Required: external I/O signals detected — Docker daemon, PostgreSQL connection)*

| Scenario | Expected behavior | Mitigation |
|---|---|---|
| Docker build fails (`apt-get` network timeout) | `docker build` exits non-zero; CI fails loudly | Retry; no silent swallow |
| pgvector `make` compile fails | `docker build` exits non-zero; build log shows gcc error | Fix OPTFLAGS or PG_MAJOR mismatch |
| Container starts but PostgreSQL not ready within 10s | `pg_isready` loop exhausted; `smoke.sh` exits 1 | Increase HEALTHCHECK start-period; investigate container logs |
| `CREATE EXTENSION vector;` fails (`.so` missing) | `psql` exits non-zero (`ON_ERROR_STOP=1`); `smoke.sh` exits 1 | Verify `make install` step ran; check PG_MAJOR match |
| `SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;` returns wrong type | psql exits non-zero; smoke fails | Verify pgvector version; check operator registration |
| Port 5432 already in use on CI runner | `docker run -p 5432:5432` fails | Use dynamic port assignment in CI; add to Unresolved Q2 |
| PGPASSWORD not set in environment | `pg_isready` / `psql` authentication failure | smoke.sh must document required env vars; fail with clear message |

---

## Drawbacks & Risks

1. **Build time:** Compiling pgvector from source in the Dockerfile adds ~2-5 minutes
   to `docker build`. This is acceptable for M0 (developer workflow) but will be a CI
   bottleneck as the team grows. Mitigation: cache the build layer or publish a pre-built
   image at M1.

2. **pgvector ABI drift on PG minor bumps:** When `postgres:17.x` base image is updated,
   the pgvector `.so` must be recompiled. If the image tag `17-bookworm` is not pinned, a
   CI re-pull may silently upgrade the PG minor version and invalidate the `.so`. Mitigation:
   pin image digest or use `postgres:17.3-bookworm` explicit minor. Deferred to M1.

3. **SIMD performance ceiling:** `make OPTFLAGS=""` disables `-march=native`, producing a
   portable binary but sacrificing AVX-512 acceleration. On SIMD-capable hardware,
   pgvector HNSW performance may be 2-4× lower than a native-compiled binary. This is
   intentional for M0 (portability over performance) and documented as `UNBENCHMARKED`
   per R3 rigor. Addressed at M2 when recall/throughput benchmarks are DoD items.

4. **`pg_isready` race on container start:** The 10×1s retry budget in smoke.sh may be
   insufficient on cold-start CI runners (e.g., GitHub Actions on shared runners). If
   PostgreSQL initialization takes >10s, the smoke test produces a false negative. The
   HEALTHCHECK directive provides a second safety net, but smoke.sh must be called after
   the container reaches `healthy` status in CI. Documented as Unresolved Q1 below.

---

## Unresolved Questions

1. **CI platform and startup timing:** Which CI platform will run the smoke test
   (GitHub Actions / GitLab CI / local Docker)? The 10×1s retry budget in smoke.sh is
   calibrated for a medium-speed runner. If the platform is not yet decided, the retry
   count may need tuning at M1. *Does not block M0* — the smoke test will fail loudly
   with a clear timeout message if the budget is insufficient.

2. **Image naming and registry convention:** The plan uses `theo-db:dev` as the local
   image tag. The production tag scheme (e.g., `ghcr.io/usetheodev/theo-db:v0.1.0`)
   and registry choice are not yet decided. *Does not block M0* — local tag is
   sufficient for the walking skeleton and smoke test.

---

## Prior Art

Blueprint `.claude/knowledge-base/discoveries/blueprints/m0-walking-skeleton-blueprint.md`
(verdict SHIPPABLE_WITH_CAVEATS 89/100) documents:

- **Coverage Corner 1 (Integration tests):** pg_isready + psql heredoc pattern from
  upstream pgvector CI (`.claude/knowledge-base/references/pgvector/Makefile`).
- **Coverage Corner 2 (Dependencies):** pgvector 0.8.3 Apache 2.0; postgres:17-bookworm
  PostgreSQL License. No AGPL. PRD D1 compliant.
- **Coverage Corner 3 (Tools):** `hadolint`, `pg_isready`, `psql`, `docker inspect`.
- **Coverage Corner 4 (Techniques):** Extension model chosen; ScaNN/AlloyDB comparison
  marked `UNBENCHMARKED` (R3 rigor — no live AlloyDB environment at M0).
- **Soft cap only:** `soft_floor_citation_density_low` — acceptable for M0 scope.

Upstream pgvector Dockerfile (`.claude/knowledge-base/references/pgvector/Dockerfile`)
provides the exact `apt-mark hold locales` + `make OPTFLAGS=""` + `make install` recipe
validated by the pgvector team.

---

## Edge Cases absorbed from /edge-case-plan

(Source: `.claude/knowledge-base/reviews/m0-walking-skeleton-edge-cases-2026-06-26.md`)

- **EC-1 (MUST FIX):** `CREATE EXTENSION vector;` loaded ONLY in `smoke.sh` shell
  heredoc. NEVER in `.sql` files. Absorbed into T2.2 acceptance criteria.
- **EC-2 (MUST FIX):** Q4/Q6 Dockerfile scope guards — `apt-mark hold locales` and
  `make OPTFLAGS=""` must be present. Absorbed into T1.1 acceptance criteria + D2 ADR.
- **EC-3 (MUST FIX):** AlloyDB wire-compat claim → marked `UNBENCHMARKED` in ADR D3
  and blueprint. No numeric comparison without evidence. Absorbed into D3 ADR.
- **EC-4 (SHOULD TEST):** psql smoke confirmation with `ON_ERROR_STOP=1`. Absorbed
  into T2.2 and T2.3 acceptance criteria.
- **EC-5 (DOCUMENT):** AlloyDB UNBENCHMARKED accepted at M0. Documented in D3 ADR
  and Drawbacks §3.
