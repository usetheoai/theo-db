---
type: Measurement
title: m159 — o gap real contra o ClickHouse, medido na mesma máquina
description: Antes deste run não havia baseline no repositório, então qualquer razão citada teria sido inventada — e a regra do projeto proíbe isso.
resource: git:f7c7b93:docs/benchmarks/m159-clickhouse-gap-verdict.md
tags: [benchmark, clickhouse, gap, baseline, honestidade, m159]
milestone: M159
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m159
    resource: git:f7c7b93:docs/benchmarks/m159-clickhouse-gap-verdict.md
    title: M159 — gap REAL vs ClickHouse
    last_modified: 2026-07-26
---

# A frase que justifica o milestone

> Até agora **nenhum baseline de ClickHouse existia no repositório** — qualquer razão **teria sido
> inventada**.

Havia um alvo declarado pelo owner — ficar dentro de uma certa razão do ClickHouse. **Mas não havia
número contra o qual medir.** Toda afirmação sobre distância seria estimativa apresentada como fato,
o que a regra de performance do projeto proíbe.

**Este run produz o número.**

# O que ele entrega

O gap **honesto por query**, e um **julgamento sobre para quais classes de query o alvo é alcançável** —
que é mais útil que um número único, porque a distância varia enormemente conforme a forma da query.

Medir **na mesma máquina** é o que torna a comparação válida; e o desvio em relação ao hardware canônico
do benchmark é **documentado como desvio**, não omitido.

# Por que isso importa para a governança

É o mesmo mecanismo que o pilar vetorial usou: um alvo declarado (superar o adversário) **só vira
avaliável quando existe medição do adversário** — foi o que [m33](/benchmarks/m33-scann-headtohead.md)
fez, e foi a partir dele que o veredito honesto pôde ser emitido.

**Sem baseline, um alvo não é atingível nem inatingível — ele é apenas retórico.**
