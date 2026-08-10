---
type: Measurement
title: m66 — estratégias de chunking: a estratégia importa, com rigor declarado
description: Separa o degrau robusto do degrau que é empate estatístico, e registra como débito honesto que uma única execução não sustenta a distinção fina.
resource: git:f7c7b93:docs/benchmarks/archive/m66-chunking.md
tags: [benchmark, chunking, rag, significancia, debito, arquivo, m66]
dataset: BEIR NFCorpus
milestone: M66
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m66
    resource: git:f7c7b93:docs/benchmarks/archive/m66-chunking.md
    title: M66 — chunking benchmark
---

**Veredito: a estratégia importa** — com o rigor declarado sobre **quanto** ela importa.

# Os dois degraus, separados

Sobre 50 queries de um corpus real:

- **Degrau robusto:** as estratégias baseadas em estrutura do texto superam a de janelas fixas por 0,025
  de nDCG@10.
- **Degrau fino:** a diferença entre as duas melhores é de **0,0055** — **empate estatístico** dentro do
  ruído. **Não é afirmado.**

**Separar o que a medição sustenta do que ela não sustenta**, dentro do mesmo resultado, é o que impede
que uma ordenação completa seja lida como se todas as diferenças fossem reais.

# A comparação justa

O parâmetro de recuperação é **adaptado para igualar o orçamento** entre as estratégias — porque
estratégias de chunking produzem números diferentes de fragmentos, e comparar sem igualar mediria
"quantos pedaços" em vez de "quão bem divididos".

# O débito honesto registrado

> **Uma única execução, com tolerância de ruído assumida.** Separar as duas melhores exigiria desvio
> pareado e ao menos três execuções.

O artefato **nomeia o que faltaria** para sustentar a distinção fina, em vez de deixá-la implícita — e o
harness passou a reportar o desvio.

É o mesmo rigor que a linhagem da busca híbrida adotaria em
[m123](/benchmarks/m123-hybrid-significance.md).

# Contexto

E o chunking **semântico foi adiado por evidência** — a literatura mede ganho de 0 a 4 pontos,
frequentemente negativo ponta a ponta, a 14× o custo. A decisão é o
[ADR 0025](/decisions/0025-m66-chunking-strategies.md).
