---
item: B-030
mode: evolve
date: 2026-08-12
verdict: pending
---

# B-030 — Um produto, três extensões: o umbrella é resíduo de uma migração concluída

## Corner 1 — Evidence

Medido em 2026-08-12.

### Não é camada — é co-propriedade de namespace

`theodb_rs/src/dtype.rs:392` cria **os dois** schemas:

```sql
CREATE SCHEMA IF NOT EXISTS theodb; CREATE SCHEMA IF NOT EXISTS ai;
```

E o umbrella `theodb` escreve dentro deles. O schema `ai` acaba com funções de **duas extensões distintas**:

| Origem | Objetos em `ai` |
|---|---|
| `theodb_rs` (Rust) | `ai._chat` (`api.rs:630`), `ai.if`, `ai.rank`, `ai.analyze_sentiment`, `ai.generate_batch`, `ai.hybrid_search`, `ai.nl_to_sql` |
| `theodb` (SQL) | `ai.generate`, `ai.summarize`, `ai.agg_summarize`, `ai._agg_summ_accum`, `ai._agg_summ_final`, `ai.nl_query`, `ai.nl_add_config`, `ai.nl_add_template`, `ai.nl_set_template_enabled`, `ai.nl_set_value_index`, `ai.nl_refresh_value_index`, `ai.nl_query_cfg` |

O mesmo em `theodb`: `theodb.embed`/`theodb.embed_batch`/`theodb.chunk` do Rust; `theodb.import_vectors_chunked`, `theodb._htap_path`, `theodb.htap_refresh`, `theodb.htap_register`, `theodb.olap`, `theodb.htap_freshness` do umbrella. Mais o schema `theodb_ml` inteiro (registry + 4 funções), criado só pelo umbrella.

**22 objetos** do umbrella, 471 linhas de código em 6 arquivos.

### A migração que justificava os dois terminou

Nenhuma `LANGUAGE plpython3u` ativa em `sql/` — as 7 ocorrências do termo são comentários históricos, verificado com `grep -riE "language +plpython3u"`. O `theodb` deixou de ser "a extensão SQL sendo reescrita" e virou invólucro.

Dois dos oito corpos-fonte já haviam chegado a **zero instruções** (`30-theodb-embed.sql`, `40-theodb-hybrid.sql` — 32 linhas de comentário afirmando criar schemas que não criavam), removidos em `d0771b3`.

### A divisão degrada ativamente o que restou

`sql/50-theodb-ai.sql`, linha 17, documenta o contorno:

> *"plpgsql (late-bound) so its body is not validated against `ai._chat` at CREATE time (`ai._chat` lives in theodb_rs, created after theodb)"*

`ai.generate` e `ai.summarize` são `LANGUAGE plpgsql` **apenas** porque a ordem entre extensões impede validação. É complexidade acidental produzindo perda de garantia.

**O destino já resolve isso.** `theodb_rs/src/api.rs` existe declaradamente para "the `extension_sql!` DDL that creates the public `theodb.*` / `ai.*` wrappers", e o macro aceita `requires = [...]` para ordenar a emissão — é exatamente o mecanismo que o umbrella não tem. Movido para lá com `requires`, `ai.generate` volta a ser `LANGUAGE sql`, validado no `CREATE`. Medido: **39 blocos `extension_sql!` em 11 módulos** já usam esse padrão, com 33 nomes registrados (`theodb_ai_wrappers`, `theodb_nl_wrappers`, `theodb_schema_bootstrap`, …).

### Não existe ADR decidindo que devem ser dois

`wiki/decisions/0029-m70-drop-pgvector.md` § D1 decide a **direção** da dependência e registra as rejeitadas:

> *"Rejeitadas: pôr o tipo no umbrella (impossível, o I/O está no `.so`) e manter a direção antiga (criaria ciclo)."*

Ambas são sobre *qual depende de qual*. "Colapsar o umbrella dentro do `theodb_rs`" nunca foi considerada — a existência de dois é premissa herdada da migração, nunca escolhida.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

Maior que o do B-031, porque muda o contrato de instalação.

| Alcance | Detalhe |
|---|---|
| `Dockerfile:124` | `CREATE EXTENSION IF NOT EXISTS theodb CASCADE` no initdb |
| `Dockerfile:93-113` | build do script de instalação + `install` do `theodb.control` |
| `Makefile` | alvo PGXS inteiro do umbrella |
| `theodb.control` | deixa de existir |
| `vector.control` | `requires = 'theodb_rs'` — **inalterado**, o shim permanece separado |
| `packaging/Dockerfile.m51-test:21` | usa `make install` do umbrella |
| Docs | `README.md`, `PRD.md:167`, `ROADMAP.md` (3 pontos), `wiki/guides/` |
| Repos irmãos | **nenhum** — `theo-rag`/`theo-memory` usam `pgvector` |

**O shim `vector` fica.** O nome é o contrato: drizzle/alembic/prisma emitem `CREATE EXTENSION vector` literalmente (ADR-0058, issue #181 mediu app que não subia). Colapsá-lo reintroduziria o bloqueio.

## Corner 4 — Verification

1. `CREATE EXTENSION theodb_rs` sozinho, em base limpa, entrega `theodb.*` + `ai.*` + `theodb_ml.*` completos — provado por comparação de `schema_snapshot.sql` antes/depois (o conjunto de objetos precisa ser um superconjunto do atual, menos os que saem por desenho).
2. Nenhum `theodb.control` nem `sql/theodb--*.sql` permanece.
3. `ai.generate` e `ai.summarize` passam a ser `LANGUAGE sql` — `SELECT prosrc, prolang::regprocedure FROM pg_proc` confirma, e o `CREATE` falharia se `ai._chat` não existisse (a validação que hoje não acontece).
4. **ACL preservada:** todo `REVOKE ... FROM PUBLIC` do umbrella sobrevive à mudança. Verificação explícita por `proacl`, porque `schema_snapshot.sql` declara que **não** cobre ACL — e a superfície `ai.*` faz HTTP de saída server-side.
5. `DROP EXTENSION theodb_rs CASCADE` não deixa objeto órfão em `theodb`/`ai`/`theodb_ml`.
6. O shim `vector` continua instalável sem `CASCADE` num banco que já tenha `theodb_rs` (o cenário do issue #181).

## Reclassificação

`suggested_mode: evolve` mantido.

## Dependência

Depende de **B-031** ser executado antes: com a cadeia de upgrade viva, o colapso arrastaria seis deltas do umbrella e exigiria um elo novo em cada cadeia. Removida a cadeia, o colapso é uma mudança de greenfield.
