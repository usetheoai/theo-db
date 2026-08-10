---
type: Decision
title: ADR 0001 — Sem fork do engine PostgreSQL
description: O engine PostgreSQL não é modificado; capacidades entram como extensões via CREATE EXTENSION.
resource: git:f7c7b93:docs/adr/0001-no-engine-fork.md
tags: [adr, arquitetura, postgresql, extensao, wire-compat]
adr_id: "0001"
adr_status: Accepted
decision_date: 2026-06-26
owner: human:paulohenriquevn
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0001
    resource: git:f7c7b93:docs/adr/0001-no-engine-fork.md
    title: ADR 0001 — Sem fork do engine PostgreSQL
    author: human:paulohenriquevn
    last_modified: 2026-06-26
---

O invariante mais antigo do projeto e o único que nenhum ADR posterior derrubou: **o engine
PostgreSQL não é reescrito nem forkado**. Tudo o que o TheoDB acrescenta entra pelo mecanismo
oficial de extensibilidade do PostgreSQL.

# Contexto

O TheoDB entrega compatibilidade PostgreSQL *como produto*. A pergunta era como obter a
extensão vetorial ([pgvector](/technologies/pgvector.md)) sem violar a licença Apache 2.0 e
sem perder wire-compatibility com o PostgreSQL 17.

# Alternativas avaliadas

**A1 — Extension model (adotada).** Compilar o `pgvector v0.8.3` como extensão sobre a imagem
oficial `postgres:17-bookworm`. Nenhuma linha do engine é modificada.

**A2 — Fork do engine com patch de vetores embutido.** Rejeitada por quatro razões: viola a
regra de projeto "sem fork do engine"; elimina a wire-compatibility garantida por `psql`/`libpq`
oficiais; cria custo de rebase a cada release minor do PG (≥ 4 por ano); e não há precedente
OSS permissivo provando que a estratégia escala.

**A3 — Engine do zero, wire-compatible, com vetores nativos.** Rejeitada: escopo multi-anos
sem prior art permissivo nessa escala; a wire-compatibility é gate de produto e exigiria
implementar o protocolo wire completo; violaria "não reinvente a roda" e YAGNI; e o risco real
é fabricar um clone incompatível em vez de um produto compatível.

# Decisão

Adotar **A1 — Extension model**. O `pgvector v0.8.3` (Apache 2.0) é compilado como extensão
sobre a imagem oficial e carregado em tempo de uso:

```dockerfile
ADD https://github.com/pgvector/pgvector.git#v0.8.3 /tmp/pgvector
RUN make OPTFLAGS="" && make install
```

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

# Consequências

Positivas: wire-compatibility 100% com o PG 17 garantida por construção; build reproduzível e
auditável (SBOM via pgvector Apache 2.0); upgrade da extensão desacoplado do engine; e
`CREATE EXTENSION` é o mecanismo oficial, não um contorno.

Riscos e mitigações:

- **ABI drift** — quando o engine avança de major (17→18) o `.so` precisa ser recompilado.
  Mitigado por CI que re-builda por versão de PG declarada. A migração 17→18 foi de fato
  executada e medida em [m135 — migração PG18](/benchmarks/m135-pg18-migration.md).
- **Política de fork** — se o upstream ficar atrás dos requisitos de desempenho, forkar a
  *extensão* é permitido com benchmark de gatilho. Este ADR não bloqueia isso; define apenas
  que o **engine** não é forkado.

# Rationale adicional

O modelo de extensões do PostgreSQL foi projetado exatamente para esse padrão — `pg_trgm`,
PostGIS e dezenas de extensões de produção o seguem. O [AlloyDB](/technologies/alloydb.md),
âncora SOTA do projeto, usa o mesmo mecanismo para sua extensão vetorial baseada em
[ScaNN](/technologies/scann.md). Não há complexidade essencial em seguir o mesmo modelo.[^adr0001]

# Como este ADR evoluiu

O [ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) **ampliou** este ADR sem
derrubá-lo: o engine continua sendo o PostgreSQL em C, não-reescrito, e a alternativa A3 segue
rejeitada — mas o projeto passou a construir **extensões próprias em Rust** via
[pgrx](/technologies/pgrx.md), o que o modelo de extensão deste ADR já permitia.

[^adr0001]: ADR 0001 — Sem fork do engine PostgreSQL
