---
type: Measurement
title: m51 — SBQ inline: read path correto, ganho de QPS não materializado
description: Atinge o gate de recall com folga mas não entrega throughput na escala medida — exatamente o que a régua anterior previra, por não haver pressão de memória.
resource: git:f7c7b93:docs/benchmarks/m51-sbq-inline.md
tags: [benchmark, sbq, quantizacao, gate, escala, m51]
milestone: M51
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m51
    resource: git:f7c7b93:docs/benchmarks/m51-sbq-inline.md
    title: M51 — SBQ-inline no theodb_hnsw
    last_modified: 2026-07-06
---

**Veredito:** read path **correto**, com o gate de recall ≥0,99 **atingido** — 0,9993. **Mas o ganho de
QPS NÃO se materializa nesta escala**, por não haver pressão de memória — **exatamente o que a
[régua anterior](/benchmarks/m50-sota-ruler.md) previu**.

Uma previsão feita antes e confirmada depois é o sinal de que o modelo mental do mecanismo está certo,
mesmo quando o resultado é negativo.

# A decisão que decorre

**Reter a implementação**, como opt-in com default desligado, e **manter o claim de ≥2× de QPS como
follow-up rastreado**, mensurável apenas em escala com pressão de memória — **não vendido como
cumprido**.

O raciocínio completo está no [ADR 0015](/decisions/0015-sbq-inline-keep-kill.md), e a medição em escala
que finalmente falsificou a tese é o [m57](/benchmarks/m57-sbq-superiority.md).

# Caveats

Escala reduzida numa máquina contendida, por decisão registrada. **Os números absolutos carregam ruído; a
leitura relativa é robusta**, consistente nos três runs com desvio de recall ≤ 0,006.

E há uma ressalva fina sobre o próprio número de recall: ele é **o único acima de 0,99 neste benchmark,
mas por comparação NÃO-CASADA** — os baselines só foram varridos até um `ef` menor, e com pool
equivalente atingiriam recall comparável. **Marcar o recall como não-casado impede que ele seja lido como
teto superior**, que seria a conclusão errada.
