---
type: Measurement
title: b018 — o planner larga o HNSW na junção filtrada, e a causa é o TAMANHO do índice, não o modelo de custo
description: Reproduzido deterministicamente. O modelo de custo é port fiel do pgvector 0.8; o que diverge é o índice ocupar 680 páginas contra 382 para o mesmo dado — 1,78×, que casa com a razão de custo medida de 1,769. Registra também um controle meu que estava errado.
tags: [planner, hnsw, custo, juncao, b-018, honest-negative]
item: B-018
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Peça relacionada: [pgrx](../technologies/pgrx.md) e o
[ADR-0065](../decisions/0065-b032-unsafe-op-marcado-por-operacao.md), que também trocou um número
herdado por um medido.

# O que reproduziu

O [[B-018]] registrava que o planner não alcança o HNSW no caminho de junção, e **não reproduziu em
seis cenários** em 2026-08-11 — parâmetro vs literal, generic plan, estatística ausente, ordem de
criação do índice. O sétimo cenário reproduz, deterministicamente:

```sql
SELECT e.id FROM embeddings e
  JOIN chunks c ON c.id = e.chunk_id
  JOIN documents d ON d.id = c.document_id
 WHERE d.tenant = 't1'                       -- <<< o filtro seletivo é o gatilho
 ORDER BY e.vector <=> $1 LIMIT 5;
```

Sem o `WHERE`, `embeddings` dirige e o HNSW serve a ordenação. **Com** ele, a ordem de junção inverte,
`embeddings` vai para o lado interno de um Nested Loop por `chunk_id`, e um `Sort` aparece — a forma
exata do relato original (`Limit → Sort → Nested Loop → Index Scan`).

O `Sort` não é evitável por knob: com `enable_sort = off` o plano continua o mesmo, marcado
`Disabled: true`. Não há caminho alternativo a gerar.

# A virada, medida

| `ef_search` | partida do HNSW | plano escolhido |
|---|---|---|
| 40 | 425,60 | **HNSW** (vence o Sort de 559,36 por 24%) |
| 56 | — | Sort |
| **64 (nosso default)** | — | **Sort** |

**A margem no melhor caso é 24%** — e é isso que explica a intermitência de 1-em-11 que o teste do
`theo-rag` declara e que seis cenários determinísticos não pegaram. Não é aleatoriedade: é uma
comparação no fio da navalha, e nove arquivos de teste escrevendo em paralelo contra um banco só
movem o custo do lado concorrente para os dois lados do fio.

# Uma retratação, registrada porque a conclusão errada quase entrou

O primeiro controle usou **pgvector 0.5.1**, que escolheu o HNSW em todos os casos — e a conclusão
seria "defeito nosso, o pgvector não tem". **Estava errada.** O `hnsw.c` do 0.5.1 tem um modelo de
custo anterior e muito mais cru:

```c
costs.numIndexTuples = (entryLevel + 2) * m;   /* ANTES do genericcostestimate */
*indexStartupCost = costs.indexTotalCost;      /* "most work happens before first tuple" */
```

O do **0.8.0** é o nosso: mesmo `ratio = (entryLevel*m + layer0TuplesMax*layer0Selectivity)/tuples`,
mesmo `layer0TuplesMax = HnswGetLayerM(m,0) * hnsw_ef_search`, mesmo `scalingFactor = 0.55`, mesmo
`startup = total × ratio`, mesma correção TOAST com as duas guardas. Nosso `am/cost.rs` é port fiel do
pgvector **atual**; comparar contra o 0.5.1 mediu a distância entre duas gerações do modelo, não entre
duas implementações dele.

# O controle justo, e a causa

pgvector **0.8.6**, mesmo esquema, mesmo dado, mesma query:

| motor | partida ef=40 | partida ef=64 | plano |
|---|---|---|---|
| pgvector 0.8.6 | 240,62 | **342,40** | **HNSW nos dois** |
| TheoDB | 425,60 | acima de 559 | HNSW só em 40 |

Mesma fórmula, custo **1,769×** maior. E a explicação não está na fórmula:

| | páginas do índice | páginas do heap | tuplas |
|---|---|---|---|
| TheoDB `theodb_hnsw` | **680** | 600 | 3000 |
| pgvector `hnsw` | **382** | 600 | 3000 |

**1,78× de páginas**, contra 1,769× de custo. O `genericcostestimate` cobra proporcionalmente a
páginas de índice, então a divergência de custo é consequência aritmética da divergência de tamanho.

**A causa do B-018 não é o modelo de custo** — que era a hipótese do item, herdada do `m175`. É o
índice ocupar 1,78× mais disco, agravado por um default de `ef_search` de 64 contra os 40 do pgvector,
que empurra o ratio ainda mais para cima.

# O que isso muda no alvo do conserto

O conserto mora no **layout de armazenamento do índice**, não em `am/cost.rs`. Mexer na fórmula de
custo para compensar o tamanho seria mentir para o planner sobre quanto o scan custa — e o custo
maior é verdadeiro: o índice é maior mesmo, e lê-lo custa mais mesmo.

Dois eixos, nesta ordem:

1. **Por que 680 contra 382** para vetores idênticos de 384 dimensões. Essa é a medição que ainda
   falta, e é a que decide o conserto.
2. **O default de `ef_search`** — 64 herdado do `SCAN_EF` fixo pré-M35, contra os 40 do pgvector.
   Baixá-lo é de uma linha e move a virada, mas troca recall por plano, o que exige medir recall antes.

# Reprodução

```bash
docker run -d --name c -e POSTGRES_HOST_AUTH_METHOD=trust ghcr.io/usetheoai/theo-db:develop
# tabelas documents/chunks/embeddings, 200/3000/3000 linhas, vector(384)
# CREATE INDEX ... USING theodb_hnsw (vector theodb_hnsw_cosine_ops)
# EXPLAIN da junção acima, com e sem o WHERE d.tenant
```

**Nota de superfície, encontrada no caminho:** o opclass do nosso AM chama-se
`theodb_hnsw_cosine_ops`; `vector_cosine_ops` — o nome do pgvector — é recusado com
`operator class "vector_cosine_ops" does not exist for access method "theodb_hnsw"`. Uma aplicação
migrando do pgvector precisa reescrever o `CREATE INDEX`, e isso não está na compatibilidade que o
shim promete.
