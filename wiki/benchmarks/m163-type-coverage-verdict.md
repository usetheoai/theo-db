---
type: Measurement
title: m163 — matriz de cobertura de tipos: o espaço cego que o benchmark nunca exercita
description: Testa cada caminho de admissão contra valores de borda por tipo, fechando a lacuna que deixou bugs da mesma classe sobreviverem repetidamente até a revisão.
resource: git:f7c7b93:docs/benchmarks/m163-type-coverage-verdict.md
tags: [benchmark, tipos, valores-de-borda, fail-closed, espaco-cego, m163]
milestone: M163
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m163
    resource: git:f7c7b93:docs/benchmarks/m163-type-coverage-verdict.md
    title: M163 — type coverage verdict
---

# O espaço cego identificado

> Este é o espaço de tipos que o A/B do benchmark **nunca exercita** — **a lacuna que deixou bugs
> recorrentes da mesma classe sobreviverem até a revisão**, depois de cada rebuild.

Esse diagnóstico é o valor do milestone. Vários milestones anteriores de cobertura tiveram bugs de
**classe de tipo** encontrados tarde — e a causa não era descuido, era que **o benchmark não tinha esses
valores nos dados**.

**Um benchmark cobre as queries que ele contém, não o espaço de entrada que o código aceita.**

# O que a matriz faz

Para **cada caminho de admissão** cruzado com **cada valor de borda por tipo** — máximos e mínimos de
inteiros, zero negativo, não-numérico, infinito, timestamps, datas, texto, booleanos e nulos —, o harness
assere o contrato fail-closed:

**ou a query roteia E é byte-idêntica ao executor de linha, ou ela recusa corretamente.**

Não há terceiro resultado aceitável. Rotear e divergir é bug; recusar quando deveria rotear é perda de
cobertura, mas não é incorreção.

# O detalhe de rigor

**A evidência de roteamento vem da MESMA execução que produz o dado comparado.**

Sem isso, seria possível verificar o roteamento numa execução e a igualdade noutra — e concluir que uma
query roteada deu resultado correto quando, na execução comparada, ela pode ter recusado. É a mesma
armadilha do falso-verde que o [m161](/benchmarks/m161-expr-routing-verdict.md) nomeou, aplicada ao
emparelhamento das evidências.
