# Backlog — tracked follow-ups (not yet milestones)

## IVF cosine/ip spherical k-means (from M49 review, council-index-storage HIGH-2)
The IVF k-means seeds with L2 (`ann/ivf.rs:59`) and uses arithmetic-mean centroids (`ann/ivf.rs:101`), NOT the
spherical k-means pgvector uses for cosine (`references/pgvector/src/ivfkmeans.c:33`). Arithmetic-mean centroids
drift off the unit sphere → part of the IVF cosine/ip recall gap (0.83-0.89 vs HNSW's 1.0) is centroid quality,
not purely list-probing approximation. Follow-up: spherical k-means for IVF cosine/ip (normalize for the
centroid update) OR the Design-A normalize-at-store escalation (blueprint ADR-2). Gate on a benchmark showing
the recall lift justifies the change. Not a v1.0 blocker (recall ≥ 0.80 gate met; scoring is correct).

## AVX2 kernels for IP/cosine (from M49 Phase 3)
`ip_dist_from_bytes`/`cosine_dist_from_bytes` are scalar-from-bytes (zero-alloc, the M49 DoD met). L2 has an
AVX2+FMA path. Add AVX2 to IP/cosine IF a latency benchmark shows they lag L2's kernel materially.

## [PRE-EXISTENTE, surfaced 2026-07-06 via harness M51] 6 pg_test de `ann::hnsw::hnsw_persist_tests` não registram
`cargo pgrx test` → `ERROR: function tests.hnsw_roundtrip_bytes_reproduces_search() does not exist` (e os outros 5
do módulo). **NÃO é regressão do M51** — provado por worktree no commit 916f77d (antes da mudança de meta v2): falha
idêntica. Causa: o `pg_test` build estava quebrado em develop (fix `MemNode: Debug` em 351022f o destravou), então
esses testes antigos (m43/m44) nunca rodaram via cargo pgrx test; agora compilam mas o SQL-gen do pgrx não emite as
6 funções (sem colisão de nome; ambos módulos têm `#[pg_schema]`). Investigar a geração de entities do pgrx_embed
para esse módulo. Não bloqueia M51 (os testes novos do M51 registram e passam). Prioridade: MÉDIA (testing.md — broken test é dívida, mas não é o caminho do M51).

**CLASSE mais ampla (mesma raiz):** `am::hnsw_page::ef_search_zero_rejected_at_guc_boundary` também falha sob
`cargo pgrx test` (o `#[pg_test(error="outside the valid range")]` não casa como esperado, embora a mensagem do pg
CONTENHA a substring — provável diferença de como o pgrx 0.16.1 casa erros de GUC check_hook raised-at-SET vs o
harness Docker de regress). Ortogonal ao M51 (diff da sessão NÃO toca guc.rs — confirmado). Todos esses testes
foram validados historicamente via o harness Docker de regress (SQL), nunca via `cargo pgrx test` (que estava
quebrado em develop até o fix MemNode 351022f). Ação: auditar a suíte pg_test contra cargo-pgrx-test e corrigir os
padrões incompatíveis (error-matching + schema-gen) num slice de higiene dedicado. Os testes NOVOS do M51
(codebook, meta v2, element tuple) registram e passam corretamente sob cargo pgrx test.
