# ADR 0051 — M139 gate 2/3: design do backend de páginas PG para o `Directory` do Tantivy

- **Status:** Aceito — VEREDITO GO (todos os 4 gates medidos em PG18 real, 2026-07-22)
- **Data:** 2026-07-21
- **Contexto:** M139 spike. Gate 1 PROVADO (`PgDirectory` custom, backend blob em memória, indexa+busca sem
  filesystem — `src/lexical/pg_directory.rs`, 5/5 testes). Este ADR desenha os gates 2 (MVCC) e 3 (crash-real+WAL),
  que sobem o storage do blob-em-memória para **páginas do Postgres**. Grounded em precedente PRÓPRIO (Rule 9).

## Decisão

O backend de páginas do `Directory` **reusa `am/page/mod.rs`** (o primitivo de blob-sobre-páginas-WAL já provado
nos AMs, M35/M99), e a visibilidade MVCC segue o **padrão MVCC-via-catálogo do `theodb_columnar` (M99)**. Não se
reinventa storage nem WAL nem MVCC — compõe-se o que já existe.

### 1. Storage: cada arquivo Tantivy = um blob sobre páginas WAL-logged

O trait `Directory` é WORM (write-once). Cada "arquivo" (segmento `.store`/`.term`/…, e o `meta.json`) vira um
blob persistido pelo primitivo existente:

- `open_write` → bufferiza; no `terminate`, escreve o blob via `am::page::write_pages(rel, &blob, MAIN_FORKNUM)`
  (extend + `GenericXLog`, crash-safe — o mesmo caminho do `ambuild`).
- `get_file_handle`/`atomic_read` → `am::page::read_blob(rel, start_block)` (share-locked).
- `atomic_write(meta.json)` → substituição atômica via `GenericXLog` (um único `GenericXLogFinish` publica a
  troca; reads nunca veem meia escrita — a atomicidade é a do WAL, não a de rename de fs).

Cada índice lexical tem uma **relação própria** (como os AMs têm a index relation); um diretório de arquivos
(nome → bloco inicial) vive numa página meta (bloco 0), padrão idêntico ao `am/page` (meta em block 0).

**Rule 9:** `am/page` já faz extend/read/WAL/pending. O backend do Directory é um *adaptador* dele ao contrato
do trait, não uma reimplementação.

### 2. MVCC: segmentos visíveis por snapshot (padrão columnar M99)

Tantivy versiona por `meta.json` (lista de segmentos). A visibilidade transacional vem de um **catálogo de
segmentos** com `xmin`/`xmax` (heap catalog, como o `theodb_columnar` faz para stripes, M99): um leitor com
snapshot anterior lê a lista de segmentos visível ao SEU snapshot (não os de txn não-commitada). O molde de
single-writer (lancedb) permite stubar `watch`/`lock` — o writer é a própria txn.

- Gate 2 (o teste): um 2º backend com snapshot anterior **não** enxerga docs de txn não-commitada; após commit, enxerga.

### 3. Crash-real + WAL replay (gate 3)

Reusa o harness `theodb_rs/isolation/crash*.sh` (provou durabilidade #46/#47): `SIGABRT` no meio da indexação →
replay do WAL (o `GenericXLog` do `am/page` é replayável por construção) → o índice responde consistente. E a
**rede cassert do M136** (`cassert-sql-safety.yml`) pega violação de invariante nesta classe de código.

### 4. Testabilidade (a barreira pgrx)

O gate 1 provou que o núcleo (`Directory` + trait de storage) é **pgrx-free** e testável standalone. O backend
de páginas é pgrx-bound (`pg_sys`), testável via `cargo pgrx test` OU um teste SQL de integração num PG real
(padrão dos AMs). A separação núcleo-pgrx-free ↔ backend-pgrx é a semente do **crate núcleo do M140**.

## Achado empírico (2026-07-21) que CORRIGE o design: buffer-then-flush é obrigatório

Um experimento (`probe_which_threads_call_directory`, crate standalone) mediu de quais threads o Tantivy chama
o `Directory`: **mesmo com `writer_with_num_threads(1)`, os `write` vêm de 4 threads distintas, não só a main.**
O Tantivy usa threads (rayon/merge/background) que chamam os métodos do `Directory` diretamente.

**Consequência dura:** SPI e o buffer manager do PG são **backend-thread-only** — chamá-los de uma thread do
Tantivy **crasharia o backend** (a classe exata de bug que este spike existe para pegar). Logo, o `SegmentStore`
de páginas PG **NÃO pode** tocar o PG no `write`. A arquitetura correta é **buffer-then-flush**:

1. Durante a indexação, o `SegmentStore` bufferiza em memória (thread-safe, ZERO chamadas PG) — qualquer thread.
2. Após `writer.commit()` RETORNAR (main thread), um `flush_to_pg(index_id)` persiste os arquivos bufferizados
   na tabela heap `bytea` via SPI — operação de main thread, dentro da txn corrente.
3. Ao abrir/reabrir, `load_from_pg(index_id)` (main thread) popula o buffer a partir do heap; o Tantivy lê do buffer.

Isto é o que o ParadeDB resolve com arquitetura pesada; para o spike, o buffer-then-flush sobre uma tabela heap
`bytea` reusa **toda** a máquina do PG (TOAST + MVCC + WAL) sem código de página/WAL custom — mais parsimonioso
que a proposta original de `am/page` (que fica como otimização de gate 4 se o TOAST for lento demais).

## Alternativas rejeitadas

- **Copiar o `MVCCDirectory` do ParadeDB** (105k LoC, AGPL) — D1 barra; estudamos, não copiamos.
- **rmgr próprio** (como o ParadeDB registra em `_PG_init`) — `GenericXLog` já resolve a crash-safety sem um
  resource manager custom; só subir para rmgr se um gate provar que o `GenericXLog` não basta (anti-YAGNI).
- **Reimplementar storage/WAL** — Rule 9: `am/page` já existe e é provado sob crash.

## Consequências

- **GO condicional:** os gates 2/3 são implementáveis compondo `am/page` + o catálogo MVCC do columnar. O risco
  residual real é o custo (merge/paralelismo — gate 4 e M140), não a viabilidade do storage.
- **Esforço:** semanas (a integração transacional), não uma sessão — declarado honestamente no blueprint.

## Referências

- Gate 1 provado: `theodb_rs/src/lexical/pg_directory.rs`; blueprint `tantivy-directory-spike-blueprint.md`.
- Precedente de storage+WAL: `theodb_rs/src/am/page/mod.rs` (`write_pages`/`read_blob`/`extend_with_item`, `GenericXLog`).
- Precedente de MVCC-via-catálogo: `theodb_rs/src/am/columnar.rs` (M99).
- Harness de crash: `theodb_rs/isolation/crash*.sh` (#46/#47). Rede cassert: `.github/workflows/cassert-sql-safety.yml` (M136).
