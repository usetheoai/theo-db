---
slug: adaptive-filter-strategy
generated_by: roadmap-feature
date: 2026-07-12
status: completed
new_milestone_id: M91
---

# Feature grill — adaptive-filter-strategy (M91)

## Feature & why now (Q1)

Seleção **ADAPTIVE** da estratégia de filtered vector search pela cardinalidade do bitmap, gated M90. Depois que o
M90 entrega o **inline** (bitmap-in-traversal), o M91 adiciona a escolha AUTOMÁTICA da estratégia em runtime pela
seletividade estimada (a cardinalidade do `TIDBitmap` já computado no M90): **ultra-seletivo (<~0.1%) → PRE** (pega
os poucos TIDs do bitmap + rerank exato, sem travessia ANN); **médio (0.1–5%) → INLINE** (o path do M90); **loose
(>~5%) → POST** (o M87, índice ANN puro + filtro barato). **Why now:** decidido na discussão com o owner
(2026-07-12) como a 2ª metade da linhagem inline/adaptive — a peça "adaptive (AM-local)" da tier ② da análise. É o
que dá **recall+custo estáveis em TODA a faixa de seletividade** sem o usuário escolher a estratégia (o que cada
estratégia fixa não consegue: post degrada em seletivo, pre é caro em loose).

## Decisions (grill answers — herdadas do split aprovado no grill do M90 + a discussão)

- **Escopo:** SÓ a orquestração adaptive das 3 estratégias (pre/inline/post) pela cardinalidade do bitmap — o
  branch no AM/executor no início do scan. **NÃO** inclui o re-plan cross-index mid-query do core (tier ③,
  não-alcançável por extensão pura — honesto; documentado como fora de escopo).
- **Gate (measurement-first):** benchmark varrendo a seletividade (0.01% → 30%) mostra recall@10 alto E custo
  (QPS/pages) baixo em TODA a faixa vs cada estratégia FIXA isolada (que degrada em parte da faixa). GO só se o
  adaptive domina o envelope das 3 fixas; honest-negative fecha.
- **Estratégia:** serial, gated M90 (o inline é 1 das 3 estratégias que o adaptive orquestra).
- **SOTA delta:** nenhum peer novo — o pgvectorscale já foi clonado no M90 (`knowledge-base/references/pgvectorscale`,
  PostgreSQL License) e cobre o design; AlloyDB fechado → só design publicado.

## Dependencies

- **M90** `[x]` — o inline (bitmap-in-traversal + Custom Scan Provider); o adaptive orquestra o inline como 1 das 3.
- **M87** `[x]` — o post-filter (a estratégia loose).
- **M89** `[x]` — build escalável.

## Definition of Done (verifiable)

1. O AM/Custom Scan ramifica em runtime pela **cardinalidade do `TIDBitmap`** (a seletividade já conhecida) entre
   PRE (fetch TIDs + rerank exato), INLINE (M90) e POST (M87), com thresholds calibrados.
2. **Benchmark varrendo a seletividade (0.01% → 30%)** mostra que o adaptive mantém **recall@10 alto E custo baixo em
   TODA a faixa**, dominando o envelope das 3 estratégias fixas isoladas (`docs/benchmarks/m91-adaptive-filter.{md,json}`).
   Honest-negative (não domina) é veredito terminal válido.
3. **EXPLAIN** revela a estratégia escolhida (ou um contador/log runtime da escolha — observabilidade, wiring triad).
4. **Zero regressão:** 250+ pg_tests GREEN; testes cobrindo a escolha em cada regime (ultra/médio/loose).
5. Sign-off council-index-storage + council-benchmark.
6. **Fora de escopo declarado:** o re-plan cross-index mid-query do core (tier ③) — documentado como limite honesto
   de extensão pura.

## Top 2 NEW risks

1. **Calibração dos thresholds de seletividade** (onde cortar pre↔inline↔post) — depende de dados/hardware; um
   threshold errado pode escolher a estratégia pior. Mitigação: calibrar empiricamente no benchmark de varredura +
   thresholds como GUC/reloption ajustável (não hardcoded). Owner: implementador.
2. **O adaptive pode não dominar as 3 fixas** em toda a faixa (ex.: overhead de decidir + custo de materializar o
   bitmap sempre) → honest-negative. Mitigação: o gate mede o envelope completo ANTES de fechar; se uma estratégia
   fixa domina, o adaptive vira só um seletor simples ou fecha honesto. Owner: implementador.

## Notes

- SOTA delta: nenhum peer novo (pgvectorscale já em refs desde o M90). Não há `_catalog.md`.
- out-of-scope cross-check: o ROADMAP não tem seção "Explicitly out of scope"; nada a checar.
- **NÃO é claim de QPS-superior** sobre o ScaNN (teto de paradigma M73/M82 permanece) — é claim de recall+custo
  estáveis em toda seletividade. A tier ③ (re-plan cross-index do core) fica explicitamente fora (limite honesto).
