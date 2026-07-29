---
slug: m168-streaming-topk
target_project: theo-db
created_at: 2026-07-29
goal: Tornar o decode do top-k de projeção colunar O(chunk-group + k) em vez de O(N), fechando o item 2b do DoD do M167 e dissolvendo o falso-admit medido em #218.
---

# M168 — decode O(k) para o top-k de projeção

## Baseline (medido, não estimado)

Instrumentação já entregue (`566a263`), ClickBench 1M × 105 colunas, `work_mem = 64MB`:

| Consulta | linhas decodificadas | batch Arrow |
|---|---|---|
| q23 `SELECT *` + LIKE + ORDER BY … LIMIT 10 | 1.000.000 | **809.738.352 B (772,2 MiB)** |
| q24 projeção estreita | 1.000.000 | 20.388.828 B |
| q25 chave de texto | 1.000.000 | 12.388.732 B |
| q26 multi-chave | 1.000.000 | 20.388.828 B |

Guard do ADR-4: mede 217,7 MiB (disco, comprimido), orçamento 512 MiB, **admite** um decode de 772,2 MiB —
1,51× acima do próprio teto, porque o Arrow expande 3,55× (#218).

## Goal

Depois do M168, o pico decodificado do caminho top-k é **um chunk-group + k linhas**, independente de N — medido
pelo mesmo instrumento que produziu a tabela acima.

## Prior art (blueprint `m168-streaming-topk-blueprint.md`)

- `SortExec` com `Some(fetch)` já consome o input **em stream** e insere num heap limitado — `references/datafusion/datafusion/physical-plan/src/sorts/sort.rs:1369-1371`.
- `StreamingTable` (`catalog/src/streaming.rs:36`) e `PartitionStream` (`physical-plan/src/streaming.rs:49`) são públicos. `TopK` **não** é (`physical-plan/src/lib.rs:64` — `mod topk;`).
- `decode_columns_v2` já itera stripe × chunk-group (`am/columnar.rs:937-985`); acumula em vez de emitir.

## ADRs

### ADR-1 — usar o `SortExec: TopK` do DataFusion, não um heap próprio

**Decisão:** alimentar `sort().limit(0,k)` com um `PartitionStream` preguiçoso.
**Alternativas:** (a) heap próprio sobre chunk-groups — rejeitada: reimplementaria a comparação multi-chave com
semântica de NULL e colação idêntica à do DataFusion, exatamente onde o M167 achou dois furos de correção
(Regra 9); (b) duas passagens (chave → corte → payload) — continua O(N) na coluna-chave, não cumpre o DoD; fica
como fallback se ADR-2 der não-viável.

### ADR-2 — afinidade de thread asseverada em runtime, não comentada

`PartitionStream: Send + Sync`, e o stream carrega `pg_sys::Relation`. O runtime é `new_current_thread` +
`block_on` com `target_partitions(1)`, então `unsafe impl Send + Sync` é verdadeiro **hoje**. Um comentário não
protege disso: trocar para `new_multi_thread` viraria corrupção silenciosa, não erro de compilação.

**Decisão:** capturar `std::thread::current().id()` na construção do stream e assertar a cada `poll_next`;
divergência = `panic!` imediato. Precedente da mesma classe: M139 (Tantivy chamando `Directory` de 4 threads).

### ADR-3 — o caminho existente não muda

`decode_columns_v2` continua como está. Extrai-se `decode_one_chunk_group` como helper compartilhado; a função
antiga passa a chamá-lo com acumuladores globais (byte-idêntica), a nova com acumuladores por chunk-group. DRY
sobre o *conhecimento* (como decodificar um chunk-group), sem tocar o comportamento provado.

## Fases

### Fase 1 — extrair o helper, sem mudança de comportamento

**T1.1** `decode_one_chunk_group` extraído; `decode_columns_v2` reescrita em termos dele.
**TDD:** o oráculo `m158_ec_harness.sql` (20 asserções) e o `m167_hits_topk_ab.sql` (15 asserções, gate H0) devem
passar **inalterados** — é refactor byte-idêntico. Um `*_mism` != 0 aqui reprova a fase.

### Fase 2 — o stream e a asserção de afinidade

**T2.1** `ChunkGroupStream` implementando `PartitionStream`, com `unsafe impl Send + Sync` justificado e a
asserção de `ThreadId` do ADR-2.
**TDD (RED):** um teste que constrói o stream numa thread e o poll numa outra DEVE entrar em panic com a mensagem
de afinidade. Sem esse teste vermelho antes, a asserção não está provada.

### Fase 3 — rotear o top-k pelo stream

**T3.1** `run_columnar_topk` usa `StreamingTable` em vez de `read_batch`.
**TDD:** os dois oráculos passam; o `EXPLAIN` do plano DataFusion mostra `SortExec: TopK(fetch=k)`.

### Fase 4 — provar

**T4.1** Pico decodificado medido pelo mesmo instrumento. **Critério:** o `bytes` do trace por batch cai de
809.738.352 para ≤ um chunk-group + k, e o número de traces passa a ser > 1 (prova de que houve stream).
**T4.2** A/B pareado no mesmo binário (harness `m167_paired_ab.sql`): throughput **não pode** regredir mais que
o piso de ruído da box (1,88×, medido no M167). Um decode O(k) 3× mais lento troca um problema por outro —
esse é o critério de honest-negative.

## Riscos declarados

- **`StreamingTable` pode exigir `'static`** no stream. Não verificado ainda; se exigir, é `Arc` + interior mutability, e o custo disso entra no ADR-2.
- **Custo por batch.** Atravessar o DataFusion N vezes em vez de 1 tem overhead fixo por batch. Se o chunk-group for pequeno demais, o overhead domina — pode ser preciso agrupar chunk-groups até um alvo de bytes.
- **A relação precisa continuar aberta** durante todo o consumo do stream. Hoje o decode termina antes do DataFusion começar; com stream, o `pg_sys::Relation` vive durante o `block_on`.

## Definition of done

- [ ] Pico decodificado independente de N, medido pelo instrumento do `566a263`
- [ ] Ambos os oráculos passam sem alteração (byte-identidade preservada)
- [ ] Teste de afinidade de thread vermelho antes, verde depois
- [ ] `EXPLAIN` mostra `SortExec: TopK(fetch=k)`
- [ ] A/B pareado sem regressão de throughput acima do piso de ruído
- [ ] CHANGELOG + issue #215/#218 atualizadas com o resultado (inclusive se for honest-negative)
