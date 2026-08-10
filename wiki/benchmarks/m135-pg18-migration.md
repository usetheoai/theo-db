---
type: Measurement
title: m135 — migração para PostgreSQL 18, guiada por evidência primária
description: 27 erros de compilação fechados lendo headers e commits do upstream em vez de tentar e errar — e o porte produziu um achado colateral grave que existia desde antes.
resource: git:f7c7b93:docs/benchmarks/m135-pg18-migration.md
tags: [benchmark, migracao, postgresql-18, evidencia-primaria, abi, m135]
milestone: M135
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m135
    resource: git:f7c7b93:docs/benchmarks/m135-pg18-migration.md
    title: M135 — migração PostgreSQL 17 → 18
    last_modified: 2026-07-21
---

**Manchete:** os **27 erros de compilação** medidos na sondagem foram fechados, e a extensão **carrega e
opera** num PostgreSQL 18 real.

# O método, e por que ele é o ponto

> O porte foi guiado por **evidência primária** — headers da versão nova, commits do upstream e código de
> peers — **não por tentativa e erro**.

Numa migração de major de um banco, tentativa e erro produz código que **compila** sem que ninguém saiba
**por que** a mudança era necessária. Ler o commit upstream que mudou a API dá a razão junto com a
correção — e é o que permite distinguir uma adaptação correta de uma que só silenciou o compilador.

# O achado colateral

O porte **produziu um achado de severidade alta que existia na versão anterior desde muito antes**.

Esse é o subproduto característico de uma migração feita com atenção: **olhar o código com os olhos de
uma plataforma diferente expõe suposições que a plataforma antiga tolerava**. Uma migração mecânica
teria portado o defeito junto.

# Por que este trabalho era obrigatório

A **recompilação a cada major é a consequência aceita** do modelo de extensão escolhido no
[ADR 0001](/decisions/0001-no-engine-fork.md) — o "ABI drift" que aquele ADR nomeou como risco e mitigou
por CI. **Este milestone é o risco se materializando e sendo pago**, exatamente como previsto.

# Contexto

A cadeia de upgrade da extensão, que é o outro lado da mesma disciplina, é
[m137](/benchmarks/m137-upgrade-chain.md).
