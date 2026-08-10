---
type: Measurement
title: dados brutos do head-to-head de carriers
description: A tabela por configuração que sustenta o veredito de carrier, com a distinção explícita entre dispersão por query e variância entre execuções.
resource: git:f7c7b93:docs/benchmarks/archive/2026-07-03-m40-carrier-headhead.md
tags: [benchmark, dados-brutos, carrier, dispersao, arquivo]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m40raw
    resource: git:f7c7b93:docs/benchmarks/archive/2026-07-03-m40-carrier-headhead.md
    title: TheoDB vector benchmark — m40-carrier-headhead
    last_modified: 2026-07-03
---

Os **dados brutos por configuração** que sustentam o
[veredito de carrier](/benchmarks/m40-carrier.md).

# A nota de método que a tabela carrega

> `mean` e `std` são **dispersão de latência por query dentro da amostra cronometrada**, **não variância
> entre execuções**; o QPS é melhor-de-N.

Essa distinção aparece em todas as tabelas desta família, e é fácil de perder: **um desvio pequeno na
coluna `std` NÃO significa que a medição é reprodutível.** Ele diz que as queries daquela execução
tiveram latências parecidas entre si — o que é outra coisa.

A variância que decide reprodutibilidade é **entre execuções**, e ela só aparece quando se roda várias
vezes e se reporta média com desvio **das execuções** — que foi exatamente o rigor que
[m45](/benchmarks/m45-pareto-sift1m.md) trouxe, e que levou à
[retratação](/benchmarks/sift1m-carrier-verdict.md) de um veredito de superioridade baseado em
melhor-de-N.

**Melhor-de-N e dispersão intra-amostra são precisamente a combinação que produz falsos positivos** — e
está tudo declarado no cabeçalho, o que permite a quem lê saber o que o número suporta.

# Contexto

O veredito derivado, com a ressalva de que gaussiano aleatório é o pior caso para índice de grafo e que a
conclusão **não generaliza** para dados reais, está em [m40 carrier](/benchmarks/m40-carrier.md).
