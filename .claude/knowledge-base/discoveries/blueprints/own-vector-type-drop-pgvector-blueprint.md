# Blueprint: Tipo vetorial próprio — remover a dependência do pgvector totalmente

> **Version 1.0** — Blueprint técnico do `/discover-execute` para `own-vector-type-drop-pgvector`. Investiga como shipar um tipo `vector` own-code no theodb_rs para remover o pgvector (e o pgvectorscale, vestigial no runtime). Evidência: código de `pgvector`, `vectorchord`, `pgvectorscale`, `postgres` (clones read-only) + web (R0). Produz o veredito **A/B/C** e a decomposição em milestones.

**Slug:** `own-vector-type-drop-pgvector`
**Created:** 2026-07-09
**Plan:** `.claude/knowledge-base/discoveries/plans/own-vector-type-drop-pgvector-plan.md`

## Context

O usuário exige remover o pgvector **totalmente** (2026-07-09) — objetivo explícito do North Star (`theo-db/CLAUDE.md`: "substituir pgvector/pgvectorscale por código próprio é o **objetivo**") e alvo dos milestones v2 M20→M22, gated em paridade medida. Hoje **não existe tipo próprio**: `theodb_rs/src/api.rs:770` declara "no competing type; reads `vector::real[]`" (ADR-0006). O pgvector fornece o **tipo `vector`** (I/O, typmod, operadores `<->`/`<=>`/`<#>`, opclasses); o TheoDB o consome. Superfície: `theodb.control:3` (`requires = 'vector, vectorscale'`), ~44 `::vector` no `theodb_rs/src`, opclasses em `theodb_rs/src/am/mod.rs:253+`. O pgvectorscale é **vestigial no runtime** (só benchmarks usam `USING diskann`).

## Objective

Decidir **se e como** shipar um tipo `vector` próprio para remover o pgvector — com recomendação fundamentada **A** (drop-in `vector`) vs **B** (`theodb.vector`) vs **C** (manter reuso, remover só vectorscale), o padrão técnico, o gate de correção, a migração, e a decomposição em milestones.

## Coverage Corner 1 — Integration Tests

**Q5 — shape dos testes end-to-end do tipo + AM, e a decomposição em milestones.**

O pgvector separa dois níveis de teste (regression SQL, expected-diff):
- **Round-trip do tipo** (isolado do índice): `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:1-44` (parse/output, typmod, overflow/NaN/Inf, malformados) + `.claude/knowledge-base/references/pgvector/test/sql/cast.sql` (malha de casts `array↔vector↔real[]`).
- **Index scan sobre o tipo via AM** (o e2e canônico): `.claude/knowledge-base/references/pgvector/test/sql/hnsw_vector.sql` — `SET enable_seqscan=off` (`:1`) → `CREATE TABLE t (val vector(3))` (`:5`) → `CREATE INDEX ON t USING hnsw (val vector_l2_ops)` (`:7`) → `SELECT * FROM t ORDER BY val <-> '[3,3,3]'` (`:11`); repetido por métrica (`vector_ip_ops`+`<#>` `:24`, `vector_cosine_ops`+`<=>` `:37`). Um arquivo por **(tipo × AM)** — a matriz é explícita.

O pgvectorscale confirma o padrão do binding: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/mod.rs:100-101,209-231` declara `DEFAULT FOR TYPE vector USING diskann … OPERATOR 1 <=> (vector, vector)` — opclasses **contra o tipo `vector` do pgvector**, não um tipo próprio.

O TheoDB hoje: `theodb_rs/src/am/hnsw_page.rs` tem 55 `#[pgrx::pg_test]` com shape idêntico porém com **oracle exato in-process** (não expected-diff): `CREATE TEMP TABLE rn (id int PRIMARY KEY, e vector(4))` (`:2190`) → `CREATE INDEX rn_idx ON rn USING theodb_hnsw (e)` (`:2196`) → `SET theodb_hnsw.ef_search=200` (`:2197`) → top-k via índice vs seqscan, **assert set-equal** (`:2201-2214`). **Este set-equal-vs-seqscan É o gate executável de não-regressão de recall** — não precisa ser inventado, só re-parametrizado para o tipo próprio.

**Decomposição recomendada: 2 milestones com gate de paridade entre eles** (ver ADR-D2). Rationale: risco isolado (M-A não toca o hot path do índice — o P0 do North Star), rollback barato via cast de coexistência, e o gate set-equal já existe.

## Coverage Corner 2 — Dependencies

**Q6 — o que zerar + migração de tabelas existentes.**

**Precedente de ALTER-de-layout no pgvector: NÃO existe.** Das 38 migrations em `.claude/knowledge-base/references/pgvector/sql/` (`vector--*.sql`), os únicos `ALTER TYPE vector` são não-layout: `SET (RECEIVE/SEND)` (`.claude/knowledge-base/references/pgvector/sql/vector--0.1.0--0.1.1.sql:10`), `SET (STORAGE=extended)` (`.claude/knowledge-base/references/pgvector/sql/vector--0.3.2--0.4.0.sql:5`), `SET (STORAGE=external)` (`.claude/knowledge-base/references/pgvector/sql/vector--0.5.1--0.6.0.sql:5`). O layout on-disk (`vl_len_ i32 · dim i16 · unused i16 · f32[dim]`, `.claude/knowledge-base/references/pgvector/sql/vector.sql:23-30`) é **estável desde a origem** — nenhuma versão o mudou.

**Migração de tabela existente = binary-coercible cast, SEM reescrita de heap** (se o tipo próprio for `#[repr(C)]` byte-idêntico ao pgvector):
- `CREATE CAST (vector AS theodb_vector) WITHOUT FUNCTION` declara binary-coercibility ([CREATE CAST](https://www.postgresql.org/docs/current/sql-createcast.html), requer superuser).
- `ALTER TABLE … ALTER COLUMN emb TYPE theodb_vector` **não reescreve o heap** quando o tipo é binary-coercible ([ALTER TABLE](https://www.postgresql.org/docs/current/sql-altertable.html)).
- **Caveat honesto (índices):** pular o heap-rewrite NÃO pula o rebuild de índices — os índices só são preservados se as operator families antiga/nova coincidem; senão, REINDEX. Para ANN: a migração de coluna é grátis, mas os índices HNSW/IVFFlat precisam de rebuild salvo se o TheoDB mapear as mesmas opfamilies.

**O que zerar:** `theodb.control:3` (`requires = 'vector, vectorscale'` → vazio/só o próprio tipo); `theodb_rs.control` (dropa `vector`); Dockerfile — remover o **stage 1 inteiro** do pgvectorscale (`Dockerfile:10-26`), o `ADD pgvector.git + make install` (`Dockerfile:70-81`), os `COPY --from=scale-builder vectorscale*` (`Dockerfile:88-89`) e os CASCADE (`Dockerfile:124,137,140,145`). **NÃO** mexer em pg_duckdb (`Dockerfile:64,147-149` — columnar MIT, independente). diskann vira **benchmark-only** (instalação local só quando o bench de comparação roda).

## Coverage Corner 3 — Tools

**Q4 — gate de paridade/correção (wire-compat 100%).** O pgvector prova paridade via regress tests (par `test/sql/*.sql` → `test/expected/*.out`, diff exato de stdout, [pg_regress](https://www.postgresql.org/docs/current/regress-run.html)). O corpus que o tipo próprio DEVE replicar (extraído com `path:linha` + strings de erro confirmadas nos `.out`):

| Categoria | Assere | Citação |
|---|---|---|
| I/O parse feliz | `[1,2,3]`, negativos, trailing dot, whitespace, precisão round-trip (shortest-decimal) | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:1-5` |
| NaN/Inf reject | "NaN not allowed"; "infinite value not allowed" | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:7-9` → `.claude/knowledge-base/references/pgvector/test/expected/vector_type.out:36,40,44` |
| Out-of-range float4 | `[4e38,1]`→"is out of range for type vector"; subnormais viram 0 (não erram) | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:13-16` → `.claude/knowledge-base/references/pgvector/test/expected/vector_type.out:66,70` |
| Syntax negativo | "must start with \"[\"", "Junk after closing right brace", `[1,,3]`, `[1, ,3]`, `''`, `[` | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:17-30` → `.claude/knowledge-base/references/pgvector/test/expected/vector_type.out:93,98,103` |
| dim=0 reject | `[]`,`[ ]`,`[,]`→"vector must have at least 1 dimension" | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:24-26` → `.claude/knowledge-base/references/pgvector/test/expected/vector_type.out:117,121` |
| typmod | `::vector(2)`→"expected 2 dimensions, not 3"; `::vector(3,2)`→"invalid type modifier"; `::vector(16001)`→"cannot exceed 16000" | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:32-37` → `.claude/knowledge-base/references/pgvector/test/expected/vector_type.out:151` |
| dim-mismatch (ops) | "different vector dimensions 2 and 1" p/ `+ - * <-> <#> <=> <+>` | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:45,49,54,91,97,107,117` |
| operadores | `<->` L2, `<#>` neg-inner, `<=>` cosine (incl. `[0,0]`→NaN, clamp [-1,1]) | `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:89-121` |
| **round-trip BINÁRIO** (o mais crítico) | `COPY t TO/FROM … FORMAT binary` com `vector(3)` + linha NULL — exercita send/recv byte-idêntico (campo `unused`==0 no wire) | `.claude/knowledge-base/references/pgvector/test/sql/copy.sql:1-14` |
| casts | int4[]/float4[]/float8[]/numeric[]→vector, vector→real[] | `.claude/knowledge-base/references/pgvector/test/sql/cast.sql` |

## Coverage Corner 4 — Techniques

**Q1 — o tipo `vector` do pgvector (o contrato a espelhar own-code).** Layout do struct (`.claude/knowledge-base/references/pgvector/src/vector.h:11-17`): `int32 vl_len_` (varlena header) + `int16 dim` + `int16 unused` (SEMPRE 0) + `float x[]` → **8 + 4·dim bytes on-disk**. I/O (`.claude/knowledge-base/references/pgvector/src/vector.c`): `vector_in` (parse `[..]`, `strtof` por elemento, `:172-275`), `vector_out` (shortest-decimal, `:285-320`), `vector_recv`/`vector_send` (wire binário: `int16 dim` + `int16 unused` + `dim`×`float4` big-endian; **`unused` VIAJA no wire e é validado ==0 no recv** `:369-397`), `vector_typmod_in` (dim 1..16000, `:337-363`). Validações: `CheckElement` (NaN/Inf reject, `:105-117`), `CheckDim` (1..16000, `:88-100`), `CheckExpectedDim` (typmod, `:76-83`). **Confirmado web:** README pgvector — `[1,2,3]`, 16000 dims, `4*dim+8` bytes, "all elements must be finite" (https://github.com/pgvector/pgvector).

**Q2 — operadores + opclasses + support procs (C e pgrx).** Esqueleto DDL C (`.claude/knowledge-base/references/pgvector/sql/vector.sql`): `CREATE TYPE vector` (shell `:6` → I/O funcs `:8-21` → tipo completo `:23-30`), distance funcs (`:34-43`), `CREATE OPERATOR <-> <#> <=>` (`:174-187`, retornam float8 = ordering operators), `CREATE OPERATOR CLASS vector_l2_ops FOR TYPE vector USING hnsw AS OPERATOR 1 <-> FOR ORDER BY float_ops, FUNCTION 1 <dist>` (`:313-327`). O core textbook: `.claude/knowledge-base/references/postgres/contrib/cube/cube--1.2.sql:358-378` (mesmo esqueleto TYPE+operadores+opclass, `FOR ORDER BY float_ops`). Regra do core: um ordering operator retorna float e **exige nomear uma opfamily btree** (`FOR ORDER BY float_ops`); `CREATE OPERATOR CLASS` **não valida completude** — é do autor ([xindex](https://www.postgresql.org/docs/current/xindex.html), [CREATE OPERATOR CLASS](https://www.postgresql.org/docs/current/sql-createopclass.html)).

Padrão **pgrx 0.16.1** (web-sourced — nenhum clone define tipo em pgrx):
- **Definir o tipo:** `#[derive(PostgresType)]` + `#[pgvarlena_inoutfuncs]` + `impl PgVarlenaInOutFuncs` (`PgVarlena<T>`) para varlena binário, OU `#[inoutfuncs]`/serde ([pgrx custom_types](https://github.com/pgcentralfoundation/pgrx/blob/develop/pgrx-examples/custom_types/README.md), [docs.rs/pgrx inoutfuncs](https://docs.rs/pgrx/latest/pgrx/inoutfuncs/index.html)). **GAP HONESTO:** um `vector` denso de **dimensão dinâmica** (flexible array member) NÃO é expressável como `#[derive(Copy,Clone)]` de tamanho fixo → a rota provável é `extension_sql!(CREATE TYPE …)` manual + funcs I/O `#[pg_extern]` (padrão-C do pgvector portado). **UNKNOWN — exige spike de validação.**
- **Operadores:** `#[pg_operator] #[opname(<=>)] #[commutator(<=>)]` → `cargo pgrx schema` gera o `CREATE OPERATOR` ([pgrx-examples/operators](https://github.com/cloudberry-contrib/pgrx/tree/main/pgrx-examples/operators)).
- **Opclass de AM:** pgrx não tem macro → `extension_sql!` cru `CREATE OPERATOR CLASS … FOR TYPE … USING <am> … FUNCTION 1 …` com `requires=[amhandler,…]`.
- **PRESERVAR o M49 metric-from-opclass:** `theodb_rs/src/am/mod.rs:251-292` (opclasses L2 default sem proc + cosine/ip com `FUNCTION 1 theodb_metric_{cosine,ip}()`) + `theodb_rs/src/am/build.rs:67-74` (`resolve_metric` via `index_getprocid(indexrel,1,1)` → fallback L2). O consumo do tipo pelo AM (a espelhar): `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/pg_vector.rs:19-21,162` (`from_datum` + `to_slice() -> &[f32]`).

**Q3 — reuse vs own (a evidência que ancora o veredito A/B/C).** **AMBOS os AMs próprios SOTA REUSAM o tipo `vector` do pgvector; nenhum reimplementa:**
- **pgvectorscale:** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/pg_vector.rs:8-16` — `//Ported from pg_vector code` + `#[repr(C)] struct PgVectorInternal { vl_len_: i32, dim: i16, unused: i16, x: … c_float }` (espelho de leitura, não `CREATE TYPE`). Web: "a complement to pgvector rather than a replacement… reuses pgvector's foundational vector type" ([dbvis](https://www.dbvis.com/thetable/pgvectorscale-an-extension-for-improved-vector-search-in-postgres/), [DeepWiki](https://deepwiki.com/timescale/pgvectorscale/1.1-installation-and-setup)).
- **VectorChord:** `.claude/knowledge-base/references/vectorchord/vchord.control:6` (`requires = 'vector'`); `.claude/knowledge-base/references/vectorchord/src/datatype/memory_vector.rs:20-29` importa `use vector::VectorBorrowed` (crate do pgvector) e o `SqlTranslatable` mapeia para o literal SQL `"vector"` (`memory_vector.rs:216-232`, `SqlMapping::As("vector")`) — glue Rust↔Datum sobre o tipo do pgvector, SEM `CREATE TYPE vector` próprio. Os tipos `rabitq4`/`rabitq8` são **codec novo** (não competem com `vector`), populados via `quantize_to_rabitq8('...'::vector)`.
- **Rationale (web):** interoperabilidade — tabelas de usuário guardam `VECTOR`; reimplementar quebraria compat binária com dados/tooling ([VectorChord overview](https://docs.vectorchord.ai/vectorchord/getting-started/overview.html)).
- **Colisão de nome `vector` é REAL:** dois extensions com `CREATE TYPE vector` → `ERROR: type "vector" already exists` (histórico pgvector vs pgvecto.rs, que teve de usar `vectors`; [Immich #7318](https://github.com/immich-app/immich/issues/7318)).

## Cross-cutting Comparison

| Projeto | Define tipo `vector` próprio? | Como consome o tipo | `requires` | Evidência |
|---|---|---|---|---|
| pgvector | **SIM** (é a origem) | — | — | `.claude/knowledge-base/references/pgvector/src/vector.c`, `sql/vector.sql:23-30` |
| pgvectorscale | **NÃO — reusa** | `#[repr(C)]` mirror + `from_datum` | `vector` | `pg_vector.rs:8-16` |
| VectorChord | **NÃO — reusa** (+ codec rabitq novo) | `use vector::VectorBorrowed`, `SqlMapping::As("vector")` | `vector` | `vchord.control:6`, `memory_vector.rs:216-232` |
| **TheoDB (hoje)** | **NÃO — reusa** | lê `::real[]` + opclass `FOR TYPE vector` | `vector, vectorscale` | `api.rs:770`, `am/mod.rs:253` |
| **TheoDB (proposto)** | **SIM** (território novo) | own I/O + `#[repr(C)]` byte-idêntico | (vazio) | este blueprint |

**O sinal central:** TheoDB seria o **primeiro** AM permissivo a shipar tipo `vector` próprio em pgrx. É complexidade **essencial** (North Star), mas sem prior-art pgrx para a definição-de-tipo denso-variável (o GAP da Q2).

## ADRs

### D1 — Veredito de nome/compat: **A (tipo próprio nomeado `vector`, drop-in) com `#[repr(C)]` byte-idêntico ao pgvector**, com C como fallback faseado

**Decisão:** shipar o tipo próprio **nomeado `vector`** (não `theodb.vector`), com layout `#[repr(C)]` **byte-idêntico** ao `Vector` do pgvector (`vl_len_ i32 · dim i16 · unused i16 · f32[]`). Isso mantém todo `'[..]'::vector` e `vector(N)` do usuário inalterados (`requires=''`), e torna a migração de tabelas existentes um **cast binário grátis** (Corner 2).

**Rationale (evidência):**
- **Drop-in** preserva a suíte de 55 pg_tests + os ~44 `::vector` + tabelas de usuário sem reescrita de dados (Q6: cast binary-coercible sem heap-rewrite se byte-idêntico).
- A **colisão de nome** (Q3, Immich #7318) só ocorre se pgvector estiver co-instalado — e o objetivo é **não** instalá-lo. Fail-fast claro (`type "vector" already exists`) se alguém tentar; documentar.
- **B (`theodb.vector`)** rejeitado: quebra todo `::vector`, exige migração de nome em tabelas de usuário + reescrita maior, sem ganho real (o namespace só importa sob co-instalação, que não queremos).
- **C (manter reuso, remover só vectorscale)** — a via de menor custo, e é o que TODO o SOTA faz (Q3). É a alternativa honesta se o custo de A se provar alto: **remover o vectorscale é barato e imediato** (vestigial no runtime), e entrega parte do pedido ("remover pgvectorscale") já. **Recomendação faseada:** executar **C imediatamente** (remover vectorscale do produto) e **A em seguida** (tipo próprio), com o gate de paridade. Assim o "remover totalmente" acontece por etapas medidas, sem sunk-cost.

**Consequências:** supersede o ADR-0006 (reuso do tipo). Risco central: o GAP pgrx (tipo denso-variável) — mitigado por um **spike** antes do commit (ADR-D3). O `unused` deve ser emitido/validado ==0 no wire para round-trip binário idêntico (Q1).

### D2 — Decomposição em **2 milestones** com gate de paridade

**Decisão:** **M-A** = tipo próprio `vector` (I/O + typmod + operadores `<->`/`<#>`/`<=>` + casts `↔ real[]`/text) **coexistindo** com pgvector, gated por paridade byte-a-byte (o corpus da Corner 3, sem tocar o AM). **M-B** = trocar opclasses para `FOR TYPE` do tipo próprio + reescrever `::vector` + trocar `requires` + migração de colunas + remover pgvector/pgvectorscale; gate = re-rodar os 55 pg_tests set-equal-vs-seqscan.

**Rationale:** risco isolado (M-A não toca o hot path do índice = o P0 do North Star); rollback barato (coexistência via cast mantém pgvector como oráculo vivo durante M-B); o gate executável já existe (`hnsw_page.rs:2201-2214`); SRP no nível de milestone. Alternativa (1 milestone) rejeitada: concentra o pior risco (regressão silenciosa de recall) sem gate intermediário e torna qualquer diff ambíguo (tipo ou opclass?).

**Consequências:** dois release cuts; um par de casts a mais (essencial). Precede: **C** (remover vectorscale) pode ser um M-0 trivial antes de M-A, OU dobrado em M-B.

### D3 — Spike pgrx obrigatório antes do commit de M-A (o GAP de Q2)

**Decisão:** antes de M-A, um spike valida que pgrx 0.16.1 consegue definir um tipo `vector` denso de **dimensão dinâmica** (flexible array / varlena) via `extension_sql!(CREATE TYPE)` + funcs I/O `#[pg_extern]` com layout `#[repr(C)]` controlado — pois nenhum peer pgrx público o fez (Q2 UNKNOWN).

**Rationale:** honestidade (Regra 3) — o "como" da definição-de-tipo em pgrx é a única peça sem prior-art. Falha do spike → reavaliar A vs C. Alternativa (assumir que dá) rejeitada — seria construir sobre incerteza.

**Consequências:** o spike é o primeiro item de M-A; seu resultado é gate de continuação.

## Recommendations for the project

1. **Executar C imediatamente** (remover pgvectorscale do produto — vestigial no runtime, Corner 2) — entrega parte do "remover totalmente" já, sem risco. Implementa ADR-D1 (fase C).
2. **M-A: tipo `vector` próprio drop-in** (ADR-D1 opção A, ADR-D2 M-A) — precedido pelo **spike pgrx** (ADR-D3). Gate: paridade byte-a-byte com o corpus da Corner 3 (incl. round-trip binário `copy.sql`).
3. **M-B: opclasses sobre o tipo próprio + remover pgvector + migração** (ADR-D2 M-B). Gate: 55 pg_tests set-equal-vs-seqscan (não-regressão de recall). Migração via `CREATE CAST … WITHOUT FUNCTION` + `ALTER COLUMN TYPE` (cast binário grátis; REINDEX dos índices ANN).
4. **Supersede ADR-0006** (reuso do tipo pgvector) com um ADR novo que registre A + o `#[repr(C)]` byte-idêntico como pré-condição do cast grátis.
5. Mover diskann para **benchmark-only** (instalação local no bench de comparação, não no produto).

## Blocked questions (if any)

Nenhuma. As 6 questions respondidas com citação. **UNKNOWN honesto** (não-BLOCKED): o padrão pgrx de definição de tipo denso-variável não tem prior-art público — endereçado por ADR-D3 (spike), não fabricado.

## Halt-loop progress (audit trail)

| Q | Corner | Status | Método |
|---|---|---|---|
| Q1 | techniques | done | Read pgvector/src/vector.{c,h} + web README |
| Q2 | techniques | done (1 UNKNOWN → ADR-D3) | Read vector.sql/hnsw.c/cube + web pgrx/pg docs |
| Q3 | techniques | done | Read vchord.control/datatype + pg_vector.rs + web |
| Q4 | tools | done | Read pgvector/test/{sql,expected} + web |
| Q5 | tests | done | Read hnsw_vector.sql + hnsw_page.rs pg_tests |
| Q6 | deps | done | Read vector--*.sql + theodb.control/Dockerfile + web |

## Related

- Plan: `.claude/knowledge-base/discoveries/plans/own-vector-type-drop-pgvector-plan.md`
- Edge-cases: `.claude/knowledge-base/reviews/own-vector-type-drop-pgvector-edge-cases-2026-07-09.md`
- Supersede: `docs/adr/0006-*` (reuso do tipo pgvector)
- Downstream: `/discover-confidence own-vector-type-drop-pgvector` → `/roadmap-feature` (M69 fase C + M70/M71 A) → `/to-plan`
