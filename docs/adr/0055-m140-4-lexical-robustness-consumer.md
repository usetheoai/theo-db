# ADR 0055 — Robustez da engine lexical (contra o binário shipado) + boundary do consumidor theo-lens

- **Status:** Aceito
- **Data:** 2026-07-22
- **Milestone:** M140.4 (MVCC/VACUUM/crash provados + primeiro consumidor)
- **Relacionado:** ADR-0051 (spike M139), ADR-0052 (heap), ADR-0053 (núcleo pgrx-free), ADR-0054 (supersede pg_textsearch), issue **#153**, `dogfood-golden-rule.md` (o anchor do M141).

## Contexto

M140.3 (v0.128.0) entregou a superfície BM25 de produção (`bm25_build`/`bm25_search`) com cache MVCC-correto —
provado por um smoke de 2 sessões. Este milestone **prova a robustez de produção** contra o binário shipado (o
padrão M99/M135), instala a disciplina de thread-safety #153 como regressão, e liga o **primeiro consumidor real**
(theo-lens). O review do M140.3 deixou um LOW rastreado (straddle SPI sob RC) resolvido aqui.

## Decisões

**D1 — Robustez provada contra o BINÁRIO SHIPADO.** `scripts/m140-4-lexical-robustness.sh` sobe um cluster real
(`cargo pgrx install`) e prova os três, medido (`docs/benchmarks/m140-4-data/robustness-evidence.txt`):
- **CRASH_OK** — um `bm25_build` COMMITADO sobrevive a `SIGABRT` + replay do WAL (o heap `lexical_files` é
  WAL-logged; `bm25_search` retorna o mesmo id antes/depois).
- **VACUUM_OK** — após 4 rebuilds (cada um DELETA as linhas antigas), `n_dead_tup` do `lexical_files` cai de 24→0
  no `VACUUM`; a busca segue correta (sem corrupção do índice vivo).
- **MVCC RR+RC** — sob REPEATABLE READ, um leitor com snapshot antigo NÃO vê o build de outra sessão; sob READ
  COMMITTED, o próximo statement do leitor vê o build (o cache invalida no snapshot novo). Os dois provam o cache
  MVCC-correto.

**D2 — Disciplina #153 como regressão estrutural (probe de threads).** `theodb_lexical::probe::ThreadRecordingStore`
(test-only, núcleo pgrx-free) registra as threads que chamam o `SegmentStore` num build multi-thread real do
Tantivy. A prova é **estrutural**: o store no caminho das threads vive no crate pgrx-free → é **impossível** por
construção tocar o PG (SPI/pg_sys) de qualquer worker thread (o crate não linka pgrx). Uma regressão que ponha SPI
numa thread teria de mover código para fora do núcleo → pega no gate `cargo tree | grep -c pgrx == 0` + review. O
outro item #153 (`panic="unwind"`) já é gateado (M140.2 review).

**D3 — Fecha o straddle SPI do M140.3 review LOW (leituras read-only via `c.select`, NÃO `get_one`).** O review
do M140.4 (council-rust-pgrx) provou que `Spi::get_one`/`get_one_with_args` em pgrx 0.19 são `connect_mut`/
`update` → `mark_mutable` → `read_only=false`: abrem um snapshot **fresco por statement** (reabrindo o straddle)
E marcam a txn mutável (quebra em read replica + queima um XID por busca). **A correção:** `read_generation` usa
`Spi::connect(|c| c.select(...))` — SPI **read-only**, sem `mark_mutable`. Assim, `read_generation` E o `load`
(também `c.select`, `pg_backing.rs`) reusam o **ActiveSnapshot da statement** sob RC e RR → geração lida e bytes
carregados consistentes (tag do cache == conteúdo); nenhum snapshot fresco entre as duas leituras. Bônus:
`bm25_search` roda em **read replica** sem burn de XID. Provado pelo eixo MVCC (RR+RC) do script de robustez.

**D4 — Consumidor theo-lens: PROOF + wiring testada agora; CUTOVER de produção = M141.** M140.4 entrega:
- **Proof e2e** (`scripts/m140-4-consumer-theolens.sh`, `docs/benchmarks/m140-4-data/consumer-evidence.txt`,
  **CONSUMER_OK**): o SHAPE real do theo-lens (relevância de traces por `input_value || output_value` do span, hoje
  `ts_rank` em `trace-read-repository.ts`) sobre `bm25_search` retorna o **trace correto** para termo distintivo,
  termo de tool Claude Code, e query natural multi-termo.
- **Wiring testada no theo-lens** (`packages/core/.../trace-bm25-search.ts` + `.test.ts`, 4/4 verdes): o helper
  `searchBm25(db, indexId, query, k)` — ADITIVO, o `ts_rank` default do `listTraces` **intocado**. O unit test
  mocka `db.execute` (não requer TheoDB) → roda no CI normal do theo-lens (não é dead-code).
- O **cutover de produção** (o `listTraces` usar `bm25_search` + a manutenção do índice bm25 + os **30 dias** de
  uso sustentado) é o **M141** (dogfood `running`). Honestidade: não reivindicamos "consumidor em produção" antes
  do M141; aqui o consumidor está **provado e ligado** (a wiring existe e é testada), alimentando o anchor do M141.

## Rationale

- **Binário shipado (D1):** "green unit suite is not enough" (M99/M135) — a robustez só vale provada no binário que
  ships; o `cargo pgrx test` não linka neste ambiente (M139), então a validação é via extensão instalada + SQL (o
  mesmo do CI cassert).
- **Separação estrutural (D2):** materializa a convenção #153 numa garantia que o compilador+gate impõem, não em
  code-review manual.
- **Read-only fecha o straddle (D3):** sem restruturar o cache; a invariante tag==conteúdo é airtight porque
  `read_generation` e `load` usam `c.select` (SPI read-only, sem `mark_mutable`) → reusam o ActiveSnapshot da
  statement. (O M140.4 review corrigiu a premissa falsa de que `Spi::get_one` seria read-only — não é.)
- **Proof-agora / cutover-M141 (D4):** o dogfood de 30 dias é explicitamente o M141 (`dogfood-golden-rule.md`);
  colapsar o cutover aqui violaria o boundary. A wiring testada + o proof e2e são a evidência que o M141 consome.

## Alternativas consideradas

- **pg-tests via `cargo pgrx test`** — rejeitado (não linka; M139/M140.3).
- **Cutover total do theo-lens agora** — rejeitado: é o escopo do M141 (30 dias de dogfood).
- **Editar o `listTraces` do theo-lens agora com o caminho bm25** — rejeitado: adicionaria código não-exercido ao
  caminho quente antes do cutover (dead-code liability); a wiring self-contained + testada é a forma honesta.
- **Restruturar o cache p/ ler geração+heap numa query só** — desnecessário: o fix `c.select` read-only já garante
  a consistência tag==conteúdo (ambas as leituras reusam o ActiveSnapshot da statement). Seria complexidade extra
  sem benefício medido.

## Consequências

- **Habilita:** a engine BM25 é reivindicável **robusta** (crash/VACUUM/MVCC provados no binário shipado) e
  **consumida** (theo-lens wiring testada + proof e2e). O M141 (dogfood) faz o cutover e mede os 30 dias.
- **Restringe:** o consumidor de produção só é reivindicável após o M141; até lá, o `ts_rank` é o default do theo-lens.
- **Rastreia:** flush-sob-merge em escala (#153) segue como risco residual documentado; o crash+VACUUM+probe cobrem
  a disciplina no regime testado.

## Referências

- ADR-0051/0052/0053/0054 (a cadeia M139→M140.3), issue #153 (a disciplina), `dogfood-golden-rule.md` (M141).
- `scripts/m140-4-lexical-robustness.sh`, `scripts/m140-4-consumer-theolens.sh`, `theodb_rs/lexical_core/src/probe.rs`.
- `docs/benchmarks/m140-4-robustness-consumer.md` (o report com as 3 evidências).
- theo-lens `packages/core/src/infrastructure/db/observability/{trace-bm25-search.ts,trace-read-repository.ts}`.
