---
slug: pg18-migration
milestone_id: M135
date: 2026-07-21
verdict_input_for: /to-plan
---

# Blueprint — migração PG17 → PG18 de extensão pgrx com Table AM + Index AM

Fontes primárias: headers e backend do **PG18.4** e **PG17.10** reais (droplet), clones locais de
`pgvector` / `pgvectorscale` / `citus`, código do `pgrx 0.19.0`, e commits upstream (R0 satisfeito).

## Coverage Corner 1 — Integration tests

**Como o campo testa entre majors.** pgvector roda matriz de **7 majors** (13→19) com `-Werror`, duas camadas:
`pg_regress` (14 arquivos SQL) + TAP (`test/t/*.pl`) onde vivem WAL/replicação e recall sob pressão de memória;
valgrind só no major mais novo. pgvectorscale roda **20 combinações** (5 majors × 2 arqs × 2 modos de build) com
`cargo clippy` + `cargo test` por feature, mais um teste dedicado de upgrade. Citus é o mais profundo: 18 alvos de
make por major incluindo `check-isolation`, `check-columnar`, `check-columnar-isolation`, `check-vanilla`
(suíte do próprio Postgres), matriz de `pg_upgrade` entre majors, e um gate meta que detecta **teste não
registrado em nenhum schedule**.

**Lacuna do campo que é NOSSA vantagem:** nenhum dos três roda suíte de **crash-recovery** por major. Nosso
`theodb_rs/isolation/` + harnesses de crash (M99) são cobertura que o campo não oferece como prior art — logo o
port precisa mantê-los rodando, porque ninguém mais os tem para copiar.

**Teste de página lossy — o único template do campo é o upstream** (`src/test/regress/sql/bitmapops.sql`):
linhas largas (texto de 107 bytes → 55 tuplas/página) + módulos co-primos (53, 59 vs 55) para garantir **mistura**
de páginas lossy e exatas + `enable_indexscan=off; enable_seqscan=off` + `work_mem = 64` (kB, o mínimo), e o
oráculo é `count(*)` idêntico. `select_parallel.sql:213` cobre o caminho compartilhado (DSA). Nem pgvector, nem
pgvectorscale, nem citus têm teste de lossy — citus porque **desligou bitmap no colunar** (ver Corner 4).

## Coverage Corner 2 — Dependencies

**Política de versões declarada:**

| Projeto | Janela | Mecanismo | Onde está declarado |
|---|---|---|---|
| pgvector | 7 majors (13→19) | `#if PG_VERSION_NUM >= N` inline | `README.md:22`; drops são eventos de CHANGELOG |
| pgvectorscale | 5 majors (14→18) | `#[cfg(feature = "pgNN")]` por site | `Cargo.toml [features]`, `default = ["pg18"]` |
| citus | 3 majors (16,17,18) | header central `pg_version_compat.h` | `configure.ac:83-84` — build **recusa** fora da janela |

**Consequência direta da decisão do owner (migrar só para o 18):** não precisamos de NENHUM `#[cfg]` de versão.
O "imposto pgrx" que pgvectorscale paga — literal de struct exaustivo obriga duplicar o bloco inteiro em
`relopt_parse_elt` (`access_method/options.rs:113/121`) — **não se aplica a nós**. Essa é a maior simplificação
que a decisão comprou.

**pgrx 0.19 `pg18` garante** bindings que casam com a ABI do PG18 e alguns wrappers seguros `#[cfg]`-gated;
**não** garante estabilidade de fonte de `pg_sys::*` entre majors nem cobre callbacks removidos/re-assinados.

## Coverage Corner 3 — Tools

`cargo pgrx init --pg18 download` compila o PG18.4 do source e dá headers autoritativos para diff contra o 17.10 —
foi assim que cada assinatura deste blueprint foi verificada. **Nenhuma ferramenta detecta drift de API antes do
compilador**; o `cargo check --features pgNN` É a ferramenta. Corolário registrado: **as release notes do PG18 NÃO
documentam nenhuma das duas quebras** que nos atingem (só a linha sobre `pg_attribute.attcacheoff`) — release
notes não são canal de aviso para autor de extensão; commits são.

## Coverage Corner 4 — Techniques

### T1 — `TupleDescData.attrs` → `compact_attrs` (9 erros)

PG18 mantém **os dois arrays**: `compact_attrs` (16 B/coluna) primeiro, `FormData_pg_attribute` (104 B) depois.
`CompactAttribute` tem 9 campos e **não contém `attname`, `atttypid` nem `atttypmod`**.

**Armadilha crítica — corrupção silenciosa, não erro de compilação:** renomear `attrs` → `compact_attrs` COMPILA
(ambos são `__IncompleteArrayField`) e passa a ler `attname`/`atttypid` em offsets de uma struct de 104 B sobre um
array de 16 B. Leitura fora de limites, sem diagnóstico. **Todos os nossos 8 sites leem campos ausentes do
compact** — nenhum pode usar `compact_attrs`.

**Resposta Regra 9:** o pgrx já resolve — `PgTupleDesc::get(i) -> Option<&FormData_pg_attribute>`
(`pgrx-0.19.0/src/tupdesc.rs:226,285-313`) tem duas implementações `#[cfg]`-gated e compila igual de PG13 a PG19,
com bounds-check. Melhor que `pg_sys::TupleDescAttr` cru, que **só existe nos bindings de pg18/pg19**.
`populate_compact_attribute` é irrelevante para nós: só é necessário quando se MUTA um TupleDesc; somos read-only.

Upstream: commit `5983a4cffc31640fda6643f10146a5b72b203eaa` (David Rowley, 2024-12-20). Ganho de ~10-25% TPS citado
é **medido por terceiros**, nunca nosso.

### T2 — Rework do bitmap scan (7 erros)

| | PG17 | PG18 |
|---|---|---|
| begin | `TBMIterator *tbm_begin_iterate(tbm)` | `TBMIterator tbm_begin_iterate(tbm, dsa, dsp)` — **por valor**, 3 args |
| iterate | `TBMIterateResult *tbm_iterate(it)` (NULL = fim) | `bool tbm_iterate(it, *out)` — out-param |
| lossy | `ntuples < 0` | **`bool lossy`** — o sentinel sumiu |
| offsets | inline no result | `tbm_extract_page_tuple(res, buf, max)` |
| TableAM | `scan_bitmap_next_block` + `next_tuple` | **`next_block` REMOVIDO**; `next_tuple(scan, slot, *recheck, *lossy_pages, *exact_pages)` |

`dsp` é o discriminador: `DsaPointerIsValid(dsp)` → iterador compartilhado; senão privado. `TBMIterator` agora é
struct pública alocável na pilha, mas o iterador interno ainda é palloc'd → `tbm_end_iterate` continua obrigatório,
**exatamente uma vez** (chamar duas vezes dispara `Assert`).

**Duas armadilhas medidas:** (1) `tbm_extract_page_tuple` retorna a contagem TOTAL da página mesmo quando excede
`max_offsets` — sem clamp, lê memória não inicializada; (2) chamá-la num resultado **lossy** desreferencia
`internal_page == NULL` → segfault (o heap guarda com `if (!tbmres->lossy)`).

Upstream: `c3953226a075` ("Remove table AM callback scan_bitmap_next_block", Melanie Plageman, 2025-03-15),
`2b73a8cd33b7` (read stream API), `de380a62b5da`, `7bd7aa4d3067`.

### T3 — Política: portar o bitmap ou desligá-lo no colunar?

**Distinção que o /to-plan precisa fazer**, porque "portar de verdade" significa coisas diferentes nos dois sites:

- `customscan.rs::materialize_bitmap` é **código real em uso** (caminho de ANN filtrado). Porte de verdade,
  preservando o contrato admit-then-recheck. Oráculo: A/B de conjunto de resultados 17 vs 18 + caso lossy forçado.
- `columnar.rs` bitmap: são **stubs que dão erro**, nunca houve implementação (M99 é append-only). O PG18 remove o
  campo `scan_bitmap_next_block`, então some sozinho. **Citus deixa `NULL` de propósito**
  (`columnar_tableam.c:2527`) para o planner não gerar plano de bitmap sobre colunar
  (`columnar_customscan.c:435-443`). Registrar stub que erra é pior que NULL: diz ao planner que suportamos.

### T4 — O que o pgvector NÃO resolve (registro honesto)

pgvector é **Index AM apenas**, com `amgetbitmap = NULL` — os 13 guards `PG_VERSION_NUM >= 180000` dele são
**todos mecânicos** e **nenhum** toca bitmap ou TupleDesc. Não é prior art para nossos 8 erros semânticos.
O que vale copiar dele: `PG_MODULE_MAGIC_EXT`, o macro que sombreia `vacuum_delay_point()` para absorver a
aridade nova sem tocar 15 call sites, e — se algum dia vetarmos um path por custo infinito — `disabled_nodes = 2`
(medimos: **não temos** esse padrão, então não se aplica).

## ADRs

**ADR-1 — Usar `PgTupleDesc::get` em vez de FFI cru.** Alternativas: (a) `pg_sys::TupleDescAttr` cru — rejeitada,
só existe nos bindings pg18/19, quebraria qualquer volta ao 17; (b) aritmética de ponteiro à mão — rejeitada, é a
que a armadilha silenciosa pune, e o layout do `CompactAttribute` já está mudando de novo no master (8 bytes).
Escolhida: acessor do pgrx (Regra 9, um único idioma, bounds-checked).

**ADR-2 — Colunar: deixar callbacks genuinamente não suportados como `NULL`, não como stub que erra.** Alternativa
rejeitada: manter stub por "mensagem de erro mais clara" — o custo é o planner gerar planos que falham em runtime,
e (achado #143) o stub sem `#[pg_guard]` **derruba o servidor**. Precedente: citus.

**ADR-3 — Sem `#[cfg]` de versão.** Consequência da decisão do owner de migrar. Alternativa (dual 17+18) foi
avaliada e rejeitada por ele: sem base instalada, o custo de manter branching permanente não se paga.

## Referências

- `knowledge-base/references/pgvector/src/{hnsw.c,ivfflat.c,hnswscan.c,ivfscan.c,hnswvacuum.c,vector.c}`
- `knowledge-base/references/pgvectorscale/{Cargo.toml,src/access_method/{vacuum.rs,options.rs,cost_estimate.rs}}`
- `knowledge-base/references/citus/{configure.ac,src/include/pg_version_compat.h,src/backend/columnar/{columnar_tableam.c,columnar_customscan.c}}`
- PG18.4 headers: `access/tupdesc.h`, `nodes/tidbitmap.h`, `access/tableam.h`, `commands/vacuum.h`, `utils/lsyscache.h`
- PG18.4 backend: `access/heap/heapam_handler.c`, `nodes/tidbitmap.c`
- Upstream commits: `c3953226a075`, `2b73a8cd33b7`, `de380a62b5da`, `7bd7aa4d3067`, `5983a4cffc31`
- Upstream test template: `postgres/src/test/regress/sql/bitmapops.sql`
