---
slug: m3-minimal-migration
created_at: 2026-06-27
goal: Provar e documentar a migração mínima vanilla-PostgreSQL→TheoDB preservando dados e índices vetoriais.
---

# Plan — M3 Minimal Migration (vanilla PostgreSQL → TheoDB)

## Goal

Entregar um **smoke test automatizado e reprodutível** + um **guia** que provam a migração de um banco
PostgreSQL+pgvector vanilla para o TheoDB via `pg_dump`/`pg_restore` padrão, measured by `bash
migrate-smoke.sh` sair com exit 0 após asserir: checksum de dados idêntico source↔target, os 4 índices
(hnsw/ivfflat/btree×2) preservados, e o índice HNSW usado por uma query ANN no target.

## Context

ROADMAP M3 (dependency M2 ✅ released v0.2.0). Wire-compatibilidade com PostgreSQL é gate do produto, então
a migração usa tooling **padrão** (`pg_dump`/`pg_restore`) — Regra 9 (não reinventar). A mecânica foi provada
empiricamente nesta sessão (ver `## Prior Art`): ambos os formatos (custom `-Fc` e plain `| psql`) preservam
dados bit-exato (checksum `227de9ac…`) e os índices vetoriais (hnsw + ivfflat), usáveis pós-restore.

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Razão de existir |
|---|---|---|---|
| `smoke.sh` | 25 (`wc -l`) | M0 (`a35e5a7`-era) | smoke M0: `pg_isready` + `CREATE EXTENSION vector` + `<=>` — template bash a espelhar |
| `.github/workflows/ci.yml` | 110 (`wc -l`) | M2 (`926160e`) | CI: jobs `harness-unit` + `image-and-bench` — onde o job de migração entra |
| `migrate-smoke.sh` | 0 (NEW) | — | smoke de migração (DoD-1/DoD-2) |
| `docs/migration/minimal-migration.md` | 0 (NEW) | — | guia (DoD-3) |

### Current callers / dependents

- `smoke.sh` é invocado pelo job `image-and-bench` do CI (`ci.yml`) e manualmente (`PGPORT=… bash smoke.sh`).
  Nenhum código importa esses scripts (são executáveis de orquestração) — não há símbolo público a quebrar.
- `migrate-smoke.sh` (NEW) será invocado pelo novo job `migration-smoke` (T3) e manualmente.

### Domain glossary

- **vanilla PostgreSQL+pgvector:** Postgres stock com a extensão `vector` (imagem `pgvector/pgvector:pg17`) — a origem da migração.
- **integrity oracle:** `md5(string_agg(embedding::text, ',' ORDER BY id))` — hash determinístico do conteúdo vetorial; igualdade source↔target prova preservação bit-exata.
- **custom format / plain format:** `pg_dump -Fc` (binário, restaurável por `pg_restore`) vs `pg_dump` (SQL texto, aplicável por `psql`).

### Architecture boundaries affected

- Nenhuma fronteira de código (DIP/layering em `rules/architecture.md`) é tocada — a migração é orquestração
  de processos externos (`pg_dump`/`pg_restore`/`psql`) sobre dois containers. Sem novos imports, sem deps novas.
- Imagens: source `pgvector/pgvector:pg17`, target `theo-db:dev` (M0+M2). Ambos pgvector 0.8.3 (medido).

## Prior Art & Related Work

- Blueprint `.claude/knowledge-base/discoveries/blueprints/m3-minimal-migration-blueprint.md` (cycle-discover,
  empírico) — documenta os 4 cantos + edge cases, todos com probe real.
- `smoke.sh` (M0) — o template de smoke bash do projeto.
- pgvector reference `.claude/knowledge-base/references/pgvector/`.

## ADRs

### ADR-1 — Standard `pg_dump`/`pg_restore`, sem ferramenta própria

**Decisão:** usar o tooling padrão do PostgreSQL para a migração.
**Rationale:** TheoDB é wire-compatible (Regra 9 / `CLAUDE.md`); o caminho padrão preserva dados+índices
(provado empiricamente). **Alternativas rejeitadas:** (a) utilitário de dump próprio — reinventa o Postgres,
mais código para manter; (b) replicação lógica — pesada demais para migração one-shot mínima, fora do escopo M3.

### ADR-2 — Smoke em bash (não pytest)

**Decisão:** `migrate-smoke.sh` em bash, espelhando `smoke.sh`.
**Rationale:** a migração é orquestração de `pg_dump`/`pg_restore`/`psql` (processos), não lógica de domínio
Python; bash é o idioma nativo e zero-dependência (parsimony rung 2). **Alternativa rejeitada:** pytest no
pacote `benchmarks` — acopla migração ao harness de benchmark (SRP) e adiciona deps sem ganho.

## Coverage Matrix

| # | Requisito (DoD) | Task |
|---|---|---|
| 1 | `pg_dump`/`pg_restore` documentado e testado contra Postgres vanilla | T1 (smoke) + T2 (guia) |
| 2 | Migração de banco vanilla com tabela vetorial preserva dados e índices num smoke | T1 |
| 3 | Guia de migração mínima publicado | T2 |
| 4 | Smoke reprodutível em CI | T3 |

## Phase 1 — Migration smoke + guide

### Task T1 — `migrate-smoke.sh` (DoD-1/DoD-2)

#### Why this step
Ação: criar um script bash que orquestra a migração end-to-end (seed source → dump → restore target →
asserts) e falha alto se qualquer oráculo divergir. Razão: é a prova executável + reprodutível do DoD-2
(dados+índices preservados); espelha `smoke.sh` (Baseline Context) e o ADR-2.

#### Files to edit
- `migrate-smoke.sh` (NEW) — orquestração + asserts; ≤ 120 linhas.

#### TDD
- `test_migrate_smoke_asserts_checksum`: rodar o smoke contra um restore **corrompido** (1 linha alterada no
  target) DEVE sair ≠ 0 com mensagem clara — prova que o assert de checksum não é teatro. Given um target com
  dados divergentes, When `migrate-smoke.sh`, Then exit≠0 e "data checksum mismatch".
- Caminho feliz: Given source vanilla seedado, When `migrate-smoke.sh`, Then exit 0 + "MIGRATION SMOKE PASSED",
  checksum source==target, 4 índices no target, `Index Scan using items_hnsw` presente.

#### Acceptance criteria
- Pass: `bash migrate-smoke.sh; echo $?` imprime `0` e a saída contém a string `MIGRATION SMOKE PASSED`.
- Pass: o assert de checksum não é teatro — rodar `migrate-smoke.sh` com 1 linha corrompida no target sai com código `≠0` e imprime `data checksum mismatch` (verificado pelo TDD `test_migrate_smoke_asserts_checksum`).
- Pass: `psql target -tAc "SELECT count(*) FROM pg_indexes WHERE tablename='items'"` retorna `4`, e `EXPLAIN (COSTS OFF) SELECT id FROM items ORDER BY embedding <-> '[…]' LIMIT 5` (com ivfflat removido, `enable_seqscan=off`) contém `Index Scan using items_hnsw`.

#### Failure scenarios (external I/O — pg_dump/pg_restore + DB)
- Restore error (versão de extensão incompatível): o script deve falhar alto com o stderr do `pg_restore`
  (não engolir). Reproduz: documentado; o smoke usa versões alinhadas (0.8.3==0.8.3).
- Checksum mismatch (corrupção/perda): assert dedicado → exit≠0 (testado no TDD acima).
- DB indisponível: `pg_isready` gate antes de operar (como `smoke.sh`).

#### Concurrency tests
(none — single-threaded orquestração sequencial de processos.)

### Task T2 — Guia de migração mínima (DoD-3)

#### Why this step
Ação: publicar `docs/migration/minimal-migration.md` com o procedimento passo-a-passo (ambos formatos),
pré-checks, verificação de integridade e troubleshooting. Razão: DoD-3 exige guia publicado; ancora no
blueprint (Prior Art).

#### Files to edit
- `docs/migration/minimal-migration.md` (NEW).

#### TDD
- `test_guide_commands_are_real`: extrair os comandos `pg_dump`/`pg_restore` do guia e confirmar que casam
  com os do `migrate-smoke.sh` (o guia não diverge do que é testado). Given o guia, When grep dos comandos,
  Then cada comando aparece no smoke (consistência doc↔teste).

#### Acceptance criteria
- Pass: `grep -Ec "pg_dump -Fc|pg_dump.*\| psql|extversion|md5\(string_agg|USING diskann" docs/migration/minimal-migration.md` retorna `≥5` (ambos formatos + pré-check de versão + oráculo de checksum + passo diskann pós-migração).
- Pass: `grep -Ec "mismatch|escala|ownership|--no-owner" docs/migration/minimal-migration.md` retorna `≥3` (os 3 riscos documentados).
- Pass: o TDD `test_guide_commands_are_real` sai `0` — cada comando `pg_dump`/`pg_restore` citado no guia aparece literalmente em `migrate-smoke.sh`.

### Task T3 — Wire migrate-smoke no CI (DoD-1 "testado")

#### Why this step
Ação: adicionar um job ao `.github/workflows/ci.yml` que sobe source+target e roda `migrate-smoke.sh`.
Razão: "documentado e **testado**" (DoD-1) é provado de forma contínua em CI, não só localmente.

#### Files to edit
- `.github/workflows/ci.yml` (job `migration-smoke`).

#### TDD
- `test_ci_runs_migrate_smoke`: o YAML contém um job que invoca `migrate-smoke.sh` com source pgvector +
  target theo-db. (validação estrutural do YAML + step presente.)

#### Acceptance criteria
- Pass: `python3 -c "import yaml,sys; w=yaml.safe_load(open('.github/workflows/ci.yml')); sys.exit(0 if 'migration-smoke' in w['jobs'] else 1)"` sai `0`.
- Pass: `grep -c "migrate-smoke.sh" .github/workflows/ci.yml` retorna `≥1` (o job invoca o smoke).

#### Concurrency tests
(none — single-threaded.)

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Mismatch de versão de extensão (source > target) — restore pode referenciar tipos/opclasses ausentes → erro | MED | pré-check de `extversion` no guia; o smoke usa versões alinhadas (0.8.3==0.8.3) | impl |
| Datasets grandes / restore plain single-statement — `COPY` + rebuild de índice pode bloquear/demorar | LOW | custom format + `pg_restore -j N` (paralelo); documentado, não automatizado (M3 é mínimo) | impl |
| Roles/ownership/ACL — roles do source ausentes no target geram erro de ownership no restore | LOW | `--no-owner` (+ `--no-acl` quando necessário) | impl |

## Unresolved Questions

- (none — every decision is resolved at plan time) — escopo mínimo provado empiricamente; diskann-pós-migração e streaming-em-escala são future work explícito, fora do M3.

## Failure scenarios

- **pg_restore (versão incompatível):** falha alto com stderr do `pg_restore`; o smoke não engole erro
  (`set -euo pipefail`). Reproduz: versões desalinhadas (documentado).
- **DB indisponível:** `pg_isready` gate antes de qualquer operação.
- **Corrupção/perda de dados:** assert de checksum dedicado → exit≠0 (TDD T1).

## Global DoD

- `bash migrate-smoke.sh` → exit 0, `MIGRATION SMOKE PASSED`, com asserts reais (checksum + índices + uso HNSW).
- Guia publicado em `docs/migration/minimal-migration.md`, comandos consistentes com o smoke.
- Job de CI roda o smoke (validado localmente; live no push).
- CHANGELOG `[Unreleased]` atualizado. Arquivos ≤ 500 linhas. Sem deps novas.

## Final Phase — Integration Validation

- Rodar `migrate-smoke.sh` end-to-end (source+target reais) → PASSED.
- Rodar o teste de corrupção → exit≠0 (assert não é teatro).
- `git status` limpo; review multi-agente READY_TO_MERGE.
