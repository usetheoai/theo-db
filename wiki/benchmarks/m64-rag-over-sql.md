---
type: Measurement
title: m64 — RAG unificado: uma SQL contra N chamadas de aplicação
description: Mede a diferença estrutural que o campo não publica, com a métrica primária sendo round-trips; e corrige o próprio benchmark para não inflar o braço rival.
resource: git:f7c7b93:docs/benchmarks/m64-rag-over-sql.md
tags: [benchmark, rag, round-trips, unificacao, straw-man, m64]
milestone: M64
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m64
    resource: git:f7c7b93:docs/benchmarks/m64-rag-over-sql.md
    title: M64 — RAG-sobre-SQL unificado
    last_modified: 2026-07-09
---

**Métrica primária: round-trips por query — estrutural.** Latência é suporte, não gate.

# O gate, provado por teste

A query unificada recupera **exatamente** o top-k filtrado do oráculo exato — **compor não degrada
recall** — e é **read-your-writes na mesma SQL e no mesmo snapshot MVCC**.

# O head-to-head que o campo não publica

Braço unificado: **1 round-trip**, p50 de **6,721 ms**. Braço de aplicação: **2 round-trips**, p50 de
**7,284 ms**. Gate de recall casado com Jaccard 1,0 — **mesmo top-k por construção**.

**A vitória estrutural é 1 contra 2.** O ganho de latência é **modesto co-localizado (~8%)** e
**amplifica sobre rede real** — e ambos os cenários são reportados, não só o favorável.

# A correção que impede o straw-man

**A tabela do benchmark ganhou chave primária.** Sem ela, o passo de hidratação do braço rival faria um
**scan sequencial de 5000 linhas**, inflando o braço rival a ~5×.

Com a chave — **que toda tabela real tem** —, aquele passo é servido por índice, e **o gap cai para os 8%
honestos**.

**Corrigir o benchmark para fortalecer o adversário** é o inverso do incentivo natural, e é o que separa
uma comparação de uma demonstração.

# O que o benchmark NÃO mede

**Não demonstra superioridade algorítmica de recuperação** — **ambos os braços usam exatamente o mesmo
top-k**. Mede **apenas** a diferença estrutural entre compor a query no banco e compor no cliente.

Uma nota de rigor adicional: um cliente também obtém read-your-writes se abrir transação explícita; o
diferencial **não é a visibilidade em si**, mas obtê-la **numa SQL única, num snapshot único, sem
coordenação**.

O racional completo, incluindo por que a perna colunar **não** é planner-integrada, está no
[ADR 0023](/decisions/0023-m64-rag-unified-not-columnar-planner.md).
