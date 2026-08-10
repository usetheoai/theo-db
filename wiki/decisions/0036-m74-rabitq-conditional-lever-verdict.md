---
type: Decision
title: ADR 0036 — RaBitQ é alavanca viável, mas o ganho é memória, não QPS
description: O melhor quantizador permissivo do SOTA foi medido a 1M×768d e entrega 32× de compressão sem entregar QPS — o AM completo não é construído, e o core fica como fundação de escala.
resource: git:f7c7b93:docs/adr/0036-m74-rabitq-conditional-lever-verdict.md
tags: [adr, rabitq, quantizacao, memoria, billion-scale, honest-negative, m74]
adr_id: "0036"
adr_status: Accepted
decision_date: 2026-07-10
milestone: M74
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0036
    resource: git:f7c7b93:docs/adr/0036-m74-rabitq-conditional-lever-verdict.md
    title: ADR-0036 — M74 veredito do lever RaBitQ
    last_modified: 2026-07-10
---

Fecha o pilar vetorial com a saída mais interessante das três previstas: uma alavanca **viável**,
com ganho **real**, **fora do eixo** que o milestone perseguia.

# O gate condicional

Este milestone só arrancaria se houvesse alavanca de quantização **ainda não refutada** pelo
[SBQ](/decisions/0018-m57-sbq-inline-not-superior.md) e pelo
[anisotrópico+AH](/decisions/0019-m59-ah-needs-code-vector-separation.md) — ambos no carrier HNSW —,
e sob regra anti-sunk-cost: proibido implementar o access method completo sem evidência prévia de
viabilidade.

# Evidência

O candidato é o [RaBitQ](/technologies/rabitq.md) — 1-bit, training-free, com bound de erro provado —
cujo core já fora vendorizado ([ADR 0032](/decisions/0032-vendor-rabitq-rs-core.md)). Spike medido a
1M × 768d:

| Índice RaBitQ | recall pico | p50 no pico | memória residente |
|---|---|---|---|
| MSTG-mem (grafo + RaBitQ) | 98,4% | **8,2 ms** | 3,4 GB |
| MSTG-disk (mmap) | 98,4% | 245 ms | **5,3 MB** |
| IVF-RaBitQ | 91% | 17,7 ms | — |
| *precisão plena (referência)* | ~0,98 | ~10–15 ms | ~3 GB |

# Decisão

**A alavanca é viável e não-refutada — mas o ganho medido é memória e escala, não superioridade de
QPS.** Portanto:

1. **Não implementar agora o AM IVF-RaBitQ completo** perseguindo ganho de QPS: a medição mostra que
   esse ganho não existe neste regime. Construir o AM inteiro só para igualar a latência que já
   temos seria esforço sem necessidade de projeto.
2. **Manter o core vendorizado como fundação** de uma feature futura de **memória e billion-scale** —
   32× de compressão, 5,3 MB residentes a 98,4% na variante em disco —, posicionada como "escala e
   custo", jamais como "mais rápido que o AlloyDB". O AM completo fica escopado como follow-up,
   gated por demanda real.
3. O veredito de superioridade de QPS do pilar continua sendo o do
   [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md). Este ADR confirma que **o melhor
   quantizador permissivo do SOTA não muda esse veredito**.

# Alternativas rejeitadas

**Implementar o AM completo agora buscando superioridade de QPS** — refutado pela medição; o esforço
é bem-vindo quando a necessidade existe, e aqui a medição diz que não existe nesse eixo. **Declarar
"nenhuma alavanca viável"** — seria desonesto na direção oposta: o RaBitQ **é** viável, correto e
eficiente em memória, e essa declaração apagaria a descoberta real. **Perseguir os 25× com mais bits
ou mais rerank** — o recall do RaBitQ 1-bit trava em 98,4%, chegar a 99+ exige rerank que come a
vantagem de latência, e nada disso fecha o gap de paradigma.[^adr0036]

# Consequências

O pilar fecha com veredito **honesto e medido**. O core fica pronto e atribuído para quando
billion-scale for demanda real, e **nenhuma complexidade acidental foi adicionada** — o AM completo
não foi construído especulativamente.

O eixo original do north star não foi alcançado por esta alavanca, e este ADR não inventa vitória:
entrega a prova medida de que o SOTA permissivo de quantização não a alcança.

[^adr0036]: ADR-0036 — M74: veredito do lever condicional de quantização (RaBitQ)
