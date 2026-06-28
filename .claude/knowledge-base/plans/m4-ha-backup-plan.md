---
slug: m4-ha-backup
created_at: 2026-06-28
goal: Entregar HA Patroni (primary+standby+failover automático com RTO medido) + backup/PITR pgBackRest com restore validado, provados por smokes reprodutíveis.
---

# Plan — M4 Operação básica (HA Patroni + backup/PITR pgBackRest)

## Goal

Entregar e provar HA + backup para o TheoDB, measured by `bash ha/failover-smoke.sh` saindo 0 com **RTO medido
≤ 30s** (failover automático preserva dados) E `bash ha/pitr-smoke.sh` saindo 0 com um **restore PITR validado**
(linha-alvo presente, mudança pós-alvo ausente), ambos contra um cluster Patroni real (3 etcd + 2 nós TheoDB).

## Context

ROADMAP M4 (dependency M1 ✅; M2 não-bloqueante). Aposta deliberada (ADR 0002): HA clássica Patroni/pgBackRest
(battle-tested, OSS/on-prem), não o storage desagregado do AlloyDB. Mecânica e licenças (MIT/MIT) levantadas no
blueprint `.claude/knowledge-base/discoveries/blueprints/m4-ha-backup-blueprint.md` (Patroni 4.1.3, pgBackRest 2.59).

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Razão de existir |
|---|---|---|
| `Dockerfile` | 63 (`wc -l`) | imagem TheoDB base (M0-M3); o HA estende-a |
| `ha/Dockerfile.ha` | 0 (NEW) | imagem `theo-db-ha` = theo-db:dev + patroni[etcd3] + pgbackrest |
| `ha/docker-compose.ha.yml` | 0 (NEW) | topologia 3 etcd + 2 nós TheoDB-patroni + volume do repo pgBackRest |
| `ha/patroni-entrypoint.sh` | 0 (NEW) | gera patroni.yml por-nó (env) + sobe patroni |
| `ha/pgbackrest.conf` | 0 (NEW) | stanza local do pgBackRest |
| `ha/failover-smoke.sh` | 0 (NEW) | smoke de failover com RTO medido (DoD-1) |
| `ha/pitr-smoke.sh` | 0 (NEW) | smoke de backup+PITR restore validado (DoD-2) |
| `docs/operations/ha-backup-runbook.md` | 0 (NEW) | runbook + licenças (DoD-3) |

### Current callers / dependents

- Scripts de orquestração (sem símbolos importados). `ha/failover-smoke.sh` e `ha/pitr-smoke.sh` são invocados
  manualmente e pelo CI (job `ha-smoke`). `Dockerfile.ha` faz `FROM theo-db:dev`.

### Domain glossary

- **DCS:** Distributed Configuration Store (etcd) — guarda o leader lock; árbitro do failover (anti-split-brain).
- **RTO:** tempo do kill do primary até um novo primary aceitar escritas (failover).
- **PITR:** Point-In-Time Recovery — restaurar a um instante via WAL replay (pgBackRest `--type=time`).
- **stanza:** namespace de configuração/repo do pgBackRest para um cluster.

### Architecture boundaries affected

- Nenhuma fronteira de código (`rules/architecture.md`) — é infraestrutura (compose + configs + scripts) sobre a
  imagem TheoDB. Sem novos imports no código. Deps novas (patroni, pgbackrest) são MIT (D1-clean).

## Prior Art & Related Work

- Blueprint `.claude/knowledge-base/discoveries/blueprints/m4-ha-backup-blueprint.md` (cycle-discover) — padrões Patroni/pgBackRest com citações.
- Referências: `patroni` (docker-compose.yml, docs), `pgbackrest` (user-guide §pitr), `cloudnative-pg` (RTO goalpost <10s).
- `smoke.sh` / `migrate-smoke.sh` — idioma de smoke bash do projeto (a espelhar).

## ADRs

### ADR-1 — Patroni + pgBackRest (MIT), etcd 3-node DCS

**Decisão:** HA via Patroni (leader-lock no etcd) + backup/PITR via pgBackRest; DCS = 3 etcd.
**Rationale:** battle-tested, roda em qualquer lugar (ADR 0002), ambos MIT (D1-clean). **Alternativas rejeitadas:**
repmgr/failover manual (sem árbitro → split-brain); CloudNativePG (operador k8s = M5, não operação básica local);
storage desagregado AlloyDB (Opção β — reabriria decisões de licença/columnar/control-plane já fechadas no PRD §15); 1 ou 2 etcd (sem quorum tolerante a falha).

### ADR-2 — archive_command via Patroni postgresql.parameters

**Decisão:** `archive_mode/archive_command` (pgBackRest) ficam nos `postgresql.parameters` do Patroni, não no
postgresql.conf de um nó. **Rationale:** sobrevivem ao failover (todo nó herda). **Alternativa rejeitada:** setar
por-nó → perde archiving após promote.

## Coverage Matrix

| # | Requisito (DoD) | Task |
|---|---|---|
| 1 | Primary+standby+failover automático com RTO medido (Patroni) | T1 (imagem) + T2 (compose/config) + T3 (failover smoke) |
| 2 | Backup contínuo + PITR + backups agendados, restore validado (pgBackRest) | T1 + T2 + T4 (pitr smoke) + T5 (runbook: cron) |
| 3 | Due-diligence de licença Patroni/pgBackRest permissiva | T5 (runbook § licenças, MIT/MIT confirmado dos LICENSE) |
| extra | Runbook de operação HA + backup/restore | T5 |

## Phase 1 — HA image + topology

### Task T1 — `theo-db-ha` image (patroni + pgbackrest)

#### Why this step
Ação: `ha/Dockerfile.ha` faz `FROM theo-db:dev` e instala `patroni[etcd3]` (pip) + `pgbackrest` (apt PGDG) +
entrypoint. Razão: os nós HA precisam do Postgres TheoDB (pgvector/pgvectorscale) gerenciado pelo Patroni, com
pgBackRest para archiving/PITR — base de T2/T3/T4.

#### Files to edit
- `ha/Dockerfile.ha` (NEW), `ha/patroni-entrypoint.sh` (NEW), `ha/pgbackrest.conf` (NEW).

#### TDD
- `test_ha_image_has_patroni_and_pgbackrest`: Given `theo-db-ha` buildada, When `docker run --entrypoint sh ... -c 'patroni --version && pgbackrest version'`, Then ambos respondem versão (exit 0).

#### Acceptance criteria
- Pass: `docker build -f ha/Dockerfile.ha -t theo-db-ha .` sai 0.
- Pass: `docker run --rm --entrypoint sh theo-db-ha -c 'patroni --version && pgbackrest version'` imprime as versões e sai 0.
- Pass: a imagem mantém pgvector (`docker run --rm --entrypoint sh theo-db-ha -c 'ls /usr/share/postgresql/17/extension/vector.control'` sai 0).

#### Concurrency tests
(none — single-threaded build/install; a concorrência real é exercida em T3.)

### Task T2 — `docker-compose.ha.yml` + Patroni config

#### Why this step
Ação: compose com 3 etcd (quorum) + 2 nós `theo-db-ha` (patroni) compartilhando `PATRONI_SCOPE`/`PATRONI_ETCD3_HOSTS`
e um volume para o repo pgBackRest; entrypoint gera `patroni.yml` com `bootstrap.dcs` tunado
(`ttl=20,loop_wait=5,retry_timeout=5` — invariante `loop_wait+2*retry_timeout≤ttl`), `archive_command` pgBackRest
nos `postgresql.parameters`, e `create_replica_methods`. Razão: a topologia é o substrato dos smokes.

#### Files to edit
- `ha/docker-compose.ha.yml` (NEW), `ha/patroni-entrypoint.sh` (estende T1).

#### TDD
- `test_cluster_forms_one_leader`: Given `docker compose -f ha/docker-compose.ha.yml up -d`, When `patronictl list` após readiness, Then exatamente 1 `Leader` + ≥1 `Replica` em `streaming` (replicação ativa).

#### Acceptance criteria
- Pass: `docker compose -f ha/docker-compose.ha.yml up -d` sobe 3 etcd + 2 patroni; após readiness, `patronictl list` mostra **1 Leader e 1 Replica streaming**.
- Pass: invariante de timing válido — `grep` no patroni.yml gerado confirma `loop_wait + 2*retry_timeout <= ttl`.
- Pass: escrita no Leader replica para a Replica (linha escrita no primary aparece na replica em `SELECT`).

#### Concurrency tests
- `#### Concurrency tests` — leader election é concorrente: o teste de formação do cluster (T2) + o de failover (T3) exercem a corrida de eleição via etcd; o invariante "exatamente 1 Leader" é a asserção race-aware (split-brain = 2 Leaders → falha).

## Phase 2 — Failover + PITR smokes

### Task T3 — `ha/failover-smoke.sh` (DoD-1, RTO medido)

#### Why this step
Ação: sobe o cluster → escreve N linhas no primary → `docker kill` no container do Leader → mede o **RTO**
(wall-clock até um ex-Replica virar Leader e aceitar escrita) → asseria RTO ≤ 30s, dados pré-kill presentes no
novo primary, e **exatamente 1 Leader** (sem split-brain). Razão: é a prova executável do DoD-1.

#### Files to edit
- `ha/failover-smoke.sh` (NEW).

#### TDD
- `test_failover_promotes_and_preserves_data`: Given cluster com dados no primary, When kill do Leader, Then dentro do RTO-alvo um ex-replica vira Leader, aceita escrita, e o checksum dos dados pré-kill bate (replicação preservou).
- Negative/race: `test_no_split_brain` — durante/após o failover, `patronictl list` nunca mostra 2 Leaders.

#### Acceptance criteria
- Pass: `bash ha/failover-smoke.sh` sai 0 e imprime `FAILOVER SMOKE PASSED — RTO=<n>s`.
- Pass: RTO medido (kill → nova escrita aceita) **≤ 30s**; falha se exceder.
- Pass: dados pré-kill presentes no novo primary (contagem/checksum); **nunca 2 Leaders** (assert anti-split-brain).

#### Failure scenarios (external I/O — etcd/DB/rede)
- Primary morto (kill): coberto (é o cenário). DCS (etcd) indisponível: documentado (failsafe) — fora do smoke mínimo.
- Replica atrasada: `maximum_lag_on_failover` exclui replicas stale da eleição (config T2).

#### Concurrency tests
- `#### Concurrency tests` — o kill-do-primary dispara eleição concorrente; asserções race-aware: RTO medido + "exatamente 1 Leader" pós-failover (prova ausência de split-brain) + dados preservados (sem perda na promoção).

### Task T4 — `ha/pitr-smoke.sh` (DoD-2, restore validado)

#### Why this step
Ação: no cluster, `pgbackrest stanza-create` + `check` → `backup --type=full` → insere linha-alvo + captura
`select current_timestamp` (com tz) → faz uma mudança pós-alvo (drop/insert) → restore `--type=time
--target=<ts> --target-action=promote` → asseria linha-alvo presente e mudança pós-alvo ausente. Razão: prova
executável do DoD-2 (PITR + restore validado).

#### Files to edit
- `ha/pitr-smoke.sh` (NEW).

#### TDD
- `test_pitr_restores_to_target`: Given backup + WAL archiving, When mudança após T e restore `--type=time` para T, Then estado == T (linha-alvo presente, mudança pós-T ausente).

#### Acceptance criteria
- Pass: `bash ha/pitr-smoke.sh` sai 0 e imprime `PITR SMOKE PASSED — restored to <ts>`.
- Pass: `pgbackrest --stanza=theodb info` lista ≥1 backup full; o restore promove e o estado bate com o alvo (linha-alvo presente, mudança pós-alvo ausente — asserts dedicados).

#### Failure scenarios (external I/O — pgBackRest/WAL)
- WAL não arquivado no desastre: PITR só replica o que chegou ao repo (documentado: `archive_timeout`).
- Alvo inalcançável/backup errado: pgBackRest auto-seleciona o backup p/ `--type=time`; falha alto se inconsistente.

#### Concurrency tests
(none — backup/restore são sequenciais; o archiving concorrente é exercido implicitamente pelo cluster ativo.)

## Phase 3 — Runbook + license + CI

### Task T5 — Runbook + due-diligence de licença (DoD-3)

#### Why this step
Ação: `docs/operations/ha-backup-runbook.md` — operação HA (formar cluster, switchover/failover, descobrir
líder), backup/PITR (stanza, backup, **cron de backups agendados**, restore), troubleshooting (split-brain,
RPO/RTO), e a **§ Licenças** confirmando Patroni MIT + pgBackRest MIT (citando os LICENSE). Razão: DoD-3 +
entregável runbook.

#### Files to edit
- `docs/operations/ha-backup-runbook.md` (NEW).

#### TDD
- `test_runbook_confirms_mit_licenses`: Given o runbook, When grep, Then cita "MIT" para Patroni e pgBackRest e os comandos do runbook (`patronictl`, `pgbackrest`) batem com os smokes.

#### Acceptance criteria
- Pass: `grep -Ec "Patroni.*MIT|MIT.*Patroni" docs/operations/ha-backup-runbook.md` ≥1 e idem pgBackRest.
- Pass: `grep -Ec "patronictl|pgbackrest|--type=time|cron" docs/operations/ha-backup-runbook.md` ≥4 (operação + agendamento documentados).

#### Concurrency tests
(none — documentação.)

### Task T6 — CI job `ha-smoke`

#### Why this step
Ação: job no `.github/workflows/ci.yml` que builda `theo-db-ha`, sobe o compose e roda failover-smoke + pitr-smoke.
Razão: "testado" contínuo (DoD-1/DoD-2) em CI, não só local.

#### Files to edit
- `.github/workflows/ci.yml` (job `ha-smoke`).

#### TDD
- `test_ci_has_ha_job`: Given o YAML, When parse, Then job `ha-smoke` existe e invoca `failover-smoke.sh` + `pitr-smoke.sh`.

#### Acceptance criteria
- Pass: `python3 -c "import yaml,sys; w=yaml.safe_load(open('.github/workflows/ci.yml')); sys.exit(0 if 'ha-smoke' in w['jobs'] else 1)"` sai 0.
- Pass: `grep -Ec "failover-smoke.sh|pitr-smoke.sh" .github/workflows/ci.yml` ≥2.

#### Concurrency tests
(none — config de CI; a concorrência real roda dentro dos smokes.)

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Split-brain por quorum DCS errado (2 etcd) ou config | MED | etcd ímpar ≥3; leader-lock; assert "exatamente 1 Leader" no smoke | impl |
| RTO/RPO não atingir meta | MED | tunar `ttl/loop_wait/retry_timeout` (invariante); `maximum_lag_on_failover`; medir no smoke | impl |
| `archive_command` perdido após failover | LOW | setar nos `postgresql.parameters` do Patroni (ADR-2) | impl |
| PITR a alvo errado (timezone) | LOW | capturar `current_timestamp` com tz; pgBackRest auto-seleciona backup | impl |

## Unresolved Questions

- (none — every decision is resolved at plan time) — escopo "operação básica" (3 etcd + 2 nós, repo local, failover medido + PITR validado); `synchronous_mode` zero-RPO, haproxy e pgbackrest-S3 são hardening futuro explícito.

## Failure scenarios

- **etcd (DCS) indisponível:** Patroni demote o primary (anti-split-brain) ou, com `failsafe_mode`, mantém se alcança todos os membros — documentado no runbook; fora do smoke mínimo.
- **pg primary morto:** failover automático (smoke T3 — RTO medido).
- **WAL não arquivado:** PITR só replica o que chegou ao repo (RPO) — documentado (`archive_timeout`).
- **Restore a alvo inconsistente:** pgBackRest falha alto; auto-seleção de backup para `--type=time`.

## Global DoD

- `bash ha/failover-smoke.sh` → exit 0, RTO medido ≤ 30s, dados preservados, 1 Leader (sem split-brain).
- `bash ha/pitr-smoke.sh` → exit 0, restore PITR validado (alvo presente, pós-alvo ausente).
- Runbook publicado com § licenças (Patroni MIT + pgBackRest MIT) + cron de backups agendados.
- CI job `ha-smoke` roda os dois smokes. CHANGELOG `[Unreleased]` atualizado. Arquivos ≤ 500 linhas.

## Final Phase — Integration Validation

- Subir o cluster real → failover-smoke (RTO medido) PASSED → pitr-smoke (restore validado) PASSED.
- `shellcheck` limpo nos scripts; YAML válido; `git status` limpo; review multi-agente READY_TO_MERGE.
