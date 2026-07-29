# M168 — decode O(k) para o top-k de projeção: veredito medido

**Data:** 2026-07-29 · **Fecha:** item 2b do DoD do M167 (#215) e o falso-admit medido em #218
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NÃO a c6a.4xlarge canônica do ClickBench
**Dados:** ClickBench `hits` / `hits_heap`, 1.000.000 linhas, 105 colunas — verificados por `count(*)`
**Binário:** `so_md5=facf2881fd8ebbbf3c486cf9333b444e`
**Artefatos:** `docs/benchmarks/m168-artifacts/`

## 1. O resultado — pico de memória

Medido pelo **mesmo instrumento** que produziu o baseline do M167 (`batch.get_array_memory_size()`, gateado em
`THEODB_ADMIT_TRACE`). Esse par antes/depois pelo mesmo instrumento era a condição escrita no plano antes de
existir código, precisamente para que "agora é O(k)" não pudesse ser prosa.

| Consulta | pico ANTES (O(N)) | pico DEPOIS | batches | redução |
|---|---|---|---|---|
| q23 `SELECT *` + LIKE + `ORDER BY … LIMIT 10` | 809.738.352 B (772,2 MiB) | **18.754.800 B (17,9 MiB)** | 99 | **43,2×** |
| q24 projeção estreita | 20.388.828 B | **644.508 B** | 99 | 31,6× |
| q25 chave de texto | 12.388.732 B | **564.412 B** | 99 | 21,9× |
| q26 multi-chave | 20.388.828 B | **644.508 B** | 99 | 31,6× |

**O pico deixou de ser função de N.** São 99 chunk-groups para 1M linhas; o pico é o maior chunk-group, não a
relação. Para o q23 isso significa 17,9 MiB contra um `work_mem` de 64 MiB — **abaixo** do orçamento, o que
dissolve o falso-admit da #218 em vez de recalibrá-lo.

### Prova de que o caminho novo é o que roda

396 linhas `theodb_decode_batch_stream` e **zero** `theodb_decode_batch:` na mesma execução das quatro consultas
(`m168-artifacts/peak-streaming.log`). As duas famílias de trace saem de sítios diferentes e mutuamente
exclusivos, então isso não é inferência: o decode eager nunca foi chamado. Sem essa contagem, oráculos passando
provariam apenas que *algum* caminho está correto — a vacuidade que o gate H0 do M167 existe para proibir.

## 2. Throughput — o critério de honest-negative, que não precisou ser usado

O plano declarava antes de começar: se o throughput regredisse acima do piso de ruído da box (1,88×, medido no
M167 § 6), o milestone reportaria isso em vez de publicar uma vitória de memória paga em latência.

A/B **pareado, mesma sessão, mesmo binário**, alternando só `theodb.enable_columnar_topk_stream`, 5 pares por
consulta, cada braço materializando as k linhas com CTAS (`benchmarks/m168_stream_ab.sql`):

| Consulta | eager (mediana) | stream (mediana) | razão | faixas por par |
|---|---|---|---|---|
| q23 | 4082,2 ms | **3488,8 ms** | **0,854** | eager 3977–6008 · stream 3352–3638 — **não se sobrepõem** |
| q25 | 103,6 ms | **92,4 ms** | **0,892** | eager 99,7–139,6 · stream 90,8–123,4 — sobrepõem |
| q26 | 134,3 ms | 120,2 ms | 0,895 | sobrepõem |
| q24 | 136,0 ms | 135,3 ms | 0,995 | sobrepõem |

**Leitura honesta:** o q23 é ganho real — as faixas por par não se sobrepõem em nenhuma das duas execuções do
harness. As três consultas estreitas ficam **dentro do ruído** da box para consultas sub-200 ms; o defensável ali
é "sem regressão", não "mais rápido". Nenhuma regride.

Por que o q23 fica mais rápido em vez de só mais econômico: decodificar 772 MiB para descartar tudo menos 10
linhas gasta banda de memória que o caminho por chunk-group não gasta. O ganho de latência é efeito colateral do
ganho de memória, não um objetivo perseguido.

## 3. Correção

| Gate | GUC ON (stream, default) | GUC OFF (fallback eager) |
|---|---|---|
| Oráculo 1M top-k (H0 9/9 + 15 asserções) | `rc=0` | H0 ok, gate final ok |
| Oráculo de fixture (20 asserções) | `rc=0` | — |
| 3 controles positivos | `rc=3` cada | — |

Os dois caminhos continuam corretos. A GUC não é só um interruptor de medição: é a saída se o streaming exibir
um comportamento que os oráculos não cobrem.

## 4. O `unsafe impl Send` — e por que ele não é um comentário

`PartitionStream` exige `Send + Sync`, e o stream carrega `pg_sys::Relation`. Isso é sólido **apenas** porque
`run_df_collect_streaming` usa `new_current_thread` + `block_on` com `target_partitions(1)`: o stream é pollado na
thread do backend e em nenhuma outra. É verdade por **configuração**, não por construção — trocar para
`new_multi_thread` daria corrupção silenciosa de memória, não erro de compilação.

Então a invariante é asseverada: `ThreadAffinity::capture()` na construção, `assert_owned()` a cada `next()`,
panic imediato se divergir. Dois `#[pg_test]` provam os dois lados — que a asserção **dispara** de outra thread
(sem isso ela poderia estar inerte) e que é silenciosa na thread dona.

Precedente na mesma classe: o M139 encontrou o Tantivy chamando `Directory` de quatro threads.

## 5. `/code-quality` — e o que ele NÃO pegou

Veredito `PASS_WITH_CAVEATS`, **HARD 0**, D1 (código morto) sem achados. As duas ressalvas do D2 são o mesmo
`use pg_sys::XactEvent as XE`, falso-positivo do detector (`pg_sys` é re-export de módulo do pgrx, não crate do
crates.io) e **pré-existente** — `git log -S` o data em `b84a29f`, do M99.

**Mas a auditoria passou por cima de código morto meu.** Minha própria checagem encontrou quatro métodos
(`column_names`, `column_typids`, `stats`, `n_chunk_groups`) com **zero chamadores**, escritos especulativamente —
violação da rung 1 da parsimony ladder. Removidos, e com eles os campos `skipped`/`emitted`, que ficaram
write-only. O D1 do Rust não detecta método `pub(crate)` sem chamador; o relatório dizer "No findings" para D1
significa "o detector não achou", não "não há".

Revalidado após a remoção: oráculos verdes, 396 batches de stream (o caminho continua ativo).

## 6. O que NÃO está provado

- **Os `#[pg_test]` não foram executados nesta box.** `cargo test` não linka neste crate (`undefined symbol:
  PG_exception_stack` — os testes precisam dos símbolos do PostgreSQL) e `cargo pgrx test` não foi rodado aqui. A
  asserção de afinidade está escrita e compilada; sua *execução* é dívida declarada.
- **Além de 1M linhas.** O M162 mediu o scan colunar a 100M e bateu em `byte array offset overflow` (offsets
  varlena i32 do Arrow > 2 GB). O streaming reduz o batch por chunk-group, o que **deve** afastar esse teto — mas
  não foi medido a 100M.
- **Planos paralelos.** `target_partitions(1)` é load-bearing para a soundness do `unsafe impl`; paralelismo
  exigiria outro design, não um ajuste de configuração.
- **O tamanho do chunk-group é o do escritor.** Não há tuning aqui: se um dia os chunk-groups ficarem muito
  pequenos, o overhead fixo por batch do DataFusion pode dominar. Não medido.

## 7. Reprodução

```bash
# pico (o postmaster PRECISA subir com a variável — o backend herda dele, não do psql)
THEODB_ADMIT_TRACE=1 pg_ctl -D <datadir> -w start
PGOPTIONS='-c work_mem=64MB' psql -f /tmp/peak.sql   # conta as linhas *_stream vs eager

# A/B pareado
psql -f benchmarks/m168_stream_ab.sql

# correção, nos dois estados da GUC
./benchmarks/m167_run_oracles.sh
PGOPTIONS='-c theodb.enable_columnar_topk_stream=off' psql -f benchmarks/m167_hits_topk_ab.sql
```
