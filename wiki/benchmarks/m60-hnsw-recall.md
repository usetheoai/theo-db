---
type: Measurement
title: m60 — recall do HNSW próprio a 500k: a premissa e o critério estavam errados
description: Refuta duas coisas de uma vez — que houvesse um gap específico contra a referência, e que o alvo absoluto fosse alcançável, já que a própria referência não o atinge.
resource: git:f7c7b93:docs/benchmarks/m60-hnsw-recall.md
tags: [benchmark, recall, hnsw, premissa, criterio, honest-negative, m60]
milestone: M60
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m60
    resource: git:f7c7b93:docs/benchmarks/m60-hnsw-recall.md
    title: M60 — Recall do HNSW próprio a 500k×768d
    last_modified: 2026-07-10
---

**Veredito: honest-negative, e o critério do milestone está mal especificado.**

# A premissa herdada, e por que ela era frágil

O milestone partia da suposição — **inferida de escalas DIFERENTES** — de que o índice próprio tinha um
gap de recall de 2 a 3 pontos **específico** contra a referência, e que o alvo absoluto de 0,99 era
alcançável.

**Comparar números vindos de escalas diferentes é a fonte do erro**, e o head-to-head no **mesmo corpus**
refuta as duas coisas.

# A medição decisiva

| Índice (mesmo corpus, 500k × 768d, GT exato) | melhor recall@10 | p50 |
|---|---|---|
| **referência** | **0,988** | 12,2 ms |
| próprio, precisão plena | 0,974 | ~15,6 ms |
| próprio, quantizado | **0,986** | mais lento |

**A própria referência só chega a 0,988.** Logo **o alvo de 0,99 é artefato do dado**, não uma barra que
alguém esteja falhando — são clusters apertados em alta dimensão, com muitos vizinhos quase
equidistantes, e a classe inteira de índice topa abaixo de 0,99 nessa distribuição.

E o caminho quantizado do índice próprio **já está em paridade** (0,986 contra 0,988, dentro do ruído de
um slot de ground truth em 500).

# A consequência

O critério é reenquadrado para **paridade medida com a referência**, e não valor absoluto — o
[ADR 0030](/decisions/0030-m60-recall-parity-not-absolute-099.md).

O gap remanescente do caminho em precisão plena fica como follow-up autorizado, com **cinco alavancas já
refutadas por medição**.

# O que finalmente moveu o recall

Nenhuma das cinco. Foi a mudança para análise **white-box** — medir a *estrutura* do grafo em vez de
varrer parâmetros — que revelou que **100% das misses eram de roteamento**, levando ao fix do
[ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md) e ao ganho de ~5 pontos em
[gap1](/benchmarks/gap1-extend-candidates.md).

**Sete tentativas black-box não acharam o que uma medição estrutural achou.**
