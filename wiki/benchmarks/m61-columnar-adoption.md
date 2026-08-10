---
type: Measurement
title: m61 — adoção do pg_duckdb: ganho sobre Parquet, honest-negative sobre o heap
description: Mede duas superfícies na mesma máquina e recusa herdar o número de outro mecanismo — o ~9× é do que foi medido aqui, não do benchmark anterior.
resource: git:f7c7b93:docs/benchmarks/m61-columnar-adoption.md
tags: [benchmark, pg-duckdb, columnar, parquet, honest-negative, m61]
milestone: M61
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m61
    resource: git:f7c7b93:docs/benchmarks/m61-columnar-adoption.md
    title: M61 — adoção columnar/HTAP
---

**Veredito:** a peça foi **embarcada com sucesso**, e o ganho analítico **materializa sobre dados em
formato colunar (Parquet): ~9× a 5M** — **não** sobre o heap row-store, onde o resultado é
honest-negative.

# A recusa que define o artefato

> O número é da superfície medida **aqui**, **não herdado** do ganho anterior — **mecanismo diferente**.

O [m30](/benchmarks/m30-columnar-scale.md) medira ~14× com um **columnstore nativo espelhado**; esta
adoção usa **leitura de arquivos Parquet**. São caminhos distintos, e **transportar o número de um para o
outro seria fabricação por associação** — o tipo de erro que passa despercebido porque os dois "são
colunar".

# As duas superfícies medidas

Ambas na **mesma máquina**, com ≥3 runs, média e desvio, aquecimento descartado e **correção casada**:

- **sobre o heap**, forçando o executor alternativo: **honest-negative** — ler dados em formato de linha
  por um motor colunar adiciona overhead;
- **sobre Parquet**: **~9× a 5M**, crescendo com a escala.

# A consequência para o produto

**O valor entregue é analytics colunar sobre arquivos**, não um acelerador transparente do heap. Isso
está registrado no [ADR 0020](/decisions/0020-m61-embed-pgduckdb.md) e determinou o posicionamento da
capacidade como aposta de lakehouse.

A superfície construída sobre esta adoção é o [m62](/benchmarks/m62-htap.md); e a rota inteira acabou
substituída por [código próprio](/features/15-lakehouse-parquet.md), com o componente externo **removido**
([ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md)).
