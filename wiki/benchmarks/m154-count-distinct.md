---
type: Measurement
title: m154 — contagem distinta roteada ao caminho vetorizado, e sempre exata
description: Usa o acumulador exato do motor, nunca uma estimativa aproximada — a escolha que preserva a garantia de resultado byte-idêntico.
resource: git:f7c7b93:docs/benchmarks/m154-count-distinct.md
tags: [benchmark, columnar, count-distinct, exatidao, cobertura, m154]
milestone: M154
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m154
    resource: git:f7c7b93:docs/benchmarks/m154-count-distinct.md
    title: M154 — COUNT(DISTINCT) roteado ao CustomScan
    last_modified: 2026-07-25
---

**Cobertura de 14 para 18 queries, com divergência zero.**

# A escolha que define o milestone

A contagem distinta usa o acumulador **EXATO** do motor — **nunca** uma estrutura probabilística de
cardinalidade aproximada.

Isso é decisivo, e o documento o enfatiza. Estruturas aproximadas são o caminho padrão da indústria para
contagem distinta em escala, e seriam **mais rápidas**.

**Mas elas quebrariam a garantia que sustenta todo o pilar colunar: resultado byte-idêntico ao do heap.**
Um resultado aproximado é, por definição, divergente — e a divergência apareceria como "erro" no gate,
ou pior, seria tolerada e o gate perderia sentido.

**Velocidade que custa a garantia de correção não é uma troca disponível aqui** — a mesma disciplina que
[m114](/benchmarks/m114-columnar-aggregate-verdict.md) estabeleceu ao verificar tanto as formas admitidas
quanto as recusadas.

# Nota de método

A **cobertura é estrutural**, isto é, **independente da amostra** — ela depende da forma da query, não do
tamanho dos dados. Por isso a medição de cobertura pode rodar sobre uma amostra menor sem perder
validade, e o documento diz isso em vez de deixar o leitor supor.

# Contexto

Sai da lista de causas do [mapa de roteamento](/benchmarks/m152-routing-map.md).
