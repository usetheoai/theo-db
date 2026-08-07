---
type: Decision
title: ADR 0013 — Pilares v1-legacy (columnar e BM25): MANTER como exceções permissivas
description: Columnar via pg_mooncake e BM25 via pg_textsearch permanecem como exceções explícitas ao mandato own-code, ancoradas em ganho medido a escala.
resource: git:f7c7b93:docs/adr/0013-v1-legacy-columnar-bm25-scope.md
tags: [adr, columnar, bm25, htap, licenca, m30, own-code]
adr_id: "0013"
adr_status: Accepted
decision_date: 2026-07-03
owner: human:paulohenriquevn
milestone: M30
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0013
    resource: git:f7c7b93:docs/adr/0013-v1-legacy-columnar-bm25-scope.md
    title: ADR 0013 — Escopo dos pilares v1-legacy
    last_modified: 2026-07-03
---

Dois pilares foram explorados sob a tese v1 (composição de extensões de terceiros) e ficaram como
medição descartável, não embarcada. Com o mandato v2 do
[ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) — código próprio, dependências
mínimas —, era preciso decidir: manter, deprecar, ou reescrever.

# Direcionadores

O north-star inclui paridade com o [AlloyDB](/technologies/alloydb.md), que **tem** columnar/HTAP;
analytics sobre dados transacionais vivos é capacidade de banco geral, não nicho. Measurement-first:
nenhum pilar fica ou some por opinião. Licença: `pg_mooncake` é **MIT** (empacota o
[pg_duckdb](/technologies/pg-duckdb.md), também MIT) e `pg_textsearch` é permissivo — enquanto os
columnar on-disk comprimidos **Citus e Hydra são AGPL, barrados**, o que torna a rota DuckDB a
única permissiva. E a regra de não reinventar: reescrever columnar do zero é inviável.

# Decisão: MANTER os dois

## Columnar — o gap de escala que o M30 fechou

A questão crítica era: columnar ganha **em escala**? O M6 medira apenas 100k e marcara o ganho de
escala como não-medido. O [M30](/benchmarks/m30-columnar-scale.md) fechou o gap, com média ± desvio
sobre 3 runs e efeito maior que a variância em todos os pontos:

| linhas | row-store (Seq Scan) | columnstore (DuckDBScan) | speedup |
|---|---|---|---|
| 100.000 | 12,7 ± 1,4 ms | 4,2 ± 0,8 ms | **2,99×** |
| 1.000.000 | 62,3 ± 3,5 ms | 7,0 ± 0,5 ms | **8,89×** |
| 5.000.000 | 285,3 ± 19,3 ms | 20,6 ± 0,3 ms | **13,87×** |

O speedup **cresce com a escala** — ~3× para ~14× — numa agregação `GROUP BY` analítica, com
resultado correto (contagem exata, média dentro de 1e-3 cross-engine, **não** byte-idêntica: a
soma do PG difere do DuckDB no último decimal) e plano `DuckDBScan` vetorizado. É a assinatura
clássica do columnar.

**Reconciliação honesta com o M6.** O [M6](/benchmarks/m6-columnar-vs-row.md) medira o row-store
*vencendo* a 100k (columnar em 44,3 ms); este run mede columnar vencendo a 100k (4,2 ms) — um
swing de ~11× no mesmo harness e na mesma família de imagem `:latest`, atribuível a drift de
versão mais cache. Por isso **o ponto de 100k é tratado como quase-paridade e NÃO é load-bearing**:
a decisão se ancora no ganho robusto a versão e muito além da variância a partir de 1M. O
row-win de 100k do M6 fica **superado e incerto**, não é citado como evidência.

## BM25 — um ganho medido que seria descartado

O [M7](/benchmarks/m7-bm25-vs-tsrank.md) mede **nDCG@10 de 0,9546 para o BM25 contra 0,5143 do
`ts_rank_cd` nativo** — um ganho grande de qualidade lexical, com peça permissiva. A perna lexical
**embarcada** continua sendo o `ts_rank_cd` nativo; o BM25 é candidato de adoção.[^adr0013]

# Consequências

O produto mantém o caminho para analytics/HTAP e para BM25, ambos com evidência medida e peças
permissivas. O custo honesto: duas dependências de terceiros permanecem como **exceções
declaradas** à regra own-code.

O caminho de adoção fica gated numa milestone futura: a imagem embarcada era PG17 e o
`pg_mooncake` tinha prebuilt só em PG18, deixando duas rotas — consertar o build PG17 from-source
(que travou num pin de rustc/MSRV) ou subir para PG18. Esta decisão não muda código de produto.

# Alternativas rejeitadas

**Deprecar e remover ambos** — mais enxuto, mas joga fora paridade de columnar e um ganho lexical
medido, contradizendo a evidência. **Reescrever ambos em Rust** — esforço enorme contra peças
maduras.

# O que aconteceu depois

A trajetória divergiu de ambos os caminhos previstos aqui: o columnar acabou sendo **reescrito
como código próprio** ([ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md)), o `pg_duckdb`
foi **totalmente removido** ([ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md)), e o BM25
próprio superou o `ts_rank_cd` ([ADR 0054](/decisions/0054-m140-3-bm25-supersede-textsearch.md)).

[^adr0013]: ADR 0013 — Escopo dos pilares v1-legacy: columnar (M6) + BM25 (M7)
