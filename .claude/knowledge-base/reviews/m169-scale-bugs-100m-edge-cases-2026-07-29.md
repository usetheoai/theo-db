# Edge Case Review — m169-scale-bugs-100m

Date: 2026-07-29
Tasks analyzed: 6 (T1.1, T1.2, T2.1, T3.1, T3.2, T4.1)
Cases found: 9 (EDGE: 5, NEGATIVE: 4 | MUST FIX: 2, SHOULD TEST: 5, DOCUMENT: 2)

## MUST FIX

### EC-1: o `ScanPlan` é O(N) — ele materializa o diretório de chunks INTEIRO antes do primeiro batch

- **Affected task:** T2.1
- **Kind:** EDGE (extremo de um cenário válido — a relação de 100M que é o alvo do milestone)
- **Family:** Resource
- **Scenario:** `plan_columnar_scan` (`columnar.rs:987`) devolve `ScanPlan { plans: Vec<StripePlan>, … }`, e cada
  `StripePlan` (`:891`) carrega `entries: Vec<codec::ChunkDirEntry>` — o diretório completo de todos os stripes,
  montado **antes** de o stream entregar um único batch. Um `ChunkDirEntry` (`columnar_codec.rs:108`) tem
  6×`u32` + 3×`bool` + 2×`u64` ≈ **48 bytes** alinhado.

  | | chunk-groups | × colunas | entries | memória |
  |---|---|---|---|---|
  | 1M, `SELECT *` (105 col) | 100 | 105 | 10.500 | 0,5 MiB |
  | **100M, `SELECT *`** | **10.000** | **105** | **1.050.000** | **48,1 MiB** |
  | 100M, projeção estreita (5 col) | 10.000 | 5 | 50.000 | 2,3 MiB |

- **Impact:** o milestone existe para tornar o decode O(chunk-group) em vez de O(N), e o **plano** do scan continua
  O(N). A 100M são ~48 MiB por scan de `SELECT *` — não fatal, mas: (a) é alocado **antes** de qualquer entrega,
  então não há sobreposição com o processamento; (b) sai **fora** da `MemoryPool`, portanto invisível à
  contrapressão e ao spill — exatamente o defeito estrutural que a Ressalva 3 do blueprint descreve; (c) o
  milestone publicaria "decode O(k)" com um termo O(N) não declarado. Isso é o mesmo tipo de alegação que o M168
  passou doze rodadas corrigindo.

  **A 1M isso é invisível (0,5 MiB), e é por isso que o M168 não o pegou.** Ele só aparece na escala que este
  milestone mira.

- **Suggested fix:** não redesenhar. **Medir e declarar** — acrescentar ao T1.2/T4.1 a medição do tamanho do
  `ScanPlan` (uma linha de trace com `plans.iter().map(|p| p.entries.len()).sum()`), e ao artefato a frase "o
  decode é O(chunk-group); o **plano** do scan permanece O(N/10.000) e a 100M custa ~48 MiB fora da MemoryPool".
  Se a medição mostrar que ele domina o pico, abre milestone próprio (lazy directory por stripe). Uma linha de
  instrumentação, não uma refatoração — `parsimony-ladder.md` degrau 1.

### EC-2: tabela colunar VAZIA — `count(*)` tem de devolver 0, não erro nem NULL

> **CORRIGIDO 2026-07-29 — este caso JÁ ESTÁ TRATADO no código; a análise abaixo estava errada.**
> `df_executor.rs:1132-1134` faz exatamente o que o "suggested fix" pede, e com o comentário explícito:
> ```rust
> let Some(cols) = first else {
>     return Ok(None); // nothing visible — caller falls back to the batch path, which handles empty correctly
> };
> ```
> Eu escrevi "hoje é indeterminado" sem ler o `else` da sonda. O `open_streaming_source` já devolve
> `Result<Option<…>>` justamente para declinar. Rebaixado de **MUST FIX** para **SHOULD TEST**: o que falta
> é o teste de regressão que trava o comportamento, não o comportamento.
- **Kind:** EDGE (extremo válido: zero linhas é um estado legítimo)
- **Family:** Boundary
- **Scenario:** `open_streaming_source` faz uma **sonda de schema** chamando `next()` uma vez
  (`df_executor.rs:1129-1130`). Numa tabela com 0 stripes a sonda devolve `Ok(None)` e não há schema para
  construir o `StreamingTable`. O caminho eager não tem esse problema: `decode_to_batch` monta um `RecordBatch`
  vazio **com schema** a partir do `ColDesc`, e `count(*)` sobre ele devolve 0 corretamente.
- **Impact:** `SELECT count(*) FROM tabela_colunar_vazia` pode errar, devolver NULL, ou declinar silenciosamente
  para o eager. O primeiro é bug visível; o terceiro é aceitável **se for intencional e testado**. Hoje é
  indeterminado — e uma tabela recém-criada é o caso mais comum de zero linhas.
- **Suggested fix:** no `open_streaming_source`, quando a sonda devolve `Ok(None)`, devolver `Ok(None)` da própria
  função (declinar para o eager) em vez de tentar construir o stream — uma linha, e o eager já trata o caso
  corretamente.

## SHOULD TEST

### EC-3: relação com EXATAMENTE um chunk-group (a sonda consome o único que existe)

- **Affected task:** T2.1
- **Kind:** EDGE
- **Suggested test:** `test_agregado_streaming_um_unico_chunk_group` — tabela com 5.000 linhas (< `CHUNK_GROUP_ROWS`
  = 10.000). Asserir `count(*) = 5000` e `sum(col)` byte-idêntico ao heap. **A sonda é o chunk-group nº 0**
  (`df_executor.rs:1139`), guardado em `pending: Some(probe)`; se o `pending` não for entregue, o agregado devolve
  0 linhas e o `count(*)` vira 0 em vez de 5000 — falso-verde perfeito, porque 0 é um resultado plausível.

### EC-4: linhas pendentes na MESMA transação, no caminho agregado

- **Affected task:** T2.1
- **Kind:** NEGATIVE
- **Suggested test:** `test_agregado_streaming_ve_escritas_da_propria_transacao` — `BEGIN; INSERT 3 linhas;
  SELECT count(*)`. Asserir que a contagem **inclui** as 3. Foi o **BLOCKER da rodada 6 do M168** no caminho do
  top-k, e a guarda (`has_unflushed_pending`) vive no `open_streaming_source` (`:1125`) — reusá-la deveria cobrir,
  mas o teste é o que prova. Reusa o desenho de `benchmarks/m168_pending_rows.sql`, que já tem controle positivo e
  asserção de não-vacuidade.

### EC-5: o fail-open do agregado engolindo cancelamento ou erro de integridade

- **Affected task:** T2.1
- **Kind:** NEGATIVE
- **Suggested test:** `test_agregado_fail_open_e_tipado` — asserir que só `DataFusionError::ResourcesExhausted`
  (via `find_root()`, não `match` na variante — lição da rodada 10) recua para o eager, e que
  `Execution("theodb: query canceled")` **sobe**. Foi o HIGH-1 da rodada 8 do M168: o catch-all fazia a consulta
  ignorar `statement_timeout` **e** refazer o scan inteiro.

### EC-6: um chunk-group que falha a decodificar NO MEIO do stream

- **Affected task:** T2.1
- **Kind:** NEGATIVE
- **Suggested test:** `test_agregado_streaming_erro_no_meio_nao_devolve_parcial` — injetar
  `"column chunk truncated on disk"` (`columnar.rs:906-926`) no chunk-group 5 de 10. Asserir erro tipado e **zero
  linhas devolvidas** — nunca um agregado parcial. O eager falha antes de entregar nada; o streaming falha depois
  de N batches, e a diferença importa: um `sum()` parcial é indistinguível de um `sum()` correto para quem lê.

### EC-7: grupo do GROUP BY atravessando fronteira de chunk-group

- **Affected task:** T2.1
- **Kind:** EDGE
- **Suggested test:** `test_group_by_chave_atravessa_chunk_groups` — chave cujos valores aparecem em batches
  diferentes (ex.: `GROUP BY (id % 3)` sobre 30.000 linhas = 3 chunk-groups). Asserir byte-idêntico ao heap. No
  eager todas as linhas de um grupo estão no mesmo batch; no streaming não. O `GroupedHashAggregateStream` do
  DataFusion acumula entre batches por desenho, mas isto **nunca foi exercitado** no nosso caminho — e é a forma
  de bug que o A/B do ClickBench pegaria só por acidente.

## DOCUMENT

### EC-8: o spill do DataFusion escreve no tmp do SO, fora da contabilidade do PostgreSQL

- **Kind:** NEGATIVE
- **Accepted risk:** o `DiskManager` default é `OsTmpDirectory` com 100 GB (`disk_manager.rs:34,58-64`) — fora de
  `temp_tablespaces`, fora do `temp_file_limit`, e não coberto pelo monitoramento de disco temporário do PG. O
  plano já lista "disco enche durante o spill" nos failure scenarios do T3.2; o que fica **documentado** é que um
  DBA que monitore `pg_stat_database.temp_bytes` **não verá** esse consumo. Não é escopo deste milestone resolver
  (exigiria plugar o `DiskManager` no `temp_tablespaces`), mas o artefato tem de dizer.

### EC-9: `avg(int)` / saída numeric — exata e independente de ordem, ao contrário de float

- **Kind:** EDGE
- **Accepted risk:** o risco de ULP que o blueprint nomeou vale para `float8`. Para a saída `numeric` o caminho é
  `AnyNumeric` = `numeric_div` do PG sobre `Decimal128` i128-exato (M114/M117) — **exato e associativo**, logo
  imune ao tamanho do batch. Registro aqui para que o T3.1 não gaste esforço procurando divergência onde a
  aritmética garante que não há, e para que ninguém conclua depois que "numeric não foi testado" quando a razão é
  que ele não pode divergir.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|---|---|---|---|---|---|
| T1.1 | 0 | 0 | 0 | 0 | 0 |
| T1.2 | 1 | 0 | 1 (EC-1, medição) | 0 | 0 |
| T2.1 | 4 | 4 | **1** (EC-2 já tratado) | **6** | 0 |
| T3.1 | 0 | 0 | 0 | 0 | 1 (EC-9) |
| T3.2 | 0 | 1 | 0 | 0 | 1 (EC-8) |
| T4.1 | 1 | 0 | 1 (EC-1, declaração) | 0 | 0 |

**Coverage check:** T2.1 é a única task que cruza fronteira de dados, e ela tem **4 EDGE + 4 NEGATIVE**
considerados. T1.1 é provisionamento (sem fronteira de dados própria — as falhas de I/O dela já estão nos
failure scenarios do plano). T3.1/T3.2 são gates de medição, cujas fronteiras estão declaradas no plano.

**Onde o plano JÁ cobria bem, e vale dizer:** os três defeitos mais caros do M168 — pendentes na mesma transação,
fail-open catch-all, e gate não-diferencial — já estão nomeados no plano como riscos com mitigação, e os testes
que EC-4/EC-5 pedem são a formalização deles, não descoberta nova. O plano também já declara o teto residual de
214.748 B/célula, que é o EDGE case mais importante do ADR-1.

**O que o plano NÃO tinha:** EC-1 (o `ScanPlan` O(N)) e EC-2 (tabela vazia). O primeiro é o achado material desta
revisão — ele contradiz parcialmente a alegação central do milestone e só aparece na escala que ele mira. O
segundo é o caso mais comum de todos (tabela recém-criada) e está indeterminado hoje.

**Verdict:** PLAN NEEDS ADJUSTMENT — absorver EC-1 (uma linha de instrumentação + uma frase no artefato)
antes do `/plan-confidence`. EC-2 **não precisa de código** (já tratado — ver a correção acima), só de teste.

## Correção pós-verificação (2026-07-29)

Duas afirmações desta revisão foram **verificadas contra o código** depois de escritas, e uma não sobreviveu:

| Afirmação | Veredito |
|---|---|
| EC-2: "hoje é indeterminado" se tabela vazia erra/NULL/declina | **FALSA** — declina explicitamente em `df_executor.rs:1132-1134` |
| EC-1: o `ScanPlan` materializa o diretório inteiro antes do 1º batch | **CONFIRMADA** — `columnar.rs:987` monta `Vec<StripePlan>` com `entries` completo |

**Achado NOVO que esta revisão não tinha:** `run_aggs_on_batch` (`df_executor.rs:619`) é `pub(super)` e tem
**dois chamadores fora do caminho colunar** — `arrow_cache.rs:199` e `:265` (o caminho heap-autoritativo do
M101). O T2.1 **não pode** alterar a assinatura dele; a versão streaming tem de ser um irmão, não um
refactor. Sem isso, a troca do T2.1 quebraria o cache Arrow do heap sem que nenhum teste do M169 percebesse.
