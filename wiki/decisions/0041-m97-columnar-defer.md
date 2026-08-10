---
type: Decision
title: ADR 0041 — DEFER de um novo pilar colunar; manter a rota pg_duckdb embarcada
description: Todo diferenciador colunar adotável está barrado por licença ou bloqueado por paradigma, e o valor colunar já é entregue — DEFER é o meio honesto entre GO e NO-GO.
resource: git:f7c7b93:docs/adr/0041-m97-columnar-defer.md
tags: [adr, columnar, htap, licenca, defer, m97]
adr_id: "0041"
adr_status: Proposed
decision_date: 2026-07-13
milestone: M97
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0041
    resource: git:f7c7b93:docs/adr/0041-m97-columnar-defer.md
    title: ADR 0041 — M97 DEFER columnar pillar
    last_modified: 2026-07-13
---

Um ciclo de investigação sem uma linha de código de produto, respondendo a uma pergunta de aposta:
construir um **novo** pilar colunar — além da superfície que já é embarcada — vale meses, sob o
portão de licença permissiva?

# Decisão: DEFER

Manter o [pg_duckdb](/technologies/pg-duckdb.md) (MIT) mais a superfície HTAP por codegen como a
resposta colunar permissiva. Adicionar um item de vigilância sobre a licença do moonlink.

# Racional

**O valor colunar é real E já entregue.** O benchmark de viabilidade
([m97](/benchmarks/m97-htap-viability.md)) mediu o DuckDB **15–23×** mais rápido que o row-store do
PG em agregações analíticas a 20M linhas — confirmando o valor. Mas esse valor **já é embarcado**,
via [ADR 0020](/decisions/0020-m61-embed-pgduckdb.md) e
[ADR 0021](/decisions/0021-m62-htap-codegen-surface.md), com ~31× de OLAP medido.

**Todo diferenciador de "ir além" está barrado ou bloqueado**, com licenças verificadas:

| Diferenciador | Situação |
|---|---|
| Sync automático row→colunar | o único peer que oferece tem o motor de sync sob **BSL 1.1 — barrado**; a casca MIT não é o valor, e o projeto é imaturo |
| Access method colunar in-Postgres | Hydra e Citus são ambos **AGPLv3 — barrados** |
| Roteamento row↔colunar num plano só | **bloqueado por paradigma** — duas engines, dois planners, como medido no [ADR 0023](/decisions/0023-m64-rag-unified-not-columnar-planner.md) |
| Vetor + analytics numa query | **já entregue** pela superfície de codegen |

**GO seria complexidade acidental** — meses reempacotando o que o pg_duckdb já dá, que é exatamente o
anti-pattern de criar abstração multi-engine tendo uma engine. **NO-GO seria forte demais** —
aposentaria uma capacidade real, entregue e medida. **DEFER é o meio honesto:** o espaço de desenho
permissivo está esgotado pelo que já embarca.[^adr0041]

# Consequências

Nenhum código de produto novo — o milestone entrega **conhecimento**. A superfície existente segue
como a resposta colunar permissiva.

**Gatilho de revisão:** reabrir se (i) o motor de sync for relicenciado para uma licença permissiva,
tornando o auto-sync obtenível, ou (ii) o reposicionamento do north star se estabilizar e for preciso
um segundo pilar **medido**.

Disciplina de posicionamento: "analytics colunar vetorizado sob demanda, aposta lakehouse" — nunca o
colunar in-memory automático do [AlloyDB](/technologies/alloydb.md).

# O que este ADR NÃO avaliou

Uma terceira opção que não estava na cédula: **construir do zero, como código próprio**. Ela foi
aberta e escolhida pelo [ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md), que supersede este
DEFER **apenas para o caminho own-code**. A proibição de *adotar* código AGPL ou BSL permanece
integralmente em vigor.

[^adr0041]: ADR 0041 — M97: DEFER a new Columnar/HTAP pillar
