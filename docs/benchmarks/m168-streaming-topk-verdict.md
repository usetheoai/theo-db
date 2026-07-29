# M168 — decode O(k) para o top-k de projeção: veredito medido

**Data:** 2026-07-29 · **Fecha:** item 2b do DoD do M167 (#215) e o falso-admit medido em #218
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NÃO a c6a.4xlarge canônica do ClickBench
**Dados:** ClickBench `hits` / `hits_heap`, 1.000.000 linhas, 105 colunas — verificados por `count(*)`
**Binário:** `so_md5=e010375381ae… (ver o cabeçalho de cada artefato)`
**Artefatos:** `docs/benchmarks/m168-artifacts/`

## 1. Memória — os dois braços, na mesma sessão

`benchmarks/m168_peak.sql` roda as quatro consultas nos **dois estados** de `theodb.enable_columnar_topk_stream`
dentro de uma sessão, e `benchmarks/m168_peak_summarize.py` produz a tabela a partir do log commitado. A primeira
versão deste verdict comparava um "antes" medido num binário anterior cujo log nunca foi commitado — a razão
principal tinha numerador não-verificável (achado de review). Agora o par é um artefato só, mesmo processo,
mesmo binário (`peak-both-arms.log`, `so_md5=e010375381ae…`).

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

O item 2 deixou de ser argumento e passou a ser medido **de verdade**. Uma versão anterior lia
`MemoryPool::reserved()` ao fim do `block_on` e reportava `0` — mas isso é zero **por construção**: tudo que
reserva é destruído dentro do bloco async, e `MemoryReservation` libera no `Drop`. O instrumento não conseguia
retornar outra coisa; era um não-resultado vendido como medição (achado de review).

Agora há uma `PeakTrackingPool` que delega ao `GreedyMemoryPool` e registra a **marca d'água** a cada
`grow`/`try_grow`. Medido nas quatro consultas: **`peak_reserved` entre 825.920 e 2.411.803 B** (0,8–2,4 MiB)
contra teto de 201.326.592 B. A retenção do TopK existe, é real, e é pequena para estas formas (k=10).

Somar os dois (17,9 + 2,4 ≈ 21 MiB) seria juntar instrumentos que **nunca amostram o mesmo instante**, então esse
total continua sendo o item não medido do § 6, não um resultado. E `peak_reserved` é amostrado nos pontos de
reserva, **depois** da compactação do `TopK` — é limite inferior da retenção visível à pool, não o pico contínuo.

Consequência para a #218, na forma que o instrumento sustenta: o falso-admit era o decode (772 MiB) superar o
orçamento do guard (512 MiB). O maior batch instrumentado caiu para 17,9 MiB, e nada fica reservado ao fim. O
**footprint total instantâneo** do caminho streaming não foi medido, e é o termo que falta para fechar a #218 com
todo o rigor.

## 2. Throughput — publicado, depois de eu tê-lo retirado com o instrumento errado

Esta seção mudou de resposta **cinco** vezes. As quatro primeiras caíram por defeito de desenho ou de ambiente; a
quinta caiu por defeito do meu *instrumento de decisão*, que é o erro mais instrutivo dos cinco.

| # | O que eu afirmei | Por que caiu |
|---|---|---|
| 1 | "as quatro consultas ficaram mais rápidas" | o harness declarava alternar os braços e **não alternava** |
| 2 | "o q24 regride ~8%" | alternava com **5 pares**, e ímpar não contrabalanceia |
| 3 | "o q23 é ~18% mais rápido" | a mediana sobre 6 pares mistura aquecimento e regime |
| 4 | **retirei tudo** — "o efeito é da mesma ordem que a dispersão da box" | **falso, e o dado nega**: a dispersão é *entre* pares, o efeito é *dentro* de cada par |
| 5 | o que está abaixo | — |

**O que me fez retirar era um teste de faixas marginais NÃO PAREADAS aplicado a um desenho pareado.**
`m168_ab_summarize.py` comparava `max(stream) < min(eager)` — descartando o pareamento, que é a razão de existir
do desenho, e o único motivo de o harness alternar os braços. Com dispersão intra-braço de 1,39× e efeito de
~0,87, as marginais **não podem** separar: aquele ramo era aritmeticamente quase infalsificável. Um instrumento
que não consegue observar o que precisa observar é a violação do R3.1 de `discover-phd-rigor.md` — a mesma regra
que este projeto escreveu depois do M162.

O summarizer passa a usar **teste do sinal exato, bilateral**, sobre os pares (implementação em
`m168_ab_summarize.py`, sem dependência externa — é uma soma binomial):

| Consulta | razões (2 execuções) | pares favoráveis | p (sinal, bilateral) | leitura |
|---|---|---|---|---|
| **q23** | 0,808 · 0,866 | **12 / 12** | **0,0005** | **efeito pareado real** |
| q24 | 1,084 · 1,038 | 4 / 12 | 0,39 | sem efeito |
| q25 | 1,022 · 0,897 | 8 / 12 | 0,39 | sem efeito |
| q26 | 1,002 · 0,989 | 8 / 12 | 0,39 | sem efeito |

**A magnitude publicável é a do regime aquecido, não a média.** Os pares 1–2 são frios (razões 0,60–0,73) e
puxam a média para baixo — para a ponta *otimista*, no sentido de exagerar o ganho. Nos pares aquecidos (5–6) as
razões são 0,853 · 0,905 · 0,920 · 0,853, ou seja **~12%**.

**O q23 é ~12% mais rápido em regime, com 12/12 pares favoráveis (p = 0,0005).** E o contrabalanceamento sustenta:
nos seis pares em que o *streaming* rodou primeiro — a posição desfavorecida pelo aquecimento — ele venceu 6/6.

Nas três consultas estreitas **não há efeito pareado**. Faz sentido mecanicamente: o ganho do q23 acompanha o
regime em que a memória também cai 43×, e onde há pouco a economizar não há o que ganhar.

**Ressalva de ambiente, que continua valendo:** esta box hospeda o runner de CI e três serviços, e a dispersão
subiu entre coletas. O teste pareado é robusto a isso *por construção* — é para isso que ele existe — mas uma
replicação em máquina dedicada e ociosa continua sendo o que fecharia a questão com folga.

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

| Asserção | corrigido (`pending-rows.log`) |
|---|---|
| `p1_pending_rows_seen` (das 3 pendentes, quantas o top-k devolveu) | **3** |
| `p1_stream_vs_eager_mism` (estado misto) | **0** |
| `p2_stream_vs_eager_mism` (puramente pendente) | **0** |
| `p3_control_diff` (controle negativo) | **10** |

**Sobre o estado defeituoso, com honestidade:** o RED **foi observado** durante o desenvolvimento — o binário com o
bug devolvia `p1_pending_rows_seen = 0` e `p1_stream_vs_eager_mism = 6` — mas **o log daquela execução não foi
preservado**, então esses dois números não são verificáveis a partir deste repositório. Uma revisão apontou que
uma versão anterior desta seção os publicava numa coluna e afirmava "o RED existiu antes do fix; não é
reconstrução", o que era exatamente a alegação sem lastro que o resto do documento evita. Ficam aqui como relato,
não como evidência.

O que **é** verificável: o oráculo existe, é auto-asseverante, tem controle positivo (`p3`) e asserção de
não-vacuidade (`p1_pending_rows_seen` tem de ser 3, senão os zeros comparam dois resultados igualmente errados), e
qualquer um pode reintroduzir o defeito removendo a guarda de `open_streaming_source` e vê-lo falhar.

A guarda virou `has_unflushed_pending`, chamável em vez de copiável, e o streaming declina para o eager quando ela
é verdadeira — fail-closed.

### Estado dos gates no binário final (`so_md5=e010375381ae…`)

| Gate | Resultado |
|---|---|
| Oráculo 1M top-k (H0 9/9 + 15 asserções) | `rc=0` |
| Oráculo de fixture (20 asserções) | `rc=0` |
| **Oráculo de pendentes (5 asserções, misto + puro)** | **ok** |
| 4 controles positivos (H0, gate final, gate EC, gate de pendentes) | todos abortam |
| Memória, dois braços | eager 1 batch / streaming 100, sonda instrumentada |

## 3.5. A busca pela janela do fail-open

O caminho streaming tem pool de `2×work_mem + 64MB`, **constante em `k` e na largura**, enquanto o eager usa
`max(work_mem, 2×batch) + 64MB`, que **escala com o dado**. Uma revisão apontou que, propagando o erro com `?`,
uma consulta que o eager servia passaria a errar por default. `run_columnar_topk` passou a cair no eager.

A pergunta empírica: **existe um `k` em que o eager serve e o streaming não?** Driver commitado
(`benchmarks/m168_window_probe.sql`, `m168_inversion.sql`, `m168_inversion_narrow.sql`), artefatos em
`m168-artifacts/`, todos com `so_md5=e010375381ae…`.

| Cenário | streaming | eager |
|---|---|---|
| 32MB · `SELECT *` 105 col · k=200000 | estoura | **também estoura** |
| 32MB · `SELECT *` 105 col · k=100000 | **estoura** (`peak_reserved=132.100.024` contra pool de 134.217.728) | **também estoura** |
| 32MB · 2 col · k=400000 (`inversion-narrow.log`) | **serve** (`peak_reserved` **43.240.056 B**, pool 134.217.728 B) | **falha** (`TopK[0] with 100.3 MB already allocated`, `pool_size` 102,9 MB decimais) |
| 32MB · 2 col · k=400000, chave composta (`window-probe.log` cenário 3) | **serve** (`peak_reserved` **50.064.565 B**) | **falha** (101,1 + 19,6 MB) |
| **32MB · `SELECT *` 105 col · k=1000** (a banda que um reviewer previu) | **serve** (`peak_reserved` **9.293.548 B**) | **serve** (1000 linhas) |

**A janela não foi encontrada — inclusive na banda prevista.** Um revisor derivou dos números da § 1 que
`SELECT *` com `k` pequeno deveria quebrar o streaming: pool fixa de 128 MB contra até 100 × 18,75 MB retidos. A
predição foi testada e **falhou**: o `peak_reserved` medido é **9,3 MB**, não 1875 MB, porque o `TopK` do
DataFusion **compacta** (`maybe_compact`) e retém as k linhas sobreviventes, não os batches inteiros. A aritmética
de "× número de batches" não vale — e só se soube disso medindo.

**A janela inversa existe.** Com projeção estreita e `k = 400000`, o streaming serve e o eager falha. Mas a
comparação tem uma ressalva que uma versão anterior omitia: **os dois braços correm sob orçamentos diferentes**
(128 MB contra 102,9 MB), porque as fórmulas das pools diferem. A conclusão qualitativa sobrevive — 43,2 MB
caberiam nos dois orçamentos, e 119,8 MB não caberiam em nenhum — mas atribuir todo o delta à retenção
confundiria duas variáveis.

E os números da retenção passaram a ser **os medidos**, não estimativas de tamanho de batch. Uma versão anterior
dizia "o streaming retém ~250 KB e o eager segura 40 MB" (≈160×) — os 250 KB eram o tamanho de *um batch*, não a
retenção, e os 40 MB não estavam em artefato algum. Medido: **43,2 MB contra ≥119,8 MB, ou ~2,8×**. Foi a
`PeakTrackingPool` que produziu esse número, e a conclusão que mais dependia dele era justamente a que não o usava.

**A folga não é constante, e a comparação tem de ser contra o mesmo denominador.** Contra a pool do DataFusion:
em k=10 a retenção medida é 0,8–2,4 MiB numa pool de 192 MiB; em k=400000 é 43,2 MB numa pool de 128 MiB — 34%.
(Uma versão anterior comparava ~21 MiB contra os 512 MiB do guard do ADR-4, misturando dois orçamentos diferentes
numa frase só, e usando como numerador justamente a soma que o § 1 declara não ser um resultado.)

**O fail-open fica, e o que ele é fica dito com precisão.** Ele **disparou** — nos cenários 1 e 2, e o
`window-probe.log` mostra o trace do decode eager dentro do braço streaming provando a degradação. O que os quatro
cenários não encontraram foi um caso em que ele **resgatasse** a consulta: nos dois em que atuou, o caminho eager
também estourou. Uma versão anterior dizia "os quatro cenários não o encontraram", confundindo *disparar* com
*resgatar*. Ele agora registra no log do servidor **incondicionalmente** — escondê-lo atrás do
flag de trace neutralizava, a jusante, um guard escrito para falhar alto, e deixava o usuário sem sinal de que a
consulta trocou de perfil de memória.

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
