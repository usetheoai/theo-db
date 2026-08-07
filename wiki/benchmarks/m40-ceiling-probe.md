---
type: Measurement
title: m40 — sonda de teto: o limite é o quantizador ou o carrier?
description: Uma medição barata, antes de qualquer implementação, que provou que melhorar o quantizador não poderia ajudar — porque o teto estava na geração de candidatos.
resource: git:f7c7b93:docs/benchmarks/m40-ceiling-probe.md
tags: [benchmark, sonda, carrier, quantizador, measurement-first, m40]
milestone: M40
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m40cp
    resource: git:f7c7b93:docs/benchmarks/m40-ceiling-probe.md
    title: M40 — Ceiling probe
    last_modified: 2026-07-03
---

**O melhor exemplo de medição barata evitando trabalho caro.**

# A pergunta

O milestone anterior nomeara "melhorar o quantizador" como a próxima alavanca de recall. Antes de
construir isso, measurement-first fez a pergunta certa:

**No nosso pipeline — carrier gera candidatos, quantizador rankeia, rerank exato corrige —, o recall está
limitado pelo quantizador ou pelo carrier?**

**Se o teto for o carrier, um quantizador melhor não pode ajudar.** Nenhuma quantidade de esforço no
ranking recupera um vizinho que **nunca entrou no conjunto de candidatos**.

# A medição

Variar os dois knobs independentemente e ver qual move o recall:

| Configuração | probes | over_fetch | recall (produto) | recall (escalar) |
|---|---|---|---|---|
| baseline | 16 | 16 | 0,770 | 0,769 |
| **mais probes** | 44 | 16 | **0,944** | 0,943 |
| mais probes | 100 | 16 | 0,944 | 0,943 |
| mais over_fetch | 16 | **64** | 0,787 | 0,787 |
| ambos no máximo | 100 | 64 | **1,000** | 0,996 |

**A leitura é inequívoca.** Aumentar `over_fetch` — que dá mais trabalho ao quantizador — move o recall
de 0,770 para 0,787: quase nada. Aumentar `probes` — que faz o **carrier** gerar mais candidatos — move
de 0,770 para 0,944.

**O teto é o carrier.** E os dois quantizadores medem praticamente o mesmo em todas as configurações, o
que confirma que o quantizador não é a variável.

# A consequência

A alavanca proposta **mirava o gargalo errado**, e isso foi estabelecido **antes de qualquer
implementação**. O milestone foi re-escopado para a pergunta que a sonda tornou óbvia: **qual carrier
próprio vence o trade-off de recall por QPS?** — que é o [m40 carrier](/benchmarks/m40-carrier.md).

Este artefato custa uma varredura de knobs. A alternativa custaria implementar uma loss anisotrópica e
descobrir depois. **É o tipo de medição que paga por si mesma muitas vezes.**
