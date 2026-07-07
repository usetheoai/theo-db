---
slug: m56-repairgraph-inplace-insert
milestone_id: M56
created_at: 2026-07-07
goal: aminsert reuses a tombstoned element slot via a proper on-disk HNSW insert (search + link), so DELETE+INSERT churn does not grow the relation, with recall preserved (measured) and crash-safety.
---

# M56 DoD-1 (fase 2) — RepairGraph / in-place insert com slot-reuse

> **Decisão do usuário (2026-07-07):** construir o RepairGraph (fase 2). Este plano é a fonte de
> verdade do design; implementação por TDD, incremental, commit por commit.

## Contexto / descoberta (pgvector + layout M35)

- **pgvector** (`.claude/knowledge-base/references/pgvector/src/hnswinsert.c:212` "try space from a deleted
  element"; `hnswvacuum.c:218` `RepairGraphElement`): o INSERT reusa o slot de um elemento deletado fazendo
  um insert HNSW próprio (search + link); o VACUUM repara neighbor lists que referenciam deletados.
- **M35 (TheoDB) NÃO tem insert in-place** — `aminsert` só faz `append_pending` (O(1)); o grafo só é escrito
  em lote (`pack`/`fold`). Este slice **constrói** o subsistema de mutação in-place do grafo on-disk.
- **Recall NÃO é corrompido** (correção da análise inicial): o scan re-pontua cada nó pelo vetor armazenado;
  reusar o slot de X para Z (com Z devidamente linkado) só muda a topologia (arcos Y→slot passam a levar a Z
  vivo, pontuado corretamente) — deriva que o HNSW tolera e o fold cura. Coexiste com navigate-through: um slot
  tombstonado NÃO reusado é navegado-através; um REUSADO vira nó vivo Z.
- **DELETE continua barato** (DoD 5 preservado): o trabalho vai no caminho de INSERT, não no de DELETE.

## Layout relevante (fatos)

- Element tuple: **tamanho fixo** (header + vetor f32 [+ código SBQ]) → Z cabe em qualquer slot tombstonado.
- Neighbor tuple: **fixo por nível** (`nbr_slots(level,m,m0)=level*m+m0` slots) → o nbr tuple de Z cabe no slot
  de X **sse `X.level ≥ Z.level`**. Constraint de reuso.

## ADRs

- **ADR-R1 — reuse só para `Z.level ≤ X.level`.** Como os níveis são geométricos (~63% no nível 0), a maioria
  dos inserts reusa; níveis altos (raros) caem no `append_pending` atual. Alternativa rejeitada: realocar o
  nbr tuple de Z em espaço novo (quebra a localidade do slot; complexidade sem ganho — níveis altos são raros).
- **ADR-R2 — ordenação crash-safe (pgvector).** Escrever o element+nbr tuple de Z PRIMEIRO (sob GenericXLog),
  DEPOIS atualizar os in-arcs dos vizinhos selecionados. Um crash após o element de Z e antes dos in-arcs deixa
  Z linkado para fora (alcançável se algum vizinho já o referenciava) — no pior caso Z é recuperado no próximo
  fold (que reconstrói). Alternativa rejeitada: WAL multi-record atômico (GenericXLog não oferece; seria fork).
- **ADR-R3 — reparo de in-arcs pendentes é DIFERIDO ao fold**, não feito no insert (mantém o insert O(log N·M),
  não O(N)). Um arco Y→slot que virou Y→Z é válido (Z vivo, pontuado certo); o fold reconstrói com links corretos.

## Fases / tarefas (TDD)

### Fase 1 — primitivos on-disk de escrita incremental
- **T1.1** `find_reusable_slot(rel, meta, need_level) -> Option<Addr>`: varre element pages por um slot com
  `deleted=1` e `level ≥ need_level`. RED: teste que, com N tombstones de nível 0, acha um slot; sem tombstones,
  `None`. Bounded scan (limita nº de páginas varridas por insert; TODO cache de free-list em meta = follow-up).
- **T1.2** `write_element_into_slot(rel, addr, tid, vec, level, nbrs)`: escreve element+nbr tuple de Z no slot
  reusado sob GenericXLog (deleted=0). RED: escreve, relê, confere tid/vec/deleted=0/neighbors.
- **T1.3** `add_neighbor_inplace(rel, nbr_addr, z_addr, z_vec, m0)`: adiciona Z à ground nbr list de N;
  se exceder m0, poda o mais distante (recalcula distâncias). GenericXLog. RED: N com m0 vizinhos + Z → poda 1.

### Fase 2 — busca de vizinhos no insert (reusa scan_core)
- **T2.1** `insert_search_ground(rel, meta, vec, ef_construction) -> Vec<Addr>`: reusa `PageNeighborSource` +
  `scan_core::ground_search` (ef=ef_construction, filtra vivos) para achar os candidatos de nível 0. RED: num
  grafo conhecido, os candidatos incluem os vizinhos exatos esperados.

### Fase 3 — orquestração no aminsert
- **T3.1** `insert_inplace(rel, meta, tid, vec) -> Result<bool>`: assign nível; se `> 0` e sem slot de nível
  suficiente → `Ok(false)` (fallback pending). Senão: search → select m0 → write Z (T1.2) → add Z aos vizinhos
  (T1.3, ordem ADR-R2). Retorna `Ok(true)`. RED: o teste de aceitação abaixo.
- **T3.2** `aminsert`: se `insert_inplace` retornou `true`, feito; senão `append_pending` (atual).

### Fase 4 — aceitação + crash + recall
- **T4.1 (aceitação, pg_test):** build N=400; DELETE 80; VACUUM (tombstone, sem compaction); `nblocks_before`;
  INSERT 80 novas linhas; assert `nblocks` NÃO cresceu (slots reusados, não pending); as 80 novas são achadas
  pelo índice; as 80 deletadas não; **recall@10 ≥ 0.9** vs seqscan exato dos vivos.
- **T4.2 (crash e2e):** build → DELETE → VACUUM → INSERT (reuse) → SIGKILL → restart → scan idêntico + as novas
  linhas presentes (durabilidade GenericXLog do insert in-place). Reusa `scripts/m56-crash-e2e.sh` estendido.
- **T4.3 (benchmark, droplet):** churn DELETE+INSERT em loop a escala; medir crescimento do relation com/sem
  slot-reuse (GUC de toggle) + recall entre folds → `docs/benchmarks/m56-slot-reuse.{md,json}`.

## Coverage Matrix

| Requisito (DoD 1) | Tarefa |
|---|---|
| reúso de slot no aminsert (hnswinsert.c) | T3.1, T3.2 |
| tombstone in-place por página (já feito M56) | — (cab7 done) |
| scan filtra tombstones (já feito M56) | — (done) |
| sem O(N)-RAM, sem EXCLUSIVE index-wide | T3.1 (insert O(log N·M), share lock) |
| recall preservado | T4.1, T4.3 |
| crash-safety | T1.2/T1.3 GenericXLog, T4.2 |

## Drawbacks & Risks

1. **Crash mid-insert (ALTO)** — mitigado por ADR-R2 (ordem) + recuperação no fold; T4.2 prova durabilidade.
2. **In-arcs stale até o fold (MÉDIO)** — válido (Z vivo, pontuado certo); deriva de topologia; T4.1/T4.3 medem recall.
3. **Concorrência insert vs scan vs vacuum (ALTO)** — o insert toma share lock (como append_pending); a mutação
   in-place de páginas sob buffer-EXCLUSIVE+GenericXLog é atômica por página; um scan concorrente vê o slot
   antigo ou o novo (ambos válidos). Compaction (exclusive) exclui inserts. Mitigação a validar em T4.2.
4. **Bounded slot scan (BAIXO)** — varrer element pages por slot livre é O(pages) no pior caso; limitar por
   insert + follow-up: free-list em meta. Honesto: v1 usa bounded scan.

## Unresolved Questions

- Free-list persistente em meta (evita o bounded scan) — follow-up após v1 funcional.
- Reuso para níveis altos (realocar nbr tuple) — follow-up; v1 cai no pending para `Z.level > slot.level`.

## Global DoD

- Todas as tarefas com pg_test verde; suíte 134→(134+novos) verde; crash e2e passa; benchmark com dados.
- Sem workaround: insert in-place PRÓPRIO (search+link), não herança-de-links.
