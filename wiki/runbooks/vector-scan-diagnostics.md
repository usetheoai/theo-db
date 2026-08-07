---
type: Runbook
title: Diagnóstico do query vetorial — recall baixo ou latência alta
description: Playbook operacional que começa verificando se o índice está sendo usado, porque essa é a causa nº 1 e nenhum ajuste de ef a resolve.
resource: git:f7c7b93:docs/ops/vector-scan-diagnostics.md
tags: [runbook, operacao, diagnostico, recall, latencia, observabilidade]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: vecdiag
    resource: git:f7c7b93:docs/ops/vector-scan-diagnostics.md
    title: Diagnóstico do query vetorial
---

O scan ANN é **opaco por natureza**. Estas ferramentas mostram **o que o scan de fato fez** — algo que as
extensões de referência não expõem por query, e que existe aqui pela decisão do
[ADR 0027](/decisions/0027-m68-vector-observability.md).

# As ferramentas

| Função | O que mostra |
|---|---|
| `theodb.explain_scan(tabela, coluna, query, ef, k)` | índice escolhido, **ef efetivo**, **pages_read**, **candidates_seen**, latência — de **um** scan |
| `theodb.scan_stats(tabela, coluna, query, ef, k)` | o mesmo, e **persiste** no catálogo |
| `theodb.index_scan_stats(rel)` | agregados por índice: contagem, médias, último `ef` |

# Passo 0 — SEMPRE primeiro: o índice está sendo usado?

**A causa nº 1 de "recall ou latência ruim" NÃO é o `ef` — é o planner não escolher o índice.**

```sql
EXPLAIN SELECT id FROM docs ORDER BY emb <=> '[...]'::vector LIMIT 10;
```

- **`Index Scan using …`** → o índice está sendo usado; prossiga.
- **`Seq Scan`**, ou **`Sort` + `Limit`** → **não** está. Nenhum ajuste de `ef` vai adiantar. Conserte a
  query:
  - precisa ser `ORDER BY <coluna> <operador> <query> LIMIT k`, **ascendente** — não `DESC`, não sem
    `LIMIT`;
  - **o operador precisa casar a opclass do índice**: `<=>` para cosseno, `<->` para L2, `<#>` para
    produto interno.

# Recall baixo

1. **Passo 0 primeiro.**
2. **Ache o menor `ef` que atinge a meta** — o default costuma ser baixo para produção:
   ```sql
   SELECT theodb.recommend_ef('docs'::regclass, 'emb', ARRAY['[...]','[...]']::text[], 0.95, 10);
   ```
   A curva é monotônica **com retornos decrescentes**: escolha o **menor** `ef` que bate a meta, não o
   máximo. O recomendador vem do [ADR 0026](/decisions/0026-m67-autotune-recommender.md), e é
   **ótimo na média, não seguro na cauda** — 12% das queries ficaram fora do alvo na medição.
3. **Recall baixo apenas com `WHERE`?** O `ef` não é a causa — **o filtro removeu candidatos**. O
   mecanismo correto é o filtro inline do
   [ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md), que a ~1% de seletividade mediu +0,48
   de recall.
4. **O teto não sobe com nenhum `ef`?** Os parâmetros de **build** fixam o teto. Reconstrua — e note que
   no [HNSW](/features/02-indice-hnsw.md) eles **não** são opções de `WITH`.

# Latência alta

1. **O `ef` está alto demais?** A curva de recall para latência é brutal no topo: de 0,90 para 0,95 é
   barato; **de 0,95 para 0,99 custa 3 a 5×**. Mire o recall que a aplicação realmente precisa.
2. **A latência não cede com nenhum `ef`?** Então **é memória, não parâmetro** — o índice está indo para
   disco. **`pages_read` alto denuncia isso.** ANN só é rápido com o grafo residente.
3. **Cold start:** logo após restart o cache está frio — latência alta **transitória, não é bug**.

# Sinal → causa → ação

| Sinal | Causa provável | Ação |
|---|---|---|
| `EXPLAIN` mostra Seq Scan | planner não escolheu o índice | corrigir a forma da query e casar a opclass |
| recall abaixo da meta, índice usado | `ef` baixo | `recommend_ef`, depois `SET` |
| recall baixo só com `WHERE` | filtro removeu candidatos | filtro inline |
| recall não sobe com `ef` alto | parâmetros de build limitam o teto | reconstruir |
| latência alta, `ef` alto | 0,99 custa 3–5× de 0,95 | mirar 0,90–0,95 |
| latência alta, **`pages_read` alto** | grafo indo para disco | garantir residência em RAM |
| **`candidates_seen` muito alto** | o beam navegou demais | reduzir `ef`; revisar filtro |
| latência alta transitória pós-restart | cache frio | aguardar warm-up |

A distinção entre as duas últimas linhas de latência é o que `candidates_seen` existe para dar:
**`pages_read` alto é I/O; `candidates_seen` alto é grafo caro de navegar.** Sem essa métrica, as duas
causas são indistinguíveis.

# Ressalvas honestas

- **`candidates_seen` no caminho quantizado** reflete o pool alargado do walk, não o `ef` do resultado —
  é a verdade do que o scan navegou, mas precisa ser lido assim.
- **O `ef` reportado é o passado à função**, não o crescido em tempo de execução. Para esse, observe o
  último `ef` nos agregados após scans reais.
- A métrica de runtime é um **catálogo consultável**, não série temporal — decisão explícita do ADR 0027,
  com exportador adiado por não haver consumidor.
