# Runbook — HA (Patroni) + backup/PITR (pgBackRest)

Operação básica de alta disponibilidade e backup do TheoDB. **Aposta (ADR 0002):** HA clássica
Patroni/pgBackRest — battle-tested, roda em qualquer lugar (OSS/on-prem) — **não** o storage desagregado do
AlloyDB. Topologia: **3 etcd** (quorum ímpar, anti-split-brain) + **N nós TheoDB-Patroni** (primary + standby).

Tudo abaixo é provado por `ha/failover-smoke.sh` (failover automático com RTO medido) e `ha/pitr-smoke.sh`
(backup + PITR com restore validado), que rodam em CI.

## Subir o cluster

```bash
docker build -f ha/Dockerfile.ha -t theo-db-ha .
docker compose -f ha/docker-compose.ha.yml up -d
# status
docker exec theodb-patroni1 patronictl -c /tmp/patroni.yml list
```

`patronictl list` mostra `Leader` (running) + `Replica` (streaming). Conecte ao primary via o nó com
`GET /primary` → 200, ou simplesmente ao membro `Leader`.

## Failover & switchover

- **Descobrir o líder:** `patronictl list` (coluna Role) ou a REST API `GET :8008/primary` (200 só no primary),
  `GET :8008/leader`, `GET :8008/health`.
- **Switchover planejado (sem perda):** `patronictl switchover theodb --leader <atual> --candidate <novo> --force`.
- **Failover automático:** ao perder o primary, o Patroni promove um standby quando o leader-lock do etcd expira
  (`ttl=20s`). RTO medido pelo smoke ≈ **22s** (`ha/failover-smoke.sh`). Tuning em `ha/patroni-entrypoint.sh`
  (`ttl`/`loop_wait`/`retry_timeout` — invariante `loop_wait + 2*retry_timeout <= ttl`).
- **Manutenção:** `patronictl pause theodb` (desliga failover automático) / `patronictl resume theodb`.
- **Anti-split-brain:** um nó só é primary enquanto detém + renova o lock no etcd; ao falhar a renovação,
  auto-demote para read-only antes do lock expirar. **Nunca rode 2 etcd** (quorum pior que 1) — sempre ímpar ≥ 3.

## Backup contínuo + PITR (pgBackRest)

WAL archiving é contínuo via `archive_command='pgbackrest --stanza=theodb archive-push %p'` nos
`postgresql.parameters` gerenciados pelo Patroni (sobrevive ao failover — todo nó herda).

```bash
# inicialização (uma vez) + verificação de archiving
docker exec theodb-patroni1 pgbackrest --stanza=theodb stanza-create
docker exec theodb-patroni1 pgbackrest --stanza=theodb check
# backup full / diff / incr
docker exec theodb-patroni1 pgbackrest --stanza=theodb --type=full backup
docker exec theodb-patroni1 pgbackrest --stanza=theodb info
```

### Backups agendados (cron)

```cron
# full aos domingos 06:30; diff de seg–sáb 06:30 (no nó primary / sidecar de backup)
30 6 * * 0  pgbackrest --stanza=theodb --type=full backup
30 6 * * 1-6 pgbackrest --stanza=theodb --type=diff backup
```

### PITR — restaurar a um instante (restore validado)

```bash
# 1) capturar o alvo COM timezone (Postgres reckoning)
psql -U postgres -d postgres -tAc "SELECT current_timestamp;"
# 2) restaurar a um instante em uma instância (data_dir parado/vazio); pgBackRest auto-seleciona o backup
pgbackrest --stanza=theodb --type=time "--target=2026-06-28 11:01:04.065+00" \
  --target-action=promote --delta restore
# 3) iniciar o Postgres; ele replica o WAL até o alvo e promove. Validar o estado no ponto-alvo.
```

`ha/pitr-smoke.sh` automatiza isto end-to-end (backup → keep-row + target → mudança pós-alvo → restore
`--type=time` numa instância standalone → asserts: keep presente, pós-alvo ausente).

## Troubleshooting

| Sintoma | Causa | Ação |
|---|---|---|
| 2 nós como Leader (split-brain) | quorum etcd errado (2 nós) ou DCS particionado | usar 3 etcd; o leader-lock previne; investigar partição de rede |
| RTO acima da meta | `ttl` alto / detecção lenta | reduzir o trio `ttl/loop_wait/retry_timeout` (manter o invariante) |
| Replica não promove no failover | replica muito atrasada (`maximum_lag_on_failover`) ou etcd indisponível | checar lag em `patronictl list`; checar saúde do etcd |
| `archive-push` falhando | stanza não criada / repo sem permissão | `pgbackrest stanza-create`; checar dono de `repo1-path` |
| PITR "recovery ended before target" | alvo sem timezone / backup errado | capturar alvo com tz; deixar pgBackRest auto-selecionar o backup p/ `--type=time` |

## Licenças (due-diligence — DoD-3)

Confirmado permissivo (D1 — sem AGPL na distribuição):

- **Patroni 4.1.3 — The MIT License (MIT)** (`.claude/knowledge-base/references/patroni/LICENSE`).
- **pgBackRest 2.58/2.59 — The MIT License (MIT)** (`.claude/knowledge-base/references/pgbackrest/LICENSE`).
- **etcd — Apache-2.0** (DCS; imagem `quay.io/coreos/etcd`).

Todas Apache-2.0-compatíveis / permissivas → liberadas sob a política de licença do TheoDB (PRD §11).
