---
type: Measurement
title: m67 — convergência do recomendador de ef, com duas ressalvas que o limitam
description: Converge e atinge os alvos, mas num corpus fácil demais para estressar a curva — e o recomendador é ótimo na média, não seguro na cauda.
resource: git:f7c7b93:docs/benchmarks/archive/m67-autotune.md
tags: [benchmark, autotune, convergencia, cauda, ressalvas, arquivo, m67]
milestone: M67
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m67
    resource: git:f7c7b93:docs/benchmarks/archive/m67-autotune.md
    title: M67 — autotune convergence benchmark
---

**Veredito: converge, com nuance.** O recomendador devolve o menor `ef` e o recall médio fica acima dos
alvos.

# As duas ressalvas que limitam a leitura

**O corpus é fácil demais.** O baseline já atinge recall pleno com um `ef` modesto, e **todos os alvos
convergem para o mesmo valor mínimo** — ou seja, **a curva de `ef` não é estressada**. Um corpus mais
difícil mostraria o comportamento de escala que aqui não aparece.

**O recomendador é ótimo na MÉDIA, não seguro na CAUDA.** Cerca de 12% das queries ficam fora do alvo.

Essa segunda ressalva é a que muda decisões operacionais: um recomendador que atinge o alvo **em média**
pode deixar uma fração relevante das queries abaixo do recall esperado — e para uma aplicação, o que dói
é a cauda, não a média.

**Reportar a fração da cauda em vez de só a média** é o que permite ao operador saber se aquilo é
aceitável no caso dele. O [runbook de diagnóstico](/runbooks/vector-scan-diagnostics.md) repassa
explicitamente essa limitação.

# O que a base de estimativa preserva

O recall estimado usa **ground truth exato amostrado** — a base honesta — e **não** um estimador sem
ground truth, que seria mais barato e não confiável.

# Contexto

A decisão de fazer um recomendador determinístico, e **não** auto-tune online, com o racional de que
mutar o parâmetro vivo oscila e colide com o do usuário, é o
[ADR 0026](/decisions/0026-m67-autotune-recommender.md).
