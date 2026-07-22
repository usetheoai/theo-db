---
slug: inline-filter-pushdown
generated_by: roadmap-feature
date: 2026-07-12
status: completed
new_milestone_id: M90
---

# Feature grill — inline-filter-pushdown (M90)

## Feature & why now (Q1)

Filtered vector search com o filtro empurrado **PARA DENTRO** da travessia do índice (inline bitmap-in-traversal),
fechando a lacuna vs o "inline filtering" do AlloyDB. O M87 é classe pgvector-relaxed_order (post-filter +
iterative re-search): o `WHERE marca=X AND preco<Y ORDER BY e <-> q` é filtrado DEPOIS que o `amgettuple` devolve a
tupla, e o recall **degrada no regime de seletividade média (0.1–5%)** — onde o post-filter descarta muitos
candidatos e o pre-filter é caro. **Why now:** discussão com o owner (2026-07-12) confirmou que o inline/adaptive
NÃO é um "gap de paradigma" (correção honesta) — é implementável por extensão via o **Custom Scan Provider** do
Postgres (o mesmo mecanismo de TimescaleDB/Citus/pg_strom, mais poderoso que o `amgettuple`), reusando o motor de
bitmap nativo. Fecha uma capacidade de produto real (filtered vector search é das necessidades mais comuns).

## Decisions (grill answers)

- **Decomposição:** 2 milestones — **M90 = inline** (Custom Scan Provider + bitmap-in-traversal); **M91 = adaptive**
  (escolha pre/inline/post pela cardinalidade do bitmap), gated M90. Serial gate-driven, measurement-first.
- **Gate:** **spike-first D3-gate** — primeiro um spike mínimo que mede recall@10 sob filtro no regime médio
  (seletividade ~1%) INLINE vs o M87 post-filter num benchmark reproduzível. **GO só se o inline melhora o recall
  medido**; honest-negative é terminal válido (igual ao D3/M83). Não construir o Custom Scan completo antes de medir.
- **DoD-chave:** recall@10 sob WHERE seletivo (~1%) **MEDIDO estritamente melhor que o M87 post-filter**
  (`docs/benchmarks/m90-*`); o Custom Scan Provider intercepta o padrão e consulta o bitmap na travessia; EXPLAIN
  mostra o custom scan; 250+ pg_tests GREEN (byte-idempotente no path sem filtro); sign-off council-index-storage +
  council-benchmark. Se o inline não bater o M87 → honest-negative fecha.
- **SOTA delta:** adicionar o **pgvectorscale** como referência (design da filtragem empurrada no DiskANN).

## Dependencies

- **M87** `[x]` — o scan IVF iterativo (`scan_ivf_aq_split`, já param `probes`/`rerank_pool`) que serve de ponto de
  integração do bitmap-in-traversal.
- **M89** `[x]` — o build escalável (para medir em índices maiores se preciso).

## Definition of Done (verifiable)

1. Um **Custom Scan Provider** (`set_rel_pathlist_hook` + `CustomScanMethods`/`CustomExecMethods`) intercepta
   `WHERE <preds escalares> ORDER BY e <-> q LIMIT k`, roda o sub-plano bitmap (BitmapAnd nativo → `TIDBitmap`,
   MVCC-correto, Regra 9) e o passa para o `scan_ivf_aq_split`, que consulta o bitmap na Fase 1 da travessia
   (candidato reprovado é PULADO antes de custar um slot do top-k; cresce probes se o top-k não encheu).
2. **recall@10 sob filtro seletivo (~1%) MEDIDO estritamente > M87 post-filter**, num benchmark reproduzível
   (`docs/benchmarks/m90-inline-filter.{md,json}`). Honest-negative (não bate o M87) é veredito terminal válido.
3. **EXPLAIN** mostra o custom scan node para a query filtrada ordenada.
4. **Zero regressão:** 250+ pg_tests GREEN; o path SEM filtro é byte-idempotente (mesmo caminho do M87).
5. Sign-off council-index-storage (integração AM/executor) + council-benchmark (medição de recall honesta).
6. Sem novas deps de runtime além do core Postgres (Custom Scan é do próprio engine).

## Top 2 NEW risks

1. **Integração Custom Scan Provider ↔ o AM/scan state** — o Custom Scan é um mecanismo pesado (planejamento +
   execução); casar o sub-plano bitmap com o `IndexScanDesc`/scan state do `theodb_ivfflat` sem quebrar MVCC/snapshot
   é o risco técnico central. Mitigação: espelhar o design do pgvectorscale (Rust+pgrx, permissivo) + spike-first +
   council-index-storage. Owner: implementador.
2. **O inline pode NÃO melhorar o recall vs M87 no regime medido** (o iterative re-search do M87 já é bom em muitos
   casos) → honest-negative. Mitigação: o gate D3 mede ANTES de construir tudo; se negativo, fecha honesto e barato
   (anti-sunk-cost). Owner: implementador.

## Notes

- SOTA delta: **pgvectorscale** clonado em `knowledge-base/references/pgvectorscale` (PostgreSQL License — permissiva,
  gate D1 PASSA; Rust+pgrx como nós, módulo `access_method/labels/` de filtragem). AlloyDB é fechado → só design
  publicado. Não há `_catalog.md` no projeto; a referência fica registrada aqui.
- out-of-scope cross-check: o ROADMAP não tem seção "Explicitly out of scope"; nada a checar.
- Slug re-slugado de `inline-filtered-ann` → `inline-filter-pushdown` (evita colisão de substring com o M87
  "Filtered ANN", que é o post-filter — capacidade distinta).
- **NÃO é claim de QPS-superior** (teto de paradigma M73/M82 permanece) — é claim de recall-estável-sob-filtro.
