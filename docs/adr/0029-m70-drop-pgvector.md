# ADR 0029 — M70: remover o pgvector (e pgvectorscale) totalmente

- **Status:** Accepted
- **Date:** 2026-07-09
- **Milestone:** M70 (roadmap v4 "Independência do pgvector") — fecha o ROADMAP_COMPLETED (M69+M70)
- **Depende de:** M69 (`docs/adr/0028` — o tipo `vector` own-code byte-idêntico) + blueprint SHIPPABLE
- **Deciders:** engenharia TheoDB

## Contexto

O M69 entregou o tipo `theodb.vector` own-code byte-idêntico ao pgvector, coexistindo. O M70 **remove o
pgvector totalmente** (pedido explícito do usuário + North Star): move o tipo para `public.vector` (drop-in),
resolve a dependência circular via **flip**, e tira o pgvector/pgvectorscale da distribuição. O TheoDB passa
a ser o **1º AM permissivo com um tipo `vector` 100% own-code** (VectorChord e pgvectorscale reusam o do
pgvector — finding do blueprint).

## Decisão

### D1 — Flip da dependência: `theodb_rs` vira a base

`theodb_rs.control requires` ZERA; `theodb_rs` provê o tipo `public.vector` + os AMs + os schemas
`theodb`/`ai` (via um bloco `extension_sql` nomeado `theodb_schema_bootstrap` que os blocos `theodb.*`/`ai.*`
e as opclasses do AM declaram em `requires`). O umbrella `theodb.control requires` vira `theodb_rs`.

**Rationale:** o tipo own-code (Rust, I/O no `.so` do theodb_rs) só pode ser criado pelo theodb_rs. Como o
umbrella USA o tipo, ele deve depender do theodb_rs. Removido o pgvector (o 3º que quebrava o ciclo), o flip
é a única topologia acíclica. **Alternativa rejeitada:** tipo no umbrella (impossível — I/O no `.so`); manter
`theodb_rs requires theodb` (ciclo). Cita `architecture.md § 2`.

### D2 — Tipo em `public.vector` (drop-in)

O tipo é `public.vector` (nome `vector`, schema public). Sem pgvector, não há colisão. `::vector` do usuário
e o `FOR TYPE vector` das opclasses resolvem ao tipo próprio SEM mudança de código. As funções I/O ficam
prefixadas `theodb_vector_*` (herança do M69, sem custo).

**Rationale:** o veredito A do blueprint quer `::vector` drop-in. **Alternativa rejeitada:** `theodb.vector`
permanente (não é drop-in). Precedente: M69 (a transição planejada).

### D3 — Migração de instalações existentes via cast binário

Upgrade de um banco com colunas `vector` do pgvector: `CREATE CAST (vector AS theodb.vector) WITHOUT
FUNCTION` + `ALTER COLUMN TYPE` (byte-cast grátis, O(1), sem reescrita de heap) + REINDEX dos índices ANN.
Documentado em `docs/ops/pgvector-migration.md`.

**Rationale:** o layout byte-idêntico (M69) torna a migração de dados grátis. **Alternativa rejeitada:**
dump/restore (downtime + reescrita O(N)).

### D4 — Gate de não-regressão: os AM pg_tests set-equal sobre o tipo próprio, SEM pgvector

A prova de que o tipo próprio substitui 100% o pgvector no AM é a suíte de pg_tests do AM
(`set-equal-vs-seqscan`: top-k índice == top-k exato) GREEN com `CREATE EXTENSION theodb_rs` **sem**
pgvector/vectorscale.

**Rationale:** o set-equal é o gate de recall executável; GREEN sobre `public.vector` own prova a remoção
sem regressão. **Alternativa rejeitada:** benchmark de performance (M70 é correção; o dado é a não-regressão).

## Consequências

**Positivas:**
- **pgvector e pgvectorscale REMOVIDOS** da distribuição (Dockerfile + requires). O TheoDB é independente do
  pgvector — o tipo `vector`, os operadores, os casts e os AMs ANN são 100% own-code.
- Imagem menor (sem o stage de build do pgvectorscale nem o `make install` do pgvector).
- Fecha o roadmap v4 e o North Star do pilar de independência.

**Negativas / caveats (honestos):**
- **REINDEX obrigatório na migração** (as opfamilies do pgvector não são compartilhadas). A coluna migra
  grátis (byte-cast); os índices ANN são recriados.
- O `ALTER COLUMN TYPE` pega `ACCESS EXCLUSIVE` breve (agendar em janela de baixa carga para tabelas quentes).
- **pg_duckdb intocado** (columnar/HTAP, MIT — independente do pilar vetorial).

**Validação (pg17 real, SEM pgvector):**
- **229/230 suíte completa GREEN** com `theodb_rs` standalone (a 1 falha é `pg_cosine_simd_per_candidate_speedup`,
  um teste de **timing SIMD** flaky sob carga — passa isolado; o M70 não tocou `vec.rs`).
- Os pg_tests do AM (`set-equal-vs-seqscan`) + 15/15 dtype + 13/13 HNSW GREEN sobre `public.vector`.
- **`CREATE EXTENSION theodb CASCADE` sem pgvector** — extensões instaladas: `theodb` + `theodb_rs`
  (zero `vector`/`vectorscale`); `'[1,2,3]'::vector` resolve ao tipo próprio. Remoção total provada end-to-end.
- **Sem claim de performance** (M70 é correção/paridade; o dado é o gate de não-regressão de recall).

## Licença (INQUEBRÁVEL, D1 do projeto)

Código ORIGINAL. VectorChord é AGPLv3/ELv2 — SÓ estudo, nunca copiado (`[[vectorchord-agpl-study-only]]`).
O tipo espelha o `vector.c` do pgvector (PostgreSQL License, permissivo). Supersede o reuso do tipo (ADR-0006
M20; evoluído pelo ADR-0028 M69).
