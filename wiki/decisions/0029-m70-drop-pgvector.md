---
type: Decision
title: ADR 0029 — Remover o pgvector (e o pgvectorscale) totalmente
description: O TheoDB torna-se o primeiro AM permissivo com tipo vector 100% próprio; a dependência é invertida, o tipo vai para public, e a migração de instalações existentes passa por real[].
resource: git:f7c7b93:docs/adr/0029-m70-drop-pgvector.md
tags: [adr, pgvector, independencia, own-code, migracao, m70]
adr_id: "0029"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M70
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0029
    resource: git:f7c7b93:docs/adr/0029-m70-drop-pgvector.md
    title: ADR 0029 — M70 remover o pgvector
    last_modified: 2026-07-09
---

O marco de independência: o [pgvector](/technologies/pgvector.md) e o
[pgvectorscale](/technologies/pgvectorscale.md) saem da distribuição, e o TheoDB passa a ser o
**primeiro AM permissivo com tipo `vector` 100% próprio**.

# D1 — flip da dependência: a extensão Rust vira a base

O `requires` da extensão Rust **zera**, e ela passa a prover o tipo `public.vector`, os access
methods e os schemas. O umbrella passa a depender dela.

O racional é topológico: o tipo próprio só pode ser criado pela extensão Rust, porque o I/O vive no
`.so` dela. Como o umbrella **usa** o tipo, ele deve depender dela. Removido o pgvector — que era o
terceiro elemento quebrando o ciclo —, o flip é a **única topologia acíclica** possível.

**Rejeitadas:** pôr o tipo no umbrella (impossível, o I/O está no `.so`) e manter a direção antiga
(criaria ciclo).

# D2 — tipo em `public.vector`, drop-in

Sem o pgvector não há colisão, então `::vector` do usuário e o `FOR TYPE vector` das opclasses
resolvem ao tipo próprio **sem mudança de código**.

# D3 — migração de instalações existentes: o custo honesto

**Aqui está a honestidade que o ADR anterior deixou pendente.** Como o tipo próprio ocupa
`public.vector` — o **mesmo nome** do pgvector —, os dois **não coexistem**. Logo o cast binário
grátis do [ADR 0028](/decisions/0028-m69-own-vector-type.md), que dependia de schemas distintos,
**não se aplica a um upgrade**.

A migração usa um intermediário neutro:

```sql
ALTER COLUMN … TYPE real[];
DROP EXTENSION vector;
CREATE EXTENSION theodb;
ALTER COLUMN … TYPE vector;
REINDEX;
```

Preserva os dados — os floats sobrevivem no `real[]` — mas **reescreve o heap** (não é O(1)) e exige
janela de manutenção. O procedimento está em
[migração do pgvector](/guides/pgvector-migration.md).

**Rejeitadas:** a rota byte-level sem reescrita (instalar o tipo próprio num schema temporário e
mover depois) não foi implementada, ficando no backlog como otimização; e dump/restore total daria
mais downtime. Instalações **novas** não precisam de migração — nascem com o tipo próprio.

# D4 — gate de não-regressão

A prova de que o tipo próprio substitui 100% o pgvector é a suíte do AM rodando **sem** o pgvector e
sem o pgvectorscale, com o gate set-equal: o top-k do índice é idêntico ao top-k exato. É o gate de
recall executável, e ele é o dado exigido — não um benchmark de performance, porque este milestone
é correção, não velocidade.

# Consequências

**Positivas:** pgvector e pgvectorscale **removidos** do Dockerfile e das dependências. O tipo
`vector`, os operadores, os casts e os access methods ANN são 100% código próprio, e a imagem fica
menor.

**Ressalvas honestas:** **REINDEX é obrigatório** na migração, já que as opfamilies do pgvector não
são compartilhadas — a coluna migra, os índices são recriados. O `ALTER COLUMN TYPE` pega
`ACCESS EXCLUSIVE` brevemente, então tabelas quentes pedem janela de baixa carga. E o
[pg_duckdb](/technologies/pg-duckdb.md) fica intocado, por ser pilar independente.

**Validação em PG17 real, sem pgvector:** 229 de 230 testes verdes com a extensão standalone — a
única falha é um teste de **timing SIMD**, flaky sob carga, que passa isolado e cujo código não foi
tocado. E `CREATE EXTENSION theodb CASCADE` sem pgvector instala apenas as extensões próprias, com
`'[1,2,3]'::vector` resolvendo ao tipo próprio: remoção total provada ponta a ponta.[^adr0029]

# Licença

Código **original**. VectorChord é AGPLv3 / Elastic License — apenas estudo, nunca copiado. O tipo
espelha o do pgvector, que está sob PostgreSQL License, permissiva.

[^adr0029]: ADR 0029 — M70: remover o pgvector (e pgvectorscale) totalmente
