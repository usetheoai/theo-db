# Blueprint — Filtered ANN planner-integrado (M52)

**Date:** 2026-07-07 · **Milestone:** M52 · **Question:** como o campo faz `WHERE … ORDER BY embedding <=> $1` com recall preservado, e qual estratégia o `theodb_hnsw` adota?

## Problema (measured)

O `theodb_hnsw` hoje (`am/scan.rs`): o scan faz o traverse → **ef** resultados → heapify → `amgettuple` pop até esgotar. **Máximo ef tuplas emitidas.** O executor do PostgreSQL aplica o `WHERE` via **recheck** DEPOIS que o AM emite cada tupla. Sob filtro **seletivo** (ex.: 1% passam), dos ef candidatos pouquíssimos sobrevivem ao filtro → o `LIMIT k` não é satisfeito → **recall colapsa** (o AM esgota antes de emitir k tuplas que passam). Este é o gap CRÍTICO do DoD.

## Prior art (2 fontes primárias — `knowledge-base/references/`)

### 1. pgvector 0.8 — iterative index scans (`references/pgvector/src/hnswscan.c`)
- GUC `hnsw_iterative_scan` ∈ {OFF, **RELAXED**, STRICT} + `hnsw_max_scan_tuples` + `work_mem` cap.
- Um segundo heap `so->discarded` guarda candidatos fora da janela ef. Quando o working set `w` esvazia (`hnswscan.c:249`) e iterative != OFF: se no limite (`tuples >= max_scan_tuples || mem > maxMemory`, `:259`) retorna o restante do discarded; senão **`ResumeScanItems(scan)`** (`:281`) CONTINUA a busca HNSW a partir do discarded (sob `HNSW_SCAN_LOCK ShareLock` para consistência com deletes).
- **RELAXED** (default): permite ordem aproximada entre iterações. **STRICT** (`:313`): só emite tupla com `distance >= previousDistance` (ordem monótona global) — mais lento, exato.
- Chave: o AM **continua produzindo candidatos em ordem de distância sob demanda**; o executor para de chamar quando o `LIMIT k` (pós-filtro) é satisfeito.

### 2. pgvectorscale — label filtering (`references/pgvectorscale/.../access_method/`)
- Filtragem por **labels** (colunas de rótulo indexadas no próprio índice), não o caso geral `WHERE`. Requer o label no índice (data model diferente). Bom para filtros conhecidos-no-build; NÃO cobre um `WHERE amount > $1` arbitrário.

### Field (citável, não no repo)
- **AlloyDB adaptive filtering:** decide em runtime entre pre-filter (scan relacional → rerank vetorial) e post-filter (ANN → recheck) por seletividade estimada. É a forma mais sofisticada; exige integração de custo planner-profunda.
- **ACORN (predicate-agnostic HNSW):** expande a vizinhança do grafo para manter conectividade sob predicado arbitrário. Research-grade, reescreve o traversal.

## Coverage Corner 1 — Integration Tests
Como testar: `EXPLAIN` provando `Index Scan` sob `WHERE`; recall@10 sob filtro seletivo (1%/10%/50%) vs seqscan exato (GT); zero regressão no path unfiltered (suíte M45/M50). O executor aplica o recheck — o teste mede o recall END-TO-END via SQL (`SELECT ... WHERE ... ORDER BY v <=> q LIMIT k`).

## Coverage Corner 2 — Dependencies
Zero nova dependência (Rule 9): reusa o `traverse` existente (`hnsw_page.rs`) com ef crescente. Um GUC `theodb_hnsw.max_scan_tuples` (precedente `ef_search`/`over_fetch` em `guc.rs`).

## Coverage Corner 3 — Tools
`EXPLAIN (ANALYZE)` para provar índice usado + rows rechecked; o harness de benchmark do M50/M51 (`run_m5x_*.py`) estendido com um predicado `WHERE`.

## Coverage Corner 4 — Techniques (SOTA-anchored)
**Iterative index scan (RELAXED), ancorado no pgvector 0.8:** quando o `amgettuple` esgota o heap e o executor pede mais, RE-BUSCAR com ef dobrado, dedup dos tids já emitidos, refill do heap; continuar até ef atingir `max_scan_tuples` (ou node_count). Ordem RELAXED (um candidato achado a ef maior pode ser mais próximo que um já emitido — aceitável, é o default do pgvector para iterative). O executor para quando o `LIMIT k` pós-filtro é satisfeito.

## ADRs

### ADR-1 — Iterative scan via re-busca com ef crescente (RELAXED), não resume-from-discarded
**Decisão:** implementar o iterative scan RE-EXECUTANDO o `traverse` com ef dobrado a cada esgotamento (dedup dos emitidos), bounded por `max_scan_tuples`. Ordem RELAXED.
**Rationale:** o `traverse` do theodb não expõe hoje um "discarded set" resumível (ele retorna os ef melhores e descarta o resto). Re-buscar com ef crescente é a forma KISS que produz o mesmo comportamento observável (mais candidatos em ordem sob demanda) reusando o traverse existente (Rule 9), sem reescrever o hot loop do grafo. É funcionalmente o RELAXED do pgvector.
**Alternativa rejeitada (resume-from-discarded, pgvector exato):** mais eficiente (não re-percorre o grafo do zero) mas exige o traverse expor + persistir o discarded set entre chamadas de amgettuple — reescrita do traverse + estado de scan complexo. Deferido como otimização (backlog) SE o benchmark mostrar o re-percurso caro demais. YAGNI até medir.
**Alternativa rejeitada (AlloyDB adaptive / ACORN):** planner-cost-integration / rewrite de traversal — complexidade alta sem necessidade provada; o RELAXED do pgvector é o SOTA permissivo shipado e suficiente para paridade (o DoD pede ≥ paridade pgvector 0.8).
**Alternativa rejeitada (pgvectorscale labels):** data model de label no índice, não o caso geral `WHERE`. Fora do escopo do DoD.

### ADR-2 — RELAXED por default (não STRICT)
**Decisão:** ordem RELAXED (default), consistente com o pgvector 0.8. STRICT (ordem monótona global) fica fora do escopo do M52.
**Rationale:** filtered queries querem k tuplas que passam o filtro em ordem *aproximada* de distância — a ordem global exata é raramente requerida e custa mais. É o default do pgvector.
**Alternativa rejeitada (STRICT):** exigiria rastrear previousDistance + descartar candidatos fora de ordem — custo sem necessidade provada. YAGNI.

## Recall preservado — por que funciona
Sob filtro seletivo, o AM continua emitindo candidatos em ordem de distância (ef crescente) até o executor ter k que passam o filtro OU `max_scan_tuples` ser atingido. No limite (ef → node_count), o AM emite TODOS os nós em ordem → recall = 1.0 sob qualquer filtro (o executor vê todos os candidatos ordenados). O `max_scan_tuples` limita o custo (trade-off recall×custo), exatamente como o pgvector.

## Cross-references
- Refs: `knowledge-base/references/pgvector/src/hnswscan.c` (iterative scan), `pgvectorscale/.../access_method/` (labels)
- Código a modificar: `theodb_rs/src/am/scan.rs` (ScanState + amgettuple), `theodb_rs/src/am/guc.rs` (max_scan_tuples)
- Upstream: M51 (o AM sobre o qual isto roda); benchmark harness M50/M51
