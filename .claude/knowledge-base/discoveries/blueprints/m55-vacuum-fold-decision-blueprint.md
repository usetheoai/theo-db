# Blueprint M55 — Manutenção do índice HNSW a escala: fold incremental vs in-place vs híbrido

Slug: `vacuum-fold-decision` · milestone_id: M55 · Date: 2026-07-07 · Verdict: SHIPPABLE_WITH_CAVEATS

Milestone de **DECISÃO+MEDIÇÃO** (não implementação — a implementação decorrente ganha milestone próprio via `/roadmap-feature` após o ADR). Síntese de dois discovery agents (council-index-storage: mecânica fold-vs-in-place + evidência dos peers; council-benchmark: design da medição baseline). Todas as citações são `file:line` reais.

## Coverage Corner 4 — Techniques (o muro + as 3 opções, SOTA-anchored)

### Baseline honesto do TheoDB hoje (o muro)
O VACUUM entra por `vacuum_rebuild` (`build.rs:171`) → `vacuum_rebuild_hnsw_structured` (`build.rs:219`): `enumerate_entries` puxa **todas** as element-tuples para um `Vec` O(N) em RAM (`build.rs:228`), filtra dead (`:236`), e `HnswIndex::build_cancellable(&live, …)` **reconstrói o grafo inteiro do zero** (`:239`). Há **duas+ cópias O(N) vivas no pico** (o `Vec` de enumerate + a estrutura do rebuild + o `pack_at` serializado). Roda sob `index_exclusive` tomado no topo (`build.rs:176` → `lock.rs:25`), transaction-scoped → cobre **~todo o rebuild**. Consequências (confirmadas no código):
- **Cliff de RAM:** ~3,07 GB só o corpus f32 a 1M×768d; pico realista **~6-10 GB** (múltiplas cópias + grafo).
- **Parada total de queries vetoriais** durante todo o rebuild O(N) sob EXCLUSIVE (não uma janela curta de pivot).
- **Um scan longo (SHARE) bloqueia o VACUUM indefinidamente** (advisory é a única serialização, bloqueante).
- O mesmo `collect_corpus` (`build.rs:28`, sem teto) limita o **BUILD**: um `CREATE INDEX` de 1M×768d exige o corpus inteiro em RAM antes do primeiro nó — no escopo da decisão (DoD).

O M48 (ADR 0014) tornou o **write-side** do fold crash-safe (shadow-write + meta-pivot atômico + reclaim boundado), mas **não tocou o compute-side** O(N). O M48 explicitamente nomeou o in-place como "escopo do M55" (`ADR 0014:59-60`).

### Opção A — fold incremental (gerações parciais sobre o meta-pivot do M48)
Reusaria `fold.rs` inteiro (pivot atômico + reclaim). **Problema honesto: HNSW não faz merge de gerações barato** — os arcos do grafo cruzam gerações; inserir/remover sem reconstruir exige alterar neighbor-lists de nós já persistidos (= edição in-place = Opção B), OU manter N sub-grafos pesquisados em paralelo (multiplica o custo de scan por #gerações, colidindo com a vitória O(ef·M) do M35). **Não fecha o muro de RAM sozinha** → absorvida como o *lado compaction* da Opção C.

### Opção B — in-place à la pgvector (`hnswvacuum.c`)
4 passes page-level, **O(#deletados) RAM, sem O(N), sem parada total**:
- **Pass 1 `RemoveHeapTids`** (`hnswvacuum.c:36`): remove heap-TIDs mortos in-place por página (buffer EXCLUSIVE por página, `:67`); hash `deleted` O(#deletados).
- **Pass 2 `RepairGraph`** (`:371`): re-roda `HnswFindElementNeighbors` p/ nós que apontavam para deletados, sobrescreve neighbor-tuple in-place (`PageIndexTupleOverwrite`, `:257`); page-level `HNSW_UPDATE_LOCK`.
- **Pass 3 `ConfirmRepaired`** (`:496`): fail-loud (`elog(ERROR)`, `:562`) se algum link vivo ainda referencia deletado.
- **Pass 4 `MarkDeleted`** (`:579`): marca `deleted=1`, zera vetor, incrementa `version` (`:681`) p/ iterative-scans. Reúso de slot acontece no **INSERT** (`hnswinsert.c:45`), não no VACUUM.
- **Contras:** quebra a invariante grafo-imutável do M35; exige máquina de `version`; reparo de links O(#deletados×ef); fragmentação (só encolhe via insert-reuse); é reescrita grande (`ADR 0014:60`).

### Opção C — híbrido (in-place p/ DELETE, fold p/ compaction) — RECOMENDADA
**Precedente forte — pgvectorscale/DiskANN faz a metade "delete barato":** `ArchivedPlainNode::delete` (`plain/node.rs:60,128`) é **tombstone puro** (`heap_item_pointer.offset = InvalidOffsetNumber`, com TODO explícito "actually optimize later"); `ambulkdelete` (`vacuum.rs:23`) marca dead in-place por página, **zero O(N)-RAM, zero rebuild**, sem reparo de vizinhos (conta com a redundância de arcos do α-pruning + compaction diferida).
- **DELETE** → tombstone in-place por página (baixa latência, zero parada); scan filtra tombstones (O(#tombstones no caminho)).
- **Compaction** (recuperar espaço + re-densificar) → o **fold O(N) crash-safe do M48 (`fold.rs`) que já existe**, disparado por threshold (`theodb.vacuum_pending_threshold`, já medido no M48).
- **Dois peers, dois pontos do espectro:** pgvector repara no vacuum (grafo denso, vacuum caro); pgvectorscale só marca (vacuum trivial + compaction diferida). C escolhe barato-no-DELETE + compaction crash-safe própria.

## Coverage Corner 1 — Integration tests
O baseline precisa de teste de concorrência (scan-em-loop vs VACUUM) para tornar a parada-total **observável** (precedente: crash-injection GUC do M48 `fold.rs:76`; scaffolds de vacuum do pgvectorscale `vacuum.rs:169`). Implementação decorrente herda "build → restart simulado → scan idêntico".

## Coverage Corner 2 — Dependencies
Zero deps novas: reusa `GenericXLog` (primitivo já usado), `fold.rs` (M48), `table_index_build_scan` (Postgres). Peers lidos read-only sob `references/` (não entram no pacote; D1 intacto).

## Coverage Corner 3 — Tools (medição — DoD item 2)
- **Peak RAM privado:** `/proc/<backend_pid>/smaps_rollup` (`Private_Dirty+Private_Clean`, **exclui shared_buffers**) amostrado a ~25ms; `clear_refs` reseta o pico antes; VmHWM como teto secundário. **Achado-chave: `maintenance_work_mem` NÃO limita o pico** (alocação via malloc do Rust, fora dos memory contexts do PG → `pg_backend_memory_contexts` também não vê).
- **Lock EXCLUSIVE:** poll de `pg_locks` (advisory/ExclusiveLock/granted, classid=index_oid) → lower-bound; `wall_ms` do VACUUM → upper-bound.
- **WAL:** delta de `pg_current_wal_lsn()` (método M48).
- **Box MEDIDA:** 15 GiB total, **7,3 GiB disponíveis, swap cheio → 1M×768 INVIÁVEL** (pico ~10-13 GB = OOM). Medir 100k/500k (box quiesada) + **projetar 1M via O(N) linear**, marcado `projected:true`. Harness `benchmarks/run_m55_vacuum_wall.py`.

## ADRs
Ver `docs/adr/0017-m55-index-maintenance-at-scale.md` (MADR 3.0): **Opção C (híbrido faseado)** — fase 1 tombstone-only in-place (espelha pgvectorscale), compaction reusa `fold.rs` do M48; fase 2 opcional (RepairGraph in-place do pgvector) se a medição de recall entre compactions exigir. Inclui o teto de memória do BUILD (`collect_corpus` streaming/batched — mesma raiz). Trigger v1.0: a implementação é pré-requisito de qualquer claim produção (`public-copy.md §3`).

## Incertezas declaradas (Rule 3)
1. RAM real a 768d não medida ainda (o item 2 do DoD substitui a estimativa por medição; 1M é projeção).
2. Degradação de recall de C sob tombstone-only entre compactions **desconhecida** p/ o nosso grafo (pgvectorscale conta com α-pruning que nosso `HnswIndex` pode não ter) — é a incerteza-chave que define se C precisa do Pass-2 na fase 1 ou fase 2; primeira coisa a medir na milestone de implementação.
3. Mapeamento classid/objsubid do advisory lock a confirmar empiricamente no run.
4. `hnsw_page.rs` (empacotamento M35) não lido em detalhe — a afirmação "element-tuples não editáveis in-place hoje" vem de `fold.rs:16-17` + `ADR 0014:23`; confirmar antes do ADR de implementação.

## Arquivos
Nosso: `theodb_rs/src/am/build.rs` (:28,:171,:219,:239), `lock.rs` (:25), `fold.rs`, `docs/adr/0014`, `docs/benchmarks/m48-am-maintenance.md`. Peers: `references/pgvector/src/hnswvacuum.c` (:36,:371,:496,:579), `hnswinsert.c` (:45), `references/pgvectorscale/.../vacuum.rs` (:23), `plain/node.rs` (:60,:128).
