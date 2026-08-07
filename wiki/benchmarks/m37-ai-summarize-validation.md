---
type: Measurement
title: m37 — validação de contrato da sumarização (e a premissa falsificada)
description: O milestone abriu supondo que a feature não existia; a verificação mostrou que existia e era testada — a auditoria anterior havia grepado só metade do código.
resource: git:f7c7b93:docs/benchmarks/m37-ai-summarize-validation.md
tags: [benchmark, validacao, ai-surface, doc-drift, honest-negative, m37]
milestone: M37
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m37
    resource: git:f7c7b93:docs/benchmarks/m37-ai-summarize-validation.md
    title: M37 — Validação funcional ai.summarize
    last_modified: 2026-07-03
---

**Tipo: validação de contrato**, e não benchmark de throughput — o documento marca isso na primeira linha.

# A premissa falsificada

O milestone abriu sob a premissa de que a sumarização **não estava implementada**. O grounding
measurement-first **falsificou isso**: a feature já estava entregue e testada.

**A causa é instrutiva:** a auditoria de features que gerou o milestone **grepou apenas o código Rust** e
perdeu a implementação que vivia no SQL. Uma varredura parcial produziu uma lacuna inexistente.

O milestone virou então **correção de drift de documentação mais esta validação**, **sem código novo** —
implementar de novo criaria duplicação e conflito de instalação.

# O que foi verificado ao vivo

As duas assinaturas, confirmadas por **introspecção do catálogo** e não por leitura de código:

| Objeto | Assinatura |
|---|---|
| `ai.summarize` | `(content text, model text DEFAULT NULL) → text` |
| `ai.agg_summarize` | agregado sobre `text` |

E a postura de segurança verificada da mesma forma: a lista de controle de acesso das duas confirma que o
**REVOKE de PUBLIC está aplicado**, com teste dedicado provando que um papel comum não executa.

**Verificar privilégio pelo catálogo, e não pela intenção do código**, é a diferença entre segurança
alegada e segurada.

# Relacionado

A feature está em [sumarização de conteúdo](/features/11-sumarizacao-conteudo.md), e o comportamento fino
do agregado — ordem indeterminada, truncamento, tratamento de nulos — está em
[funções generativas em SQL](/guides/sql-ai-functions.md).
