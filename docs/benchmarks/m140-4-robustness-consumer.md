# M140.4 — robustez (crash/VACUUM/MVCC) + probe #153 + consumidor theo-lens (medido)

> Medido 2026-07-22 no e2e-runner (165.227.121.20, 32GB), PostgreSQL **18.4** (pgrx 0.19.0), extensão `theodb_rs`
> (feature `spike-lexical`) instalada via `cargo pgrx install` — a validação contra o **binário shipado** (o
> `cargo pgrx test` não linka neste ambiente; padrão M99/M135/CI-cassert). Evidências:
> `docs/benchmarks/m140-4-data/{robustness,consumer}-evidence.txt`. Scripts: `scripts/m140-4-lexical-robustness.sh`,
> `scripts/m140-4-consumer-theolens.sh`. Probe: `theodb_rs/lexical_core/src/probe.rs`. ADR: `docs/adr/0055`.

## Headline

**A engine BM25 de produção é robusta (crash/VACUUM/MVCC provados no binário shipado), tem a disciplina de
thread-safety #153 travada por construção, e é consumida pelo theo-lens (proof e2e + wiring testada).** Fecha o
M140; alimenta o anchor do M141 (dogfood `running`).

## Evidência 1 — robustez contra o binário shipado (`m140-4-lexical-robustness.sh`)

```
antes do crash: bm25_search(300,'blk_crashzeta') top id = 2   [esperado 2]
servidor caiu (esperado)
WAL replay ocorreu no restart
após crash+replay: top id = 2   [esperado 2]
CRASH_OK
VACUUM: n_dead_tup antes=24 depois=0 ; busca pós-VACUUM hits=10
VACUUM_OK
OK   MVCC RR: A (snapshot antigo) NÃO vê o build de B = 0
OK   MVCC RC: A vê o build de B no próximo statement (cache invalida no snapshot novo) = 1
M140_4_ROBUSTNESS_OK
```

| Gate | O que prova |
|---|---|
| **CRASH_OK** | Um `bm25_build` COMMITADO sobrevive a `SIGABRT` + replay do WAL — o heap `lexical_files` é WAL-logged (herdado, M139 gate 3). Crash pré-commit = abort (índice antigo visível). |
| **VACUUM_OK** | Após 4 rebuilds (cada um DELETA as linhas antigas), `VACUUM theodb.lexical_files` recupera as **24 tuplas mortas → 0** sem corromper o índice vivo (busca pós-VACUUM: 10 hits). |
| **MVCC RR** | Leitor com snapshot antigo (REPEATABLE READ) **NÃO** vê o build de outra sessão (0) — o cache é keyed pela geração que o snapshot enxerga. |
| **MVCC RC** | Sob READ COMMITTED, o próximo statement do leitor vê o build (1) — o cache invalida corretamente no snapshot novo (não serve stale). |

O **straddle SPI** (M140.3 review LOW + M140.4 review HIGH) fica fechado: `read_generation` foi corrigido para
usar `Spi::connect(|c| c.select(...))` — SPI **read-only** (o M140.4 review provou que `Spi::get_one` NÃO é
read-only em pgrx 0.19: marca a txn mutável → snapshot fresco por statement + quebra em replica). Com `c.select`,
`read_generation` e `load` reusam o ActiveSnapshot da statement → tag do cache == conteúdo carregado, sob RC e RR;
e `bm25_search` roda em read replica sem burn de XID (ADR-0055 D3).

## Evidência 2 — probe de thread-safety #153 (`probe.rs`, `cargo test -p theodb_lexical`)

`test_directory_calls_recorded_and_store_is_pgrx_free` + `test_recorded_index_is_searchable_over_injected_store`
(2/2 verdes; 13 no núcleo total). O `ThreadRecordingStore` registra as threads que chamam o `SegmentStore` num
build multi-thread real do Tantivy. A prova é **estrutural**: o store no caminho das threads vive no crate
**pgrx-free** (`cargo tree -p theodb_lexical | grep -c pgrx == 0`) → é **impossível** por construção tocar o PG
(SPI/pg_sys) de qualquer worker thread. Uma regressão que ponha SPI numa thread teria de sair do núcleo → pega no
gate zero-pgrx + review. O `panic="unwind"` (o outro item #153) já é gateado (M140.2).

## Evidência 3 — consumidor theo-lens (`m140-4-consumer-theolens.sh` + `trace-bm25-search.test.ts`)

```
OK   bm25_build indexa os spans do theo-lens = 5
OK   bm25_search retorna o TRACE certo p/ o termo distintivo 'blkfoxzz' = trace-bbb
OK   bm25_search casa o trace de tool Claude Code = trace-ddd
OK   bm25_search casa o trace por query natural multi-termo = trace-aaa
CONSUMER_OK — theo-lens shape (input||output do span) busca traces via bm25_search
```

O SHAPE real do theo-lens (relevância de traces por `input_value || output_value` do span, hoje `ts_rank` em
`trace-read-repository.ts`) sobre `bm25_search` retorna o **trace correto**. A wiring no theo-lens
(`trace-bm25-search.ts` — `searchBm25(db, indexId, query, k)`, ADITIVO, `ts_rank` default intocado) é testada por
unit test (4/4, mock `db.execute` — roda no CI normal do theo-lens, não é dead-code).

## Boundary honesto (consumidor-proof vs M141)

M140.4 entrega o consumidor **provado e ligado** (o proof e2e + a wiring testada). O **cutover de produção** — o
`listTraces` do theo-lens usar `bm25_search`, a manutenção do índice bm25, e os **30 dias** de uso sustentado — é
explicitamente o **M141** (dogfood `running`, `dogfood-golden-rule.md`). Não reivindicamos "consumidor em
produção" antes do M141; aqui a engine está provada robusta e o consumidor está preparado.

## Consequência para o roadmap

- **Gate M140.4 PASSA** — MVCC/VACUUM/crash provados no binário shipado, #153 travado por construção, consumidor
  theo-lens provado+ligado. **Fecha o M140** (M140.1→M140.4 completos).
- Próximo: **M141** (dogfood `running`) — o cutover do theo-lens + os 30 dias, usando esta engine.

## Reprodução

```bash
# no e2e-runner (pgrx 0.19 + PG18):
cd theodb_rs && cargo pgrx install --features spike-lexical --pg-config ~/.pgrx/18.4/pgrx-install/bin/pg_config
bash scripts/m140-4-lexical-robustness.sh     # CRASH_OK + VACUUM_OK + MVCC RR/RC
bash scripts/m140-4-consumer-theolens.sh      # CONSUMER_OK
cargo test -p theodb_lexical probe            # probe #153 (stock, zero-pgrx)
# no theo-lens:
npx vitest run packages/core/src/infrastructure/db/observability/trace-bm25-search.test.ts
```
