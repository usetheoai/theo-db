---
type: Feature
title: Índice HNSW (theodb_hnsw)
description: Access method HNSW próprio, page-native com travessia sob demanda; o recall se ajusta em tempo de query por ef_search, não por opções de build.
resource: git:f7c7b93:docs/features/02-indice-hnsw.md
tags: [feature, indice, hnsw, ann, access-method]
feature_status: entregue
milestone: M21+M35
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat02
    resource: git:f7c7b93:docs/features/02-indice-hnsw.md
    title: Criar índices HNSW
---

**Status: entregue.** O TheoDB tem um access method [HNSW](/technologies/hnsw.md) **próprio**. Desde a
reestruturação page-native, a persistência é em páginas com travessia **sob demanda**, o que trocou um
custo O(N) por O(ef·M) — medido em [m35](/benchmarks/m35-hnsw-structured-scan.md), com paridade de
recall contra o HNSW do [pgvector](/technologies/pgvector.md) como baseline.

# Criar o índice

```sql
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;

CREATE INDEX products_hnsw
ON products
USING theodb_hnsw (description_embedding theodb_hnsw_cosine_ops);
```

| Opclass | Métrica | Operador correspondente |
|---|---|---|
| `theodb_hnsw_l2_ops` | L2 (euclidiana) — **default** | `<->` |
| `theodb_hnsw_cosine_ops` | cosseno | `<=>` |
| `theodb_hnsw_ip_ops` | produto interno | `<#>` |

A opclass **precisa casar** com o operador usado na consulta; caso contrário o índice não é usado.

Aplicações que escrevem a sintaxe do pgvector (`USING hnsw (col vector_cosine_ops)`) funcionam pelo
alias descrito no [ADR 0058](/decisions/0058-pgvector-compat-shim.md) — que aponta para **este mesmo
handler**, sem segunda implementação.

# A armadilha dos parâmetros de build

**`m` e `ef_construction` NÃO são opções de `WITH`.** São constantes do build — `m` fixo em 16 e
`ef_construction` fixo em 64 —, e o segundo só muda por variável de ambiente, para benchmarks.

Passar `WITH (m = …)` ou `WITH (ef_construction = …)` faz o `CREATE INDEX` **falhar** com
`unrecognized parameter`. Esta é a confusão mais provável para quem vem do pgvector.

As opções de `WITH` que este access method aceita são de **quantização e storage**, compartilhadas com
os demais: `sbq_bits`, `pq_subspaces`, `pq_bits` (que só aceita `4`), `aq_threshold`,
`separate_storage` e `refine`. Sem nenhuma delas, o índice guarda os vetores em precisão plena. Ver
[quantização vetorial](/features/19-quantizacao-vetorial.md).

# O knob de recall é em tempo de query

```sql
SET theodb_hnsw.ef_search = 100;   -- maior = mais recall, mais latência
```

Para escolher o valor sem tentativa e erro, existe o recomendador determinístico decidido no
[ADR 0026](/decisions/0026-m67-autotune-recommender.md):

```sql
SELECT theodb.recommend_ef('products'::regclass, 'description_embedding', ARRAY[...], 0.95, 10);
```

Ele acha o **menor** `ef` que atinge o recall-alvo, por bisecção — o que é correto porque `recall(ef)`
é monotônico não-decrescente.

# Acompanhar a construção

```sql
SELECT phase FROM pg_stat_progress_create_index;
```

A fase `building graph` indica que o algoritmo está montando o grafo; ela desaparece ao terminar.

# Qualidade do grafo — o que a medição mostrou

Duas decisões medidas afetam diretamente o recall deste índice:

- **`extendCandidates` está ligado por padrão**
  ([ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md)), porque sem ele o recall
  degradava com a escala. O custo é um build 2 a 3× mais lento, com opt-out por variável de ambiente.
- O critério de recall do projeto é **paridade com o pgvector**, e não um valor absoluto
  ([ADR 0030](/decisions/0030-m60-recall-parity-not-absolute-099.md)) — porque o próprio pgvector não
  alcança 0,99 no corpus medido.

# Manutenção

O VACUUM e a compaction deste índice seguem o desenho do
[ADR 0017](/decisions/0017-m55-index-maintenance-at-scale.md), com a garantia de crash-safety do
[ADR 0014](/decisions/0014-m48-crash-safe-fold-reclaim-mechanism.md). Em índices muito grandes, a
recuperação de espaço pode exigir `REINDEX` explícito, conforme registrado no
[ADR 0047](/decisions/0047-m104-scaling-tradeoffs-deliberate.md).

# Alternativas

O [índice IVFFlat](/features/03-indice-ivfflat.md) é a alternativa por listas invertidas, e o
[índice SymQG](/features/17-indice-symqg.md) é a linha experimental. Para diagnosticar recall baixo ou
latência alta em produção, ver o [runbook de diagnóstico](/runbooks/vector-scan-diagnostics.md).
