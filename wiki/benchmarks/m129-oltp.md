---
type: Measurement
title: m129 — pilar OLTP com as ferramentas padrão da indústria
description: O caminho transacional É o do PostgreSQL, então o valor do run é provar o gate de wire-compatibility ponta a ponta e estabelecer baseline — não descobrir performance nova.
resource: git:f7c7b93:docs/benchmarks/m129-oltp.md
tags: [benchmark, oltp, pgbench, hammerdb, wire-compat, m129]
milestone: M129
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m129
    resource: git:f7c7b93:docs/benchmarks/m129-oltp.md
    title: M129 — Official-benchmark OLTP pillar
    last_modified: 2026-07-21
---

Terceira aplicação do padrão adotar-e-envolver.

# O enquadramento que define o valor do run

**O caminho OLTP do TheoDB É o do PostgreSQL** — a extensão não é um fork de engine. Portanto este run
**não descobre performance nova**; ele prova o **gate de wire-compatibility ponta a ponta** e estabelece
uma **baseline** com as duas ferramentas de campo.

Saber **o que um benchmark pode e não pode revelar** antes de rodá-lo é o que evita concluir demais dele.

# O que a camada própria acrescenta

Além dos dois drivers externos, o run **pareia explicitamente com o gate retido de ACID e crash-safety**
— porque throughput de OLTP publicado sem garantia de durabilidade é o problema que o
[ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md) aponta nas ferramentas de mercado, que
frequentemente reportam números com sincronização desligada.

**Três sessões separadas**, cada uma com artefato próprio, o que permite ver dispersão entre execuções em
vez de um ponto único.

# Guardas de licença

Uma das ferramentas é copyleft, e por isso entra **como driver externo, fora da árvore** — nunca
empacotada. É a disciplina registrada no ADR e verificada pela
[auditoria de licenças](/references/license-audit.md).

# Ressalva

Máquina compartilhada, **não canônica**, com containers co-residentes ativos durante o run.
