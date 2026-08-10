---
type: Measurement
title: m98 — gate de coexistência: pgrx, DataFusion e Arrow num crate só
description: Um gate go/no-go respondido por build, link e testes — e não por medição de performance, porque a pergunta era de viabilidade.
resource: git:f7c7b93:docs/benchmarks/m98-coexistence.md
tags: [benchmark, gate, pgrx, datafusion, arrow, upgrade, m98]
milestone: M98
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m98
    resource: git:f7c7b93:docs/benchmarks/m98-coexistence.md
    title: M98 — pgrx upgrade + DataFusion/Arrow coexistence
    last_modified: 2026-07-14
---

**Veredito: gate passado**, provado por build, link e testes.

# A pergunta

O go/no-go do pilar de planner único: **o ferramental de extensão, o
[DataFusion](/technologies/datafusion.md) e o [Arrow](/technologies/arrow.md) coexistem num único crate,
e o DataFusion executa dentro de um backend do PostgreSQL?**

É pergunta de **viabilidade**, não de performance — e por isso a evidência correta é **compilar, linkar e
rodar**, não medir latência. **Um gate mal escolhido mede a coisa errada com rigor.**

# O que também foi resolvido no caminho

O upgrade do ferramental, com migração de edição da linguagem, foi feito junto — e um detalhe merece
registro:

**O tipo `vector` teve de ser remapeado manualmente** para continuar se chamando `vector` após a
mudança de como o ferramental classifica tipos externos. O ganho de fazer isso corretamente: **sem
REINDEX e sem mudança no SQL do usuário**.

Preservar o nome de um tipo através de um upgrade de ferramental é exatamente o que separa uma migração
transparente de uma que quebra toda aplicação instalada — o mesmo tipo de cuidado que o
[shim de compatibilidade](/decisions/0058-pgvector-compat-shim.md) exerce noutro nível.

# O que este gate destravou

Todo o pilar colunar próprio — [substrato](/benchmarks/m99-columnar-tam.md),
[executor vetorizado](/benchmarks/m100-datafusion-executor.md) e a
[co-residência com o vetorial](/decisions/0044-m103-vector-columnar-coresidence.md) — mais o
[lakehouse próprio](/features/15-lakehouse-parquet.md) que substituiu a dependência externa.
