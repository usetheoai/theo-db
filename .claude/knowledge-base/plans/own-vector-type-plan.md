---
slug: own-vector-type
milestone_id: M69
created_at: 2026-07-09
goal: Shipar o tipo `theodb.vector` own-code byte-idêntico ao pgvector, provado por uma suíte de paridade 100% GREEN em pg17 real, sem tocar o AM.
---

# Plano de Implementação: Tipo vetorial próprio own-code (M69)

## Goal

Shipar o tipo `theodb.vector` own-code no theodb_rs, com layout `#[repr(C)]` byte-idêntico ao `Vector` do pgvector, **coexistindo** com o pgvector, provado por uma **suíte de paridade pg_test 100% GREEN** (I/O text + binário + typmod + operadores + cast byte-compat com pgvector) em pg17 real — **sem tocar o AM/índice** (zero regressão no hot path P0).

- **Métrica observável:** a suíte de paridade `dtype` pg_test passa 100% (GREEN) em pg17 real, incluindo o teste de cast binário `WITHOUT FUNCTION` bidirecional com o `vector` do pgvector (prova o layout byte-idêntico).

## Context

O usuário exige remover o pgvector **totalmente** (roadmap v4). Este plano entrega o **M69** — a fundação: o tipo próprio coexistindo com o pgvector. Decisões já resolvidas a montante:

- **Discover (blueprint SHIPPABLE 99.7)** `.claude/knowledge-base/discoveries/blueprints/own-vector-type-drop-pgvector-blueprint.md` — veredito **A** (tipo próprio nomeado `vector`, `#[repr(C)]` byte-idêntico, drop-in), decomposto em M69 (tipo coexistindo) + M70 (opclasses + migração + remover pgvector).
- **Spike ADR-D3 RETIRADO POSITIVO** (`docs/spikes/m69-theovec-pgrx-feasibility/REPORT.md`) — 7/7 pg_test em pg17 real provam a viabilidade + a receita completa (layout, 6 traits pgrx, CREATE TYPE via extension_sql, length-coercion cast p/ typmod, cast binário p/ paridade). Este plano **reusa o spike como fundação**.
- **Naming/coexistência (ADR-D5 abaixo):** o tipo próprio é `theodb.vector` (schema `theodb`, nome `vector`) — coexiste com `public.vector` (pgvector) sem colisão (schemas diferentes). O M70 fará `ALTER TYPE theodb.vector SET SCHEMA public` ao remover o pgvector → drop-in.
- **Licença (INQUEBRÁVEL, D1):** código ORIGINAL; técnica de varlena aprendida do **pgvector** (PostgreSQL License) + `pg_sys`. **VectorChord é AGPL** (`[[vectorchord-agpl-study-only]]`) — SÓ estudo, nunca copiar.

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Papel / por que |
|---|---|---|---|
| `theodb_rs/src/dtype.rs` | (NEW) | — | O tipo `theodb.vector`: header `#[repr(C)]`, datum plumbing, parse/format, I/O, typmod, recv/send, operadores, casts, DDL. |
| `theodb_rs/src/lib.rs` | 164 | f7d7ca4 | Composition root — declara os `mod` (`:31-48`). Adicionar `mod dtype;`. |
| `theodb_rs/src/vec.rs` | 521 | (M46/M49) | Kernels f32 `pub(crate)`: `l2_distance:41`, `inner_product:49`, `cosine_distance:61`. Reuso p/ os operadores (sem reinventar — Regra 9). |
| `theodb_rs/src/pg.rs` | 56 | (ADR-1) | Helpers de erro tipado `err_input:8`/`err_external:19`/`err_unsupported:31` (`ereport`). Reuso p/ validação. |
| `theodb_rs/CHANGELOG` (raiz `CHANGELOG.md`) | — | — | Entry `[Unreleased] § Added` (Regra 6). |

### Current callers / dependents

- **`vec.rs` kernels** (`l2_distance`/`inner_product`/`cosine_distance`) — chamados hoje por `ann/scan_core.rs`, `ann_query.rs`, `sbq.rs` (via `*_from_bytes`). O M69 ADICIONA novos chamadores (os operadores do tipo); NÃO altera as assinaturas existentes → zero impacto nos callers atuais.
- **`pg.rs::err_input`** — chamado por `api.rs`, `autotune.rs`, `migrate.rs`, `nl.rs`. O M69 adiciona chamadas; sem alteração de assinatura.
- **O tipo `theodb.vector` (NEW)** — nenhum caller de produção em M69 (é um tipo novo, coexistente). O wiring-triad pillar (a) é satisfeito pelos pg_tests que criam colunas `theodb.vector` + o teste de cast que o exercita end-to-end. Os `::vector` de produção (hybrid/embed/vectorizer/pq/api/sbq) continuam usando o `public.vector` do pgvector — sua reescrita é **M70**, não M69.

### Domain glossary

- **varlena** — o formato de valor de tamanho variável do Postgres (header de 4 bytes com o comprimento; `postgres.h` SET_VARSIZE/VARSIZE).
- **typmod** — type modifier; para `vector(N)` codifica a dimensão N (`vector_typmod_in`).
- **length-coercion cast** — o cast `(tipo AS tipo) WITH FUNCTION tipo(tipo,int,bool)` que o Postgres chama para APLICAR/enforçar o typmod em atribuições (pgvector `vector.sql:154`).
- **binary-coercible cast** — `CREATE CAST ... WITHOUT FUNCTION`: reinterpreta os bytes sem função quando dois tipos têm layout idêntico.
- **recv/send** — I/O binário do wire (`COPY ... FORMAT binary`); o campo `unused` viaja no wire e é validado ==0.

### Architecture boundaries affected

Per `.claude/rules/architecture.md`: o tipo é um módulo coeso (`dtype.rs`) com responsabilidade única (o tipo `theodb.vector` e suas operações). Depende de `vec.rs` (kernels) e `pg.rs` (erros) — dependências internas estáveis, sem indireção nova (DIP não exige interface aqui — KISS). Budget de arquivo: 500 LoC (`architecture.md`); se `dtype.rs` exceder com operadores+casts+testes, os operadores/casts vão para `dtype_ops.rs` (declarado como sub-decisão da Fase 3).

## Prior Art & Related Work

- **Blueprint** `own-vector-type-drop-pgvector-blueprint.md` (SHIPPABLE 99.7) — o veredito A + o corpus de paridade (Corner 3) + a receita de opclass (Corner 4, para o M70).
- **Spike** `docs/spikes/m69-theovec-pgrx-feasibility/` (7/7 GREEN) — a fundação: `theovec_spike_lib.rs` (a implementação de referência own-code a portar para `theodb.vector`).
- **pgvector** (`.claude/knowledge-base/references/pgvector/src/vector.c`, `sql/vector.sql`, `test/`) — PostgreSQL License; o contrato/semântica a espelhar (parse, typmod, operadores, casts, corpus de teste).
- **pgvectorscale** `pg_vector.rs` (PostgreSQL License) — o padrão de consumo do datum em pgrx (`#[repr(C)]` mirror).

## ADRs

### D1 — Reusar o layout `#[repr(C)]` byte-idêntico ao pgvector (não um layout próprio)

**Decisão:** o header on-disk é `{ varlena: u32, dim: u16, unused: u16, elements: [f32;0] }` (8 + 4·dim bytes), bit-a-bit igual ao `Vector` do pgvector; `SET_VARSIZE` little-endian (`size << 2`); o campo `unused` sempre 0.

**Rationale:** o layout byte-idêntico é a pré-condição do cast binário `WITHOUT FUNCTION` (coexistência em M69 + migração grátis em M70). Provado no spike (`binary_compat` GREEN). **Alternativa rejeitada:** layout próprio (ex. sem `unused`, ou `u32` dim) — perderia a coercibilidade binária, forçando reescrita de heap na migração do M70 (custo O(N) em toda tabela). Cita Regra 9 (não reinventar o layout) + KISS.

### D2 — Definir o tipo via `extension_sql!(CREATE TYPE)` + funcs I/O `#[pg_extern]` (não derive)

**Decisão:** o tipo é criado por `extension_sql!` (shell `bootstrap` → I/O funcs → tipo completo), com os 6 traits pgrx (`FromDatum`, `IntoDatum` com `type_oid()->Oid::INVALID`, `UnboxDatum`, `SqlTranslatable`, `ArgAbi`, `BoxRet`) implementados à mão sobre `NonNull<Header>`.

**Rationale:** pgrx 0.16.1 NÃO tem derive para tipo varlena de **dimensão-variável** (flexible array) — `#[derive(PostgresType)]` cobre só structs de tamanho fixo (spike § receita; confirmado empiricamente). **Alternativa rejeitada:** `#[derive(PostgresType)] + PgVarlenaInOutFuncs` — `PgVarlena<T>` fixa `varsize = size_of::<T>()`, incompatível com o array trailing de dim variável. Cita a evidência do spike (7/7 com esta rota).

### D3 — Reusar os kernels f32 de `vec.rs` para os operadores (não reimplementar distância)

**Decisão:** os operadores `<->`/`<#>`/`<=>` chamam `vec::l2_distance`/`inner_product`/`cosine_distance`.

**Rationale:** Regra 9 (não reinventar) — os kernels já existem, testados, e o AM os usa. **Alternativa rejeitada:** reimplementar as distâncias em `dtype.rs` — duplicaria conhecimento (DRY) e divergiria do AM. Cita `parsimony-ladder.md` rung 4 (reusar o que existe).

### D5 — Naming: o tipo próprio é `theodb.vector` (schema `theodb`) em M69; `SET SCHEMA public` em M70

**Decisão:** o tipo é `theodb.vector` (schema `theodb`, nome `vector`). Coexiste com `public.vector` (pgvector) sem colisão (schemas distintos). Casts bidirecionais `theodb.vector ↔ public.vector`. O M70, ao remover o pgvector, faz `ALTER TYPE theodb.vector SET SCHEMA public` → drop-in (`::vector` do usuário resolve ao tipo próprio).

**Rationale:** o veredito A do blueprint quer o nome final `vector` drop-in, mas o pgvector ocupa `public.vector` em M69 (colisão real — Immich #7318, blueprint Q3). Usar o schema `theodb` evita a colisão SEM prefixar o nome do tipo (é `vector`, não `theovec`), então o `SET SCHEMA` do M70 é trivial. **Alternativa rejeitada (A):** nome distinto `theovec` (o spike) → exigiria RENAME em M70 + confundiria o usuário. **Alternativa rejeitada (B):** `theodb.vector` permanente (namespaced) → não é drop-in (quebra `::vector`), contra o veredito A.

### D6 — Supersede ADR-0006 (reuso do tipo pgvector)

**Decisão:** este plano supersede o ADR-0006 ("coexists with pgvector; no competing type"). Registrar um ADR novo em `docs/adr/` na Fase 5.

**Rationale:** o ADR-0006 fixou o reuso; o North Star (substituir pgvector) exige o tipo próprio. **Alternativa rejeitada:** manter ADR-0006 (reuso) — contradiz o pedido explícito de remoção total.

## Dependency Graph

```
Fase 1 (tipo + I/O text) ──▶ Fase 2 (typmod + binário) ──▶ Fase 3 (operadores + casts)
        │                                                          │
        └──────────────────────────┬───────────────────────────────┘
                                    ▼
                         Fase 4 (gate de paridade completo)
                                    ▼
                         Fase 5 (integration validation — droplet pg17 real)
```

Fase 1 é a fundação (bloqueia todas). Fases 2 e 3 dependem de 1 (o tipo + datum plumbing); podem ser sequenciais (2→3) pois o cast binário da Fase 3 reusa o recv/send da Fase 2. Fase 4 depende de 1-3. Fase 5 é terminal.

## Phase 1 — Tipo `theodb.vector` + I/O text (fundação)

### T1.1 — Header `#[repr(C)]` + `TheoVec` + datum plumbing + I/O text + CREATE TYPE

#### TDD
- RED: `#[pg_test] fn roundtrip_text_io` — `SELECT '[1,2,3]'::theodb.vector::text` == `[1,2,3]` (falha: tipo não existe).
- RED: `#[pg_test(error = "NaN not allowed")] fn nan_rejected` — `SELECT '[1,NaN,3]'::theodb.vector`.
- RED: `#[pg_test(error = "at least 1 dimension")] fn dim0_rejected` — `SELECT '[]'::theodb.vector`.
- RED (EC-2, boundary): `#[pg_test] fn dim_boundary` — `'[1]'::theodb.vector` (dim=1) e dim=16000 round-trip OK; `'[...]'::theodb.vector` com 16001 dims → erro "cannot exceed 16000" (paridade `vector.c:88-100`).
- RED (EC-1, memória): `#[pg_test] fn datum_roundtrip_no_uaf` — loop SQL 1000× `SELECT ('[1,2,3]'::theodb.vector)::theodb.vector(3)` (cast recebe E retorna o mesmo ptr) sem crash — pega double-free/use-after-free na disciplina `into_raw`/`Drop`.
- GWT: Given o tipo criado, When parse de `[1,2,3]`, Then round-trip byte-idêntico; When NaN/Inf/dim0, Then erro tipado (paridade `vector_type.sql`).
- GREEN: implementar `dtype.rs` (header, `TheoVec`, 6 traits, `parse_theovec`/`format_theovec` espelhando `vector.c:172-320`, `theodb_vector_in`/`out`, CREATE TYPE shell+full no schema `theodb`). **DISCIPLINA DE MEMÓRIA (EC-1):** `into_datum` DEVE consumir `self` via `into_raw()` (`mem::forget`) — o Drop libera SÓ no path onde o valor não é retornado; nunca ambos (double-free) nem nenhum (leak). Espelha o spike (`into_raw` + `Drop`).

#### Files to edit
- `theodb_rs/src/dtype.rs` (NEW) — o tipo core.
- `theodb_rs/src/lib.rs` — adicionar `mod dtype;` (após `:45 mod vec;`).

#### Deep file dependency analysis
`dtype.rs` usa `pg.rs::err_input` (validação) e `pgrx::pg_sys` (palloc0/pfree/pg_detoast_datum_copy). `lib.rs` só ganha uma linha `mod dtype;` — sem impacto nos outros mods. O schema `theodb` já é criado pela umbrella (o tipo entra nele via `CREATE TYPE theodb.vector`).

#### Why this step
**Ação:** portar a fundação do spike (`theovec_spike_lib.rs`) para `dtype.rs` como `theodb.vector`. **Raciocínio:** o spike provou (ADR-D3, 7/7) que esta é a única rota viável em pgrx (D2); o I/O text é a base testável sem índice (isola a superfície do tipo — SRP). Cita o spike REPORT § receita.

#### Concurrency tests
(none — single-threaded). Manipulação de varlena in-memory, sem locks/atomics/threads.

#### Acceptance criteria
- `SELECT '[1,2,3]'::theodb.vector::text` retorna exatamente `[1,2,3]`; `'[1,NaN,3]'::theodb.vector` levanta ERROR contendo "NaN not allowed"; `'[]'::theodb.vector` levanta ERROR "at least 1 dimension" (oracle: os 3 pg_tests da T1.1 GREEN).
- `wc -l theodb_rs/src/dtype.rs` retorna ≤ 500 (budget de produção `architecture.md`).
- `grep "err_input\|err_" theodb_rs/src/dtype.rs` mostra uso do helper tipado de `pg.rs` (não `panic!` cru) nos paths de validação.

#### DoD
- `cargo pgrx test pg17 roundtrip_text_io nan_rejected dim0_rejected` GREEN (droplet).
- `git commit` `feat(dtype): T1.1 tipo theodb.vector + I/O text`.

## Phase 2 — typmod + I/O binário (recv/send)

### T2.1 — typmod_in + length-coercion cast + recv/send binário

#### TDD
- RED: `#[pg_test(error = "expected 3 dimensions, not 2")] fn typmod_enforced_on_column` — `CREATE TEMP TABLE t(e theodb.vector(3)); INSERT INTO t VALUES('[1,2]')`.
- RED: `#[pg_test] fn typmod_ok` — `'[1,2,3]'::theodb.vector(3)::text` == `[1,2,3]`.
- RED: `#[pg_test] fn binary_roundtrip_copy` — `COPY t TO ... FORMAT binary` → `COPY t2 FROM ... FORMAT binary` preserva os valores + linha NULL (exercita recv/send com `unused`==0).
- RED (EC-3, recv adversário): `#[pg_test(error = "expected unused to be 0")] fn recv_rejects_nonzero_unused` — wire binário construído com `unused=1` é rejeitado no recv (paridade `vector.c:378-388`).
- GREEN: `theodb_vector_typmod_in` (dim 1..16000), `theodb_vector` length-coercion cast `(theodb.vector,int,bool)` + `CREATE CAST (theodb.vector AS theodb.vector)`, `theodb_vector_recv`/`send` (wire `int16 dim + int16 unused + f32[] big-endian`, espelha `vector.c:369-416`).

#### Files to edit
- `theodb_rs/src/dtype.rs` — adicionar typmod_in, length-coercion cast, recv/send + estender a DDL do CREATE TYPE (TYPMOD_IN/RECEIVE/SEND).

#### Deep file dependency analysis
Estende o CREATE TYPE da T1.1 (mesma `extension_sql!` block OU um bloco `requires`-encadeado). O recv/send usa `pg_sys::StringInfo`/`pq_getmsgfloat4`/`pq_sendfloat4` (via pgrx `pg_sys`). Sem novo caller externo.

#### Why this step
**Ação:** completar o I/O — typmod enforce (o length-coercion cast, a peça não-óbvia do spike) + o wire binário (que o spike deferiu). **Raciocínio:** o typmod enforce e o round-trip binário são casos do corpus de paridade pgvector (`vector_type.sql:32-37`, `copy.sql`); sem eles a paridade byte-a-byte não fecha. Cita spike REPORT § "deferido para o M69" (recv/send) + § receita (length-coercion cast).

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- `theodb.vector(3)` enforça a dimensão em INSERT/cast; `COPY FORMAT binary` round-trip byte-idêntico incl. NULL; `unused`==0 validado no recv.
- Paridade das mensagens de erro de typmod com pgvector (`expected N dimensions, not M`).

#### DoD
- `cargo pgrx test pg17 typmod_enforced_on_column typmod_ok binary_roundtrip_copy` GREEN.
- `git commit` `feat(dtype): T2.1 typmod + recv/send binario`.

## Phase 3 — Operadores + casts (incl. cast byte-compat com pgvector)

### T3.1 — Operadores `<->`/`<#>`/`<=>` + casts (`real[]`/`float8[]`/`text` + pgvector bidirecional)

#### TDD
- RED: `#[pg_test] fn operators_match_kernels` — `'[0,0]'::theodb.vector <-> '[3,4]'` == 5; `<#>` == neg-inner; `<=>` == cosine (valores vs `vec.rs` oracle).
- RED: `#[pg_test] fn casts_array_roundtrip` — `ARRAY[1,2,3]::real[]::theodb.vector::real[]` == `{1,2,3}`; `float8[]`, `text`.
- RED: `#[pg_test] fn binary_compat_with_pgvector` — `CREATE CAST (vector AS theodb.vector) WITHOUT FUNCTION` + `'[1,2,3]'::vector::theodb.vector::text` == `[1,2,3]` (e o inverso), **testado em dim=1, dim=3 E dim=128 (EC-4)** — o layout `8+4·dim` tem que ser byte-idêntico em qualquer dim, não só dim=3. **O GATE do layout byte-idêntico.**
- GREEN: `theodb_vector_l2/ip/cosine_distance` (reuso `vec.rs`), CREATE OPERATOR `<->`/`<#>`/`<=>`, casts `real[]`/`float8[]`/`text`↔ + `CREATE CAST (public.vector AS theodb.vector) WITHOUT FUNCTION` + inverso.

#### Files to edit
- `theodb_rs/src/dtype.rs` (ou `dtype_ops.rs` NEW se `dtype.rs` > 500 LoC — decisão de split registrada aqui) — operadores + casts + DDL.

#### Deep file dependency analysis
Os operadores chamam `vec::{l2_distance,inner_product,cosine_distance}` (D3). O cast com pgvector requer o `public.vector` presente (coexistência — pgvector instalado). Se split em `dtype_ops.rs`, adicionar `mod dtype_ops;` no `lib.rs` e a DDL de operadores/casts com `requires=[dtype_type]`.

#### Why this step
**Ação:** dar ao tipo próprio os operadores (reusando `vec.rs`) + a malha de casts, incl. o cast binário com o pgvector. **Raciocínio:** os operadores são exigidos para o tipo ser usável em `ORDER BY`; o cast byte-compat é o GATE que prova o layout idêntico (a métrica do Goal) + habilita a coexistência/migração. Cita D3 (reuso kernels) + spike `binary_compat` GREEN.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- `<->`/`<#>`/`<=>` retornam valores idênticos aos kernels `vec.rs`; casts round-trip; **cast binário `WITHOUT FUNCTION` pgvector↔theodb.vector funciona nos 2 sentidos**.
- Se split: `dtype.rs` e `dtype_ops.rs` ambos ≤ 500 LoC.

#### DoD
- `cargo pgrx test pg17 operators_match_kernels casts_array_roundtrip binary_compat_with_pgvector` GREEN.
- `git commit` `feat(dtype): T3.1 operadores + casts + byte-compat pgvector`.

## Phase 4 — Gate de paridade completo

### T4.1 — Suíte de paridade espelho do corpus pgvector

#### TDD
- RED: `#[pg_test] fn parity_vector_type_corpus` — espelha `pgvector/test/sql/vector_type.sql`: I/O feliz, NaN/Inf, out-of-range (`4e38`), syntax negativo (`must start with "["`, junk, `[1,,3]`, `[1, ,3]`, `''`), dim0, typmod erros. Cada caso assere output/erro idêntico ao pgvector.
- RED: `#[pg_test] fn parity_cast_corpus` — espelha `cast.sql`: int4[]/float4[]/float8[]/numeric[]→theodb.vector, theodb.vector→real[].
- RED: `#[pg_test] fn parity_copy_binary` — espelha `copy.sql`: round-trip binário.
- GREEN: nenhum código novo de produção (as Fases 1-3 já implementaram); esta fase ADICIONA os testes de paridade que exercitam o corpus completo. Se um caso falhar, corrigir o parse/format da Fase 1-2 (loop de paridade).

#### Files to edit
- `theodb_rs/src/dtype.rs` (`mod tests`) — os testes de paridade.

#### Deep file dependency analysis
Só testes; exercita as funções das Fases 1-3. Referência: `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql`, `cast.sql`, `copy.sql` (os casos + `expected/*.out` para as mensagens exatas).

#### Why this step
**Ação:** provar paridade byte-a-byte com o pgvector via o corpus de teste dele. **Raciocínio:** o Goal (métrica) é "suíte de paridade 100% GREEN"; esta fase É a métrica. É o gate de correção do blueprint (Corner 3) — sem ele, "byte-idêntico" é opinião, não fato. Cita blueprint Corner 3.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- Todos os casos do corpus `vector_type.sql`/`cast.sql`/`copy.sql` passam com output/erro idêntico ao pgvector.
- Cobertura: cada categoria da tabela do blueprint Corner 3 tem ≥ 1 teste.

#### DoD
- `cargo pgrx test pg17 parity_` GREEN (todos os testes de paridade).
- `git commit` `test(dtype): T4.1 gate de paridade pgvector completo`.

## Phase 5 — Integration Validation

### T5.1 — Validação end-to-end em pg17 real + wiring + ADR + CHANGELOG

#### TDD
- A suíte `dtype` COMPLETA (`cargo pgrx test pg17 dtype` ou o módulo) 100% GREEN em pg17 real com **pgvector coexistindo** (ambos os tipos instalados).
- Re-rodar os 55 pg_tests do AM (`hnsw_page`) — **devem continuar GREEN** (o M69 não tocou o AM; prova zero-regressão).

#### Files to edit
- `docs/adr/00NN-m69-own-vector-type.md` (NEW) — supersede ADR-0006 (D6); registra D1/D2/D3/D5.
- `CHANGELOG.md` — `[Unreleased] § Added`.
- `theodb_rs/src/lib.rs` — confirmar `mod dtype;` wired.

#### Deep file dependency analysis
O ADR referencia o blueprint + spike. O CHANGELOG é o contrato público. Nenhuma mudança de código de produção nesta fase (só doc + a validação).

#### Why this step
**Ação:** provar o milestone inteiro em ambiente real + fechar a rastreabilidade (ADR/CHANGELOG). **Raciocínio:** "eat your own cooking" — a paridade + a não-regressão do AM juntas provam que o tipo próprio coexiste corretamente sem tocar o hot path. Cita `cycle-plan.md` (Integration Validation mandatória).

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- Suíte `dtype` 100% GREEN + 55 pg_tests do AM GREEN, em pg17 real com pgvector coexistindo.
- ADR novo supersede ADR-0006; CHANGELOG atualizado (Regra 6).
- `/code-quality` verdict ∈ {PASS, PASS_WITH_CAVEATS}.

#### DoD
- Droplet: `cargo pgrx test pg17` (suíte dtype + AM) GREEN.
- `git commit` `feat(dtype): T5.1 integration validation + ADR + CHANGELOG`.

## Coverage Matrix

| Requisito (escopo M69) | Task(s) | Status |
|---|---|---|
| (1) tipo `#[repr(C)]` byte-idêntico + datum plumbing + CREATE TYPE | T1.1 | Covered |
| (2) I/O completo: in/out + typmod + length-coercion cast + recv/send binário | T1.1, T2.1 | Covered |
| (3) operadores `<->`/`<#>`/`<=>` + support functions | T3.1 | Covered |
| (4) casts real[]/float8[]/text + cast bidirecional com pgvector | T3.1 | Covered |
| (5) naming/coexistência sem colisão (`theodb.vector`) | T1.1 (D5) | Covered |
| (6) gate de paridade byte-a-byte (corpus pgvector) | T4.1, T5.1 | Covered |

**Coverage: 6/6 requisitos mapeados (100%)**

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Divergência sutil de parse/format vs pgvector (ex. shortest-decimal do float, subnormais) faz um caso de paridade falhar | MÉDIA | O gate de paridade (T4.1) espelha o corpus exato do pgvector (`expected/*.out`); qualquer divergência falha um teste ANTES do merge. `f32::to_string()` do Rust pode diferir do `float_to_shortest_decimal` do pg — se divergir, usar o formato do pg (ryu/grisu) ou ajustar. | impl |
| `dtype.rs` excede 500 LoC (tipo + I/O + operadores + casts + testes) | BAIXA | Split de operadores/casts em `dtype_ops.rs` (registrado na Fase 3); testes co-locados não contam no budget de produção. | impl |
| O cast binário com pgvector exige o `public.vector` presente — se a stack de teste não tiver pgvector, os testes de byte-compat falham no setup | MÉDIA | A stack de teste do droplet já instala pgvector (apt); documentar a dependência de teste. É coexistência por design (M69). | impl |
| Regressão acidental no AM (se `dtype.rs` tocar `vec.rs` ou o schema) | ALTA→BAIXA | O M69 NÃO altera `vec.rs` (só chama) nem o AM; a Fase 5 re-roda os 55 pg_tests do AM como gate de não-regressão. | impl |

## Unresolved Questions

- **Formato do float no `_out`:** `f32::to_string()` (Rust) vs `float_to_shortest_decimal_bufn` (pgvector) podem diferir em casos de borda (ex. `0.1`). Resolvido em T4.1 se um caso de paridade falhar (fallback: replicar o algoritmo do pg). Não bloqueia o design; é um detalhe de implementação que o gate de paridade cobre.
- Fora isso: (none — as demais decisões estão resolvidas pelo blueprint + spike).

## Failure scenarios

(none — no external I/O touched). O tipo `theodb.vector` é puramente in-memory (manipulação de varlena); não toca HTTP/DB-driver/queue/socket. Todo I/O é o de tipo do Postgres (in/out/recv/send), coberto pelos testes de paridade.

## Global Definition of Done

- [ ] Todas as tasks T1.1–T5.1 `committed`.
- [ ] Suíte de paridade `dtype` 100% GREEN em pg17 real (a métrica do Goal).
- [ ] 55 pg_tests do AM GREEN (não-regressão — M69 não tocou o hot path).
- [ ] Cast binário `WITHOUT FUNCTION` pgvector↔theodb.vector nos 2 sentidos (layout byte-idêntico provado).
- [ ] `dtype.rs` (+ `dtype_ops.rs` se split) ≤ 500 LoC produção cada; lint clean.
- [ ] Código ORIGINAL: `grep -ri "AGPL\|tensorchord\|vectorchord" theodb_rs/src/dtype*.rs` retorna 0 linhas (licença D1); ADR novo em `docs/adr/` supersede ADR-0006 (verificável: arquivo existe + cita 0006).
- [ ] CHANGELOG `[Unreleased] § Added` contém 1 entry M69 (verificável: `grep "M69" CHANGELOG.md` retorna ≥1) — Regra 6.
- [ ] `/code-quality` emite verdict ∈ {PASS, PASS_WITH_CAVEATS} (verificável: JSON `verdict` field).
- [ ] `git diff --stat` do PR NÃO lista `theodb_rs/src/am/` nem `theodb_rs/src/vec.rs` como modificados (prova que M69 não tocou o AM/kernels — isso é M70).

## Final Phase: Integration Validation

A Fase 5 (T5.1) É a integration validation: a suíte completa em pg17 real (dtype + AM) com pgvector coexistindo. O plano NÃO está completo até: (a) paridade 100% GREEN, (b) AM 55 pg_tests GREEN, (c) cast byte-compat GREEN, (d) `/code-quality` PASS. Se qualquer um falhar, o plano falhou.
