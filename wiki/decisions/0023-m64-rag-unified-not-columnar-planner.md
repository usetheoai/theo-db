---
type: Decision
title: ADR 0023 — RAG-over-SQL: o retrieval é planner-integrado; o processamento colunar continua statement-level
description: Corrige uma premissa incorreta do critério de pronto — não existe plano híbrido row+colunar porque não existe planner único sobre as duas engines.
resource: git:f7c7b93:docs/adr/0023-m64-rag-unified-not-columnar-planner.md
tags: [adr, rag, planner, htap, honestidade, yagni, m64]
adr_id: "0023"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M64
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0023
    resource: git:f7c7b93:docs/adr/0023-m64-rag-unified-not-columnar-planner.md
    title: ADR 0023 — M64 RAG-over-SQL
    last_modified: 2026-07-09
---

Um ADR que **corrige o próprio critério de pronto** em vez de fingir cumpri-lo.

# O achado arquitetural

O critério pedia "join com agregação colunar, **planner-integrado**". Isso pressupõe um **plano
híbrido único** entre row-store e colunar — capacidade que depende de **um planner único
controlando ambos os mecanismos de armazenamento**.

O TheoDB, por construção, combina PostgreSQL (executor HNSW row-store) e DuckDB (OLAP sobre
Parquet) através de **duas engines independentes, com planners independentes e sem mecanismo de
planejamento conjunto**. O impedimento **não é falta de engenharia** — decorre da arquitetura de
duas engines.

Para evitar ambiguidade futura, três níveis de "unificado":

| Nível | Existe no TheoDB? |
|---|---|
| Uma única SQL enviada ao servidor | **sim** |
| Um único plano do planner PostgreSQL | **sim** |
| Um único plano envolvendo PostgreSQL **e** DuckDB | **não** — duas engines, dois planners |

"Planner-integrado" aqui refere-se aos dois primeiros níveis. O terceiro é o que **não existe**.

# Decisão D1 — entregar a query unificada row-store; documentar o colunar como dois statements

A query de referência é planner-integrada, row-store, com **uma** ida ao servidor:

```sql
WITH retrieved AS (
  SELECT id, content FROM t
  WHERE cat = $c ORDER BY emb <=> $q LIMIT k
)
SELECT string_agg(content, E'\n') FROM retrieved;
```

A perna colunar é documentada honestamente como **dois statements** — o retrieval mais o
`theodb.olap_sql()` que o cliente roda, reusando o padrão do
[ADR 0021](/decisions/0021-m62-htap-codegen-surface.md) —, **não** um plano híbrido.

Por que a cláusula literal é inalcançável: o [pg_duckdb](/technologies/pg-duckdb.md) proíbe
execução DuckDB dentro de função (medido); o índice `theodb_hnsw` é row-store; e o Parquet vive no
DuckDB. O SOTA que faz isso first-class — o colunar in-memory do
[AlloyDB](/technologies/alloydb.md), o TiFlash do TiDB — consegue porque tem **uma engine e um
planner** donos de ambos os stores.

E há um ponto de honestidade fino: numa agregação sobre apenas `k` linhas do retrieved-set **não há
trabalho analítico suficiente para justificar a participação da engine colunar**. Chamar isso de
"RAG colunar" seria desonesto.

**Rejeitadas:** fingir uma query colunar planner-integrada, e construir um custom scan unindo
row-store e Parquet num plano — nível PhD, exigiria reescrever o planner e pressupõe engine única.

# Decisão D2 — não construir `theodb.rag_query()`

Zero código de produção novo. O RAG unificado é um **padrão de query** que o usuário escreve; o
milestone entrega guia, benchmark e prova de correção. O precedente é o
[ADR 0022](/decisions/0022-m63-vector-join-lateral-not-node.md): SQL dinâmico num helper arriscaria
o pushdown, e a query varia por schema, então uma view fixa não generaliza.

# O valor real entregue

**Prova de correção:** a query composta recupera **exatamente** o top-k filtrado do oráculo exato —
compor não degrada o recall — e uma linha inserida na transação é recuperável na **mesma SQL e no
mesmo snapshot MVCC**. Nota de rigor registrada: um cliente app-layer também obtém read-your-writes
se abrir transação explícita; o diferencial não é a visibilidade em si, mas obtê-la **numa SQL
única, num snapshot único, sem coordenação adicional**.

**Benchmark unified contra app-layer** — o head-to-head "1 SQL contra N chamadas" que o campo não
publica ([m64](/benchmarks/m64-rag-over-sql.md)). A 5000 linhas, dim 128, k=10, 3 runs × 50 reps:
braço unificado com **1 round-trip** e p50 de **6,721 ms**, contra app-layer com **2 round-trips** e
p50 de **7,284 ms**, com gate de recall casado (Jaccard 1,0, mesmo top-k por construção).

**O que o benchmark NÃO mede:** não demonstra superioridade algorítmica de retrieval — ambos os
braços usam exatamente o mesmo top-k. Mede apenas a diferença **estrutural** entre compor in-SQL e
compor no cliente. O ganho de latência é modesto co-locado (~8%) e amplifica sobre rede real.

**Correção de honestidade aplicada:** a tabela do benchmark ganhou `PRIMARY KEY (id)` — sem ela, o
hydrate do braço app-layer faria seqscan de 5000 linhas, um straw-man que inflava o braço rival a
~5×. Com a PK, que toda tabela real tem, o gap cai para os 8% honestos.[^adr0023]

# Consequências

O RAG unificado é first-class **para retrieval**, com recall herdado e consistência transacional. A
perna colunar **não é planner-integrada**, e isso está dito. O rerank de segunda ordem fica para o
[ADR 0024](/decisions/0024-m65-ai-rerank-cross-encoder.md).

[^adr0023]: ADR 0023 — M64 RAG-over-SQL: unified retrieval é planner-integrado
