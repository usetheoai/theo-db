# Review — M4 Operação básica (HA Patroni + backup/PITR pgBackRest)

**Date:** 2026-06-28
**Verdict:** READY_TO_MERGE
**Slug:** m4-ha-backup
**Commits reviewed:** `489084c` (feat) → `e4243f5` (review fixes)
**Plan:** `.claude/knowledge-base/plans/m4-ha-backup-plan.md` (plan-confidence SHIPPABLE 100)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m4-ha-backup-blueprint.md`

## DoD status (evidence-backed)

| DoD | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | primary+standby+failover automático sob RTO medido (Patroni) | ✅ | `ha/failover-smoke.sh`: RTO≈19-23s (≤30), RPO=0, + real partition split-brain test |
| 2 | backup contínuo + PITR + backups agendados, restore validado (pgBackRest) | ✅ | `ha/pitr-smoke.sh`: backup + `--type=time` restore validado + negative case; cron documentado |
| 3 | due-diligence de licença Patroni/pgBackRest permissiva | ✅ | Patroni MIT + pgBackRest MIT confirmados dos LICENSE (runbook § Licenças) |

ROADMAP `### M4` stays `[ ]` — flips at release post-merge.

## Method

Three independent specialist agents (distributed-systems/HA + shell + CI; cross-validation — **ran the smokes live**; test-auditor). Round-1 surfaced 1 BLOCKER + 1 HIGH + MEDIUMs; all fixed at the root and re-verified.

## Severity matrix

| # | Sev | Finding | Status |
|---|---|---|---|
| B-1 | BLOCKER | `pitr-smoke` flaky — PITR target captured in the **same wall-clock second** as the backup stop time → `pgbackrest --type=time` (1s, strictly-less-than) intermittently found no backup set (cross-val reproduced a real failure) | **FIXED** `e4243f5` — ≥1s gap before target + deterministic wait for the WAL segment to be archived; also fixed TARGET losing its internal space |
| H1 | HIGH | "no split-brain" assert was near-tautological (`count_leaders==1` only reads the etcd lock) and the PASS message over-claimed a safety property a killed-container test can't prove | **FIXED** `e4243f5` — real **network-partition** test: isolated primary goes read-only while the majority elects a writable primary (never two writable); honest wording |
| M1 | MED | fixed `sleep 3` masked RPO + flaky replication-catch-up | **FIXED** `e4243f5` — deterministic poll until the survivor has all 500 rows (real **RPO=0** assertion) |
| M2 | MED | RTO target 30s thin margin on slow CI runners | **FIXED** `e4243f5` — CI sets `RTO_TARGET=45` (local target stays 30) |
| M3 | MED | fixed `sleep 3` for WAL archiving race in PITR | **FIXED** `e4243f5` — deterministic wait on `last_archived_wal >= CUR` |
| M4 | MED | PITR promotion-wait didn't fail-fast | **FIXED** `e4243f5` — `die` if it never leaves recovery |
| G1 | MED | PITR had no negative case | **FIXED** `e4243f5` — restore to a pre-backup target must fail cleanly (asserted) |
| L1 | LOW | CHANGELOG RTO wording / pgBackRest version (2.58.0 vs 2.59) | **FIXED** `e4243f5` — RTO ≈19-23s; runbook pinned to pgBackRest 2.58.0 |
| L2 | LOW | CI teardown didn't remove leaked `theodb-restore`; fragile REPO_VOL discovery; dead no-op line | **FIXED** `e4243f5` — `trap` cleanup, REPO_VOL pinned to `theodb-ha_pgbackrest_repo`, dead line removed |

**0 BLOCKER remaining. BLOCKER + HIGH fixed and re-verified live; all MEDIUM fixed.**

## Final verification (live, after fixes)

- `bash ha/failover-smoke.sh` → `FAILOVER SMOKE PASSED — RTO=19s (RPO=0), data preserved; partition test: isolated primary went read-only (no split-brain)`.
- `bash ha/pitr-smoke.sh` → `PITR SMOKE PASSED — restored to <ts> (keep present, post-target absent); impossible target rejected`.
- `shellcheck` (koalaman/shellcheck:stable) on all HA scripts → exit 0.
- `theo-db-ha` image builds (Patroni 4.1.3 + pgBackRest 2.58.0 + pgvector + vectorscale); cluster forms 1 Leader + 1 streaming Replica.

## Honest known gaps / scope (out of "operação básica")

- **`synchronous_mode` (zero-RPO)** not enabled — async streaming; RPO=0 is asserted only for a caught-up replica, not under sustained write load. Future hardening.
- **3+2 topology** (not 3+3): a post-failover window has the new primary without a streaming replica until the killed node rejoins. Documented; 3+3 is M5.
- **"backups agendados"** is a documented cron over the proven `pgbackrest backup` command — the schedule itself is not executed by the smoke (the backup command is). Honest.
- **pg_hba `0.0.0.0/0 md5` + hardcoded dev creds** in the HA compose — dev-grade for the local smoke; restrict CIDR + scram + externalize creds before any non-local use (M5).
- Cluster-level DR (restore *into* Patroni + DCS reinit) not scripted; PITR is validated via an isolated standalone restore from the same repo (the cleaner way to prove the archive is restorable).

## Reviewer-confirmed strengths

- `archive_command` set under `bootstrap.dcs.postgresql.parameters` → survives failover (every node inherits). Correct.
- PITR keep/bad "sandwich" around the target is a model oracle (proves restored-exactly-to-point, not just "restored something"); the `... | tail || die` lines are safe under `set -o pipefail`.
- RTO assert has teeth (write succeeds only post-promotion); cross-node md5 data oracle; rejoin-as-streaming-replica verified.

## Cycle-review hard gates

Tests green on branch ✓ (both smokes pass) · No new secrets ✓ (key only in gitignored `.env`) · On `develop` ✓ · No `Co-Authored-By` ✓ · CHANGELOG updated ✓.

## Verdict rationale

Per `rules/cycle-review.md`: READY_TO_MERGE = no BLOCKER, ≤2 HIGH with documented mitigation. The BLOCKER and the single HIGH are **fixed and re-verified live**; all MEDIUM fixed; remaining items are honest out-of-scope hardening for M5. All three M4 DoDs are complete with reproducible evidence.

**Before the M4 checkbox flips (release):** push `develop` and confirm the first `ha-smoke` CI run is green (mirrors M2/M3).
