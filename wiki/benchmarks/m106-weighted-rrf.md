---
type: Measurement
title: m106 — RRF ponderável: evidência de que os pesos mudam o ranking
description: Prova que uma capacidade documentada mas não embarcada passou a existir de fato, e que o default é byte-idêntico à fusão anterior.
resource: git:f7c7b93:docs/benchmarks/m106-weighted-rrf.md
tags: [benchmark, rrf, busca-hibrida, compatibilidade, m106]
milestone: M106
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m106
    resource: git:f7c7b93:docs/benchmarks/m106-weighted-rrf.md
    title: M106 — Weighted RRF
---

# A fórmula

$$ \mathrm{score}(d) = w_{vec} \cdot \frac{1}{k + \mathrm{rank}_{vec}(d)} + w_{txt} \cdot \frac{1}{k + \mathrm{rank}_{txt}(d)} $$

# O claim duplo

**Os pesos mudam mensuravelmente o ranking fundido** — o que move uma capacidade **documentada mas não
embarcada** para o estado de entregue. Documentar algo que não existe é o drift que
[m37](/benchmarks/m37-ai-summarize-validation.md) encontrou na direção oposta; aqui a correção é fazer
existir.

**E o default de peso 1,0 em ambas as pernas é byte-idêntico à fusão anterior.** Essa segunda metade é
o que torna a mudança segura: **nenhum usuário existente vê diferença** a menos que peça.

Provar compatibilidade por **identidade byte a byte**, e não por "deve ser equivalente", é o padrão que
este repositório aplica em toda mudança de formato ou de semântica.

# Validação de borda

Os pesos são validados **finitos e não-negativos na fronteira**, com erro tipado em peso negativo. E
**zero desabilita uma perna** — comportamento definido, não indefinido.

# Contexto

A superfície é a [busca híbrida](/features/06-busca-hibrida.md); a medição decision-grade da fusão em
dataset real é [m53](/benchmarks/m53-hybrid-beir.md), e o teste de significância que a linhagem passou a
exigir é [m123](/benchmarks/m123-hybrid-significance.md).
