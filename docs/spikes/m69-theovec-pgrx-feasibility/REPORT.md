# Spike M69 (ADR-D3) — Viabilidade de tipo `vector` próprio em pgrx 0.16.1

**Data:** 2026-07-09 · **Verdito:** ✅ **RETIRADO POSITIVO** — a fase A do blueprint é viável.
**Ambiente:** droplet DO c-8, PostgreSQL 17.10 (PGDG) real + pgvector (apt) + pgrx 0.16.1, cargo pgrx test.

## Pergunta (blueprint ADR-D3)

Nenhum AM permissivo shipa um tipo `vector` próprio em pgrx — VectorChord e pgvectorscale reusam o do
pgvector (`requires='vector'`). O padrão pgrx de **definir** um tipo denso de **dimensão-variável**
(varlena com flexible array) não tinha prior-art público. **É viável definir esse tipo em pgrx 0.16.1
com layout `#[repr(C)]` byte-idêntico ao `Vector` do pgvector?** Se não, o veredito A do blueprint cai.

## Resultado: 7/7 pg_test GREEN (pg17 real)

| Teste | O que prova |
|---|---|
| `roundtrip_text_io` | I/O text (`'[1,2,3]'::theovec::text` == `[1,2,3]`) |
| `typmod_dim_ok` | `theovec(3)` parseia |
| `typmod_dim_mismatch_rejected_on_column` | typmod **enforça** no INSERT (via length-coercion cast) |
| `nan_rejected` | rejeição NaN — erro tipado (negative-case, paridade pgvector) |
| `operator_l2_distance` | binding operador `<->` ↔ tipo (`[0,0]<->[3,4]`==5) |
| `table_column_and_order_by` | coluna `theovec(2)` + `ORDER BY e <-> ...` |
| **`binary_compat_with_pgvector`** | **layout byte-idêntico: `CREATE CAST (vector AS theovec) WITHOUT FUNCTION` funciona nos 2 sentidos** |

Código: `theovec_spike_lib.rs` (367 linhas, ORIGINAL — ver caveat de licença abaixo).

## A receita descoberta (fundação do M69)

1. **Layout `#[repr(C)]` byte-idêntico ao pgvector** (`vector.h:11-17`):
   `struct { varlena: u32, dim: u16, unused: u16, elements: [f32; 0] }` — 8 + 4·dim bytes on-disk.
2. **SET_VARSIZE little-endian:** `varlena = (size << 2) as u32`.
3. **Datum plumbing pgrx 0.16.1** (6 traits, API ditada pelo pgrx): `FromDatum::from_polymorphic_datum`,
   `IntoDatum` (`into_datum` + `type_oid()->Oid::INVALID` + `is_compatible_with->true`),
   `UnboxDatum::unbox`, `SqlTranslatable` (`SqlMapping::As("theovec")`), `ArgAbi::unbox_arg_unchecked`,
   `BoxRet::box_into`.
4. **`CREATE TYPE` via `extension_sql!`** (shell `bootstrap` → I/O funcs `#[pg_extern]` → tipo completo
   com `INPUT/OUTPUT/TYPMOD_IN, STORAGE=external, INTERNALLENGTH=variable`). pgrx NÃO tem derive para
   tipo varlena de tamanho variável → a rota é `extension_sql!` cru (confirmado).
5. **Typmod ENFORCEMENT exige o length-coercion cast** (a peça não-óbvia): função
   `theovec(theovec, integer, boolean)` + `CREATE CAST (theovec AS theovec) WITH FUNCTION ... AS IMPLICIT`
   — espelha `pgvector/sql/vector.sql:134,154`. Sem ela, `theovec(N)` parseia mas não enforça no INSERT.
6. **Migração:** `CREATE CAST (vector AS theovec) WITHOUT FUNCTION` prova que o layout é reinterpretável
   sem reescrita (o gate de migração binária do M70).
7. **Estrutura do crate:** `[[bin]] name = "pgrx_embed_<crate>"` + `pgrx-tests/pgXX` nas features +
   `.control` — usar `cargo pgrx new` para o esqueleto (não montar à mão).
8. **Negative-case em pg_test:** um pg ERROR faz longjmp, não é panic Rust → usar `#[pg_test(error="...")]`,
   NÃO `catch_unwind`.

## Deferido para o M69 (fora do escopo do spike de viabilidade)

- `recv`/`send` binário (wire, COPY BINARY) — o layout está provado; falta implementar as funções (o campo
  `unused`==0 no wire, `vector.c:369-416`).
- Operadores `<#>`/`<=>` (só `<->` no spike), casts com `real[]`/`float8[]`/`text`, opclasses do AM (M70).
- Corpus de paridade completo (`vector_type.sql`/`cast.sql`/`copy.sql`).

## Caveat de licença (INQUEBRÁVEL)

O código do spike é **ORIGINAL**. A **técnica** de manipulação de varlena foi aprendida de fontes
permissivas (`pgvector` = PostgreSQL License; docs pgrx; `postgres.h` SET_VARSIZE). O **VectorChord é
AGPLv3/ELv2** — proibido na distribuição Apache do TheoDB (D1); foi **só estudado**, nunca copiado.
