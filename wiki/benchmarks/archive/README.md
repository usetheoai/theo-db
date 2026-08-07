---
type: Reference
title: Arquivo de benchmarks — o que está aqui e por quê
description: Artefatos de milestones lançados que nenhuma superfície viva cita; movidos, nunca apagados, porque evidência se arquiva.
resource: git:f7c7b93:docs/benchmarks/archive/README.md
tags: [referencia, arquivo, retencao, evidencia]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: archreadme
    resource: git:f7c7b93:docs/benchmarks/archive/README.md
    title: Benchmarks — archive
---

# O critério de arquivamento

Aqui vivem artefatos de milestones **já lançados** que **não são citados** por nenhuma superfície viva —
nem documentação de produto, nem decisão registrada, nem trilha de auditoria.

O critério é **referência viva, não idade**. Um benchmark antigo continuamente citado por um ADR
permanece na raiz; um recente que ninguém referencia pode ser arquivado.

# A regra que governa isto

> **Arquivar, nunca deletar** evidência.

Movidos para cá **para desafogar o topo do diretório mantendo a evidência reproduzível**.

Essa distinção é o que permite que a política de higiene não entre em conflito com a de honestidade: um
número publicado precisa continuar verificável, mesmo depois de deixar de ser relevante. Apagar
evidência de um resultado antigo tornaria impossível auditar uma decisão que se apoiou nele.

**A separação entre "não é mais citado" e "não é mais verdade" também importa** — arquivamento não é
retratação. Artefatos **retratados**, como o
[veredito de carrier](/benchmarks/sift1m-carrier-verdict.md), permanecem na raiz **com o aviso no topo**,
justamente porque continuam sendo citados e precisam avisar quem os alcança.

# O que está aqui

Corridas antigas de dataset ([glove](/benchmarks/archive/2026-06-27-glove-25-angular.md),
[cosseno](/benchmarks/archive/2026-06-27-pgvector-cosine.md),
[L2](/benchmarks/archive/2026-06-27-pgvector-l2.md)), medições de carrier
([m40](/benchmarks/archive/2026-07-03-m40-carrier-headhead.md)), milestones de superfície de IA
([rerank](/benchmarks/archive/m65-rerank.md), [chunking](/benchmarks/archive/m66-chunking.md),
[auto-tune](/benchmarks/archive/m67-autotune.md)), e validações de componente
([estimador RaBitQ](/benchmarks/archive/rabitq-estimator-validation.md)).
