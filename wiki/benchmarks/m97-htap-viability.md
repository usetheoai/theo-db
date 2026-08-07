---
type: Measurement
title: m97 — viabilidade colunar: 15–23× em agregações analíticas
description: Confirma que o valor colunar é real e grande, o que torna a decisão de adiar um novo pilar mais interessante — ela não nega o valor, nega que haja espaço permissivo para capturá-lo melhor.
resource: git:f7c7b93:docs/benchmarks/m97-htap-viability.md
tags: [benchmark, columnar, viabilidade, clickbench, m97]
milestone: M97
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m97
    resource: git:f7c7b93:docs/benchmarks/m97-htap-viability.md
    title: M97 — Columnar/HTAP viability
    last_modified: 2026-07-13
---

Tabela analítica de 20M linhas, no idioma de um benchmark público conhecido, com os dois lados **sobre os
mesmos dados e na mesma máquina**.

# Resultado

| Query | row-store | colunar | ganho |
|---|---|---|---|
| agregação com agrupamento | 1786 ms | 82 ms | **21,8×** |
| agrupamento em duas colunas | 1671 ms | 72 ms | **23,2×** |
| contagem filtrada | 776 ms | 50 ms | **15,5×** |

**O valor colunar é real, grande e consistente.**

# Por que isso torna a decisão seguinte mais interessante

Este benchmark **fundamenta um DEFER**, não um GO — o
[ADR 0041](/decisions/0041-m97-columnar-defer.md).

A lógica: **o valor é real E já é entregue** pela peça embarcada. E **todo diferenciador de "ir além"
está barrado por licença ou bloqueado por paradigma** — as alternativas in-Postgres são AGPL, o motor de
sincronização automática é de licença restritiva, e roteamento num plano só é impossível com duas
engines.

**GO seria complexidade acidental** — meses reempacotando o que já existe. **NO-GO seria forte demais** —
aposentaria uma capacidade real e medida.

**Um benchmark que mede um ganho de 20× e conclui "não construa" é o oposto do padrão**, e só é
defensável porque a conclusão não é sobre o valor, e sim sobre **onde ele já está capturado e o que a
licença permite**.

O desfecho, aliás, contradisse a própria decisão de forma produtiva: o
[ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md) abriu a terceira opção que não estava na cédula
— **construir do zero como código próprio** — e foi por ali que o pilar acabou entregue.
