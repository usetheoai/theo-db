# Blueprint de implementação — M56 fase 1: tombstone in-place no `theodb_hnsw`

Slug: `inplace-maintenance-tombstone` · milestone_id: M56 · Date: 2026-07-07 · Verdict: SHIPPABLE_WITH_CAVEATS

Escopo = fase 1 do ADR 0017 (tombstone-only, SEM RepairGraph). council-index-storage leu o layout real, os dois entrypoints de VACUUM, o scan, o fold, os locks, e os peers pgvector/pgvectorscale. Todas as citações são `file:line` reais.

## Achado load-bearing: o layout v2 JÁ tem espaço para `deleted`+`version` sem re-empacotar

O header da element-tuple (`hnsw_page.rs:33-41`): `E_TAG=0`, `E_LEVEL=1`, **bytes 2..4 = pad (sempre-zero, "keeps the i64 tid 4-aligned")**, `E_TID=4`, ... `E_VEC=20`. `encode_element` nunca escreve os bytes 2-3. **Cabem `deleted` (byte 2) + `version` (byte 3)** — espelha `HnswElementTupleData` do pgvector (`hnsw.h:361-362`). Consequências:
- Tamanho da tuple **inalterado** (`elem_size` intocado); endereços analíticos de `pack_at` inalterados; `elems_per_page` inalterado.
- **Editável in-place** via `GenericXLog` (sem `PageIndexTupleOverwrite`) — igual pgvector (`hnswvacuum.c:687-689`).
- **Retrocompatível na leitura sem REINDEX** — índice v1/v2 tem pad=0 → deleted=0/version=0 (vivo). **REINDEX é OPCIONAL** (só necessário para ativar o `deleted_count` no meta, gatilho de compaction por ratio).

## Coverage Corner 4 — Techniques (o plano de implementação)

### 1. Layout + version (aditivo, sem magic bump)
- Element tuple: `E_DELETED=2`, `E_VERSION=3` (pad livre). `decode_element` expõe `deleted:bool`+`version:u8` no `ElementView` (`hnsw_page.rs:194-224`). Sem mudança de tamanho, sem bump.
- Meta v3 aditivo: trailer `deleted_count:u32` (+ reservado `insert_page:u32` p/ fase 2). Novo `HNSW_STRUCT_VERSION_TOMBSTONE=3`; decode trata v1/v2/v3 com default. Precedente exato do SBQ v2 (M51).
- REINDEX opcional; CHANGELOG `Changed`.

### 2. DELETE (`ambulkdelete`) — sweep in-place por página
Substitui `vacuum_rebuild` O(N)-RAM sob EXCLUSIVE. Varre `elem_first..elem_first+elem_npages` (iteração de `enumerate_entries:462`); por página: `ReadBuffer`+`LockBuffer(EXCLUSIVE)`+`GenericXLog` (molde `try_add_to_page:161-194`); por item dead (callback `ambulkdelete:176`): escreve `E_DELETED=1`+bump `E_VERSION` in-place; `GenericXLogFinish`. **Elimina o advisory EXCLUSIVE index-wide** → fim da parada-total (86s→janela por-página). **NÃO zera o vetor** (tombstone-only navega PELO nó → precisa do f32 p/ navegação; zerar quebraria recall — diferença crítica vs pgvector, que repara e zera). `deleted_count` gravado 1× ao fim via `pivot_meta_page`. Crash-safe: cada página é um GenericXLog atômico; crash → dead não-tombstoned pego pelo heap-recheck (backstop); re-VACUUM completa. Sem nova janela de REINDEX.

### 3. Scan filtra tombstones — navega-através-mas-não-emite
`Cand` (`hnsw_page.rs:481-489`) ganha `deleted:bool` decodificado no `load`. O walk (`ann/scan_core.rs::ground_search_nodes:117-155`): tombstone **entra** em visited/cands e é expandido (preserva conectividade — nós vivos ainda o apontam; sem isso o grafo desconecta e o recall despenca), mas é **excluído do result set** (pular no push + no rerank SBQ `hnsw_page.rs:687`). Entry point tombstone continua válido p/ navegação, só não emite. Custo O(#tombstones no caminho). **Backstop de correção:** o executor recheca visibilidade no heap (`scan.rs:308-311`) → leitura stale é segura por construção.

### 4. Reúso de slot no aminsert — DEFERIDO à fase 2 (divergência honesta do ADR)
O `HnswFreeOffset` do pgvector (`hnswinsert.c:44-116`) reusa o slot **religando o nó no grafo** = mutação in-place do grafo = quebra a imutabilidade M35 que a fase 1 preserva. É **fase 2**. Na fase 1, `aminsert` fica pending-append (inalterado); o reclaim de slots tombstoned é via **compaction** (item 5). `version` é gravado (byte livre) como **hook forward-compat**, NÃO load-bearing na fase 1 (o iterative scan do M52 re-traversa+dedup por tid, não cacheia endereços — `scan.rs:315-339`).

### 5. Compaction — reusa `fold.rs` do M48
O compaction JÁ existe (`vacuum_rebuild_hnsw_structured` → `fold::fold`, meta-pivot atômico). Mudanças mínimas: `enumerate_entries` filtra `!ev.deleted` (dropa tombstones no rebuild); `amvacuumcleanup` (`mod.rs:196`) dispara fold também quando `deleted_count/node_count > X%` (novo GUC `theodb.hnsw_tombstone_compact_ratio`, molde `vacuum_pending_threshold`). Meta-pivot continua o único rewrite crash-safe.

### 6. Teto de memória do BUILD — parcial, com limitação honesta (Rule 3)
`collect_corpus` streaming/batched via o callback (`build_callback:118`) remove **uma** cópia O(N) (o corpus Vec). MAS o `HnswIndex` em RAM é inerentemente O(N) (`self.vectors.push`, `hnsw.rs:112`) — o piso O(N) do grafo **só cai com um builder on-disk** (fora da fase 1). Escopo honesto do item: "remover cópias redundantes", declarando que o piso O(N) permanece (a estimativa 3-10 GB @1M do M55 NÃO é fechada por isto — merece milestone próprio, ADR já separa M-impl-2).

## Coverage Corner 1 — Integration tests (fatiar testável do FFI)
- **Puro `#[test]` (CI):** decode/encode roundtrip do `deleted`+`version`; `is_tombstone`; `decode_meta` v3 roundtrip + rejeição de trailer truncado; filtro de tombstone em `ground_search_nodes` via `MemNeighborSource`+`MemNode.deleted` (navega-através-não-emite vs oráculo brute-force).
- **`#[pg_test]` (FFI, local não-CI):** `mark_tombstone_on_page` sob GenericXLog (re-read vê deleted=1); `ambulkdelete` sem advisory EXCLUSIVE; scan pós-tombstone dropa dead + mantém vivos; build→restart→scan idêntico.
- **e2e crash-injection:** reusa o GUC de crash do M48 (`fold.rs:76`); crash no meio do sweep → recover → dead filtrados/vivos intactos/re-VACUUM completa/sem REINDEX.
- **Concorrência (o win observável):** scan-em-loop vs DELETE concorrente → ausência de parada-total (o muro M55) + correção. Harness `run_m55_vacuum_wall.py`.
- **Recall (decide a fase 2):** tombstone X% aleatório sem compaction → recall@k vs rebuild-limpo. Degrada > limiar → dispara fase 2. **Primeira coisa a medir.**

## Coverage Corner 2/3 — Deps/Tools
Zero deps novas (reusa GenericXLog, `try_add_to_page` molde, fold.rs, pivot_meta_page). GUC de crash M48; harness M55; `THEODB_SCAN_PROFILE` p/ `pages_read` incluindo tombstones (wiring metric).

## ADRs / decisões do owner antes de implementar
1. **Item 4** (slot-reuse no insert é fase-2, não fase-1) — CONFIRMADO deferir (preserva M35; alinha com pgvectorscale tombstone-only). O DoD "reúso de slot no aminsert" era over-specified (modelo pgvector, não pgvectorscale) → o reclaim da fase 1 é via **compaction**.
2. **Item 6** (BUILD streaming remove cópias, não o piso O(N) do grafo) — escopo honesto.

## Riscos
1. Recall entre compactions (nós fantasma desviam a busca; nosso grafo pode não ter α-pruning do DiskANN) → medição decide fase 2.
2. `deleted_count` stale após crash → inócuo (só heurística), documentar.
3. Concorrência sweep vs scan → buffer SHARE/EXCLUSIVE serializa por página; stale seguro por heap-recheck; `with_page_item` libera pin por read.

## Arquivos
`hnsw_page.rs` (:33-41,:194-224,:456-474,:481-563,:604-711), `ann/scan_core.rs` (:23-42,:117-155), `build.rs` (:28-45,:118-166,:219-266), `mod.rs` (:161-188,:196-225), `page.rs` (:161-194,:337-360), `fold.rs` (:56-100), `lock.rs`. Peers: `references/pgvector/src/hnswvacuum.c`, `hnswinsert.c`, `hnsw.h:357-367`; `pgvectorscale/.../plain/node.rs:60,128`.
