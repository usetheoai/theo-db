---
slug: m1-core-packaging
created_at: 2026-06-28
goal: Formalizar o M1 (Core + empacotamento) com evidência — suíte de regressão PG17 upstream 100%, extensões habilitáveis, zero AGPL.
---

# Plan — M1 Core + empacotamento

## Goal

Formalizar a distribuição PostgreSQL-compatível do TheoDB com evidência, measured by `docker run --rm
theo-db-regress` imprimir `All 225 tests passed` (suíte PG17.10 upstream 100%), `CREATE EXTENSION` habilitar
vector/vectorscale/plpython3u, e o sweep de licença confirmar **zero AGPL** no pacote.

## Context

ROADMAP M1 (dependency M0 ✅). Boa parte já foi shippada por M0/M2 (imagem + extensões); M1 a formaliza com
evidência reprodutível. ADR 0001 (no engine fork): o engine é o PGDG `postgresql-17` 17.10 inalterado — a
suíte upstream prova que o **empacotamento** não regrediu o SQL core.

## Baseline Context

### Files that will be touched

| Arquivo | Estado | Razão |
|---|---|---|
| `packaging/Dockerfile.regress` | (NEW) | runner `FROM theo-db:dev` que builda pg_regress+regress.so (REL_17_10) e roda installcheck |
| `packaging/run-regress.sh` | (NEW) | initdb + start + pg_regress contra a distribuição |
| `docs/packaging/packaging-and-tuning.md` | (NEW) | deliverable: extensões + tuning + relatório regressão + licença |
| `.github/workflows/ci.yml` | CI (4 jobs) | + job `pg-regression` |
| `Dockerfile` | 64 LoC | imagem base (não muda — só é a base do runner) |

### Current callers / dependents

- `packaging/run-regress.sh` é invocado pelo `ENTRYPOINT` do runner e pelo job CI `pg-regression`. Sem
  símbolos de código importados (orquestração).

### Domain glossary

- **installcheck:** rodar a suíte `src/test/regress` contra um servidor já em execução (vs `make check` que sobe um temporário).
- **pg_regress / regress.so:** o runner de testes do PostgreSQL + a lib C de funções de teste.
- **no engine fork (ADR 0001):** o binário do engine é o PGDG inalterado; só adicionamos extensões.

### Architecture boundaries affected

- Nenhuma fronteira de código. M1 é empacotamento + evidência (imagem/runner/docs). Deps do core permanecem permissivas (zero AGPL).

## Prior Art & Related Work

- Blueprint `.claude/knowledge-base/discoveries/blueprints/m1-core-packaging-blueprint.md` (cycle-discover).
- Referência: `.claude/knowledge-base/references/supabase-postgres/` (packaging de Postgres + extensões).
- Método: PostgreSQL upstream `src/test/regress` (`make installcheck`).

## ADRs

### ADR-1 — DoD-1 via `make installcheck`/pg_regress da fonte REL_17_10 contra a distribuição

**Decisão:** provar a compat upstream rodando `pg_regress` (fonte 17.10, flags Debian casadas) contra o engine
da distribuição num runner `FROM theo-db:dev`. **Alternativas rejeitadas:** (a) confiar no `make check` do
PGDG sem evidência local — a DoD pede relatório versionado na distribuição; (b) embarcar a árvore de testes na
imagem de produção — incha; o runner é uma imagem throwaway.

## Coverage Matrix

| # | Requisito (DoD) | Task |
|---|---|---|
| 1 | 100% dos testes de regressão PG17 upstream passam na distribuição | T1 (runner) |
| 2 | Extensões MVP pré-instaladas + habilitáveis via CREATE EXTENSION + tuning documentado | T2 (doc) + evidência |
| 3 | Due-diligence de licença (`loop-check-licence`); zero AGPL no pacote | T2 (doc § licença) + sweep |
| extra | Relatório regressão + relatório licença + doc de tuning | T2 |
| extra | Regressão rodando em CI | T3 |

## Phase 1 — Regression runner + packaging doc + CI

### Task T1 — `packaging/Dockerfile.regress` + `run-regress.sh` (DoD-1)

#### Why this step
Ação: runner `FROM theo-db:dev` que clona REL_17_10, configura com as flags Debian, builda libpq+pg_regress+regress.so,
e roda `pg_regress` contra um cluster TheoDB efêmero. Razão: é a prova executável + reprodutível do DoD-1.

#### Files to edit
- `packaging/Dockerfile.regress` (NEW), `packaging/run-regress.sh` (NEW).

#### TDD
- `test_regression_suite_100pct`: Given o runner buildado, When `docker run --rm theo-db-regress`, Then exit 0 e a saída contém `All 225 tests passed` (falha se qualquer teste regredir).

#### Acceptance criteria
- Pass: `docker build -f packaging/Dockerfile.regress -t theo-db-regress .` sai 0.
- Pass: `docker run --rm theo-db-regress` sai 0 e imprime `All 225 tests passed`.
- Pass: o runner é `FROM theo-db:dev` (engine sob teste = a distribuição) — `grep -c "FROM theo-db:dev" packaging/Dockerfile.regress` ≥ 1.

#### Concurrency tests
(none — pg_regress paraleliza internamente; o smoke é uma orquestração sequencial.)

### Task T2 — Packaging/tuning doc + license sweep (DoD-2/DoD-3)

#### Why this step
Ação: `docs/packaging/packaging-and-tuning.md` — extensões habilitáveis + tuning conjunto + relatório de
regressão (225/225) + § licença (zero AGPL: apt + 293 crates Rust). Razão: deliverables DoD-2/DoD-3.

#### Files to edit
- `docs/packaging/packaging-and-tuning.md` (NEW).

#### TDD
- `test_doc_covers_dods`: Given o doc, When grep, Then cobre as 4 extensões + "225 tests passed" + "zero AGPL".

#### Acceptance criteria
- Pass: `grep -Ec "vector|vectorscale|plpython3u|plpgsql" docs/packaging/packaging-and-tuning.md` ≥ 4.
- Pass: `grep -Ec "225 tests passed|zero AGPL|0 AGPL" docs/packaging/packaging-and-tuning.md` ≥ 2.
- Pass: o sweep real confirma 0 crates AGPL (`cargo metadata` sobre o pgvectorscale → 0 Affero/AGPL; apt scan → só falso-positivo ca-certificates).

#### Concurrency tests
(none — documentação.)

### Task T3 — CI job `pg-regression`

#### Why this step
Ação: job no `.github/workflows/ci.yml` que builda o runner e roda a suíte. Razão: "testado" contínuo (DoD-1).

#### Files to edit
- `.github/workflows/ci.yml` (job `pg-regression`).

#### TDD
- `test_ci_has_pg_regression`: Given o YAML, When parse, Then job `pg-regression` existe e roda o runner.

#### Acceptance criteria
- Pass: `python3 -c "import yaml,sys; w=yaml.safe_load(open('.github/workflows/ci.yml')); sys.exit(0 if 'pg-regression' in w['jobs'] else 1)"` sai 0.
- Pass: `grep -c "theo-db-regress" .github/workflows/ci.yml` ≥ 1.

#### Concurrency tests
(none — config CI.)

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Mismatch de flags de build/locale causa diffs de expected não-relacionados ao engine | MED | casar flags via feature surface Debian; suíte verde (225/225) confirma; documentar qualquer diff residual | impl |
| Custo de manter a suíte verde a cada bump de minor do PG | LOW | runner parametrizado por `PG_TAG`; roda em CI | impl |
| Sweep de licença desatualiza a cada bump de crate | LOW | re-rodar `cargo metadata` no bump; gate de release | impl |

## Unresolved Questions

- (none — every decision is resolved at plan time) — escopo: regressão core (parallel_schedule), extensões, licença; `make check-world`/TAP completos são hardening futuro.

## Failure scenarios

- **Build do runner falha (dep/flag):** falha alto no `docker build` (logs do configure/make); corrigido na fonte.
- **Algum teste de regressão falha:** `pg_regress` sai ≠0 + `regression.diffs` mostra o diff — investigar (engine vs env).
- **Crate AGPL aparece num bump:** o sweep `cargo metadata` detecta → bloqueia release (política de licença, PRD §11).

## Global DoD

- `docker run --rm theo-db-regress` → `All 225 tests passed` (exit 0).
- Extensões habilitáveis (vector/vectorscale/plpython3u) — evidenciado.
- Zero AGPL (apt + 293 crates Rust) — evidenciado.
- Doc de packaging/tuning publicado; CI job `pg-regression`. CHANGELOG atualizado.

## Final Phase — Integration Validation

- Build + run do runner → 225/225 PASSED ao vivo.
- Sweep de licença → 0 AGPL. Extensões → CREATE EXTENSION ok.
- `git status` limpo; review READY_TO_MERGE.
