# /review — M54 Vectorizer declarativo (bgworker + fila crash-safe)

Date: 2026-07-07 · Slug: `vectorizer-declarative` · milestone_id: M54 · Range: `v0.44.0..HEAD`

## Verdict: READY_TO_MERGE (após review-fixes)

Dois council specialists no território FFI novo (o projeto nunca usou `BackgroundWorker`): council-rust-pgrx (bgworker FFI) + council-index-storage (crash-safety da fila). Ambos **NEEDS_FIXES** → todos os HIGH endereçados; e2e re-verde + 13 pg_test.

## DoD (5 itens) — todos cumpridos com evidência

1. **Discover** — blueprint `m54-vectorizer-declarative-blueprint.md` + ADR 0016 (bgworker in-process, revisita 0007). ✅
2. **create_vectorizer + trigger + fila + worker em batch** — `theodb.create_vectorizer` (regclass) + trigger `_vectorizer_enqueue`; fila crash-safe; worker consome **em batch via embed_batch** (N→1 HTTP) com fallback per-job. ✅
3. **Chunking** — `theodb.chunk_text` (janela de caracteres com overlap, v1). ✅
4. **Crash-safety + e2e** — 13 pg_test (claim/lease, reclaim de owner morto, fencing, dead-letter, reaper) + `scripts/vectorizer-e2e.sh` (container preload + stub): INSERT→embedding aparece; UPDATE→re-embed; falha→retry bounded→failed. ✅
5. **Métrica** — `theodb.vectorizer_stats()` (processados/falhados + fila por estado). ✅

## Reviewers + findings (todos os HIGH FIXED)

**council-rust-pgrx: NEEDS_FIXES → READY** — território FFI novo, auditado contra pgrx 0.16.1 source:
- **H-1 (HIGH) FIXED:** `PgTryBuilder::catch` só faz `FlushErrorState` (não aborta o txn); um ERROR mid-statement (ex. dim errada no `::vector`) deixava SPI/snapshot sujo antes do COMMIT do `transaction()` → warnings em prod, PANIC sob `--enable-cassert`. Fix: helper `in_subtxn` (BeginInternalSubTransaction + Rollback/Release). Fixa também o double-process do fallback (M-1).
- M-2 (renew_lease morto), M-3 (sigterm), M-4 (target_table `%s` cru) → FIXED.
- Confirmado SÓLIDO (não regredir): shutdown limpo no idle, sem panic-across-C (o `#[pg_guard]` converte), sem leak de heap (err via `panic_any`/unwind roda Drop), fencing H1 correto, todas as escritas read-write (`connect_mut`).

**council-index-storage: NEEDS_FIXES → READY** — lente crash-safety/estado persistente:
- **HIGH-2 (orphan leak) FIXED:** job preso em `processing` no teto de tentativas (worker crashou antes de reportar) nunca reclamado (attempts<max false) nem dead-lettered → leak eterno em `processing`. Fix: `_vectorizer_reap_orphans` + chamada no loop + pg_test.
- **HIGH-3 (lease overrun) FIXED:** `renew_lease` nunca chamado; fallback (10×90s) estourava o lease de 120s → duplo-processamento sob multi-worker. Fix: renova antes de cada job do fallback.
- **HIGH-1 (embed-in-txn / xmin horizon) — documentado honestamente:** o embed roda síncrono in-txn (embed lê GUCs via SPI). Sob endpoint saudável ~100ms (negligível); worst-case bounded pelo timeout. É o padrão de dblink/pgsql-http. Docstring corrigida (afirmava falsamente "no txn"); async-embed 3-fases rastreado no backlog. Aceito para v1.
- M-1 (owner `bgw-{pid}` → uuid), M-3 (índice parcial) → FIXED.
- Confirmado SÓLIDO: claim SQL race-free (`FOR UPDATE SKIP LOCKED`), attempts-on-claim, fencing, lease-committed-não-lock, atomicidade do batch.

## Deferidos honestos (backlog — v1 single-worker OK)
HIGH-1 async-embed (mitigado por timeout); M-2 latest-wins e L-1 target-fencing (janelas de multi-worker); multi-worker/multi-DB; chunking recursivo. Todos rastreados em `knowledge-base/backlog.md § M54`.

## Hard gates
Failing tests: NENHUM (13 pg_test + e2e verde). Sem secrets (OPENAI_API_KEY só em header/`.env` gitignored; stub local). Sem commit em main; sem Co-Authored-By; CHANGELOG + blueprint + ADR + backlog registrados.

**Verdict:** READY_TO_MERGE
