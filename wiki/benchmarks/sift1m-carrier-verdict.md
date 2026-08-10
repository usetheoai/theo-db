---
type: Measurement
title: veredito de carrier em SIFT1M — RETRATADO
description: A alegação de superioridade não sobreviveu à medição rigorosa; a página é mantida por proveniência, com instrução explícita de não citar os multiplicadores.
resource: git:f7c7b93:docs/benchmarks/sift1m-carrier-verdict.md
tags: [benchmark, retratacao, superioridade, rigor, proveniencia]
status: deprecated
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: s1mcv
    resource: git:f7c7b93:docs/benchmarks/sift1m-carrier-verdict.md
    title: SIFT1M carrier verdict (retracted)
    last_modified: 2026-07-03
---

> ⚠️ **RETRATADO.** A alegação de superioridade **não sobreviveu** à medição rigorosa com média e desvio.
> Sob 500 queries e pelo menos 3 execuções cronometradas com ground truth exato, **o veredito é
> PARIDADE** — as fronteiras se entrelaçam dentro do ruído entre execuções.
>
> **Não cite os multiplicadores desta página como claim.**

# O que causou o falso positivo

O documento nomeia os três ingredientes:

- **melhor-de-N** em vez de média com desvio,
- **200 queries** em vez de 500,
- **cache quente**.

Cada um sozinho inclina o resultado; juntos, produziram um sinal de superioridade que **duas execuções
subsequentes contradisseram**, dando ora inferioridade, ora paridade.

**A instabilidade entre execuções é o sintoma clássico de um efeito menor que o ruído** — e a resposta
correta não é escolher a execução favorável, é aumentar o rigor até o instrumento decidir.

# Por que a página é mantida

**Por proveniência.** Apagar um resultado retratado esconderia que ele existiu e foi citado; mantê-lo com
a retratação no topo **preserva a história e impede a citação**.

É a mesma prática do [ADR 0012](/decisions/0012-benchmark-data-degeneracy.md), que registrou a correção
"em vez de sobrescrever a história silenciosamente", e do
[m6](/benchmarks/m6-columnar-vs-row.md), marcado como superado e não citável.

# O que substitui

[m45](/benchmarks/m45-pareto-sift1m.md) — média e desvio, 500 queries, 3 execuções, com gate de efeito
maior que variância. **Veredito: paridade.** O índice próprio é **competitivo, não superior**.
