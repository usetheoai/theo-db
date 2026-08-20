---
type: Feature
title: Índice IVFFlat (theodb_ivfflat)
description: Access method próprio por listas invertidas, com lists definido no build e probes ajustado por query; é também o carrier da quantização estilo ScaNN.
resource: git:f7c7b93:docs/features/03-indice-ivfflat.md
tags: [feature, indice, ivfflat, ann, access-method, quantizacao]
feature_status: entregue
milestone: M9+M21+M34
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat03
    resource: git:f7c7b93:docs/features/03-indice-ivfflat.md
    title: Criar um índice IVFFlat
---

**Status: entregue.** Access method **próprio** em Rust, validado e medido no harness de recall
([m9](/benchmarks/m9-ivfflat.md)). O [pgvector](/technologies/pgvector.md) e a superfície nativa dele
foram removidos no [ADR 0029](/decisions/0029-m70-drop-pgvector.md).

# Criar o índice

```sql
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;

CREATE INDEX products_ivfflat_idx
ON products
USING theodb_ivfflat (description_embedding theodb_ivfflat_cosine_ops)
WITH (lists = 100);
```

| Opclass | Métrica | Operador |
|---|---|---|
| `theodb_ivfflat_l2_ops` | L2 — **default** | `<->` |
| `theodb_ivfflat_cosine_ops` | cosseno | `<=>` |
| `theodb_ivfflat_ip_ops` | produto interno | `<#>` |

# Os dois knobs

Diferente do [HNSW](/features/02-indice-hnsw.md), aqui **existe** uma opção de build real:

- **`lists` (build)** — o número de listas invertidas, ou seja, de centroides do k-means. Definido em
  `WITH (lists = N)`.
- **`probes` (query)** — quantas listas o scan sonda. Ajustado por `SET theodb_ivfflat.probes = N`;
  quanto maior, maior o recall e a latência.

A relação entre os dois é o que governa o custo: o scan lê aproximadamente `probes/lists` do índice —
e é exatamente essa razão que o modelo de custo do planner usa, conforme registrado no
[ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md).

# Este é o carrier da quantização

O IVFFlat é o access method sobre o qual a linha de quantização do projeto foi construída, porque
**batch-scan é o carrier certo** — duas tentativas de quantizar sobre o HNSW foram refutadas por
medição ([ADR 0018](/decisions/0018-m57-sbq-inline-not-superior.md) e
[ADR 0019](/decisions/0019-m59-ah-needs-code-vector-separation.md)).

As reloptions de quantização e storage:

```sql
CREATE INDEX products_aq_idx
ON products
USING theodb_ivfflat (description_embedding theodb_ivfflat_l2_ops)
WITH (lists = 1000, pq_subspaces = 16, pq_bits = 4, separate_storage = 1);
```

`separate_storage = 1` é o layout que **separa os códigos dos vetores de precisão plena** — a lição
medida de que co-localizá-los faz o walk paginar o vetor inteiro só para ler alguns bytes de código.
Detalhes em [quantização vetorial](/features/19-quantizacao-vetorial.md).

# Filtro por label dentro da travessia

Quando a consulta filtra por um conjunto pequeno de labels, o filtro **inline** decidido no
[ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md) evita a fome do post-filter — medido em
+0,48 de recall e ~20× de QPS a ~1% de seletividade. Requer coluna de label declarada e reconstrução
do índice.

# A armadilha de dados que este índice expôs

O [ADR 0012](/decisions/0012-benchmark-data-degeneracy.md) nasceu aqui: um profiler mostrou
`nonempty_lists=1/100`, isto é, **todos os vetores numa lista só** — porque os dados de teste eram
idênticos entre si. O profiler permanece disponível e é o tripwire dessa classe de problema:

```sql
-- THEODB_SCAN_PROFILE=1 no ambiente do servidor
-- nonempty_lists próximo de 1/N sobre dados distintos = k-means degenerado
```

# Escolha entre IVFFlat e HNSW

A decisão de default está em [decisão de índice M2](/decisions/m2-index-decision.md). Em resumo: o
HNSW venceu em recall, QPS e tempo de build nos regimes medidos; o IVFFlat é a escolha quando o
build precisa ser rápido ou quando se quer usar quantização em escala.
