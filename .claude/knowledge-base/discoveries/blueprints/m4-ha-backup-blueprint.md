# Blueprint — M4 Operação básica (HA Patroni + backup/PITR pgBackRest)

**Version:** 1.0 · **Date:** 2026-06-28 · **Slug:** m4-ha-backup · **Cycle:** discover
**Method:** prior-art research over the cloned `patroni` (4.1.3) + `pgbackrest` (2.59) references (+ cloudnative-pg
for RTO goalposts), distilled into an actionable wiring blueprint. Every claim cites a reference path.
**Bet (ADR 0002):** classic Patroni/pgBackRest HA — battle-tested, runs anywhere (OSS/on-prem) — NOT a copy of
AlloyDB's disaggregated storage (that would be Opção β, reopening D1/D2/D7).

## Context

M4 requires **primary + standby + automatic failover with a measured RTO** (Patroni) and **continuous backup +
PITR with a validated restore** (pgBackRest), runnable via docker-compose locally (operação básica, not k8s —
that is M5). TheoDB nodes are `theo-db:dev` (postgres 17 + pgvector + pgvectorscale + plpython3). The reference
projects ship the pieces but **no combined Patroni+pgBackRest+TheoDB compose** — that integration is net-new.

## Coverage Corner 1 — Integration Tests (the oracles)

- **Failover smoke (DoD-1):** form the cluster → write rows to the primary → `docker kill` the primary
  container → poll `patronictl list` (or REST `GET /leader`) until a former replica holds the lock → measure
  wall-clock **RTO** (kill → new primary accepts writes) → assert RTO ≤ target AND the pre-kill rows are present
  on the new primary (replication preserved). The reference README only demonstrates *planned* `switchover`
  (`patroni/docker/README.md`); the kill-primary automatic-failover measurement is net-new.
- **PITR smoke (DoD-2):** `stanza-create` → `backup` (full) → insert a known row + capture `select
  current_timestamp` (with tz) → insert a "bad" row / drop a table AFTER the target → restore `--type=time
  --target=<ts> --target-action=promote` → assert the DB is at the target point (known row present, post-target
  change absent). Worked example: `pgbackrest/doc/xml/user-guide.xml` §pitr (2169-2340).
- **RTO goalpost:** cloudnative-pg targets primary-switch **< 10s** (`cloudnative-pg/docs/src/e2e.md:37`);
  Patroni defaults (`ttl=30`) give RTO ≈ 30s; tuned (`ttl=20,loop_wait=5,retry_timeout=5`) ≈ 15-20s. TheoDB M4
  declares a measured target and proves it — `UNBENCHMARKED` until the smoke runs.

## Coverage Corner 2 — Dependencies

- **Patroni 4.1.3** — MIT (`patroni/LICENSE:1`). Install `pip install 'patroni[etcd3]'` into `theo-db:dev`
  (`patroni/docs/installation.rst:80`).
- **pgBackRest 2.59** — MIT (`pgbackrest/LICENSE:1-4`). Install via PGDG `apt-get install pgbackrest`
  (parsimony: no source build).
- **etcd** — DCS; use the upstream `quay.io/coreos/etcd` image (Apache-2.0), **3 nodes** (odd quorum). All three
  licenses are D1-clean (no AGPL). DoD-3 satisfied.

## Coverage Corner 3 — Tools

- **Topology** (adapt `patroni/docker-compose.yml`): 3× etcd (`ETCD_INITIAL_CLUSTER` static bootstrap, client
  2379/peer 2380) + N× TheoDB-patroni nodes sharing `PATRONI_SCOPE` + `PATRONI_ETCD3_HOSTS`. Minimum for the DoD:
  3 etcd + 2 patroni; recommended 3+3 (so a standby remains after failover).
- **Operate:** `patronictl list` (leader/replica/lag table), `patronictl switchover|failover|pause|resume`, REST
  API on `:8008` (`GET /primary|/leader|/replica|/health`, `POST /switchover|/failover`) —
  `patroni/docs/{patronictl,rest_api}.rst`. Routing optional via haproxy (5000→primary, 5001→standby).
- **pgBackRest:** `pgbackrest --stanza=<scope> {stanza-create,check,backup,info,restore}`; local repo at
  `repo1-path` on a shared volume.

## Coverage Corner 4 — Techniques

- **Split-brain avoidance:** a node runs as primary only while it holds + renews the etcd leader lock; on lock-
  renewal failure it self-demotes to read-only before the lock expires (`patroni/docs/dcs_failsafe_mode.rst:9,20`).
  Requires **odd etcd quorum ≥ 3** (2 etcd is strictly worse than 1). Optional `failsafe_mode: true` keeps a
  primary up during a *pure DCS outage* only if it can reach ALL members.
- **RTO tuning (timing invariant):** `loop_wait + 2*retry_timeout <= ttl` (`patroni/docs/dynamic_configuration.rst:16-20`).
  Lower the triple together to shrink RTO; too low → false failovers.
- **RPO guard:** `maximum_lag_on_failover` excludes stale replicas from election; `synchronous_mode` for zero
  data loss (write-availability tradeoff). Prefer `switchover` over `failover` when healthy.
- **pgBackRest archiving via Patroni params:** put `archive_mode=on` + `archive_command='pgbackrest --stanza=…
  archive-push %p'` under Patroni `postgresql.parameters` so every node inherits it after failover (not in one
  node's postgresql.conf) — `patroni/postgres0.yml:86-88`, `patroni/docs/patroni_configuration.rst:209`.
- **Standby bootstrap from backup:** Patroni `create_replica_methods: [pgbackrest, basebackup]` clones new
  standbys from the repo (offloads the primary) — `patroni/docs/replica_bootstrap.rst:132-143`.
- **PITR target hygiene:** capture target via `select current_timestamp` WITH timezone; let pgBackRest
  auto-select the backup for `--type=time` (`pgbackrest/doc/xml/user-guide.xml:2185,2296-2300`).

## Drawbacks & Risks

- **Split-brain from wrong DCS quorum** — MED — mitigate: odd etcd ≥ 3; never 2 etcd; leader-lock model.
- **RPO loss on unplanned failover of a lagging replica** — MED — mitigate: `maximum_lag_on_failover`,
  `synchronous_mode`, prefer switchover.
- **archive_command lost after failover if set per-node** — LOW — mitigate: set under Patroni params.
- **PITR target wrong without timezone** — LOW — mitigate: `current_timestamp` with tz.

## Unresolved Questions

- (none — the minimal scope (3 etcd + 2 patroni + local pgBackRest repo, measured failover + validated PITR) is
  fully resolved by the references; synchronous_mode zero-RPO, haproxy routing, and pgbackrest-S3 are explicit
  future hardening, out of "operação básica".)

## ADRs

- **ADR-1 — Patroni + pgBackRest over the alternatives.** Rejected: (a) repmgr/manual failover — Patroni is the
  battle-tested DCS-arbitrated standard (no split-brain); (b) CloudNativePG — k8s-operator (that is M5, not local
  operação básica); (c) AlloyDB-style disaggregated storage — Opção β, reopens D1/D2/D7 (ADR 0002). Both deps are
  MIT (D1-clean) and run anywhere.
- **ADR-2 — etcd as DCS (3 nodes).** Rejected: Consul/ZooKeeper (heavier), single etcd (no fault tolerance),
  2 etcd (worse quorum than 1). 3 etcd is the standard odd quorum.

## References

- `.claude/knowledge-base/references/patroni/` — `docker-compose.yml`, `docker/README.md`, `postgres0.yml`,
  `docs/{dynamic_configuration,dcs_failsafe_mode,replica_bootstrap,patronictl,rest_api,installation}.rst`,
  `LICENSE`, `patroni/version.py`.
- `.claude/knowledge-base/references/pgbackrest/` — `doc/xml/user-guide.xml` (§pitr), `LICENSE`, `src/version.h`.
- `.claude/knowledge-base/references/cloudnative-pg/` — `docs/src/e2e.md` (RTO goalpost, inspiration only).
