---
type: Invariant
title: Um callback extern C-unwind gerado por macro_rules! sem frame de guarda derruba a instância inteira
description: Sem #[pg_guard] o unwinder sai da pilha (_URC_END_OF_STACK, signal 6) e mata o postmaster — e o atributo NÃO pode ser aplicado dentro da macro.
resource: docs/benchmarks/m135-pg18-migration.md
tags: [pgrx, ffi, unsafe, crash]
timestamp: 2026-07-30T00:00:00Z
---

# Um callback `extern "C-unwind"` gerado por `macro_rules!` **sem frame de guarda** derruba a instância inteira

## O invariante

[panic-atraves-da-fronteira-c](panic-atraves-da-fronteira-c.md) registra que o pgrx **desenrola corretamente** —
mas isso tem uma **pré-condição** que aquele conceito não nomeava: **existir o frame de guarda**.

Sem ele, o unwinder chega ao fim da pilha (`_URC_END_OF_STACK`), o processo recebe **`signal 6`**, e o
PostgreSQL derruba **a instância inteira**. Medido: **30 stubs** gerados por `macro_rules!` sem `#[pg_guard]`,
chamados direto pelo C — **três comandos SQL triviais** derrubavam o servidor, e isso vivia no código **desde o
M99**.

## A armadilha dentro da armadilha

**`#[pg_guard]` não pode ser aplicado ali.** A higiene dos fragmentos `$arg` do `macro_rules!` não sobrevive à
expansão do atributo, e não compila. O caminho é chamar **`pgrx_extern_c_guard`** diretamente dentro do corpo do
stub.

## Terceiro invariante do mesmo porte — stub-com-erro é pior que `NULL`

Registrar um stub que **lança erro** num callback do `TableAmRoutine` é **pior** do que deixar `NULL`:

| | O planner faz |
|---|---|
| callback = `NULL` | **roteia ao redor** — cai em `Seq Scan`, a consulta funciona |
| callback = stub que erra | **planeja contando com a capacidade** e falha em runtime |

Precedente do Citus citado no artefato. A ausência declarada é informação para o planner; a presença mentirosa
não é.

## Relacionados

- [invariant/panic-atraves-da-fronteira-c](panic-atraves-da-fronteira-c.md) — o conceito que esta pré-condição completa
- [invariant/pg18-compact-attrs-rename-silencioso](pg18-compact-attrs-rename-silencioso.md)
- [invariant/tableam-routine-em-topmemorycontext](tableam-routine-em-topmemorycontext.md)
