---
type: Technology
title: pgvectorscale
description: A extensão que trouxe StreamingDiskANN e quantização binária ao PostgreSQL; foi o substituto permissivo de qualidade-ScaNN do projeto até ser removida junto com o pgvector.
resource: https://github.com/timescale/pgvectorscale
tags: [tecnologia, extensao, diskann, quantizacao, removido]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pgvs-repo
    resource: https://github.com/timescale/pgvectorscale
    title: pgvectorscale, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O pgvectorscale é uma extensão escrita em Rust que acrescenta ao PostgreSQL o índice **StreamingDiskANN**
— uma variante de [DiskANN](/technologies/diskann.md) — e a quantização binária estatística, sob licença
permissiva. Ela **reusa o tipo `vector`** do [pgvector](/technologies/pgvector.md) em vez de definir o
próprio.[^recalled]

# Papel neste acervo — três contribuições, uma remoção

**Foi o substituto permissivo de qualidade-ScaNN.** O [ADR 0004](/decisions/0004-scann-fork-decision.md)
decidiu **não construir** um índice nativo porque ela já entregava recall na faixa exigida
([m14](/benchmarks/m14-scann-fork-decision.md)) — o primeiro uso real do fork-gate.

**Foi baseline de quantização.** A paridade de memória e recall da quantização binária própria foi medida
contra ela ([m22](/benchmarks/m22-sbq-parity.md)).

**Foi fonte de padrão de desenho, lida diretamente.** Duas decisões vieram de **abrir o código dela**: o
mecanismo de tombstone in-place da manutenção
([ADR 0017](/decisions/0017-m55-index-maintenance-at-scale.md)) e — crucialmente — a descoberta de que
filtro por label se resolve com **chave de scan**, e não com a máquina pesada de planner que o escopo
original previa ([ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md)).

**Ler o código de um peer permissivo** é o que evitou construir a solução cara duas vezes.

# A remoção

Saiu junto com o pgvector no [ADR 0029](/decisions/0029-m70-drop-pgvector.md). Instruções remanescentes
que mencionem o índice dela são **históricas e não se aplicam** — os access methods hoje são próprios.

# Uma nota de licença que ela ensinou

A licença de topo dela é permissiva, mas o binário **linka estaticamente a árvore transitiva de crates
Rust** — e é *esse* código que embarca. Isso tornou a varredura de licenças sobre o conjunto fixado de
dependências uma **obrigação de pré-release**, registrada em
[auditoria de licenças](/references/license-audit.md).

[^pgvs-repo]: pgvectorscale, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
