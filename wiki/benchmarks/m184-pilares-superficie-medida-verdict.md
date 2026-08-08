---
type: Measurement
title: m184 (parcial) — a superfície real dos pilares no binário default, e a primeira divergência da tabela de maturidade
description: Consulta ao catálogo do PostgreSQL mostra que o SymQG está registrado como access method no binário default, contra a nota que o classificou como experimental — a avaliação por leitura errou onde a execução responde.
resource: benchmarks/artifacts/m184/pillar-surface-measured.json
tags: [benchmark, m184, pilares, maturidade, divergencia, catalogo, superficie-sql, parcial]
milestone: M184
generated: { by: claude-code/opus-5, at: 2026-08-08T05:00:00Z }
sources:
  - id: surface
    resource: benchmarks/artifacts/m184/pillar-surface-measured.json
    title: Catálogo do PostgreSQL consultado no binário default, CPU dedicada
---

**Entrega parcial do M184.** Mede o eixo mais barato e mais objetivo da tabela de maturidade — *o que o
usuário de fato recebe no binário default* — consultando o catálogo do PostgreSQL em vez de ler
documentação. É exatamente o método que a tabela original **não** usou.

# O método

Droplet `c-8` de CPU dedicada, imagem `ghcr.io/usetheodev/theo-db:0.139.0`,
`CREATE EXTENSION theodb CASCADE`, e então `pg_am` / `pg_proc` consultados. Nenhuma feature flag.

# O que o binário default expõe

| access method | tipo |
|---|---|
| `theodb_columnar` | table |
| `theodb_hnsw` | index |
| `theodb_ivfflat` | index |
| **`theodb_symqg`** | **index** |

| superfície de função | quantidade |
|---|---|
| grafo (`graph`/`bfs`/`ppr`) | **23** |
| parquet / lakehouse | **4** |
| **lexical / BM25** | **0** |

# A primeira divergência medida

**O SymQG está registrado como access method no binário default.** A tabela de 2026-08-07 o classificou
como **1 — "experimental, não recomendado como default"**, lendo o `feature_status` da wiki.

As duas coisas são compatíveis em texto e não em consequência: *não recomendado* não é o mesmo que
*ausente*, e a nota 1 foi atribuída como se fosse. Um usuário pode escrever `USING theodb_symqg` hoje,
sem feature flag, e receber o índice que o [e2](/benchmarks/e2-symqg-inpg-verdict.md) mediu como **2,6–3,9×
mais lento** que a alternativa a recall casado.

Isso **agrava** o M176 em vez de aliviá-lo: não é código morto atrás de uma flag, é superfície pública
medida como pior. A decisão de promover-ou-aposentar deixa de ser higiene de repositório e passa a ter
consequência para quem instala.

**Confirmações, ditas com a mesma clareza:** o pilar lexical tem **zero** funções expostas — a nota 2
("fora do binário default") estava certa, e agora por catálogo, não por leitura do `Cargo.toml`. Grafo e
lakehouse estão presentes e amplos (23 e 4 funções), coerentes com as notas 3.

# O que este artefato NÃO mede

Ele cobre **um** eixo da régua — presença na superfície. **Não** mede performance, qualidade, crash-safety
nem cobertura de teste, que são os outros eixos das notas atribuídas. As notas 0–5 **continuam sem
verificação completa**, e o M184 continua aberto.

Um limite honesto de método: contar `pg_extern` no fonte e contar `pg_proc` no catálogo **discordam** —
`graph.rs` tem 9 `pg_extern` e o catálogo mostra 23 funções com nome de grafo, porque o `api.rs` é um
facade único ([ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md)) e há SQL declarativo em
`extension_sql!`. **O catálogo é a fonte de verdade**; a contagem no fonte subestima.

# Relacionados

- A tabela que este milestone audita: § Roadmap v7 do `ROADMAP.md`
- O veredito que mediu o SymQG como mais lento: [e2](/benchmarks/e2-symqg-inpg-verdict.md)
- O milestone que decide promover ou aposentar: M176
- A restrição de facade que explica a divergência de contagem: [ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md)
