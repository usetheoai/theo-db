# M168 — decode O(k) para o top-k de projeção: veredito medido

**Data:** 2026-07-29 · **Fecha:** item 2b do DoD do M167 (#215) e o falso-admit medido em #218
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NÃO a c6a.4xlarge canônica do ClickBench
**Dados:** ClickBench `hits` / `hits_heap`, 1.000.000 linhas, 105 colunas — verificados por `count(*)`
**Binário:** `so_md5=facf2881fd8ebbbf3c486cf9333b444e`
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

O que **sobrevive** a isso: o mesmo instrumento subconta o item 1 nos **dois** braços (o caminho eager também
segura os buffers enquanto monta seu batch), então a **razão ~43×** se preserva mesmo com os absolutos
subestimados. É a razão que este documento afirma; o absoluto é um piso.

Consequência para a #218: o falso-admit era o decode (772 MiB) superar o orçamento do guard (512 MiB). Com o
maior batch em 17,9 MiB, o excesso deixa de existir por ordem de grandeza — mesmo com os três subcontos acima,
não há caminho de 17,9 MiB a 512 MiB.

## 2. Throughput — e a regressão que o próprio harness escondia

**Este é o achado mais importante desta seção, e ele contradiz a primeira versão deste verdict.**

O harness declarava alternar os braços a cada par. Não alternava: rodava eager-depois-stream nas 20 iterações
(achado de review). O drift é grande e monotônico — o q23 eager caiu 5742 → 4036 ms ao longo dos 5 pares — e era
sempre pago pelo braço que ia primeiro, que era sempre o eager. Isso dava ao streaming uma vantagem sistemática.

Com a alternância implementada, em **três execuções** (`paired-ab-stream.log`):

| Consulta | eager | stream | razão | as três execuções | leitura |
|---|---|---|---|---|---|
| q23 | 4213,6 ms | **3431,9 ms** | **0,814** | 0,833 · 0,828 · 0,814 | **ganho real** — faixas não se sobrepõem (3284–3703 vs 3966–5742) |
| q24 | 129,1 ms | 139,1 ms | **1,077** | 1,093 · 1,202 · 1,077 | **regressão real de ~8–20%**, consistente em direção |
| q25 | 99,3 ms | 102,1 ms | 1,029 | — | dentro do ruído (faixas se sobrepõem) |
| q26 | 140,5 ms | 147,2 ms | 1,048 | — | dentro do ruído |

**O q24 regride, e isso é entregue como regressão, não como ruído.** A explicação é a que o plano previu no
§ Riscos antes de existir código: atravessar o DataFusion 100 vezes em vez de 1 tem overhead fixo por batch, e
numa projeção estreita há pouca memória a economizar para pagá-lo.

**O trade-off, declarado em vez de escondido:** 31× de memória por ~8% de latência numa consulta que já roda em
130 ms. O default fica ON porque memória é o recurso que causa OOM e derruba o backend, enquanto 10 ms numa
consulta de 130 ms não derruba nada — mas quem discordar tem
`theodb.enable_columnar_topk_stream = off` e o número acima para decidir.

### O piso de 1,88× do M167 NÃO se aplica aqui

Uma versão anterior citava-o como gatilho de honest-negative. Está errado, e generoso na direção errada: aquele
piso é **entre execuções**, e o próprio verdict do M167 chama comparação cross-run de erro de categoria. O desenho
pareado existe para escapar dele. Com 1,88× como limiar, uma regressão de 1,8× passaria como "sem regressão".

O piso certo é a dispersão pareada dentro de cada consulta: q23 1,45× · q25 1,20× · q26 1,23× · q24 1,04×. É
contra ela que o q23 (ganho) e o q24 (regressão) se destacam, e as outras duas não.

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
