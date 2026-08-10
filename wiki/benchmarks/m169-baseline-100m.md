---
type: Measurement
title: m169 — baseline a 100M: 28 de 43 queries completam
description: A métrica é CONCLUSÃO, não velocidade — o critério é a consulta terminar, o que é a pergunta certa quando o gargalo é memória.
resource: git:f7c7b93:docs/benchmarks/m169-baseline-100m.md
tags: [benchmark, escala, conclusao, memoria, baseline, m169]
milestone: M169
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m169b
    resource: git:f7c7b93:docs/benchmarks/m169-baseline-100m.md
    title: M169 — baseline ClickBench a 100M
---

**28 de 43 consultas completam.**

# A métrica é conclusão, não velocidade

> Este é o número que o milestone existe para mover; ele é uma medição de **CONCLUSÃO**, não de
> velocidade — o critério é *a consulta termina*.

**Escolher a métrica certa para o gargalo dominante** é o que torna este artefato útil. A 100M o limite
não é quão rápido a query roda: é **se ela roda**. Publicar latências das 28 que completam e ignorar as
15 que não completam descreveria mal o sistema.

É a mesma lógica pela qual [m88](/benchmarks/m88-billion-scale-verdict.md) reportou o build estourando
memória como o achado principal, e não como nota de rodapé.

# Proveniência

O cabeçalho registra **hash do binário, número de núcleos e memória total** — o mínimo para que a
comparação com uma corrida posterior signifique alguma coisa.

# O que veio depois

O delta medido está em [m169 delta](/benchmarks/m169-t41-delta.md), e o estado final em
[m169 t41](/benchmarks/m169-t41.md). A regressão que apareceu no caminho — e o custo honesto da correção
— está no [ADR 0059](/decisions/0059-m169-fail-open-cobre-falha-de-spill.md).
