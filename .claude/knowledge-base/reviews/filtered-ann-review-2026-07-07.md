# /review — M52 Filtered ANN (iterative scan)

Date: 2026-07-07 · Slug: `filtered-ann` · milestone_id: M52 · Range: `v0.42.0..HEAD`

## Verdict: READY_TO_MERGE (após review-fixes)

Dois council specialists (index-storage, benchmark). Ambos READY_TO_MERGE após fixes.

## Reviewers + findings

**council-index-storage: READY_TO_MERGE** (direto) — a interação executor/AM (o RISCO ALTO do DoD) é SOUND:
- **Sem loop infinito:** 3 bounds compõem (pop encolhe heap finito; `fresh` filtrado por `!emitted.contains` → todo elemento é emitível → progresso; `gather` bounded por `node_count` via o `visited` set → `fresh` esvazia → exhausted). O caso patológico (mesmos tids) não ocorre (todos em `emitted` → fresh vazio).
- **Terminação completa:** 3 saídas (emitted≥cap, ef-ceiling, fresh-vazio) cobrem todo caminho não-pop.
- **amrescan reset correto:** `emitted.clear()` a cada rescan → nested-loop-join não pula/duplica; ef reseta para a base.
- **xs_recheckorderby=false (RELAXED):** espelha pgvector exatamente; WHERE é heap Filter (não index qual) → correto.
- **FFI sound:** `rel` pinado por RelationGetIndexScan; `query` é clone owned; sem ponteiro dangling. Bônus: o lock de fold (transaction-scoped) cobre a re-busca → sem torn-read vs VACUUM.
- 1 MEDIUM (iterative default-ON diverge do pgvector default-OFF → **FIXED**: guc.rs documenta a divergência deliberada) + 3 LOW (testes diretos de terminação/rescan → backlog; re-percurso ADR-1; work_mem bound).

**council-benchmark: NEEDS_FIXES → READY_TO_MERGE** — lente "mediu ou supôs?". Achou um erro de honestidade GRAVE (correto):
- **[HIGH-1]** o artefato citou os números do M50 **INVERTIDOS** ("theodb 0.59 atrás") — o M50 raw diz theodb 0.6227 vs pgvector 0.590 (theodb à FRENTE unfiltered). A alegação "gap base do M50, não M52" era FALSA (o sinal reverte sob filtro) → **FIXED**: números corrigidos + reformulado para "variância de query set sobre base ~equivalente".
- **[HIGH-2]** o mecanismo "iterative mal dispara a 10%" não era medido → **FIXED**: controles medidos (iterative ON=0.58 vs OFF=0.49 a 10% → DISPARA e recupera +0.09; 50% ON==OFF; seed-99 inverte o sinal → ruído de amostra pequena).
- **[MEDIUM-1]** parity_gate com tolerância 0.01 embutida no nome enganoso → **FIXED**: renomeado `theodb_ge_pgvector_within_1pct` + tolerância explícita.
- O que já era limpo (revalidado): GT exato compartilhado, apples-to-apples, agregados batem com o raw, QPS 3× honestamente atribuído, recall determinístico sob carga.

## Evidence (image theodb:m52, 5 pg_test + benchmark)
- pg_test: `filtered_scan_preserves_recall_via_iterative` (recall preservado sob cat=7 == exato), `iterative_scan_off_when_max_scan_tuples_zero` (OFF switch), `traverse_presize` (unfiltered inalterado), + os do M51.
- Benchmark: **gate 1% ATINGIDO** (theodb 0.973 ≥ pgvector 0.967); iterative dispara+recupera a 10% (medido); 50% não-seletivo; QPS 3× (re-busca vs resume, follow-up ADR-1).

## Hard gates
Failing tests: NENHUM entre os testes do M52 (rodam em hnsw_page::tests que registra). Sem secrets; sem commit em main; sem Co-Authored-By; CHANGELOG atualizado; blueprint + backlog registrados.

## Caveats honestos (não bloqueantes)
Box muito contendida (load 15-20): recall determinístico/confiável, QPS ruidoso (caveat). Otimização resume-from-discarded + testes diretos de terminação → backlog.

**Verdict:** READY_TO_MERGE
