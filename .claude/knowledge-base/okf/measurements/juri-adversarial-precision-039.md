---
type: Measurement
title: O júri adversarial descartou 11 de 18 achados — precision 0.39
description: Medida da precisão de um review multi-agente: a maioria dos descartes era convenção deliberada lida como defeito.
tags: [review, juri, processo]
timestamp: 2026-07-30T00:00:00Z
---

# O júri adversarial descartou 11 de 18 achados — precision **0.39**

## O número

Num ciclo de review com júri adversarial: **18 achados levantados, 11 descartados** → precisão de **0,39**.

A maioria dos descartes era da mesma natureza: *"os line pins vão apodrecer"* e *"número repetido"* — refutados
como **convenção deliberada** do doc-set (o padrão "Verificado em …:NNN") e consistentes com os artefatos de
benchmark existentes.

## As duas leituras, e ambas são verdadeiras

1. **O júri está funcionando.** Descartar 61% dos achados é o júri fazendo o trabalho dele — sem ele, onze
   correções desnecessárias entrariam no código, cada uma com risco próprio.
2. **Os agentes geram muito falso-positivo.** Precisão de 0,39 significa que ler os achados brutos, sem o júri,
   custaria mais do que renderia.

## Por que os descartes ficam no audit trail

Um achado refutado **não é ruído a apagar**: ele registra que a convenção foi questionada e defendida. Quando a
mesma objeção voltar no ciclo seguinte, o rastro responde sem custar outra rodada.

## Uso prático

Ao dimensionar um review multi-agente, conte com ~1/3 de achados acionáveis. Um pipeline que trate cada achado
como verdade — ou que dispense o júri para "ir mais rápido" — está trocando um custo visível (a rodada do júri)
por um invisível (correções desnecessárias em código que funcionava).

## Relacionados

- [technique/controle-positivo](../techniques/controle-positivo.md)
