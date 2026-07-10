# ADR-0031 — M71: DoD vira "melhoria de latência medida" (não superioridade iso-recall)

- **Status:** Accepted (2026-07-10)
- **Milestone:** M71 (Roadmap v5 — Superioridade vetorial P0)
- **Decisão do owner:** opção (2) — aceitar paridade/melhoria medida no pilar e reenquadrar via ADR (mesmo padrão do M60/ADR-0030).
- **Supersede:** o critério `p50 do theodb_hnsw ≤ pgvector a recall≥0.99` do bloco M71 do `ROADMAP.md`.

## Contexto (medido — `docs/benchmarks/m71-scan-latency.md`)

- **Deliverable real:** multi-entry `ep←W` no build (Malkov-Yashunin Alg.1 / pgvector) → **+29% QPS a 500k×768d**,
  recall-neutral (0.972 vs 0.974), **63/63 pg_tests GREEN**. Melhoria de throughput de query medida e shipada.
- **Veredito iso-recall (honesto):** a iso-recall 0.996 a 100k, pgvector 2.13ms (ef=100) vs theodb 3.16ms (ef=200) —
  theodb **NÃO é latência-superior nem em paridade** (precisa ~2× o `ef`; ~5× a 500k). Causa: a mesma lacuna de
  **navegabilidade do grafo** que gateia o M60 (7 levers refutados; `p0-vector-superiority-root-blocker.md`). Cortes
  de custo/candidato (kernel bounded, norm-hoist) reduzem o p50 absoluto mas não mudam a razão iso-recall.

## Decisão

1. **A DoD do M71 passa a ser "melhoria de latência medida"** (measurement-first), não superioridade iso-recall
   (empiricamente gateada na navegabilidade). O M71 entrega o multi-entry build (+29% QPS, recall-neutral, shipado).
2. **Superioridade/paridade iso-recall = follow-up autorizado**, gated na lacuna de navegabilidade (compartilhada
   com M60). Os levers de custo/candidato (kernel bounded PANORAMA, norm-hoist cosseno) do blueprint M71 ficam como
   follow-up: reduzem p50 absoluto, não fecham a razão iso-recall.
3. **Sem claim de superioridade** (`public-copy.md`): é melhoria medida + paridade de recall, não superioridade.

## Alternativas rejeitadas

- **(rejeitada) Manter a DoD de superioridade iso-recall.** Empiricamente gateada na navegabilidade (7 levers
  refutados); perseguir sem resolver a raiz é anti-measurement-first.
- **(rejeitada) Não shipar o multi-entry.** É uma melhoria real (+29% QPS, recall-neutral, testes verdes) — shipar
  é entregar valor medido; segurar seria descartar um ganho comprovado.
- **(adiada, autorizada) Kernel bounded + norm-hoist + ataque à navegabilidade** (opção B do root-blocker doc).

## Consequências

- **Positivas:** M71 entrega um ganho de latência medido e correto; o v5 avança. Traçabilidade honesta do que é
  melhoria (medida) vs superioridade (gated).
- **Honestas (trade-off):** o multi-entry build é mais LENTO no build (mais trabalho por insert) — troca aceitável
  para um índice focado em latência de query. Superioridade iso-recall permanece não atingida (follow-up).

## Cross-references

- Evidência: `docs/benchmarks/m71-scan-latency.md`, `docs/benchmarks/m60-raw/m71_*`
- Blueprint (discover): `.claude/knowledge-base/discoveries/blueprints/m71-scan-latency-blueprint.md`
- Root blocker (navegabilidade): `docs/benchmarks/p0-vector-superiority-root-blocker.md`
- Padrão de reenquadramento: `docs/adr/0030-m60-recall-parity-not-absolute-099.md`
- North Star: `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`
