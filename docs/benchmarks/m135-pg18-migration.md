# M135 — migração PostgreSQL 17 → 18 (medido)

> Medido em 2026-07-21 na droplet (165.227.121.20). PG18.4 instalado do source oficial via
> `cargo pgrx init --pg18 download`; instância dedicada em `/tmp/pg18data`, porta 28918, com
> `shared_preload_libraries=theodb_rs`. Baseline PG17.10 na porta 28900.
> Plano: `.claude/knowledge-base/plans/pg18-migration-plan.md`. Blueprint:
> `.claude/knowledge-base/discoveries/blueprints/pg18-migration-blueprint.md`.

## Headline

**Os 27 erros de compilação medidos na sondagem foram fechados; a extensão carrega e opera num PostgreSQL 18.4
real.** O porte foi guiado por evidência primária (headers do 18.4 + commits upstream + código de peers), não por
tentativa e erro — e produziu um achado colateral de severidade alta (#143) que existia no PG17 desde o M99.

| | Antes | Depois (medido) |
|---|---|---|
| `cargo check --features pg18` | **27 erros** | **0 erros** |
| `cargo build` sem flags | falhava (default era pg17) | exit 0, `default = ["pg18"]` |
| `CREATE EXTENSION` no PG18.4 | impossível | `extversion = 1.0.0` |
| `CREATE INDEX` sobre tabela colunar | **abortava o servidor** (signal 6) | `ERROR` tipado, sessão viva |
| Bitmap sobre colunar | planner gerava plano que falharia | `Seq Scan` — planner roteia ao redor |
| Features de versão declaradas | `pg13`–`pg17`, **nenhuma jamais compilada** | só `pg18`, compilada e provada |

## 1. Como os 27 erros se distribuíram (medido, não estimado)

| Classe | Erros | Natureza | Resolução |
|---|---|---|---|
| `TupleDescData.attrs` → `compact_attrs` | 9 | **armadilha silenciosa** | `PgTupleDesc::get` do pgrx (ADR-1) |
| `relopt_parse_elt.isset_offset` | 10 | mecânica | campo `0` nos 10 literais |
| Rework do bitmap scan | 7 | **semântica** | porte real do iterador + callbacks colunares a `NULL` |
| `vacuum_delay_point(bool)` | 1 | mecânica | `false` (estamos em VACUUM, não ANALYZE) |
| `get_ordering_op_properties` → `CompareType` | 1 | semântica | `COMPARE_LT`, não recast de `BTLessStrategyNumber` |

**Hipótese falsificada pela medição:** apostei que a dor viria do `GenericXLog` (54 referências) e dos Index AMs.
**Zero erros nos dois.** A API de WAL e a de índice atravessaram 17→18 intactas. Quebrou exatamente onde o PG18
mexeu de propósito — e registrar a intuição errada importa porque ela volta na próxima migração.

## 2. A armadilha que não é erro de compilação

O PG18 mantém **os dois** arrays no `TupleDesc`: `compact_attrs` (16 B/coluna) primeiro, `FormData_pg_attribute`
(104 B) depois. Renomear `attrs` → `compact_attrs` **compila** — ambos são `__IncompleteArrayField` — e passa a
ler `attname`/`atttypid`/`atttypmod` em offsets de uma struct de 104 B sobre um array de 16 B. Leitura fora de
limites, sem diagnóstico, nomes de coluna e OIDs de tipo virando lixo.

Nossos 8 sites leem exatamente esses três campos, que o `CompactAttribute` **não contém**. A resposta foi Regra 9:
`PgTupleDesc::get` (pgrx `tupdesc.rs:226,285-313`) já tem as duas implementações `#[cfg]`-gated e compila igual de
PG13 a PG19, com bounds-check. Verificamos antes de adotar que `from_pg_unchecked` tem `need_release: false` e
`need_pfree: false` — o `Drop` é no-op, seguro sobre um ponteiro emprestado.

Critério de aceite verificado: `grep -rn "\.attrs\.as_ptr()\|compact_attrs" theodb_rs/src/` → **0 ocorrências**.

## 3. O porte do bitmap (a parte que podia dar resultado errado em silêncio)

Quatro mudanças simultâneas, e só uma delas é erro de compilação:

| | PG17 | PG18 |
|---|---|---|
| begin | `TBMIterator *tbm_begin_iterate(tbm)` | `TBMIterator tbm_begin_iterate(tbm, dsa, dsp)` — por valor |
| iterate | `TBMIterateResult *` (NULL = fim) | `bool tbm_iterate(it, *out)` — out-param |
| lossy | `ntuples < 0` | **`bool lossy`** — o sentinel sumiu |
| offsets | inline no result | `tbm_extract_page_tuple(res, buf, max)` |

Duas armadilhas fechadas explicitamente: `tbm_extract_page_tuple` devolve a contagem **total** da página mesmo
excedendo o buffer (sem clamp, lê memória não inicializada), e chamá-la num resultado lossy desreferencia
`internal_page == NULL`. `MAX_TUPLES_PER_PAGE` foi **medido**, não assumido: compilamos
`char probe[MaxHeapTuplesPerPage]` contra os headers reais do 18.4 e lemos o tamanho do símbolo — `0x123` = **291**,
com `char blk[BLCKSZ]` = `0x2000` confirmando o bloco de 8 kB de que o valor depende. Um `const_assert` sobre
`BLCKSZ` trava essa dependência.

### Oráculo A/B (medido)

Receita do upstream `bitmapops.sql` (linhas de 107 B → ~55 tuplas/página, `cat = id%100` sobre 200 000 linhas,
`enable_seqscan=off`, `work_mem` alternando entre 64 MB e 64 kB), com o Custom Scan `theodb_vecfilter` forçado:

```
PG18   ids_exato  {7,107,207,307,407}     ids_lossy  {7,107,207,307,407}
PG17   ids_exato  {7,107,207,307,407}     ids_lossy  {7,107,207,307,407}
```

**Idêntico nos dois eixos**: entre regimes de memória (o contrato admit-then-recheck sobreviveu) e entre majors
(o porte não mudou comportamento). Um porte que tratasse lossy como exato perderia linhas; um sem o clamp leria
lixo — nenhum dos dois produz este resultado.

E o oráculo do próprio upstream, sobre heap puro:

```
count(*) com work_mem=64MB : 23        count(*) com work_mem=64kB : 23
```

## 4. Colunar: callbacks não suportados agora são `NULL` (ADR-2)

O PG18 removeu `scan_bitmap_next_block` do `TableAmRoutine`. Nossos dois callbacks eram **stubs que davam erro** —
o que dizia ao planner que suportamos bitmap, então ele podia planejar um e falhar em runtime. Citus chegou à
mesma conclusão para o colunar dele (`columnar_tableam.c:2527` deixa `NULL`, com a consequência de planner
documentada em `columnar_customscan.c:435-443`).

Medido no PG18 — o planner roteia ao redor em vez de gerar um plano que falharia:

```
EXPLAIN (COSTS OFF) SELECT count(*) FROM c18 WHERE a = 1 AND b = 1;
 Aggregate
   ->  Seq Scan on c18
         Disabled: true
         Filter: ((a = 1) AND (b = 1))
```

## 5. Achado colateral: #143 — crash de servidor que existia desde o M99

Investigando por que registrávamos stub em vez de `NULL`, descobrimos que **três comandos SQL derrubavam a
instância inteira**:

```sql
CREATE TABLE crashme (a int) USING theodb_columnar;
INSERT INTO crashme VALUES (1);
CREATE INDEX ON crashme(a);   -- servidor morre aqui
```

```
fatal runtime error: failed to initiate panic, error 5, aborting
LOG:  server process was terminated by signal 6: Aborted
DETAIL:  Failed process was running: CREATE INDEX i_cbm_a ON cbm(a);
```

Causa: o macro `columnar_unsupported!` gerava 30 callbacks `extern "C-unwind"` **sem `#[pg_guard]`**. Em pgrx 0.19
um `ERROR` do PG é levantado como `panic_any`, então frames Rust desenrolam antes do `ereport` — mas esses stubs
são chamados **direto pelo C do PostgreSQL**, sem frame de guarda, e o unwinder saía da pilha
(`_URC_END_OF_STACK`, o "error 5"). O próprio cabeçalho do arquivo (linha 17) já declarava a regra que o macro era
o único lugar a violar.

`#[pg_guard]` não resolve aqui: é um proc macro que re-emite uma chamada usando os nomes de parâmetro que parseou,
e esses nomes vêm dos fragmentos `$arg` do `macro_rules!` — a higiene não sobrevive e **não compila**
(`cannot find value _s in this scope`). Aplicamos `pgrx_extern_c_guard` diretamente, que é no que o atributo
expande de qualquer forma (pgrx `lib.rs:126` o reexporta). Nenhum valor com destrutor está vivo no frame do stub,
então o longjmp do ereport ao sair da guarda não pode pular um `Drop`.

Verificado no `.so` publicado, atrás do gate anti-restart-silencioso:

```
ERROR:  XX000: theodb_columnar: index build over columnar ... is not supported
 sessao_viva | 1
 count       | 1        (dados intactos)
 crashes antes=4  depois=4
```

## 6. Funcional no PG18.4 (medido)

```
PostgreSQL 18.4 on x86_64-pc-linux-gnu, compiled by gcc 13.3.0, 64-bit
CREATE EXTENSION theodb_rs  →  extversion 1.0.0

AM vetorial   : CREATE INDEX theodb_hnsw sobre 2 000 vetores; top-10 retornado
colunar       : 70 000 linhas; count/sum/max corretos (70000 / 1819780 / 58)
#143          : ERROR tipado, sessão sobrevive
bitmap colunar: planner não emite Bitmap Heap Scan
lossy         : resultado idêntico sob 64 kB e 64 MB, e idêntico ao PG17
```

## Limites honestos

1. **O A/B do bitmap não conseguiu forçar o sub-plano visível no `EXPLAIN`.** O `theodb_vecfilter` guarda o
   caminho de bitmap em `children[1]`, que o `EXPLAIN` não exibe; confirmamos que ele é inicializado no código
   (`customscan.rs:541`), e o resultado é idêntico entre regimes de memória e entre majors. Mas **não temos prova
   direta, por instrumentação, de que `materialize_bitmap` executou nesta query** — a igualdade é consistente com
   a execução correta e também seria consistente com o caminho não ter sido tomado. Registrado como cobertura
   parcial, não como prova.
2. **`cargo pgrx test` continua não linkando nesta droplet** (`CopyErrorData` indefinido no link) — a mesma
   limitação dos M131/M132/M134. O teste unitário `m92_v1b_materialize_bitmap_exact`, que constrói um `TIDBitmap`
   à mão e é o oráculo direto do porte, **existe no código mas não foi executado**. Esta é a lacuna mais relevante
   deste milestone e não deve ser lida como "testado".
3. **As suítes de crash e isolamento contra o binário 18 ainda não rodaram** neste artefato — ver § Estado do DoD.
4. **Benchmark de sanidade 18 vs 17 ainda não rodou.** Os 119 artefatos existentes foram medidos no PG17 e, com a
   migração, descrevem uma configuração que não distribuímos mais. Até esse item rodar, **nenhuma alegação de
   performance nossa está apoiada na versão que enviamos** — isso vale ser dito em voz alta.

## Estado do DoD (honesto)

| Item | Estado |
|---|---|
| `cargo build --features pg18` 0 erros | ✅ medido |
| `grep` sem `attrs.as_ptr()`/`compact_attrs` | ✅ 0 ocorrências |
| Bitmap portado; lossy preserva resultado | ✅ A/B idêntico — ⚠️ com o caveat do limite 1 |
| Planner não emite bitmap sobre colunar | ✅ medido |
| Features `pg13`–`pg17` removidas | ✅ `default = ["pg18"]` |
| #143 fechado | ✅ verificado no `.so` publicado |
| Suítes de crash + isolamento no 18 | ❌ **pendente** |
| Benchmark de sanidade 18 vs 17 | ❌ **pendente** |
| Packaging publicando o 18 | ❌ **pendente** |

**Três itens do DoD estão abertos.** O milestone não está completo, e este artefato não afirma que está.
