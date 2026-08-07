---
type: Decision
title: ADR 0035 — Veredito MEDIDO do north star vetorial contra ScaNN/AlloyDB
description: Paridade own-code de recall alcançada; superioridade de QPS sobre o ScaNN medida como não-alcançável por extensão Postgres permissiva; throughput multi-cliente superior no regime 128d clusterizado.
resource: git:f7c7b93:docs/adr/0035-m73-northstar-vector-verdict.md
tags: [adr, veredito, north-star, scann, honest-negative, m73]
adr_id: "0035"
adr_status: Accepted
decision_date: 2026-07-10
milestone: M73
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0035
    resource: git:f7c7b93:docs/adr/0035-m73-northstar-vector-verdict.md
    title: ADR-0035 — M73 veredito do North Star vetorial
    last_modified: 2026-07-10
---

O veredito rastreável que o [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md)
exigia. Registra **onde o TheoDB está**, não uma mudança de mandato — essa é decisão separada, no
[ADR 0033](/decisions/0033-north-star-reposition-proposal.md).

# Evidência consolidada

## Eixo 1 — TheoDB contra pgvector

**Recall: paridade de valor alcançada.** O gap foi fechado pelo
[extendCandidates](/decisions/0034-hnsw-extend-candidates-navigability.md): f32 de 0,974 para
**0,990**, SBQ de 0,986 para **0,994** a 500k, contra 0,994 do pgvector.

**Fronteira de latência, honesta:** a iso-recall alta, o TheoDB ainda precisa de ~1,8× o `ef` do
pgvector a 500k — o fix subiu o teto de recall, não igualou a eficiência de recall por `ef`.

**Multi-cliente** ([m72](/benchmarks/m72-qps-multiclient.md), 1M × 128d, 8 clientes concorrentes): a
recall casado ~0,91, o `theodb_hnsw` **supera** o pgvector — 0,917 a 597,7 QPS contra 0,9095 a 539,5
QPS, isto é **+11%**, com p50 de 13,6 contra 16,5 ms — e alcança um recall (0,97 a 354 QPS) em que o
pgvector platôa antes (~0,914) **neste regime clusterizado de 128d**, que é justamente o regime-alvo
do extendCandidates. O build também é ~3× mais rápido. **Honesto:** é o regime favorável ao TheoDB; a
fronteira de alta dimensão e alto recall permanece do pgvector.

## Eixo 2 — o gap de paradigma até o ScaNN

O head-to-head [m33](/benchmarks/m33-scann-headtohead.md) mediu o [ScaNN](/technologies/scann.md)
~**25×** acima em QPS. A vantagem é quantização anisotrópica mais Asymmetric Hashing com LUT SIMD —
não grafo em precisão plena.

O melhor quantizador permissivo do SOTA, o [RaBitQ](/technologies/rabitq.md), medido a 1M × 768d: 8,2
ms a 98,4% de recall — **competitivo** com precisão plena (~10–15 ms), **não** 25×. O ganho dele é
**memória** (5,3 MB residentes na variante em disco), não QPS.

# O veredito

1. **Paridade own-code classe-pgvector de RECALL: ALCANÇADA.** O TheoDB tem tipo vetorial próprio,
   access method HNSW próprio, e recall de valor equivalente ao pgvector a 500k.
2. **Superioridade de QPS vetorial sobre o AlloyDB/ScaNN: NÃO-ALCANÇÁVEL** como extensão Postgres
   permissiva. Perseguida por todos os caminhos honestos e medida: os 25× do ScaNN vêm do algoritmo
   dele — AH-LUT anisotrópico em 128d, com anos de tuning — somados ao fato de **não pagar o imposto
   de MVCC, WAL e heap** que qualquer extensão paga.
3. **Trade-off documentado:** código próprio, paridade de recall, e throughput multi-cliente
   **competitivo a superior no regime 128d clusterizado**, com a **fronteira de alta dimensão e alto
   recall ainda do pgvector**. Regime-dependente, medido, sem claim universal.

**Posicionamento permitido:** "paridade de recall classe-pgvector com índice vetorial próprio" e
"eficiência de memória RaBitQ para billion-scale". **Jamais** "mais rápido que o AlloyDB no
vetor".[^adr0035]

# Alternativas rejeitadas

**Declarar superioridade** — nenhum benchmark a sustenta; o oposto foi medido. **Declarar fracasso do
pilar** — desonesto na outra direção: a paridade own-code de recall **é** entrega real, e a fundação
de memória é diferencial genuíno. **Adiar o veredito esperando uma alavanca mágica** — os caminhos já
foram medidos; o veredito honesto é o entregável.

# Consequências

O north star ganha a **prova medida de onde o TheoDB está**, com rastreabilidade total.

**Honestidade:** o eixo "superar o AlloyDB no QPS vetorial" é medido como não-alcançável por extensão
permissiva. **Isso não é falha de execução — é a fronteira do que a arquitetura permite.** Os
diferenciais reais ficam em abertura, portabilidade, independência de modelo, AI-native/HTAP e
custo/escala, não em QPS vetorial puro.

Confirmado e estendido pelo caminho do access method no
[ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md).

[^adr0035]: ADR-0035 — M73: veredito MEDIDO do North Star vetorial vs ScaNN/AlloyDB
