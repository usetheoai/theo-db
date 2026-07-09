# Diagnóstico do query vetorial — recall baixo / latência alta (M68)

Playbook operacional para diagnosticar o scan vetorial (`theodb_hnsw`/`theodb_ivfflat`) em produção, usando as
superfícies de observabilidade own-code do TheoDB (M67/M68). O scan ANN é opaco por natureza — estas ferramentas
mostram **o que o scan de fato fez**, algo que pgvector/pgvectorscale não expõem por-query.

## As ferramentas

| Função | O que mostra |
|---|---|
| `theodb.explain_scan(index_table, vector_col, query, ef, k)` | índice escolhido, **ef efetivo**, **pages_read**, **candidates_seen** (o pool navegado no beam), latência, results — de UM scan |
| `theodb.scan_stats(index_table, vector_col, query, ef, k)` | pages_read + candidates_seen + latência de um scan; **persiste** no catálogo |
| `theodb.index_scan_stats(rel)` | agregados por índice: n_scans, avg_pages_read, avg_candidates, avg_latência, last_ef |

> **Nota honesta (não há `amexplain` no PG17/PG18):** o Postgres não tem um hook para o AM injetar linhas no
> `EXPLAIN` do plano. `theodb.explain_scan` é uma **função diagnóstica separada** — o mesmo padrão de
> Qdrant (`/telemetry`) e Milvus (métricas). É a forma correta e portável.

## Passo 0 — SEMPRE primeiro: o índice está sendo usado?

A causa nº 1 de "recall/latência ruim" NÃO é o `ef` — é o **planner não escolher o índice** (cai em seqscan).

```sql
EXPLAIN SELECT id FROM docs ORDER BY emb <=> '[...]'::vector LIMIT 10;
```

- Se o plano mostra **`Index Scan using ..._idx`** → o índice está sendo usado. Prossiga.
- Se mostra **`Seq Scan`** ou **`Sort` + `Limit`** → o índice NÃO está sendo usado. Corrija a query:
  - Precisa de `ORDER BY <coluna> <operador-de-distância> <query> LIMIT k` **ascendente** (não `DESC`, não sem `LIMIT`).
  - O operador de distância deve casar a opclass do índice (`<=>` cosine, `<->` L2, `<#>` inner product).

## Recall baixo

1. **O índice está sendo usado?** (Passo 0). Se não, o `ef` é irrelevante — conserte a query primeiro.
2. **Descubra o ef mínimo que atinge a meta** (o default 64 costuma ser baixo para produção):
   ```sql
   SELECT theodb.recommend_ef('docs'::regclass, 'emb', ARRAY['[...]','[...]']::text[], 0.95, 10);
   ```
   Aplique: `SET theodb_hnsw.ef_search = <valor>;`. A curva recall(ef) é monotônica com **diminishing returns**
   — escolha o **menor** ef que bate a meta, não o máximo.
3. **Recall baixo só em query filtrada (`WHERE`)?** O `ef` não é a causa — o filtro remove candidatos. Habilite
   o iterative scan (o scan cresce o ef até `max_scan_tuples` sob filtro seletivo — M52). Veja `theodb_hnsw.max_scan_tuples`.
4. **O teto não sobe com nenhum ef?** Os parâmetros de **build** (`m`, `ef_construction`) fixam o teto de recall.
   Rebuild com valores maiores (`REINDEX` ou recriar o índice `WITH (m=..., ef_construction=...)`).

## Latência alta

1. **O ef está alto demais?** A curva recall→latência é brutal no topo: 0.90→0.95 é barato; **0.95→0.99 custa
   3-5×**. Mire a recall que a aplicação realmente precisa (0.90-0.95), não o máximo. Use `recommend_ef` para o
   menor ef.
2. **A latência não cede com NENHUM ef?** É **memória, não parâmetro** — o índice está spillando para disco (cauda
   longa). Diagnostique com `theodb.explain_scan` / `theodb.index_scan_stats`: **`pages_read` alto** denuncia I/O
   pesado (o grafo não cabe em RAM). ANN só é rápido com o grafo residente em memória.
3. **Cap de pior-caso:** `theodb_hnsw.max_scan_tuples` limita quantas linhas o iterative scan varre.
4. **Cold start:** logo após um restart, o cache está frio — latência alta **transitória**, não é bug.

## Tabela sinal → causa → ação

| Sinal (onde ver) | Causa provável | Ação |
|---|---|---|
| `EXPLAIN` mostra Seq Scan | planner não escolheu o índice | `ORDER BY <dist> LIMIT` asc; operador casa a opclass |
| recall abaixo da meta, índice usado | ef baixo | `recommend_ef` → `SET ef_search` (menor que bate a meta) |
| recall baixo só com `WHERE` | filtro removeu candidatos | iterative scan / `max_scan_tuples` |
| recall não sobe com ef alto | build params (`m`/`ef_construction`) limitam o teto | rebuild com valores maiores |
| latência alta, ef alto | ef alto demais (0.99 = 3-5× de 0.95) | mirar 0.90-0.95 |
| latência alta, `pages_read` alto (`explain_scan`) | grafo spillando pra disco | garantir residência em RAM |
| `candidates_seen` muito alto (`explain_scan`) | o beam navegou muito (ef alargado / filtro) | reduzir ef; revisar filtro |
| latência alta transitória pós-restart | cache frio | aguardar warm-up (não é bug) |

## Caveats honestos

- **`candidates_seen` no caminho aproximado (SBQ/AQ):** reflete o pool alargado do walk (`ef · over_fetch`), não o
  `ef` do result — é a verdade do que o scan navegou.
- **`ef_effective` do `explain_scan`** é o ef **passado** à função, não o ef **crescido pelo iterative scan** (M52,
  que vive no executor real, fora do coletor). Para o ef iterative real, observe `last_ef` em `index_scan_stats`
  após scans reais.
- A métrica runtime é o **catálogo consultável** `theodb._index_scan_stats` (não Prometheus histogram — v1).
