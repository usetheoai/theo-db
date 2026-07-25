# Review — M155 (spike Top-N — HONEST-NEGATIVE medido)

**Data:** 2026-07-25 · **Branch:** develop · **Tipo:** spike measurement-first (sem código de produção; só medição + docs + re-escopo).
**Council:** council-benchmark (auditoria de honestidade dos números — mandato do owner: nunca mascarar números).

## Verdict: READY_TO_MERGE (spike)

Sem código de produção → sem superfície de correção/segurança/wiring. O deliverable é a MEDIÇÃO honesta + o veredito.
A auditoria de números (benchmark council) confirma que o honest-negative é justificado pelos dados e nada foi mascarado.
Uma infidelidade doc↔artefato foi pega e CORRIGIDA: a célula q25 dizia `top-N heapsort` mas o artefato bruto
(`m155_spike_explain.txt:22`) reporta `still in progress  Memory: 0kB` — o doc agora cita o rótulo verbatim + nota de
rodapé (a conclusão "Sort não é gargalo" sustenta-se pelo tempo ~1,6ms, independente do rótulo). Derivação do Sort
uniformizada (delta-incremental; q33 = ~2,6ms). Byte-fiel ao artefato = o mandato do owner ("nunca mascare números").

## O achado (measurement-first, refuta a hipótese)

EXPLAIN ANALYZE das Top-N do ClickBench (v0.146.0) — números do `m155_spike_explain.txt`:
- **PG já usa `Sort Method: top-N heapsort`** (heap O(n log k) = o algoritmo do TopK do DataFusion) → não há sort completo a evitar.
- Sort node: **~2ms** (q24/25/26), **~4ms** (q33) — NÃO é gargalo.
- CustomScan (scan + materialização row-by-row, gargalo M148): **~150ms** para 13005 linhas (~98% do tempo).
- q23 (`SELECT *` LIKE '%google%'): 0 linhas casaram → 1377ms é o scan, não o Sort (honestamente não-medível p/ TopK).
- Cobertura marginal = **0** (`columnar_customscan_count=21`; as Top-N já roteiam; nenhuma declina por Sort/Limit).

## Veredito honesto

**HONEST-NEGATIVE: não implementar o roteamento-ao-TopK.** Seria complexidade sem valor (CLAUDE.md esforço≠complexidade,
anti-sunk-cost): um operador novo de alto risco de correção para ~2-4ms em queries que já roteiam, mirando um sort
completo que o PG não faz. Byte-identidade do top-k com empates do LIMIT é mal-definida (PG não-determinístico na
fronteira). **Lever real apontado** (materialização preguiçosa de colunas de saída — ataca o M148 no regime SELECT *
top-N) como candidato futuro M156. Owner delegou a decisão aos dados com o mandato "nunca mascare números".

## Hard gates
- Sem código de produção alterado (só docs + ROADMAP re-escopo). Sem secrets. Sem commit em main. Sem trailer Co-Authored-By.
  CHANGELOG `[Unreleased]` (Changed — M155 re-escopo honest-negative). ✓

## Evidência
- `docs/benchmarks/m155-topn-spike.md` + `docs/benchmarks/m155-artifacts/{m155_spike_explain.txt, m155_base_coverage.json}`.
