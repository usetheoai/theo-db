---
type: Feature
title: Analítico colunar (theodb_columnar)
description: Table Access Method próprio append-only com zone-map por chunk-group e pushdown vetorizado opt-in; UPDATE e DELETE falham com erro tipado, por desenho.
resource: git:f7c7b93:docs/features/14-analitico-colunar.md
tags: [feature, columnar, table-access-method, datafusion, zone-map, append-only]
feature_status: entregue
milestone: M99+M100+M114+M115
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat14
    resource: git:f7c7b93:docs/features/14-analitico-colunar.md
    title: Consultas analíticas sobre armazenamento colunar
---

**Status: entregue.** Armazenamento colunar **próprio** via Table Access Method, em formato column-major
com dicionário de mínimo e máximo por chunk-group, e **MVCC delegado a um catálogo heap** — o truque de
correção decidido no [ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md), que evita reimplementar
MVCC, a coisa de maior risco que um TAM poderia fazer.

Os agregados, `GROUP BY` e `WHERE` vetorizados vêm de um `CustomScan` sobre
[DataFusion](/technologies/datafusion.md).

# Uso

```sql
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;

CREATE TABLE eventos (...) USING theodb_columnar;
```

# Ganhos medidos

| Operação | Ganho | Artefato |
|---|---|---|
| `GROUP BY` | 4,53–9,75× | [verdict](/benchmarks/columnar-groupby-verdict.md) |
| `min`/`max` por fast-path de zone-map | **~1300–1400×** | [verdict](/benchmarks/columnar-minmax-zonemap-verdict.md) |
| skip por zone-map | 7,29× | [verdict](/benchmarks/columnar-zonemap-verdict.md) |
| 43 queries ClickBench | byte-idênticas ao heap | [m128](/benchmarks/m128-clickbench-columnar.md) |
| seqscan plano (sem `CustomScan`) | **paridade-ou-mais-lento** que heap — ~16–26× no full-scan | [m99](/benchmarks/m99-columnar-tam.md) |

# As cinco ressalvas que mudam o uso

**1. O pushdown vetorizado é opt-in, e o default é DESLIGADO.** Sem ligar a GUC
`theodb.enable_columnar_agg`, a tabela funciona como **storage** e o agregado roda pelo plano nativo do
PostgreSQL. É a primeira coisa a checar quando o ganho esperado não aparece.

**2. O seqscan plano é paridade-ou-mais-lento que heap — por desenho.** Um `SELECT` normal decodifica
**todas as colunas** de cada stripe: o TAM não recebe a lista de projeção do planner
(`theodb_rs/src/am/columnar.rs:1015-1021`), então não existe decodificação só das colunas projetadas
nesse caminho. O custo medido é de **~16–26× no agregado full-scan**
([m99-columnar-tam.md](/benchmarks/m99-columnar-tam.md)). O ganho de projeção e vetorização existe
**apenas** no caminho `CustomScan` do M100 — com a GUC ligada e a forma admitida. Sem ele, a vitória da
tabela colunar é **o tamanho em disco pela compressão**, não a latência de leitura.

**3. Nem toda forma de agregado faz pushdown.** O que não é admitido **cai para o plano nativo**, de
forma fail-safe — o resultado continua correto, só não acelerado.

**4. É colunar em disco, não in-memory automático.** A paridade *literal* com o colunar do
[AlloyDB](/technologies/alloydb.md) está declaradamente fora de escopo — é a aposta diferente que o
[ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) registra, forçada pela barreira
de licença sobre as alternativas AGPL.

**5. A superfície DML é append-only.** `UPDATE`, `DELETE`, tuple-lock, parallel scan, sample scan,
TID-range scan e `CREATE INDEX` **falham com erro tipado** em tabelas colunares. O bitmap scan não erra:
os callbacks ficam nulos de propósito e o planner **desvia** dessa forma.

**Use tabelas heap para dados mutáveis; a colunar é para carga analítica append-only.** Isso é escopo
declarado, não limitação temporária — o ADR 0042 registra que reivindicar "columnar HTAP atualizável"
seria over-claiming, já que a única implementação de referência é AGPL e teria de ser redesenhada
clean-room.

# Limites de escala

O comportamento sob memória limitada e a política de compaction estão no
[ADR 0047](/decisions/0047-m104-scaling-tradeoffs-deliberate.md); a interação entre streaming e spill
que produziu uma regressão medida está no
[ADR 0059](/decisions/0059-m169-fail-open-cobre-falha-de-spill.md).

# Relacionados

Analytics sobre **arquivos externos** é outro caminho: [lakehouse Parquet](/features/15-lakehouse-parquet.md).
E a co-residência entre índice vetorial e colunas analíticas está no
[ADR 0044](/decisions/0044-m103-vector-columnar-coresidence.md).
