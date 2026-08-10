---
type: Measurement
title: m169 — delta medido: de 28 para 30 queries
description: Declara explicitamente o que ficou constante entre as duas corridas, porque sem isso a subtração não significa nada.
resource: git:f7c7b93:docs/benchmarks/m169-t41-delta.md
tags: [benchmark, delta, controle, comparabilidade, m169]
milestone: M169
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m169d
    resource: git:f7c7b93:docs/benchmarks/m169-t41-delta.md
    title: M169 — delta medido 28/43 → 30/43
---

**De 28 para 30 consultas que completam — mais duas.** Medição de **conclusão sob o mesmo teto**.

# A frase que define o rigor deste artefato

> **O que ficou constante — sem isto a subtração não significa nada.**

E então uma tabela comparando, lado a lado, o que mudou e o que não mudou entre as duas corridas: o
binário mudou (é a variável), enquanto **núcleos e memória permaneceram idênticos**.

**Um delta entre duas medições só é atribuível à mudança se tudo o mais for igual** — e a única forma de
alguém verificar isso é o artefato **listar** o que foi mantido constante.

É a versão explícita do que o controle não modificado fazia em
[m46](/benchmarks/m46-highrecall-qps.md), e do A/B na mesma janela térmica de
[m41](/benchmarks/m41-hnsw-qps.md): **isolar a variável, e mostrar que se isolou.**

# O custo escondido nas duas queries recuperadas

**Duas dessas consultas voltaram a completar pelo caminho eager** — ou seja, **com o consumo de memória
que este milestone existia para remover**.

O [ADR 0059](/decisions/0059-m169-fail-open-cobre-falha-de-spill.md) registra isso como
**honestamente pior do que o previsto**, e deixa aberta e nomeada a medição que faltou: se elas passariam
pelo caminho novo com mais descritores de arquivo disponíveis.

**Um delta positivo cujo mecanismo é registrado como insatisfatório** é mais útil que um delta positivo
apresentado como vitória limpa.
