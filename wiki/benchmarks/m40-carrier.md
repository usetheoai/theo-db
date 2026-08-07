---
type: Measurement
title: m40 — head-to-head entre os carriers próprios (grafo contra listas)
description: O IVFFlat próprio vence em dados sintéticos, mas o documento declara que gaussiano aleatório é o pior caso para índice de grafo e o veredito não generaliza.
resource: git:f7c7b93:docs/benchmarks/m40-carrier.md
tags: [benchmark, carrier, hnsw, ivfflat, generalizacao, m40]
milestone: M40
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m40car
    resource: git:f7c7b93:docs/benchmarks/m40-carrier.md
    title: M40 — Carrier head-to-head
    last_modified: 2026-07-03
---

Nasce do re-escopo que a [sonda de teto](/benchmarks/m40-ceiling-probe.md) forçou: se o carrier é o
limitante, **qual carrier próprio vence o trade-off de recall por QPS?**

**Método:** os dois access methods persistidos sobre o **mesmo corpus** com **ground truth exato por
força bruta**, varrendo o knob de query de cada um e comparando **a QPS casado**.

# Veredito

**O IVFFlat vence em dados sintéticos** — a QPS casado, ele tem recall substancialmente maior ao longo de
quase toda a curva. E o HNSW é **3 a 5× mais lento a recall casado**, o que o documento lê como **espaço
real de otimização**, não como derrota definitiva.

# A ressalva que limita a generalização

Declarada no próprio veredito: **gaussiano aleatório sintético é o PIOR CASO para um índice de grafo**.
Em dados uniformes de alta dimensão os pontos são quase equidistantes, e a navegabilidade do grafo — que
é justamente a vantagem dele — não tem estrutura para explorar.

**O veredito NÃO generaliza para dados reais estruturados em escala**, e o documento diz qual medição
seria necessária para isso.

Essa ressalva se provou correta: em dados reais, o HNSW **domina** — como
[m32](/benchmarks/m32-scale-sift1m.md) e a [decisão de índice default](/decisions/m2-index-decision.md)
mostram. **Um veredito de carrier medido em corpus sintético teria levado à escolha errada** se a
ressalva não estivesse ali.

# Por que isso importa como método

Este par de artefatos — a sonda e o head-to-head — mostra o padrão completo: **medir barato para achar a
variável certa, medir a variável certa, e declarar o regime em que o resultado vale.** O terceiro passo
é o que impede o segundo de virar conclusão errada.
