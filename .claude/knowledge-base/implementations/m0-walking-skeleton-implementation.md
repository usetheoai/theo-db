---
slug: m0-walking-skeleton
milestone_id: M0
date: 2026-06-26
plan: .claude/knowledge-base/plans/m0-walking-skeleton-plan.md
verdict: IMPLEMENTATION_COMPLETE
commits:
  - sha: 5ea6d67
    tasks: [T1.1, T1.2]
    msg: "feat(T1.1/T1.2): Dockerfile — pgvector 0.8.3 on postgres:17-bookworm with HEALTHCHECK"
  - sha: ef532c2
    tasks: [T2.1, T2.2, T2.3]
    msg: "feat(T2.1-T2.3): smoke.sh — pg_isready loop + CREATE EXTENSION vector + <=> query (EC-1)"
  - sha: 0db4d60
    tasks: [T3.1, T3.2]
    msg: "docs(T3.1/T3.2): ADR 0001 no-engine-fork + CHANGELOG entries for M0"
---

# Implementation Log — M0 Walking Skeleton

**Verdict: `IMPLEMENTATION_COMPLETE`**

All 8 tasks across 4 phases completed. All 3 DoDs validated empirically on 2026-06-26T14:56:51-03:00.

---

## Wiring Triad Checklist

Per `cycle-implement.md § Wiring triad`: caller + integration test + runtime metric.

| Task | Caller | Integration test | Runtime metric | Status |
|------|--------|-----------------|----------------|--------|
| T1.1 — Dockerfile | `docker build` invocation (DoD-1 validation run) | `docker build -t theo-db:dev .` exits 0 | HEALTHCHECK `pg_isready` (runtime signal visible via `docker inspect`) | ✓ COMPLETE |
| T1.2 — HEALTHCHECK | Docker daemon exec loop | `docker inspect --format='{{.State.Health.Status}}'` → `healthy` within 15s | Container health status observable externally | ✓ COMPLETE |
| T2.1 — pg_isready loop | `smoke.sh` orchestrates the wait | `smoke.sh` exits 0 (loop completes without timeout) | Loop outputs are visible in smoke.sh stdout | ✓ COMPLETE |
| T2.2 — CREATE EXTENSION vector | `smoke.sh` heredoc block | Output: `CREATE EXTENSION` + `0.025368153802923787` in smoke.sh stdout | SQL output echoed by psql | ✓ COMPLETE |
| T2.3 — exit code validation | `set -euo pipefail` + `ON_ERROR_STOP=1` | `echo "SMOKE PASSED"` printed only on success; `EXIT: 0` confirmed | Exit code observable by CI (`bash smoke.sh; echo $?`) | ✓ COMPLETE |
| T3.1 — ADR 0001 | Referenced in CLAUDE.md + ADR itself | `test -f docs/adr/0001-no-engine-fork.md` exits 0 | File present in repo (git-tracked) | ✓ COMPLETE |
| T3.2 — CHANGELOG | CHANGELOG.md `[Unreleased] ### Added` | 3 entries verified in `CHANGELOG.md` lines 17–19 | CHANGELOG is the public contract signal | ✓ COMPLETE |
| T4.1 — DoD validation | This document + evidence below | Full DoD matrix validated (see § Evidence) | Implementation log committed to `develop` | ✓ COMPLETE |

---

## DoD Evidence (T4.1)

Execution timestamp: **2026-06-26T14:56:51-03:00**  
Host: `ubuntu-linux-x86_64`  
Image built: `theo-db:dev` (sha256:`1d883fb8...`)  
Container: `theo-dod-t41` (port `127.0.0.1:5436:5432`)

### DoD-1: Container builds and accepts PostgreSQL wire connection

```
docker build → EXIT 0
docker run -p 5436:5432 → Container: 2046ca12ae51...
Health probe [3]: status: healthy
psql -c "SELECT version();" → EXIT 0

PostgreSQL 17.10 (Debian 17.10-1.pgdg12+1) on x86_64-pc-linux-gnu,
compiled by gcc (Debian 12.2.0-14+deb12u1) 12.2.0, 64-bit
```

**PASS**

### DoD-2: CREATE EXTENSION vector + <=> similarity query in automated smoke test

```
$ PGHOST=127.0.0.1 PGPORT=5436 PGUSER=postgres PGPASSWORD=postgres bash smoke.sh

CREATE EXTENSION
       ?column?
----------------------
 0.025368153802923787
(1 row)

SMOKE PASSED
EXIT: 0
```

Cosine distance `[1,2,3] <=> [4,5,6]` = `0.025368153802923787`. **PASS**

### DoD-3: ADR "sem fork do engine PostgreSQL" in docs/adr/

```
$ test -f docs/adr/0001-no-engine-fork.md && echo PASS
PASS

$ head -2 docs/adr/0001-no-engine-fork.md
# ADR 0001 — Sem fork do engine PostgreSQL
**Status:** Accepted
```

**PASS**

---

## Phase-by-Phase Summary

### Phase 1 — Container infrastructure (T1.1, T1.2) — commit `5ea6d67`

- **T1.1 (Dockerfile):** RED confirmed (no Dockerfile → `docker build` exits 1). GREEN: wrote `Dockerfile` from reference at `.claude/knowledge-base/references/pgvector/Dockerfile` with `make OPTFLAGS=""` (portable binary) and `apt-mark hold locales` (blocks ~100MB perl pull). REFACTOR: none needed (minimal Dockerfile). WIRING: `docker build` caller + build-exit-0 integration assertion.
- **T1.2 (HEALTHCHECK):** Added `HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 CMD pg_isready -h localhost -p 5432 -U postgres -q` in same commit. Runtime metric: `docker inspect` exposes health status externally.

### Phase 2 — Smoke test (T2.1, T2.2, T2.3) — commit `ef532c2`

- **T2.1 (pg_isready loop):** RED: `smoke.sh` absent → `bash smoke.sh` exits 127. GREEN: wrote `#!/usr/bin/env bash set -euo pipefail` + 10-retry `pg_isready` loop.
- **T2.2 (CREATE EXTENSION + EC-1):** `CREATE EXTENSION IF NOT EXISTS vector;` only in `.sh` heredoc (EC-1 constraint: never in `.sql` file). `SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;`.
- **T2.3 (exit code):** `psql -v ON_ERROR_STOP=1` ensures SQL errors propagate. `echo "SMOKE PASSED"` as oracle (last line of script, guarded by `set -e`).

### Phase 3 — Documentation (T3.1, T3.2) — commit `0db4d60`

- **T3.1 (ADR 0001):** Three alternatives evaluated: A1 (extension model — adopted), A2 (engine fork — rejected: violates CLAUDE.md Rule 3 + wire-compat break + 4×/year rebase cost), A3 (scratch — rejected: YAGNI + years of work + risk of incompatible clone). AlloyDB SOTA anchoring: AlloyDB uses the same extension mechanism for ScaNN.
- **T3.2 (CHANGELOG):** 3 entries added under `[Unreleased] ### Added` covering Dockerfile, smoke.sh, and ADR.

### Phase 4 — End-to-end validation (T4.1) — this commit

- All 3 DoDs empirically validated (evidence above). Progress checkpoint and implementation log written.

---

## Parsimony Ladder Compliance

Per `rules/parsimony-ladder.md § The ladder`:

| Phase | Rung hit | Justification |
|-------|----------|---------------|
| T1.1 GREEN | Rung 3 (native platform feature) | Used official `postgres:17-bookworm` base image — no from-scratch OS needed |
| T1.1 GREEN | Rung 4 (already installed dep) | `pgvector v0.8.3` built from upstream git tag — no reimplementation |
| T2.1–T2.3 GREEN | Rung 2 (stdlib) | `pg_isready` and `psql` are bundled PostgreSQL tools — no external scripting library added |
| T3.1 GREEN | Rung 6 (minimum that works) | ADR is a markdown file — no template engine, no tooling overhead |

No parsimony arguments used to skip tests, validation, error handling, or security.

---

## Constraints respected

- **EC-1 (critical):** `CREATE EXTENSION` appears ONLY in `smoke.sh` heredoc. No `.sql` file in the repo contains `CREATE EXTENSION`. Verified: `grep -r "CREATE EXTENSION" .` → only `smoke.sh`.
- **License gate (D1):** postgres:17-bookworm (PostgreSQL License) + pgvector v0.8.3 (Apache 2.0). No AGPL dependency.
- **No engine fork (TheoDB Rule 3 / ADR 0001):** engine PostgreSQL unmodified. Extension only.
- **Wire compatibility (TheoDB Rule 6):** `postgres:17-bookworm` base ensures 100% wire compatibility.
- **Commits without Co-Authored-By** (project policy): confirmed in all 3 implementation commits.

---

## Cross-references

- Plan: `.claude/knowledge-base/plans/m0-walking-skeleton-plan.md`
- Plan-confidence: `.claude/knowledge-base/audits/m0-walking-skeleton-plan-confidence-2026-06-26.md` (SHIPPABLE 93/100)
- Deps-audit: `.claude/knowledge-base/audits/m0-walking-skeleton-deps-audit-2026-06-26.md` (PASS_WITH_CAVEATS 89/100)
- Cycle contract: `.claude/rules/cycle-implement.md`
- Upstream blueprint: `.claude/knowledge-base/discoveries/blueprints/m0-walking-skeleton-blueprint.md`
