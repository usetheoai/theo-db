---
type: Measurement
title: m132 — o sintoma reportado NÃO reproduz, e o que era real foi corrigido
description: O worker funciona ponta a ponta; o defeito verdadeiro era que uma falha não dizia por quê — e o milestone entrega diagnosticabilidade em vez de um conserto inventado.
resource: git:f7c7b93:docs/benchmarks/m132-vectorizer-diagnosability.md
tags: [benchmark, nao-reproducao, diagnosticabilidade, vectorizer, honestidade, m132]
milestone: M132
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m132
    resource: git:f7c7b93:docs/benchmarks/m132-vectorizer-diagnosability.md
    title: M132 — worker-failure diagnosability
    last_modified: 2026-07-21
---

# A manchete

**O sintoma reportado NÃO reproduz.** O worker embeda linhas novas ponta a ponta.

**O que era real — e o que este milestone corrige — é que, quando um job de fato falha, nada diz por
quê.**

# Por que isso é um resultado, e não uma desculpa

A não-reprodução é **medida antes de qualquer mudança de código**, e registrada como tal.

Isso importa porque o caminho fácil seria **consertar algo** e declarar o issue resolvido — produzindo
uma correção para um defeito que talvez não existisse, com o sintoma real intacto e a confiança no
processo corroída.

**Separar "não reproduz" de "não é problema" é o movimento correto.** O relato existia por um motivo, e
investigar levou ao defeito verdadeiro, que é de **observabilidade**: uma falha silenciosa é
indistinguível de um bug de execução, e é isso que torna o relato original compreensível.

Esse tipo de correção é o que evita que o próximo relato do mesmo sintoma custe outra investigação
inteira.

# Enquadramento

O documento nota que **este milestone mede comportamento, não performance** — então a máquina não
canônica é irrelevante para a conclusão. Escolher a ressalva certa para o tipo de medição é o mesmo
cuidado do [m98](/benchmarks/m98-coexistence.md).

# Contexto

A feature é o [vectorizer](/features/16-vectorizer.md), cuja fila crash-safe com dead-letter é
justamente a superfície que este milestone tornou diagnosticável. Um defeito irmão, no caminho de
self-host, está documentado no [guia de self-host](/guides/self-host-quickstart.md).
