# Backlog — tracked follow-ups (not yet milestones)

> Cleaned 2026-07-20 after the M118/M120/M121/M122/M123 releases. Items delivered as milestones are moved to
> **§ Recently closed** (traceability) and removed from the open list. Verify against the shipped code before
> reopening any item — several here predate a fix.

## Open follow-ups

### ALTA

- **Benchmark SBQ-inline ≥2× QPS em escala com pressão de memória** (M51 gate-of-value). O M51 provou recall≥0.99
  (0.9993) do read path SBQ-inline, mas a 25k/128d (sem pressão de memória) SBQ é parity-to-slower vs f32. O claim
  `≥2× QPS a recall≥0.99 vs pgvector` só é mensurável em **escala com pressão de memória** (≥250k @1536d ou 1M
  @768d) numa **box quieta**. O run deve incluir um ponto f32 a `ef_search` elevado (≥1600; exige subir
  `MAX_EF_SEARCH=1000`) para fechar o UNBENCHMARKED do teto de recall casado. Ver `docs/adr/0015-sbq-inline-keep-kill.md`
  + `docs/benchmarks/m51-sbq-inline.md § 4`.

### MÉDIA

- **pg_test schema-gen — 6 testes `ann::hnsw::hnsw_persist_tests` (+ classe correlata) não registram** sob
  `cargo pgrx test` (`function tests.hnsw_roundtrip_bytes_reproduces_search() does not exist`, e o
  `am::hnsw_page::ef_search_zero_rejected_at_guc_boundary` error-match). Pré-existente (não regressão de M51/M122);
  validados historicamente via o harness Docker de regress (SQL). Ação: slice de higiene auditando a suíte pg_test
  contra o SQL-gen do pgrx_embed (schema-gen + error-matching).
- **Multi-worker vectorizer + dedup processing-aware** (reforçado pelo review M122 — council-index-storage/council-rust-pgrx).
  Hoje 1 worker/DB (`WORKER_DBNAME='postgres'`), seguro. N workers (o SKIP LOCKED já suporta) + launcher por-DB são o
  próximo passo de throughput. **Bloqueado por:** dedup por `(vectorizer_id, source_pk)` cobrir também `processing`
  (hoje o partial-unique só cobre `pending`) — sob multi-worker um par processing+pending do mesmo pk pode ser
  reivindicado concorrente, e a fase-C separada do M122 alarga a janela de reorder de vetor stale. Fixes correlatos:
  latest-wins (version/enqueued_at write-if-newer), owner-fence no UPDATE do alvo.
- **SBQ fold v2 crash-safety e2e** (M51 review L1): pg_test que builda `WITH (sbq_bits=4)`, dispara
  `theodb.test_crash_phase=1` num VACUUM fold e após recovery assere `decode_meta` v2 + top-k correto. O fold
  (meta-pivot M48) já é crash-proven p/ v1; o codebook é payload no item block-0 protegido atomicamente.
- **M52 multi-seed + ON/OFF formal no harness** (M52 review HIGH-2): estender `run_m52_filtered_ann.py` com
  loop multi-seed (mean±std do delta por seletividade) + varredura `max_scan_tuples ∈ {0, 20000}` (prova o trigger
  do iterative a 10%/50%). O gate 1% já é medido; isto fecha o "por que 10%/50%".

### BAIXA

- **AVX2 para IP** (M49 Phase 3): cosine já tem kernel AVX2+FMA (M58, `vec.rs:327`); IP segue scalar-from-bytes.
  Adicionar AVX2 ao IP SÓ se um benchmark de latência mostrar que ele fica materialmente atrás do L2 (YAGNI até medir).
- **M52 testes diretos de terminação/rescan** (review LOW): `max_scan_tuples=5` (cap), self-join (emitted.clear
  evita skip/dup), exit por ef-ceiling. A terminação já é airtight por construção (3 bounds); testes reforçam.
- **Quote-ident nas scan-stats** (M67/M68 review LOW): `exact_topk`/`recall_at_ef`/`scan_stats` (`am/autotune.rs`)
  interpolam `vec_col`/`query` via `format!`. Mitigado (REVOKE FROM PUBLIC + `tbl` via `regclass::text`). Fix:
  `quote_ident` + bind onde possível. Sobe se expostas a role não-privilegiado.
- **Migração byte-level de instalações pgvector** (M70 review): o caminho `real[]` funciona (reescreve o heap,
  precisa janela). Uma migração byte-level (layout byte-idêntico do M69) exigiria instalar o tipo próprio num schema
  temporário + `ALTER TYPE … SET SCHEMA`. Greenfield (caso primário) não precisa.
- **Chunking recursivo separator-aware** (M54): o splitter recursivo (parágrafo→frase→palavra→char, à la LangChain)
  é upgrade do window-de-caracteres v1 (suficiente hoje).
- **post_json destrutor-leak no longjmp** (M122 review LOW-1, pré-existente): um `ereport(ERROR)` é `siglongjmp`;
  frames Rust entre o raise em `post_json` e o catch não são Rust-unwound → ~KB vazado por embed falho. Não-M122,
  não-blocker.

## Recently closed (traceability — do NOT reopen without verifying the shipped code)

| Item (origem) | Entregue como | Veredito |
|---|---|---|
| async-embed 3-fases (M54 council-index HIGH-1) | **M122** (v0.108.0) | fix REAL (fonte pgrx + medido 0/28 held) |
| BEIR paired significance (M53 council-benchmark) | **M123** (v0.109.0) | PARITY medido (honest-negative, p=0.25) |
| Filtro estruturado fail-closed (M53 council-security F1) | **M120** (v0.107.0) | 22023 fail-closed (achou fail-OPEN non-array) |
| IVF spherical k-means (M49 council-index HIGH-2) | **M121** (v0.107.0) | no-op PROVADO (scale-invariant) → revertido |
| Iterative scan resume-from-discarded (M52 follow-up) | **M118** (v0.106.0) | own-path ~1.95× + recall 1.0 (pgvector-DoD falsificado) |
| AVX2 cosine kernel (M49 Phase 3) | **M58** | `vec.rs:327` AVX2+FMA cosine |
