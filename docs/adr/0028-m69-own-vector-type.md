# ADR 0028 — M69: tipo vetorial próprio `theodb.vector` (own-code, byte-idêntico ao pgvector)

- **Status:** Accepted
- **Date:** 2026-07-09
- **Milestone:** M69 (roadmap v4 "Independência do pgvector")
- **Evolui:** a postura "no competing type" (M20, `api.rs:770`; contexto do ADR-0006 "código próprio") — o TheoDB passa a **shipar** um tipo vetorial próprio, em vez de só reusar o do pgvector. Não revoga o ADR-0006 (LOCKED, estratégico).
- **Depende de:** blueprint SHIPPABLE `own-vector-type-drop-pgvector` (veredito A) + spike ADR-D3 (7/7 pg_test)
- **Deciders:** engenharia TheoDB

## Contexto

O North Star manda substituir pgvector/pgvectorscale por código próprio. Até o M68, o TheoDB **reusava** o tipo `vector` do pgvector (M20: "no competing type; reads `vector::real[]`"). O M69 é a fundação da remoção total: um tipo `vector` próprio own-code, coexistindo com o pgvector, provado byte-a-byte. O M70 removerá o pgvector.

**Prior-art honesto (blueprint Q3):** os dois AMs vetoriais próprios SOTA de licença permissiva — VectorChord e pgvectorscale — **reusam** o tipo do pgvector (`requires='vector'`). O TheoDB seria o 1º AM permissivo a shipar tipo `vector` próprio em pgrx. O spike ADR-D3 retirou o risco (7/7 pg_test em pg17 real).

## Decisão

### D1 — Layout `#[repr(C)]` byte-idêntico ao `Vector` do pgvector

O header on-disk é `{ varlena: u32, dim: u16, unused: u16, elements: [f32;0] }` (8 + 4·dim bytes), bit-a-bit igual ao pgvector (`vector.h:11-17`); `SET_VARSIZE` little-endian (`size << 2`); `unused` sempre 0 (viaja no wire e é validado). É a pré-condição do cast binário `WITHOUT FUNCTION` — coexistência em M69 + **migração grátis em M70** (sem reescrita de heap).

**Alternativa rejeitada:** layout próprio — perderia a coercibilidade binária, forçando reescrita O(N) de toda tabela na migração. Cita Regra 9 + KISS.

### D2 — Definição via `extension_sql!(CREATE TYPE)` + funcs I/O `#[pg_extern]` (não derive)

pgrx 0.16.1 não tem derive para tipo varlena de dimensão-variável. Os 6 traits pgrx (`FromDatum`, `IntoDatum` com `type_oid()->Oid::INVALID`, `UnboxDatum`, `SqlTranslatable`, `ArgAbi`, `BoxRet`) são implementados à mão sobre `NonNull<VecHeader>`. Confirmado empiricamente (spike + 14/14 pg_test M69).

### D3 — Reusar os kernels f32 de `vec.rs` para os operadores

`<->`/`<#>`/`<=>` chamam `vec::l2_distance`/`inner_product`/`cosine_distance` (o `<#>` é o inner-product **negativo**, paridade pgvector). Regra 9 / DRY — não reimplementar distância; o AM usa os mesmos kernels.

### D5 — Naming: `theodb.vector` (schema `theodb`) em M69; `SET SCHEMA public` em M70

O tipo é `theodb.vector` — nome `vector` no schema `theodb`. Coexiste com `public.vector` (pgvector) **sem colisão** (schemas distintos; colisão de nome de tipo é real — Immich #7318). As funções são prefixadas `theodb_vector_*` (evitam colisão com as funções `vector_*` do pgvector no schema public). Os operadores `<->`/`<#>`/`<=>` são criados unqualified em public, sobrecarregados por tipo de arg (`theodb.vector` vs `vector` — sem conflito). O M70 fará `ALTER TYPE theodb.vector SET SCHEMA public` ⇒ `::vector` do usuário resolve ao tipo próprio (drop-in, veredito A).

**Alternativas rejeitadas:** (A) nome distinto `theovec` — exigiria RENAME em M70; (B) `theodb.vector` permanente namespaced — não é drop-in (quebra `::vector`).

## Consequências

**Positivas:**
- Fundação da remoção total do pgvector (M70). O cast binário `WITHOUT FUNCTION` provado (14/14) habilita a migração grátis.
- Own-code: I/O (text + wire binário), typmod (parse + enforce via length-coercion cast), operadores, casts — o TheoDB deixa de depender do tipo do pgvector para o AM (o rebind das opclasses é M70).
- Zero regressão no hot path P0: o M69 NÃO tocou o AM (13/13 HNSW pg_tests GREEN).

**Negativas / caveats (honestos):**
- Território novo — nenhum peer permissivo shipa tipo próprio em pgrx. Mitigado pelo spike + o gate de paridade (14/14).
- O tipo coexiste com o pgvector em M69 (o AM ainda usa `public.vector`); a coexistência exige o pgvector instalado (por design; removido em M70).
- `recv/send` binário: `send` construído em Rust big-endian (robusto, sem StringInfo FFI); `recv` via `pq_getmsgint`/`pq_getmsgfloat4` — validado por COPY BINARY round-trip.

**Validação:** 14/14 dtype pg_tests GREEN (paridade `vector_type`/`cast`/`copy` binário + byte-compat dim-variado + typmod + negative-cases + memória sem UAF) + 13/13 HNSW AM GREEN (não-regressão), em pg17 real + pgvector coexistindo. **Sem claim de performance** (M69 é correção/paridade; o dado exigido é o gate de paridade byte-a-byte, cumprido).

## Licença (INQUEBRÁVEL, D1 do projeto)

Código ORIGINAL. Técnica de varlena aprendida de fontes permissivas (pgvector = PostgreSQL License; `postgres.h`; docs pgrx). **VectorChord é AGPLv3/ELv2 — SÓ estudo, nunca copiado** (`[[vectorchord-agpl-study-only]]`).
