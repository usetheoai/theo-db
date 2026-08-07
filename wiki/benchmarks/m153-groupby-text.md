---
type: Measurement
title: m153 — agrupamento por texto roteado ao caminho vetorizado
description: Relaxa uma recusa por collation com dois guards — determinismo verificado na admissão e reordenação garantida na troca — e mantém divergência zero.
resource: git:f7c7b93:docs/benchmarks/m153-groupby-text.md
tags: [benchmark, columnar, collation, group-by, cobertura, m153]
milestone: M153
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m153
    resource: git:f7c7b93:docs/benchmarks/m153-groupby-text.md
    title: M153 — GROUP BY texto roteado ao CustomScan
    last_modified: 2026-07-25
---

**Cobertura de 18 para 21 queries, com divergência zero** — resultado byte-idêntico ao heap em todas as
43.

# A recusa que foi relaxada, e por que ela existia

Agrupar por texto era recusado por causa de **collation**: a ordem e a igualdade de strings dependem de
regras locais, e o motor vetorizado não as reproduz automaticamente.

**Relaxar essa recusa sem cuidado produziria resultado errado** — agrupamentos que o PostgreSQL
consideraria iguais poderiam ser separados, ou vice-versa.

# Os dois guards, e o que cada um cobre

- **Determinismo de collation, verificado na admissão** — a query só é aceita quando a collation
  envolvida é determinística. Cobre a **igualdade** dos grupos.
- **Reordenação acima, na troca do nó** — cobre a **ordem** do resultado.

**Igualdade e ordem são propriedades distintas**, e cada uma precisa do seu guard. Um guard só deixaria
uma delas exposta — e o resultado errado apareceria apenas em dados com acentuação ou caixa mista, isto
é, tarde.

# O gate

**Divergência zero** contra o heap nas 43 queries. Ampliar cobertura sem esse gate seria trocar correção
por velocidade, que é exatamente o que o
[ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md) aponta como falha das ferramentas de
benchmark de mercado.

# Contexto

Vem da lista de causas do [mapa de roteamento](/benchmarks/m152-routing-map.md).
