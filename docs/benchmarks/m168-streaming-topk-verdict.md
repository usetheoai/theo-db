# M168 — decode O(k) para o top-k de projeção: veredito medido

**Data:** 2026-07-29 · **Fecha:** item 2b do DoD do M167 (#215) e o falso-admit medido em #218
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NÃO a c6a.4xlarge canônica do ClickBench
**Dados:** ClickBench `hits` / `hits_heap`, 1.000.000 linhas, 105 colunas — verificados por `count(*)`
**Binário:** `so_md5=f98b5b4cb2fd (ver o cabeçalho de cada artefato)`
**Artefatos:** `docs/benchmarks/m168-artifacts/`

## 1. Memória — os dois braços, na mesma sessão

`benchmarks/m168_peak.sql` roda as quatro consultas nos **dois estados** de `theodb.enable_columnar_topk_stream`
dentro de uma sessão, e `benchmarks/m168_peak_summarize.py` produz a tabela a partir do log commitado. A primeira
versão deste verdict comparava um "antes" medido num binário anterior cujo log nunca foi commitado — a razão
principal tinha numerador não-verificável (achado de review). Agora o par é um artefato só, mesmo processo,
mesmo binário (`peak-both-arms.log`, `so_md5=0c90ef7d6ef385df81d798cc070b9daa`).

| Consulta | eager: batches / bytes | streaming: batches / maior batch | razão |
|---|---|---|---|
| q23 `SELECT *` + LIKE | 1 / **809.738.352** | 100 / **18.754.800** | **43,2×** |
| q24 projeção estreita | 1 / 20.388.828 | 100 / 644.508 | 31,6× |
| q25 chave de texto | 1 / 12.388.732 | 100 / 564.412 | 21,9× |
| q26 multi-chave | 1 / 20.388.828 | 100 / 644.508 | 31,6× |

São **100** chunk-groups (`CHUNK_GROUP_ROWS = 10_000`, `columnar_codec.rs:24`, 1M linhas). Uma versão anterior
reportava 99 e um máximo sobre 99: a sonda de schema decodifica o primeiro chunk-group e retornava antes do
trace, então ela não era medida — e podia ser a maior. Agora é traçada (`probe=1`) e o máximo é sobre todos.

O summarizer **assere** o que a tabela assume — os dois braços presentes, eager em exatamente 1 batch, streaming
em mais de 1, sonda instrumentada — e sai não-zero se algo falhar. Uma tabela plausível a partir de uma execução
malformada é o modo de falha que ele existe para impedir.

### "Maior batch", não "pico do processo"

A palavra certa importa. O número medido é o **maior `RecordBatch` Arrow individual instrumentado**, e o pico
real do processo é maior, por três vias que o instrumento não conta:

1. **Dentro do batch.** `build_arrow_from_decoded` constrói arrays novos enquanto os buffers decodificados ainda
   estão vivos; no instante do trace os dois coexistem.
2. **Entre batches.** O `TopK` do DataFusion retém os batches que ainda possuem linhas sobreviventes até compactar.
   **Não medido** — declarado, não estimado.
3. **A pool.** `GreedyMemoryPool` contabiliza só reservas registradas no DataFusion; os buffers de decode e o
   `RecordBatch` são alocados fora dela.

**A simetria vale para o item 1, e SÓ para ele** (correção de review — a versão anterior estendia o argumento aos
três). Os dois braços usam o mesmo instrumento no mesmo ponto, decodificam as mesmas colunas com a mesma
codificação, então a razão buffer:array se cancela na razão. Os itens 2 e 3 **não** são simétricos: o braço eager
tem 1 batch e o streaming tem 100, e o TopK retém os que ainda têm linha sobrevivente.

O item 2 deixou de ser argumento e passou a ser medido: `run_df_collect_streaming` agora lê
`MemoryPool::reserved()` ao fim do `block_on`. Medido nas quatro consultas: **`reserved_at_end=0`** contra um teto
de 201.326.592 B. A retenção do TopK, para estas formas (k=10), não deixa nada reservado ao fim.

Isso **não** prova que o pico intermediário foi zero — `reserved()` é o estado final, não o máximo — então o item
2 continua parcialmente aberto e assim declarado. O que ele fecha é a preocupação concreta que motivou a
pergunta: não há reserva pendurada ao término.

Consequência para a #218, na forma que o instrumento sustenta: o falso-admit era o decode (772 MiB) superar o
orçamento do guard (512 MiB). O maior batch instrumentado caiu para 17,9 MiB, e nada fica reservado ao fim. O
**footprint total instantâneo** do caminho streaming não foi medido, e é o termo que falta para fechar a #218 com
todo o rigor.

## 2. Throughput — e as DUAS conclusões minhas que o harness produziu, ambas erradas

Esta seção mudou de resposta duas vezes, e o registro das duas é mais útil que o número final.

**Primeira versão:** "as quatro consultas ficaram mais rápidas". Falso — o harness declarava alternar os braços e
não alternava; rodava eager-depois-stream nas 20 iterações, e o drift monotônico era sempre pago pelo braço que ia
primeiro, que era sempre o eager (achado de review).

**Segunda versão:** "o q24 regride ~8%". Também falso — a alternância foi implementada, mas com **5 pares**, e com
ordem alternada um número ímpar não contrabalanceia: um braço come uma posição-primeira a mais, incluindo o par 1,
o mais frio (achado de review).

**Terceira, com 6 pares** (`benchmarks/m168_stream_ab.sql`, tabela produzida por
`benchmarks/m168_ab_summarize.py` a partir de `paired-ab-stream.log`, nunca transcrita à mão):

| Consulta | razões (2 execuções) | mediana | dispersão eager | leitura |
|---|---|---|---|---|
| q23 | 0,817 · 0,818 | **0,817** | 1,30× | **efeito real** — as faixas por par nunca se sobrepõem, e as duas execuções concordam na 3ª casa |
| q24 | 1,001 · 1,036 | 1,018 | 1,35× | dentro da dispersão |
| q25 | 1,025 · 0,971 | 0,998 | 1,21× | dentro da dispersão |
| q26 | 1,000 · 1,045 | 1,022 | 1,20× | dentro da dispersão |

**A regressão que a segunda versão entregou não se estabelece com o desenho corrigido.** O honesto agora é: o q23
é ~18% mais rápido, e nas três consultas estreitas **não há efeito detectável** — as medianas (0,998–1,022) ficam
dentro de uma dispersão intra-braço de 1,20–1,35×.

O summarizer **recusa** publicar tabela de execução malformada: exige as 4 consultas, contagem de pares igual e
**par**, um único `so_md5` entre execuções, e o bloco per-pair presente (é ele que sustenta qualquer alegação de
faixa — a primeira versão citava faixas de um bloco que fora filtrado do log commitado).

### O piso de 1,88× do M167 NÃO se aplica aqui

Uma versão anterior citava-o como gatilho de honest-negative. Errado, e generoso na direção errada: aquele piso é
**entre execuções**, e o próprio verdict do M167 chama comparação cross-run de erro de categoria. O piso certo é a
dispersão pareada por consulta, que a tabela acima traz — é contra ela que o q23 se destaca e as outras três não.

## 3. Correção — e o defeito que só a revisão pegou

**BLOCKER encontrado em review: o streaming perdia as escritas da própria transação.**

`plan_columnar_scan` foi extraída do **meio** de `decode_columns_v2` e deixou para trás a guarda que estava no
**topo** — a que detecta linhas escritas pela transação corrente e ainda não descarregadas em stripe, e cai no
caminho legado. Um scan planejado só a partir de `read_visible_stripes` não as enxerga.

Consequência: `BEGIN; INSERT; SELECT … ORDER BY … LIMIT` sobre uma tabela que **já tinha stripes** não devolvia as
linhas recém-inseridas. Em silêncio. Uma consulta não ver as escritas da própria transação é violação dura de
correção.

**Nenhum oráculo existente podia pegar isso, e não por acaso:** `m167_hits_topk_ab.sql` e `m158_ec_harness.sql`
carregam em massa e depois só leem — a forma `BEGIN; INSERT; SELECT` nunca é construída. E o caso *puramente*
pendente funcionava por acidente (a sonda devolve `None` e o eager assume), então só o estado **misto** quebrava:
justamente o que um smoke test tende a não montar.

`benchmarks/m168_pending_rows.sql` constrói os dois estados. Medido no binário **com** o defeito e no binário
corrigido:

| Asserção | com o bug | corrigido |
|---|---|---|
| `p1_pending_rows_seen` (das 3 pendentes, quantas o top-k devolveu) | **0** | **3** |
| `p1_stream_vs_eager_mism` (estado misto) | **6** | **0** |
| `p2_stream_vs_eager_mism` (puramente pendente) | 0 | 0 |
| `p3_control_diff` (controle negativo) | 10 | 10 |

O RED existiu antes do fix; não é reconstrução. A guarda virou `has_unflushed_pending`, chamável em vez de
copiável, e o streaming declina para o eager quando ela é verdadeira — fail-closed.

### Estado dos gates no binário final (`so_md5=0c90ef7d6ef385df81d798cc070b9daa`)

| Gate | Resultado |
|---|---|
| Oráculo 1M top-k (H0 9/9 + 15 asserções) | `rc=0` |
| Oráculo de fixture (20 asserções) | `rc=0` |
| **Oráculo de pendentes (5 asserções, misto + puro)** | **ok** |
| 4 controles positivos (H0, gate final, gate EC, gate de pendentes) | todos abortam |
| Memória, dois braços | eager 1 batch / streaming 100, sonda instrumentada |

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

Revalidado após a remoção: oráculos verdes, 400 batches de stream (o caminho continua ativo).

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
