---
type: Measurement
title: m115 — composabilidade do agregado colunar: quatro formas que falhavam
description: Consumir o valor de um agregado acelerado dentro de uma expressão envolvente quebrava com erro obscuro de catálogo; as quatro formas viram teste byte-idêntico contra o heap.
resource: git:f7c7b93:docs/benchmarks/m115-columnar-composability-verdict.md
tags: [benchmark, columnar, composabilidade, planner, regressao, m115]
milestone: M115
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m115
    resource: git:f7c7b93:docs/benchmarks/m115-columnar-composability-verdict.md
    title: M115 — columnar-aggregate CustomScan composability
    last_modified: 2026-07-19
---

# O defeito

Consumir o **valor** de um agregado colunar acelerado **dentro de uma expressão envolvente** falhava com
um erro obscuro de lookup de catálogo.

Esse é o pior tipo de defeito de integração: a operação isolada funciona, e ela **quebra ao ser
composta** — que é justamente como SQL real é escrito. Um benchmark que só mede a forma isolada nunca o
encontraria.

# O que passou a ser testado

**As quatro formas que falhavam** rodam agora **byte-idênticas ao heap** com a aceleração engajada, mais
um join sobre o resultado agrupado, mais uma verificação de regressão no nível de topo.

**Byte-idêntico contra o heap** é o oráculo certo: o resultado acelerado não pode ser "equivalente", tem
de ser **igual** — porque o usuário não escolheu acelerar, ele só escreveu SQL.

# O mecanismo

A troca do nó acontece em fases distintas do planejamento, com a ordenação do resultado agrupado
preservada. Mexer no planner é território de alto risco, e é por isso que a evidência exigida foi
identidade, e não amostragem.

# Contexto

Completa o par com [m114](/benchmarks/m114-columnar-aggregate-verdict.md): um verifica **quais formas**
são aceleradas, o outro verifica que a aceleração **compõe** com o resto da query.
