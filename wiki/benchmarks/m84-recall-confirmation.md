---
type: Measurement
title: m84 — confirmação em recall alto, e o bug que o teto de 0,80 escondia
description: O ganho anterior fora medido a um teto de recall causado por um pool de rerank travado em 64 — um max cujo piso sempre vencia; corrigido, o ganho se mantém em recall alto.
resource: git:f7c7b93:docs/benchmarks/m84-recall-confirmation.md
tags: [benchmark, recall, bug, storage-separation, m84]
dataset: SIFT1M
milestone: M84
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m84
    resource: git:f7c7b93:docs/benchmarks/m84-recall-confirmation.md
    title: M84 — pg_scann v5 high-recall confirmation
    last_modified: 2026-07-11
---

**Veredito: GO** — o layout separado **mantém o ganho em recall alto**.

# A pergunta decisiva

O [m83](/benchmarks/m83-split-storage-spike.md) mediu 6–12×, **mas a um teto de recall de ~0,80**. Um
ganho grande num regime de recall baixo pode simplesmente não existir onde a aplicação opera.

**Confirmar o ganho no ponto de operação que importa** é o que separa um número de uma capacidade.

# O bug que causava o teto

O pool de rerank estava **efetivamente travado em 64**: a expressão usava um `max` cujo **piso sempre
vencia**, porque o valor configurado nunca o excedia. **O parâmetro existia e não fazia nada.**

Essa classe de defeito — um knob silenciosamente inerte — é especialmente perigosa em benchmark: ele
produz uma curva plausível que não responde ao controle, e a explicação natural ("é o teto do
algoritmo") é errada.

# O resultado

Corrigido o pool, **o ganho se mantém na faixa de recall 0,98 a 0,998** — o ponto de operação real.

# Continuação

A observação de que as leituras aleatórias de vetores em precisão plena **erodem o ganho na fronteira de
alto recall** motivou o passo seguinte: um tier de rerank comprimido, medido em
[m85](/benchmarks/m85-sq8-refine.md).
