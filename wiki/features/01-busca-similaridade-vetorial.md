---
type: Feature
title: Busca por similaridade vetorial (KNN)
description: Busca dos vizinhos mais próximos sobre uma coluna de embeddings, com três operadores de distância own-code e entrada por vetor literal ou por texto.
resource: git:f7c7b93:docs/features/01-busca-similaridade-vetorial.md
tags: [feature, vetorial, knn, embeddings, sql]
feature_status: entregue
milestone: M20
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat01
    resource: git:f7c7b93:docs/features/01-busca-similaridade-vetorial.md
    title: Busca por similaridade vetorial
---

**Status: entregue.** Os kernels de distância são **código próprio** do TheoDB, operando sobre o tipo
`vector` próprio decidido no [ADR 0028](/decisions/0028-m69-own-vector-type.md) — o
[pgvector](/technologies/pgvector.md) foi removido no
[ADR 0029](/decisions/0029-m70-drop-pgvector.md). Os operadores mantêm a **mesma grafia** do pgvector,
o que é o que torna a migração drop-in.

Nenhum número de desempenho aparece nesta página: eles vivem nos artefatos de benchmark, e a regra do
projeto é que performance é claim medido, nunca afirmação em documentação de feature.

# Consulta base

```sql
SELECT *
FROM tabela
ORDER BY coluna_embedding <operador> '[...]'::vector
LIMIT k;
```

A ordenação **crescente** coloca os vetores mais semelhantes no topo, porque a métrica retorna
**distância, não similaridade** — valor menor significa maior similaridade.

# Operadores de distância

| Operador | Métrica | Função equivalente | Uso típico |
|---|---|---|---|
| `<->` | L2 (euclidiana) | `theodb.l2_distance` | vetores numéricos gerais |
| `<#>` | produto interno | `theodb.inner_product` | quando a métrica de treino foi IP |
| `<=>` | cosseno | `theodb.cosine_distance` | **embeddings de texto** — o caso mais comum |

# Habilitando a extensão

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Isso provê o tipo `vector` próprio e os access methods ANN. Aplicações que escrevem
`CREATE EXTENSION vector` continuam funcionando pelo shim descrito no
[ADR 0058](/decisions/0058-pgvector-compat-shim.md).

# Entrada por texto, em vez de vetor

```sql
SELECT *
FROM products
ORDER BY description_embedding::vector
         <=> theodb.embed('running shoes', 'text-embedding-3-small')
LIMIT 5;
```

A assinatura é `theodb.embed(content text, model text DEFAULT NULL)` — **conteúdo primeiro, modelo
depois**. Omitir o segundo argumento usa o modelo default. Para vários textos numa chamada só existe
`theodb.embed_batch(text[], model)`, que é o acelerador N→1 discutido no
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md).

O cast `::vector` garante a compatibilidade de sintaxe com os operadores.

# Padrões comuns

**Combinar com filtro relacional** — o ponto em que vetor e relacional juntos ganham, e a razão de ser
do posicionamento de [unificação](/decisions/0005-unification-as-differentiator.md):

```sql
SELECT * FROM products
WHERE category_id = 3
ORDER BY description_embedding::vector <=> theodb.embed('comfortable shoes')
LIMIT 5;
```

**Expor o score de distância**, quando a aplicação precisa do valor e não só da ordem:

```sql
SELECT *,
       description_embedding::vector <=> theodb.embed('casual hoodie') AS distance
FROM products
ORDER BY distance
LIMIT 3;
```

**Parametrizar a query**, que é o padrão para aplicações:

```sql
SELECT * FROM items
ORDER BY item_embedding::vector <=> theodb.embed(:search_text)
LIMIT :limit;
```

# Restrições

- **A métrica da consulta deve ser a mesma usada na criação do índice.** Consultar com `<=>` um índice
  construído para L2 não usa o índice.
- **Bulk search não é suportado** — não há como fazer várias buscas KNN numa única operação. O padrão
  disponível para juntar duas tabelas por similaridade é o `LATERAL` do
  [ADR 0022](/decisions/0022-m63-vector-join-lateral-not-node.md).
- Sem índice, a busca é sequencial exata. Os índices que a aceleram estão em
  [índice HNSW](/features/02-indice-hnsw.md) e [índice IVFFlat](/features/03-indice-ivfflat.md), e a
  escolha do default está registrada em [decisão de índice](/decisions/m2-index-decision.md).

# Conceitos relacionados

Filtro relacional eficiente sobre a busca em
[acelerar consultas](/features/08-acelerar-consultas.md); combinação com a perna lexical em
[busca híbrida](/features/06-busca-hibrida.md); e manutenção automática da coluna de embedding em
[vectorizer](/features/16-vectorizer.md).
