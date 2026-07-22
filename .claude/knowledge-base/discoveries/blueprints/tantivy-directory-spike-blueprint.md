# Blueprint — M139: `Directory` do Tantivy sobre block storage do Postgres (spike-gate)

> DISCOVER do M139. Pesquisa web (WebSearch/WebFetch, 2026-07-21) + estudo do ParadeDB (clone local, AGPL
> **study-only**). Objetivo: responder, com framework de medição, se o TheoDB pode ter engine lexical própria
> sobre Tantivy dentro do banco. **O risco não é o BM25 — é fazer o Tantivy viver no PG com MVCC+WAL+crash.**

## Pergunta que decide (a única)

Conseguimos implementar o trait `Directory` do Tantivy sobre páginas do Postgres, com MVCC e WAL, **sobrevivendo
a crash real com replay**? Se sim → engine lexical própria é viável (M140). Se não → NO-GO honesto, e o valor é
ter descoberto em semanas em vez de trimestres (o método que poupou o M73/M74).

## Coverage Corner 4 — Técnicas (o contrato do `Directory`)

O `Directory` do Tantivy (`quickwit-oss/tantivy` v0.26.0, **MIT** — D1 verificado via Cargo.toml) é uma
abstração **WORM** (write-once read-many). Métodos obrigatórios:

| Método | Contrato | Implicação para backend-PG |
|---|---|---|
| `open_write(path)` | writer bufferizado append-only de um segmento novo | escrever em páginas PG (relação própria ou fork), append |
| `get_file_handle` / `atomic_read` | leitura random de um arquivo (mmap no default) | ler páginas PG; sem mmap → `FileHandle` que lê via buffer manager |
| `atomic_write(path, data)` | **substitui atomicamente** o conteúdo (reads nunca veem escrita parcial) | usado p/ `meta.json`; precisa de atomicidade — via WAL ou versionamento |
| `delete` | remove arquivo | no-op + GC (preservar versões p/ leitores concorrentes) |
| `sync_directory` / `watch` / `lock` | durabilidade + notificação de mudança em meta.json + lock de escrita | `watch`/`lock` **stubáveis** sob single-writer (ver lancedb) |

**Achado decisivo (lancedb/tantivy-object-store, Apache-2.0 — D1-limpo, molde melhor que o ParadeDB AGPL):** um
`Directory` não-filesystem real que **stuba `watch` e `lock`** ("o processo de indexação garante que não há
writers concorrentes"), faz `atomic_write`/`atomic_read` por **nomes versionados** (`meta.json.{version}`,
write_version tem precedência p/ read-after-write), e **delete é no-op** (preserva versões p/ leitores; GC à
parte). Prova que Tantivy roda sobre um backend arbitrário sem suportar todos os recursos.

## O que o backend-PG exige ALÉM do object-store (a parte cara, e por que o ParadeDB forka)

O lancedb guarda blobs num object store — sem transação. Nós precisamos das **semânticas transacionais do PG**,
que é onde mora o custo (e o motivo de o `pg_search` do ParadeDB ter **105.286 LoC**, 3,2× o TheoDB inteiro):

1. **Storage em páginas PG** — o `Directory` escreve/lê via o **buffer manager** do PG (não o filesystem), em
   páginas de uma relação própria (ou `RelationGetNumberOfBlocks`/`ReadBuffer`/`GenericXLog`). É o mesmo padrão
   dos nossos index AMs (`am/hnsw_page.rs`, M35) — temos precedente próprio de page-native storage + WAL.
2. **WAL / crash-safety** — cada mutação de página passa por `GenericXLog` (ou um **rmgr próprio**, como o
   ParadeDB registra em `_PG_init` sob `shared_preload_libraries`) p/ ser replayável. Temos precedente: os AMs
   já usam `GenericXLog` (M35/M48, provados sob crash com os harnesses `isolation/crash*.sh`).
3. **MVCC** — segmentos do Tantivy visíveis conforme `xmin`/`xmax` por segmento; um leitor com snapshot anterior
   não enxerga docs de txn não-commitada. O ParadeDB tem um `MvccSatisfies` de 5 modos. Precedente próprio: o
   `theodb_columnar` já faz MVCC-via-heap-catalog (M99).

## Coverage Corner 1 — Integration/crash tests (o DoD que dá poder ao spike)

O molde é nosso: `theodb_rs/isolation/crash*.sh` (os harnesses que provaram durabilidade #46/#47 sob `SIGABRT`
real + replay do WAL). O spike DEVE reusar esse método: `SIGABRT` no meio da indexação → replay → o índice
responde consistente. E agora temos a **rede cassert do M136** (`cassert-sql-safety.yml`) — o PG `--enable-cassert`
pega violação de invariante nesta classe exata de código unsafe (a lição #1 do ParadeDB, a classe do #143).

## Coverage Corner 2 — Dependencies

- **tantivy 0.26.0** (MIT, edition 2021) — deps: rayon, serde, regex, zstd/lz4, crossbeam-channel, tantivy-columnar. D1-limpo.
- Nenhuma dep AGPL. O `deny.toml` (M136) barra AGPL transitiva automaticamente.

## Coverage Corner 3 — Tools

- pgrx 0.19 expõe `pg_sys` (buffer manager, GenericXLog) — os AMs já usam. cassert-CI (M136) é a rede.

## ADR-1 (proposto) — GO/NO-GO por protótipo mínimo, medido

**Decisão:** o veredito é dado por um **protótipo mínimo** que satisfaz, em ordem, os 4 gates do DoD:
(1) `Directory`-sobre-PG indexa N docs e responde uma busca **sem tocar o filesystem**; (2) MVCC (2º backend com
snapshot anterior não vê txn não-commitada); (3) **crash real** (`SIGABRT` + replay → consistente); (4) medição
de custo (tamanho do índice + latência vs o `pg_textsearch` do M138 no mesmo corpus). **VACUUM, merge e
paralelismo ficam declaradamente FORA** (entram no M140 se GO). Um spike que passa no caminho feliz e esconde o
custo (merge/paralelismo) é o anti-pattern que o DoD proíbe — por isso o gate (3) é crash-real, não "indexar e buscar".

**Alternativa rejeitada:** adotar o `pg_search` do ParadeDB — AGPL (D1 barra a distribuição), e reimplementá-lo
seria copiar 105k LoC. Estudamos, não copiamos (mesma postura do VectorChord).

## ADR-2 (proposto) — upstream-first; fork só se o protótipo doer

O ParadeDB **forka** o Tantivy (feature própria), o que sugere que o upstream não basta dentro de um banco. O
protótipo confirma se precisamos: se um `Directory` custom sobre o Tantivy MIT stock resolve, **não forkamos**
(anti-sunk-cost, D3). Se precisar, o veredito aciona a Política de Fork (upstream-first, diff mínimo, CI de rebase).

## Drawbacks & Risks

| # | Risco | Sev | Mitigação |
|---|---|---|---|
| R1 | O ParadeDB forka o Tantivy → upstream pode não bastar dentro de um banco → herdamos custo de fork | ALTA | protótipo confirma antes de comprometer; D3 (saída quando upstream alcançar) |
| R2 | Spike passa no caminho feliz e esconde o custo real (VACUUM/merge/paralelismo) | ALTA | DoD exige **crash real com replay**, não "indexar e buscar" |
| R3 | Tantivy embute um `tokio::Runtime` (lancedb notou) → nested-runtime panic dentro do PG | MÉDIA | `spawn_blocking`/backend síncrono; medir cedo |
| R4 | `Directory` custom sutil (atomicidade de `atomic_write`, durabilidade) | MÉDIA | versionamento (lancedb) OU WAL; single-writer stuba watch/lock |

## Veredito da discovery

**GO para o spike** (não GO para produção — é spike). A viabilidade não está descartada: temos precedente
próprio dos três ingredientes caros (page-native storage + WAL nos AMs; MVCC-via-catalog no columnar; harnesses
de crash), Tantivy é D1-limpo, e há um molde D1-limpo (lancedb) de `Directory` não-fs. O **honest boundary**: o
protótipo (implementação) é a parte de semanas — MVCC+WAL+crash sobre páginas PG é a integração transacional que
o object-store análogo NÃO cobre. Este blueprint de-risca e define o gate; a implementação é o M139 propriamente.

## Fontes

- Tantivy `Directory` trait: https://docs.rs/tantivy/latest/tantivy/directory/trait.Directory.html
- lancedb/tantivy-object-store (Apache-2.0): https://github.com/lancedb/tantivy-object-store
- Tantivy Cargo.toml (MIT, 0.26.0): https://github.com/quickwit-oss/tantivy
- ParadeDB `pg_search` (AGPL — study-only): `.claude/knowledge-base/references/paradedb/`
- Precedente próprio: `theodb_rs/src/am/hnsw_page.rs` (M35), `theodb_rs/isolation/crash*.sh` (#46/#47), `am/columnar.rs` (M99)

## Gate 1 — PROVADO (medido 2026-07-21)

`test_pg_directory_indexes_and_searches ... ok` (`cargo test`, crate standalone pgrx-free): o Tantivy 0.26
indexa 3 docs no `PgDirectory` NOSSO (não `MmapDirectory`) e recupera o doc com 'lazy' por busca de termo,
com `total_bytes() > 0` provando que o storage é o `PgDirectory`, **sem tocar o filesystem**. A impl do trait
`Directory` (`src/lexical/pg_directory.rs`, atrás da feature `spike-lexical`) compila API-correta para o 0.26.

**Achados:** (a) tantivy 0.26 integra **D1-limpo** na árvore de deps do `theodb_rs` (arrow 58/datafusion), 0
conflitos, `cargo deny` verde; (b) o núcleo é **pgrx-free** — o `cargo test` in-crate falha no LINK (símbolos
PG), então o núcleo lexical deve viver num **crate separado sem pgrx** (a direção do M140, "crate núcleo sem
pgrx") e é testável standalone. Gates 2 (MVCC), 3 (crash-real+WAL sobre páginas PG) e 4 (custo vs pg_textsearch)
seguem — são a continuação de semanas do spike.

## Gate 2 (MVCC) — PROVADO (medido 2026-07-21, PG18 real)

Arquitetura **buffer-then-flush** (forçada pelo probe de threads): o buffer é `MemStore` (pgrx-free, Tantivy
escreve de qualquer thread); `pg_backing::flush(index_id, &store)` roda na main thread pós-commit e persiste no
heap `theodb.lexical_files(index_id, path, data bytea)` — MVCC+WAL+TOAST do PG de graça (Rule 9).

Medido (função `lexical_spike_*`, instalada via `cargo pgrx install --features spike-lexical`):
- **Round-trip:** `roundtrip('lazy')=1`, `roundtrip('missing')=0` — bytes sobrevivem buffer→flush→heap→load→search.
- **MVCC cross-session (DoD literal):** sessão A faz `BEGIN; flush_only(999); pg_sleep; COMMIT`; sessão B mede
  `search(999,'lazy')` = **0 durante a txn A aberta** (snapshot de B não vê) e **1 após o COMMIT** (visível).
  `GATE2_MVCC_OK`.

**Consequência:** a integração transacional (a parte cara — 105k LoC no ParadeDB) FUNCIONA para o spike via
buffer-then-flush + heap MVCC, sem código de página/WAL custom. Falta o gate 3 (crash-real com replay — o heap
é WAL-logged, então PG recupera; o teste é `SIGABRT` mid-flush + replay) e o gate 4 (custo vs pg_textsearch).
