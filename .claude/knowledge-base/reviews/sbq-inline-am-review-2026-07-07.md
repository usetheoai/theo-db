# /review — M51 SBQ inline no AM

Date: 2026-07-07 · Slug: `sbq-inline-am` · milestone_id: M51 · Range: `v0.41.0..HEAD`

## Verdict: READY_TO_MERGE (após review-fixes)

Três council specialists (formato/storage, Rust/pgrx, vector-ANN). Todos READY_TO_MERGE após fixes.

## Reviewers + findings

**council-rust-pgrx: READY_TO_MERGE** (direto) — memory-safety / panic-across-C. Traçou o read path SBQ: `traverse` sob `#[pg_guard] extern "C-unwind"`; `from_meta_bytes`/`decode_meta` validam tamanho ANTES de `Vec::with_capacity` (sem OOM/OOB com bytes corruptos); o cast de `rd_options` para `TheodbIvfflatOptions` (#[repr(C)], sbq_bits **appended** → ivfflat ABI intacto) é sound; `qcode_owned: Option<Vec<u8>>` vive até o fim da traverse (nenhum borrow escapa); o rerank re-lê via `with_page_item` (SharePin RAII, nenhuma slice da página escapa). 2 INFO (F1 dim cross-check, F2 cosmético) — F1 aplicado como bônus.

**council-index-storage: NEEDS_FIXES → READY_TO_MERGE** — Index AM / page format / crash-safety. Versionamento correto (magic+version, v1 byte-idêntico, v3+ rejeitado); element tuple com código APÓS o vec, `elems_per_page(dim, code_len)` correto (sem overflow); fold passa `meta.sbq_bits` → **sem downgrade v2→v1 silencioso**, crash-safety M48 (meta-pivot) preservada; partial-read M31 intacto. Findings resolvidos:
- **[H2 HIGH]** `decode_element`/`hamming_bytes` não fail-fast em código truncado → **FIXED**: `load` valida `ev.code_bytes.len() == qcode.len()` → Err tipado (Rule 8, paridade com o meta trailer que já validava); cobre entry+upper+ground (mesmo `qcode` em todo `load`).
- **[M1 MED]** `hamming_bytes` truncava ao menor → **FIXED**: guard upstream garante equal-length em produção.
- **[L2 LOW]** msg de versão presumia v2 → **FIXED**: neutra.
- **[L1 LOW]** teste crash v2 e2e → backlog (não-bloqueante: fold meta-pivot já crash-proven, codebook é payload no item block-0 pivotado atomicamente).

**council-vector-ann: NEEDS_FIXES → READY_TO_MERGE** — correção ANN + honestidade. Algoritmo do read path **PROVADO correto** (Hamming no walk + rerank exato f32 dos walk_ef sobreviventes + truncate ef; padrão SBQ-em-grafo canônico; guard de comprimento Rule 8). Honest-negative (2-bit→0.52) corretamente diagnosticado. Findings de honestidade resolvidos:
- **[HIGH-1/HIGH-2]** o artefato apresentava o 0.9993 do SBQ como "vantagem de recall / teto superior" ao f32/pgvector — mas é **artefato de knob assimétrico** (SBQ varrido a walk_ef=6400 via over_fetch=16 vs ef=400 dos baselines; `over_fetch` e `ef_search` são o mesmo lever) → **FIXED**: reformulado para "o read path RECUPERA recall corretamente (o gate D3)"; nota de comparação NÃO-CASADA na tabela; tetos f32/pgvector a pool comparável marcados UNBENCHMARKED; ADR 0015 razão-de-reter corrigida (sobrevive à poda).
- **[LOW-1]** teste 2-bit vs honest-negative 128d → comentário de dependência de dim adicionado.

## Evidence (image theodb:m51, 12+ pg_test)

- Formato v2: codebook roundtrip (2) + meta v1/v2 (3) + element codec (1) + pack_sbq (1) + reloption e2e (1) + fail-fast código truncado (1).
- Read path: recall gate `sbq_traverse_hamming_then_rerank_recall_high` (recall@10 ≥ 0.9, walk_ef<node_count) + `create_index_with_sbq_bits_scans_correctly` (ef cobre → rerank == exato) + scan_core 4/4 + traverse f32 recall-neutral (v1 intacto).
- Benchmark T4.1: recall gate ≥0.99 **ATINGIDO** (0.9993); a 25k sem pressão de memória SBQ é parity-to-slower vs f32 (M50-predicted); ≥2× QPS = follow-up rastreado. ADR 0015 keep/kill (RETÉM).

## Hard gates
Failing tests: NENHUM entre os testes do M51. Pré-existentes (ann::hnsw, ef_search — classe pgrx-test, provados ortogonais ao M51) no backlog. Sem secrets; sem commit em main; sem Co-Authored-By; CHANGELOG atualizado; ADR 0015 registrado.

## Caveats honestos (não bloqueantes)
Escala reduzida (25k×128 gaussiano) + box contendida — decisão explícita do usuário; o ganho de QPS ≥2× é follow-up em escala com pressão de memória (rastreado). L1 (teste crash v2 e2e) no backlog.

**Verdict:** READY_TO_MERGE
