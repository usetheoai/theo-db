---
type: Measurement
title: m143 — remoção total: o último componente C++ saiu
description: O lakehouse passa a ser código próprio no build default, provado na imagem embarcada sem a dependência, com a escrita falhando fechado em tipo não suportado.
resource: git:f7c7b93:docs/benchmarks/m143-pgduckdb-removal.md
tags: [benchmark, remocao, parquet, own-code, fail-closed, m143]
milestone: M143
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m143
    resource: git:f7c7b93:docs/benchmarks/m143-pgduckdb-removal.md
    title: M143 — remoção total do pg_duckdb
    last_modified: 2026-07-22
---

**Manchete: a dependência foi removida por completo — o último componente C++ do projeto saiu.**

O lakehouse — ler, escrever e agregar Parquet externo — passa a ser **código próprio** no build
**default**, numa **imagem só**; a imagem opcional do tier-out foi **aposentada**.

# O que a validação prova

Provado **na imagem embarcada, sem a dependência**:

- **round-trip completo** — escrever, ler e agregar — funcionando sem ela;
- **leitor multi-tipo**, cobrindo inclusive tipos aninhados;
- **escrita falhando fechado** em tipo não suportado, com erro tipado em vez de gravar algo errado;
- o **catálogo de extensões sem a dependência** — verificação direta, não inferência;
- **o delta de tamanho** confirmado.

**Verificar o catálogo** é o análogo de asserir a ausência que o
[tier-out](/benchmarks/m142-pgduckdb-tiering.md) já fazia — e é o que distingue "removemos" de "achamos
que removemos".

# A economia que viabilizou a decisão

**118 MB de C++ removidos contra +9 MB de Rust adicionados** — cerca de 1/13 do tamanho, com paridade
byte a byte provada previamente pelo [spike](/benchmarks/parquet-reader-owncode-spike.md).

# O efeito colateral mais interessante

A remoção **fez uma restrição desaparecer**. O desenho de codegen existia **só** porque a dependência
proibia execução dentro de função; sem ela, as funções passam a fazer o trabalho internamente, e a
superfície **simplifica**.

**Remover uma dependência que simplifica o código em vez de complicá-lo** é o resultado que a linhagem
inteira — [adoção](/benchmarks/m61-columnar-adoption.md),
[superfície](/benchmarks/m62-htap.md), [tier-out](/benchmarks/m142-pgduckdb-tiering.md), remoção —
levou a alcançar. A decisão é o [ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md).
