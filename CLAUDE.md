# CLAUDE.md — TheoDB

Instruções específicas deste projeto para o Claude Code. Complementam (não substituem)
as regras globais em `~/.claude/CLAUDE.md` e as convenções de ciclo em
`../.claude/rules/`. Quando houver conflito, a regra mais específica vence.

## Contexto

TheoDB é um banco de dados **open-source, PostgreSQL-compatível**, concorrente direto do
**AlloyDB** (nosso alvo SOTA), entregue como **edição downloadable** que roda em qualquer lugar.
Produto definido em [`PRD.md`](./PRD.md); visão e roadmap em [`README.md`](./README.md).
Decisões fechadas: PRD §15 (D1–D7).

---

## Princípio guia do projeto: Esforço ≠ Complexidade

> **Não medimos esforço. Medimos complexidade — e ela é medida pela necessidade do
> projeto, nunca pelo trabalho empregado.**

A complexidade de um sistema deve refletir a dificuldade **essencial** do problema que ele
resolve — não a quantidade de esforço que gastamos, não quão elaborado o código parece, não
quanto tempo investimos. São dois eixos independentes:

- **Esforço ALTO é bem-vindo** quando a necessidade do projeto justifica. Forkar uma extensão,
  escrever uma suíte de benchmark reproduzível, manter um CI de rebase contínuo, perseguir um
  ganho de recall — se há necessidade real e evidência, **não medimos esforço**.
- **Complexidade desnecessária é proibida** — sempre. Abstração prematura, generalização
  especulativa, camada de indireção sem valor, reimplementar o que já existe. Isso é
  complexidade *acidental* (auto-imposta), e nenhum montante de esforço a justifica.

### Complexidade essencial vs. acidental

| | Essencial (aceita) | Acidental (eliminada) |
|---|---|---|
| Origem | O problema/projeto exige | Nós nos impomos |
| Exemplo TheoDB ✅ | Integrar ScaNN-quality ANN ao planner do Postgres | Criar uma abstração "multi-engine" tendo só Postgres |
| Exemplo TheoDB ✅ | Manter fork de `pgvector` com CI de rebase (com benchmark provando o ganho) | Reescrever pgvector do zero "para ter controle" |
| Teste | Remover isso quebra um requisito real? | Remover isso só remove indireção? |

### Regras operacionais derivadas

1. **A decisão de FAZER vem da necessidade do projeto** — não de quão fácil ou difícil é.
   Nunca rejeite a coisa certa por dar trabalho; nunca faça a coisa errada por ser fácil.
2. **A decisão de COMO fazer é a mais simples que resolve** — independentemente do esforço.
   Caminhe a parsimony-ladder (`../.claude/rules/parsimony-ladder.md`) antes de escrever código.
3. **Esforço investido NUNCA justifica manter complexidade (anti-sunk-cost).** "Já gastei
   tempo nisso" não é razão para manter. Se o upstream alcançar nosso fork, desfazemos o fork (D3).
4. **Esforço NUNCA é métrica de valor.** PRs/commits não valem pelo tamanho. Diff menor que
   resolve o mesmo problema é melhor.
5. **Complexidade essencial nunca é cortada em nome de "simplicidade".** Testes, validação de
   borda, error handling, segurança e acessibilidade são essenciais — ver
   `../.claude/rules/parsimony-ladder.md § Never on the chopping block`.

---

## North Star — igualar ou superar o AlloyDB (Opção α)

> **Mandato do CTO (2026-06-27):** entregar um banco **igual ou superior ao AlloyDB**. Fonte de verdade
> LOCKED: [`docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`](./docs/adr/0002-north-star-equal-or-superior-to-alloydb.md).

**Como (Opção α):** igualar/superar o AlloyDB em **capacidades e resultados** para usuários
OSS/on-prem/model-agnostic; **vencer já hoje** em abertura, custo, portabilidade e independência de modelo;
buscar **superioridade de performance no pilar vetorial comprovada por benchmark** (`docs/benchmarks/`).

- **Measurement-first:** o harness de recall@k reproduzível é o **1º item de M2** e pré-requisito de qualquer
  claim de performance. Hoje não existe (tudo `UNBENCHMARKED`).
- **Fork é condicional** ao benchmark de gatilho (D3); não forkar antes de medir (anti-sunk-cost).
- **Columnar (lakehouse, D2) e HA (Patroni)** são apostas **diferentes e competitivas**, não cópias do
  AlloyDB — forçado pela licença permissiva (D1 barra AGPL). Paridade interna *literal* (Opção β) exigiria
  reabrir D1/D2/D7 — fora de escopo até novo ADR.
- **Esforço ≠ Complexidade:** esforço alto é bem-vindo (ScaNN-as-PG-AM, fork com CI de rebase, suíte de
  benchmark); o COMO é medir-depois-construir-o-essencial. Performance só vira claim com benchmark
  (`../.claude/rules/public-copy.md`).

---

## Regras específicas do TheoDB

1. **Ancore no SOTA AlloyDB.** Decisões de produto/arquitetura espelham o alvo (AlloyDB) e
   se afastam dele apenas com justificativa explícita (ex.: licença, OSS). Ver dossiê em `PRD.md`.
2. **Licença Apache 2.0; AGPL é proibida na distribuição (D1).** Nenhuma dependência AGPL
   entra no pacote. Só Apache 2.0 / MIT / BSD / PostgreSQL License. Due-diligence é gate de
   release (PRD §11; rodar `loop-check-licence`).
3. **Sem fork do engine PostgreSQL.** A regra "sem fork" vale para o engine. **Extensões**
   (`pgvector`/`pgvectorscale`) podem ser forkadas sob a **Política de Fork** (D3): upstream-first,
   gatilho por benchmark reproduzível, diff mínimo, CI de rebase, saída quando o upstream alcançar.
4. **Não reinvente.** Compomos sobre PostgreSQL + extensões maduras (Regra 9). Código próprio
   só onde nenhuma peça OSS permissiva resolve.
5. **Performance é claim, não opinião.** Nenhuma afirmação de performance ("Nx mais rápido")
   sem benchmark reproduzível e publicado em `docs/benchmarks/` (`../.claude/rules/public-copy.md`).
   Metas de design são marcadas como metas, não como fatos.
6. **100% wire-compatible com PostgreSQL é gate**, não feature opcional.
7. **Honestidade extrema (Regra 3).** Diga quando algo é incerto, quando um trade-off existe
   (ex.: nosso columnar é lakehouse/DuckDB, não in-memory como o AlloyDB — D2), e quando uma
   técnica ainda não foi validada.

---

## Fluxo de trabalho

- Branch de trabalho: **`develop`** (nunca `main` — Regra 4). `main` recebe só releases.
- Toda mudança visível entra no [`CHANGELOG.md`](./CHANGELOG.md) `[Unreleased]` (Regra 6).
- Commits **sem** trailer `Co-Authored-By` (política do projeto — `../.claude/rules/cycle-review.md`).
- Trabalho não-trivial passa pelos ciclos: `cycle-discover` → `cycle-plan` → `cycle-implement`
  → `cycle-code-quality` → `cycle-review` (ver `../.claude/rules/`).
- Decisões arquiteturais viram ADRs em `docs/adr/` quando a estrutura de engenharia existir
  (D1–D7 a promover).


## Integrating with the Theo platform (local live-test)

This is a module of the Theo platform. To develop it against the **live platform** and — if it
exposes an HTTP API — wire it into the unified edge as a capability (the way `theo-memory` is
`/v1/memory`), use the local-env umbrella in **theo-workspace**.

**Start here → `theo-workspace/docs/README.md`**
(repo `usetheodev/theo-workspace`; local sibling: `../../theo-cloud/theo-workspace`).

Read in order:

1. `docs/local-env-system-design.md` — where this module fits (a data-plane capability behind the Traefik edge; the engine is k8s-native and lives in the cloud).
2. `docs/ONBOARDING.md` — `make up-local` + `make memory-up` to bring the platform up and live-test in the browser. Per-role workflows + troubleshooting.
3. `docs/adding-a-microservice.md` — expose this module at the edge: **Level 1** (edge route, ~10 min, theo-workspace only) → **Level 2** (per-tenant Model B isolation, across theo-workspace + theo-cloud + this module's auth contract). **theo-memory is the reference implementation — copy it.**

Honest notes from the live validation (2026-06-25/26): run the control-plane with `make up-local`
(GHCR `:develop` is behind the current source); the LLM/provider is always the **real** API (no
mock); after editing `theo-workspace/dev/traefik/dynamic.yml` recreate traefik (single-file bind
mount + `watch:true` misses inode changes).
