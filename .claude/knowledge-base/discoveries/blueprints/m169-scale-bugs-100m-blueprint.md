# M169 — blueprint: o teto de offsets `i32` no caminho agregado colunar

**Data:** 2026-07-29 · **Milestone:** M169 · **Fase:** discover

## Conformidade R0 — declarada, com o reparo

O agente de discover **não tinha `WebSearch`/`WebFetch`** no contexto dele (ambos retornaram
`No such tool available`), então a primeira versão deste blueprint violava a regra R0 de
`.claude/rules/discover-phd-rigor.md` ("todo deep research DEVE usar WebSearch + WebFetch e citar").
**A varredura web foi executada depois, no thread principal**, e as citações estão em § Evidência web.
O eixo de acervo local foi cumprido pelo agente (checkouts de `arrow-rs`, `datafusion`, `duckdb`, `citus`).

Ressalva de versão: o checkout de `datafusion` no acervo é `3a29d6bd` (2026-07-13), linha de
desenvolvimento pós-tag 54.0.0; nós resolvemos `arrow 58.3.0`, o acervo aponta `59.1.0`. As
dispatch-tables citadas são estáveis desde muito antes, mas divergências de detalhe são possíveis.

## 1. A cadeia causal — fechada até a linha do `panic!`

| # | Local | Fato |
|---|---|---|
| 1 | `df_executor.rs:611` (agg) e `:747` (GROUP BY) | ambos chamam `decode_to_batch` |
| 2 | `df_executor.rs:486` | `decode_columns_v2` — **relação inteira**, buffers acumulados por todos os chunk-groups |
| 3 | `df_executor.rs:160` | varlena → `DecodedColumn::Cells` → `cells_to_array` |
| 4 | `df_executor.rs:300-307` | `25 \| 1042 \| 1043 => (DataType::Utf8, StringArray::from_iter(…))` |
| 5 | `arrow-array/src/builder/generic_bytes_builder.rs:87` | `T::Offset::from_usize(…).expect("byte array offset overflow")` |

**É um `panic!`, não um `Result`.** Quando a mensagem aparece, a memória já foi toda alocada — o panic
é o fim de um caminho que já pagou o pico.

**O multiplicador que o enunciado do milestone não mencionava:** `df_executor.rs:305` faz
`String::from_utf8_lossy(b).into_owned()` **por célula**. A 100M isso são ~100M `String` heap-alocadas,
vivas ao mesmo tempo que os `Option<Vec<u8>>` de origem e que a array Arrow em construção — **três
cópias coexistentes** da coluna de texto. É o mesmo padrão que o deep-dive do M159 nomeou como "a ponte
de decode". **O teto `i32` é o sintoma que grita; o ×3 de memória é o que mata a corrida.**

## 2. As três alternativas

| | Teto | Custo de memória | Implementação |
|---|---|---|---|
| **(a) `LargeUtf8`** (offsets `i64`) | eliminado | **+4 B/linha** = +400 MB/coluna a 100M | pequena |
| **(b) decode em lotes** (chunked) | **deslocado**, não eliminado | **negativo** — 43,2× menos no maior batch (M168, medido a 1M) | **já construída** (M168) |
| **(c) `Utf8View`** | eliminado (múltiplos buffers) | **16 B/linha** = 1,6 GB/coluna a 100M | maior (builder + braço em `arrow_value_to_datum`) |

Layout do `Utf8View` confirmado em `arrow-array/src/array/byte_view_array.rs:64-130`: views de `u128`,
inline até 12 bytes, prefixo de 4 bytes acima disso. **O prefixo inline não ajuda `LIKE '%google%'`** —
`contains` precisa varrer o valor todo.

## 3. Suporte no DataFusion 54 — verificado no código, não suposto

| Capacidade | `Utf8` | `LargeUtf8` | `Utf8View` | Evidência |
|---|---|---|---|---|
| `LIKE`/`NOT LIKE`/`ILIKE` | ✅ | ✅ | ✅ | `arrow-string/src/like.rs:238,247,256` |
| GROUP BY 1 col | ✅ | ✅ | ✅ | `aggregates/group_values/mod.rs:178,181,184` |
| GROUP BY multi | ✅ | ✅ | ✅ | `multi_group_by/mod.rs:939,940,955` |
| `COUNT(DISTINCT)` | ✅ | ✅ | ✅ | `functions-aggregate/src/count.rs:243,246,249` |

**Duas armadilhas:** (i) o kernel de LIKE é **estritamente homogêneo** — par misto cai em
`InvalidArgumentError` (`like.rs:292-294`), e nosso literal é hardcoded `Utf8` (`df_executor.rs:553`);
(ii) a **coerção lógica salva** (`type_coercion/binary.rs:1709-1722` resolve `(LargeUtf8, Utf8) →
LargeUtf8`), mas só porque usamos a API de `Expr` lógica — montar `PhysicalExpr` direto perderia o
guarda-chuva.

## 4. Como os peers resolvem

**O padrão consagrado não é um tipo — é nunca materializar a coluna inteira.**

- **DuckDB:** `STANDARD_VECTOR_SIZE = 2048` (`vector_size.hpp:16`). O executor é um pipeline de vetores;
  o teto de 2 GB por coluna **não existe no modelo**. E o `string_t` com `PREFIX_BYTES=4`/`INLINE_BYTES=12`
  (`string_type.hpp:28-29`) é o design que o Arrow **adotou** como `StringView` — nasceu aqui/no Umbra.
- **DataFusion no próprio produto:** `schema_force_view_types` default **true** (`config.rs:1151-1153`) —
  quando ele lê Parquet de verdade, já entrega `Utf8View` **sobre batches**. As duas técnicas juntas.
- **Citus/cstore:** `stripeReadContext` (`columnar_reader.c:65-66`) — leitura por stripe, um MemoryContext
  por stripe. Mesmo princípio, vocabulário do PG.

**Ninguém no SOTA resolve isso trocando `Utf8` por `LargeUtf8` e mantendo a materialização O(N)** — essa
combinação é a que consome mais memória de todas.

## 5. A máquina do M168 resolve? **Sim para o DoD, com um limite calculável e três ressalvas**

`CHUNK_GROUP_ROWS = 10_000` (`columnar_codec.rs:24`); `ColumnarChunkStream::next` aloca buffers frescos
por chunk-group (`columnar.rs:1186-1188`). O overflow passa a exigir que **um chunk-group** exceda 2 GB:

```
2^31 / 10.000 = 214.748 bytes por célula, em média
```

**Derivado, não medido.** Para ClickBench `URL` a margem é de ordens de magnitude; para um repositório de
documentos com textos de 1 MB, **o teto ainda existe**. Streaming **move** o teto de "2 GB por coluna por
relação" para "2 GB por coluna por 10.000 linhas" — declarar isso, não deixar implícito.

### Ressalva 1 — `sum(float8)`/`avg(float8)` dependem do batching (ameaça o gate A/B)

`SumAccumulator::update_batch` chama `arrow::compute::sum` **por batch** e acumula com `add_wrapping`
(`functions-aggregate/src/sum.rs:441-448`); o kernel usa acumuladores por lane sobre `chunks_exact(64)`
com redução em árvore (`arrow-arith/src/aggregate.rs:179-197,264`). A **ordem de associação** depende do
comprimento do array → resultados podem diferir no último ULP. IEEE-754 não é associativo.

- **ClickBench não bate:** todos os `SUM`/`AVG` roteáveis são sobre inteiros (verificado nas queries) →
  `Int64`/`Decimal128`, exatos e independentes de ordem.
- **O harness de tipos do M163 bate:** ele exercita float por design.
- `count`, `count_distinct`, `min`/`max`, `sum(int*)`: sem risco.

**Status: `UNBENCHMARKED`** — hipótese de leitura de código. É a primeira coisa que o A/B a 1M deve mirar.

### Ressalva 2 — streaming **não** limita a tabela hash do GROUP BY (confirmada no upstream)

Streaming limita o **scan**. A tabela hash é **O(grupos distintos)** e independe do tamanho do batch.
ClickBench q32/q33 (`GROUP BY WatchID, ClientIP`) a 100M produz dezenas de milhões de grupos.

**Confirmação upstream** (varredura web, § Evidência): o issue [#7191](https://github.com/apache/datafusion/issues/7191)
do apache/datafusion se chama literalmente *"Memory is coupled to `group by` cardinality, even when the
aggregate output is truncated by a `limit` clause"*, e o [#13831](https://github.com/apache/datafusion/issues/13831)
documenta OOM em `GroupedHashAggregateStream::group_aggregate_batch()` **apesar** do MemoryPool.

**Consequência para o DoD:** o item "a corrida completa as 43 sem ser OOM-killed" **pode não ser
satisfeito** só ligando o streaming. Ver § 7.

### Ressalva 3 — o spill existe e está ligado; o streaming é o que o torna alcançável

`oom_mode = OutOfMemoryMode::Spill` quando o modo não é `Partial` e o disk manager tem tmp habilitado
(`grouped_hash_stream.rs:493-512`); o default é `OsTmpDirectory` com 100 GB
(`disk_manager.rs:34,58-64`), e construímos o runtime sem desabilitar disco (`df_executor.rs:1313`).
**O spill está ligado hoje, por default.**

Mas **nunca dispara** no agregado, porque o gatilho é pressão na `MemoryPool` — e o batch eager é alocado
**fora** dela. Pior: o dimensionamento eager é `max(work_mem, 2*batch) + 64MB` — **a pool é dimensionada a
partir do batch que já existe**, estruturalmente incapaz de exercer contrapressão.

Com o streaming, a pool vira orçamento **fixo** e o spill vira alcançável. **Esta é a razão mais forte
para ligar o streaming ao agregado — mais forte que o teto `i32`.**

## 6. ADR M169-1 — ligar `ColumnarChunkStream` ao agregado; **não** trocar o tipo Arrow

**Decisão:** substituir `decode_to_batch` por `open_streaming_source` + `run_df_collect_streaming` nos
dois call-sites (`df_executor.rs:611`, `:747`), sob a GUC do M168, mantendo `DataType::Utf8`. Manter o
eager como fallback tipado (mesma forma do `ResourcesExhausted` do top-k).

| Alternativa | Por que rejeitada |
|---|---|
| **A — `LargeUtf8` com decode eager** | resolve o teto e **agrava** o problema maior: +400 MB/coluna somados a um pico que já OOM-killa. Trata o sintoma que grita e piora o que mata. Ladder degrau 1. |
| **B — `Utf8View` com decode eager** | 1,6 GB/coluna de overhead; ganho de prefixo **nulo** para `LIKE '%…%'`; maior custo. Mesmo defeito de A. |
| **C — streaming + `LargeUtf8` juntos** | duas mudanças num commit cujo gate é `diverged=0`. Se divergir, não se sabe qual causou. Rejeitada por diagnosticabilidade. |
| **D — só aumentar `work_mem`/a caixa** | não é correção; e o M162 não distingue OOM-da-caixa de OOM-nosso — é por isso que o DoD pede baseline primeiro. |

**ADR M169-2 — `LargeUtf8` fica condicional.** Gatilho para reabrir: medição mostrando célula média
>214 KB num chunk-group, **ou** a decisão de publicar "sem limite de 2 GB por coluna de texto". Se
reaberto, `LargeUtf8` antes de `Utf8View` (metade do overhead no regime que importa).

## 7. Impacto no DoD do M169 — o discover mudou o milestone

O item **"a corrida completa as 43 sem ser OOM-killed"** foi escrito antes desta investigação e a
Ressalva 2 mostra que ele mistura dois mecanismos independentes. Emenda aplicada no `ROADMAP.md`:
o item passa a separar o OOM **de scan** (que o streaming ataca) do OOM **de cardinalidade de GROUP BY**
(que ele não ataca), exigindo medição de q32/q33 e — se o spill não bastar — a **declaração honesta do
limite** em vez de uma promessa que a arquitetura não sustenta.

## Evidência web (reparo da R0)

- [apache/datafusion #7191 — *Memory is coupled to `group by` cardinality…*](https://github.com/apache/datafusion/issues/7191) — confirma a Ressalva 2.
- [apache/datafusion #13831 — *OOM in `GroupedHashAggregateStream::group_aggregate_batch()`*](https://github.com/apache/datafusion/issues/13831) — OOM apesar do MemoryPool.
- [apache/datafusion #8003 — *GroupedHashAggregateStream should create smaller spill batches*](https://github.com/apache/datafusion/issues/8003) — o spill grava um batch único, subótimo no merge.
- [apache/arrow-rs #3228 — *better document when we need `LargeUtf8` instead of `Utf8`*](https://github.com/apache/arrow-rs/issues/3228) — o panic de 2 GB dispara mesmo com elementos individuais pequenos.
- [pola-rs/polars #27783](https://github.com/pola-rs/polars/issues/27783) — string views acima de 2 GB geradas por um sistema podem não ser lidas por outros: **interoperabilidade é um risco do `Utf8View`** que o acervo não mostrava.
- [Apache Arrow — Data Types](https://arrow.apache.org/docs/cpp/api/datatype.html) e [`StringViewArray` (docs.rs)](https://docs.rs/arrow/latest/arrow/array/type.StringViewArray.html) — layouts.
- [DataFusion — Configuration Settings](https://datafusion.apache.org/user-guide/configs.html) — `schema_force_view_types`.

## 8. O que NÃO foi verificado

| # | Item | Status |
|---|---|---|
| 1 | Comprimento médio/máximo da coluna `URL` do ClickBench | `UNBENCHMARKED` — a margem do teto de 214 KB é derivada |
| 2 | Divergência de ULP em `sum(float8)` sob batching | hipótese de código, não medida |
| 3 | Qual `AggregateMode` nosso plano produz (Single vs Partial) | decide se o OOM mode é `Spill` ou `EmitEarly`; exige `EXPLAIN` |
| 4 | Se o spill do DataFusion é seguro dentro de um backend PG (limpeza sob cancelamento) | não verificado |
| 5 | Quanto do OOM-kill em 24/43 é caixa de 15 GB vs. nosso pico | é o que o baseline do DoD separa |
| 6 | `pg_mooncake` como peer | checkout só tem a casca Rust; execução no DuckDB embutido |

**Verificado depois do blueprint, no thread principal:** o q23 do M168 **é** literalmente o q23 do
ClickBench (`SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10`), e ele **roteia
hoje** (400 traces `theodb_decode_batch_stream` no artefato a 1M). O M162 o mediu como `native row-exec`
em 2026-07-26, **antes** de o M167 shipar (2026-07-29) — a alegação do ROADMAP é consistente.

## Sequência recomendada para o `/to-plan`

baseline (DoD 1) → streaming nos dois call-sites → A/B a 1M **incluindo o harness de tipos do M163 com
foco em float** (Ressalva 1) → medir o pico do agregado com alta cardinalidade (q32/q33) **antes** de
prometer "as 43 completam" → só então 100M.
