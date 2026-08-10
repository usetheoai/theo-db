---
type: Decision
title: ADR 0056 — Tier-out do pg_duckdb: imagem default enxuta e imagem htap opcional
description: O colunar próprio maduro tornou o pg_duckdb desnecessário no caminho default, então ele sai da imagem principal com um guard fail-closed que diz ao cliente qual imagem puxar.
resource: git:f7c7b93:docs/adr/0056-m142-pgduckdb-htap-tiering.md
tags: [adr, pg-duckdb, empacotamento, tiering, fail-closed, m142]
adr_id: "0056"
adr_status: Accepted
decision_date: 2026-07-22
milestone: M142
owner: human:paulohenriquevn
amends: ["0020"]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0056
    resource: git:f7c7b93:docs/adr/0056-m142-pgduckdb-htap-tiering.md
    title: ADR 0056 — Tier-out do pg_duckdb
    last_modified: 2026-07-22
---

Resolve o follow-up que o [ADR 0020](/decisions/0020-m61-embed-pgduckdb.md) deixara em aberto — o
tiering, "se o peso incomodar".

# O que mudou na justificativa

1. **O colunar próprio amadureceu.** O [TableAM próprio](/decisions/0042-m99-own-code-columnar-tam.md)
   entrega colunar transparente **dentro do banco**, sobre tabelas PostgreSQL vivas, com MVCC e
   pushdown — exatamente o terreno em que o [pg_duckdb](/technologies/pg-duckdb.md) medira
   **honest-negative** (0,63–0,89× sobre o heap).
2. **O pg_duckdb está fora do caminho quente AI-native.** Já fora provado que não há plano único
   entre as duas engines, e o retrieval é 100% PostgreSQL.
3. **O espaço colunar permissivo está esgotado**, conforme o
   [ADR 0041](/decisions/0041-m97-columnar-defer.md).

O valor **único** restante é lakehouse de **arquivos externos**. E ele é o **único componente C++** de
uma stack Rust mais PostgreSQL: +170 MB, `shared_preload_libraries` no boot, e superfície de SSRF via
httpfs. Manter isso no default que a maioria puxa é custo sem retorno para quem não usa lakehouse.

# Decisões

**D1 — guard fail-closed em runtime, não concatenação condicional da extensão.** As funções da
superfície apenas **constroem** statements, então criam-se normalmente sem o pg_duckdb. A extensão
continua **idêntica nas duas imagens** — sem version skew, com a cadeia de upgrade intacta — e as
funções que produzem statements ganham um guard que levanta erro tipado de "feature não suportada"
com a **dica de qual imagem puxar**. No default, o cliente recebe um erro claro com o próximo passo,
nunca um statement quebrado.

**D2 — a imagem htap é uma camada, não um fork.** Ela parte da imagem default e re-adiciona o
pg_duckdb, ficando **em sincronia por construção** e sem duplicar o build principal.

**D3 — compatibilidade sinalizada como mudança com marca de breaking**, e não como remoção: a
capacidade **não** some, continua opt-in; o que muda é a **superfície default**.[^adr0056]

# Alternativas rejeitadas

**Concatenação condicional**, com duas variantes da extensão — criaria version skew e complicaria o
upgrade. **Deixar as funções sem guard** — o cliente receberia um erro obscuro sobre uma função
inexistente. **Dockerfile htap auto-contido** — duplicaria o build e derivaria. **Remover a capacidade
de vez** — rejeitada pelo owner: ela vira opt-in, não desaparece.

# Validação medida

Provado ponta a ponta: as duas imagens com delta de pelo menos 150 MB, smoke do default asserindo a
**ausência** do pg_duckdb mais o guard levantando o erro correto, smoke do htap asserindo presença
mais o fluxo completo, e o teste de guard verde nas duas.

O CI passou a buildar as duas imagens para evitar drift.

# O que veio depois

Este tier-out foi **passo intermediário**: o [ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md)
removeu o pg_duckdb por completo e aposentou a imagem htap.

[^adr0056]: ADR 0056 — Tier-out do pg_duckdb
