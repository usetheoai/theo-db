---
type: Decision
title: ADR 0015 — SBQ-inline no theodb_hnsw: reter, com o claim de QPS delimitado
description: A quantização SBQ inline preserva recall ≥0,99 e é opt-in sem regressão, mas não entrega ganho de QPS a 25k — o benefício é de escala com pressão de memória, ainda não medido.
resource: git:f7c7b93:docs/adr/0015-sbq-inline-keep-kill.md
tags: [adr, sbq, quantizacao, hnsw, m51, honest-negative]
adr_id: "0015"
adr_status: Accepted
decision_date: 2026-07-06
milestone: M51
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0015
    resource: git:f7c7b93:docs/adr/0015-sbq-inline-keep-kill.md
    title: ADR 0015 — SBQ-inline no theodb_hnsw
    last_modified: 2026-07-06
---

Um gate keep/kill explícito, decidido por medição — e um exemplo de recusa a afirmar um ganho que
o regime medido não podia mostrar.

# Contexto

O M51 implementou a quantização **SBQ inline** no `theodb_hnsw` (layout v2): códigos SBQ nos
element tuples, o walk do [HNSW](/technologies/hnsw.md) pontuando por distância de Hamming
(barata), e rerank exato em f32 sobre o topo `k·over_fetch`. A aposta era mudar o assíntota de QPS
atacando o custo por candidato do scan.

O critério exigia um gate anti-sunk-cost: reter **apenas se** `recall@10 ≥ 0,99` for preservado
**e** o efeito for maior que a variância; caso contrário, honest-negative e ADR mantendo f32.

# Evidência medida

Em n=25k × 128d, cosine, 3 runs ([m51](/benchmarks/m51-sbq-inline.md)):

- **Gate de recall ≥0,99: ATINGIDO.** SBQ-inline (8 bits, `ef=400`, `over_fetch=16`) dá
  `recall@10 = 0,9993` — prova de que o read path recupera recall corretamente. **Ressalva
  registrada:** é o único spec acima de 0,99 *neste* benchmark, mas por comparação **não-casada** —
  os baselines f32 e pgvector só foram varridos até `ef=400`, e com `ef≈6400` casado atingiriam
  recall comparável.
- **QPS a 25k: o SBQ NÃO é mais rápido** — paridade ou pior contra f32 em recall casado (0,946 a
  93 qps contra f32 0,93 a 95 qps); no gate ≥0,99, custa QPS (27–38 qps). **Sem pressão de
  memória** — o corpus f32 cabe em RAM a 25k —, a compressão não tem onde ganhar.
- Honest-negative: a configuração 2-bit com `ef=100` topa em recall 0,52, porque a navegação por
  Hamming é lossy. O gate exige bits e carrier adequados.

# Decisão: RETER, com o claim delimitado

**Reter** porque o read path é correto e recupera recall ≥0,99 — o gate central —, e porque é
**opt-in** (`WITH (sbq_bits=N)`, default 0 = f32), portanto **zero regressão** em índices
existentes.

**Não é kill** porque o benefício de QPS do SBQ é propriedade de **escala com pressão de
memória**, não medível a 25k. Matar aqui seria sobre-interpretar uma calibração de escala
limitada.

Este ADR **não afirma** ganho de QPS. Afirma um SBQ-inline correto e preservador de recall cujo
benefício de escala está pendente de medição.[^adr0015]

# Critério de reabertura

Esta é a cláusula de saída que faltava ao AM próprio. Reabrir a decisão de composição — AM próprio
contra compor sobre pgvector e pgvectorscale — **se**, medido em escala com pressão de memória
(≥250k a 1536d, ou 1M a 768d, em máquina quieta):

- o SBQ-inline seguir **≤ pgvector + DiskANN** no Pareto recall×QPS realista, **e**
- nenhuma outra alavanca pendente (co-localização de vizinhos, LUT16 ADC) fechar o gap.

Nesse caso o custo de manter um AM próprio — rebase, superfície de crash-safety — deixa de se
justificar.

# Alternativas rejeitadas

**Kill (manter só f32):** o read path é correto, opt-in e sem regressão; matar descartaria trabalho
correto por medição fora do regime-alvo. **Afirmar o ganho de QPS:** desonesto — não foi medido no
regime onde ele existe.

[^adr0015]: ADR 0015 — SBQ-inline no theodb_hnsw: keep/kill do AM próprio
