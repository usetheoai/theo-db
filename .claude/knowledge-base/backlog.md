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

## [M51 follow-up rastreado] Benchmark SBQ-inline ≥2× QPS em escala com pressão de memória
O M51 provou recall≥0.99 (0.9993) do read path SBQ-inline, mas a 25k/128d (sem pressão de memória) o SBQ NÃO é
mais rápido que f32 (parity-to-slower) — consistente com o veredito do M50. O claim `≥2× QPS a recall≥0.99 vs
pgvector` só é mensurável em **escala com pressão de memória** (≥250k @1536d ou 1M @768d) numa **box quieta**
(o QPS a esta box contendida é poluído). Requer: box dedicada/quieta OU o streaming build do M55 (`collect_corpus`
materializa o corpus em RAM sem teto). Ver `docs/adr/0015-sbq-inline-keep-kill.md` (critério de reabertura da
decisão de composição) + `docs/benchmarks/m51-sbq-inline.md § 4`. Prioridade: ALTA (é o gate de valor do M51).
> **Nota de review (council-vector-ann):** o mesmo run de escala deve incluir um ponto f32 a `ef_search` elevado (≥1600, exige subir o cap MAX_EF_SEARCH=1000) para fechar o UNBENCHMARKED do teto de recall casado — converte a nota honesta atual em medição.


## [M51 review L1] Teste de crash-safety end-to-end do fold v2 (SBQ)
council-index-storage (não-bloqueante): adicionar um pg_test que builda `WITH (sbq_bits=4)`, dispara
`theodb.test_crash_phase=1` num VACUUM fold, e após recovery assere que `decode_meta` ainda dá v2 com
`sbq_bits==4` e o scan retorna o top-k correto. O mecanismo de fold (meta-pivot M48) já é crash-proven para v1;
o codebook é payload dentro do item block-0 que o pivot protege atomicamente — por isso não-bloqueante. Prioridade: MÉDIA.

## [M52 follow-up] Iterative scan resume-from-discarded (otimização de QPS)
O iterative scan do M52 (ADR-1) re-busca o grafo inteiro com ef dobrado a cada esgotamento (KISS). O pgvector 0.8
resume do `discarded` set (não re-percorre) → ~3× mais rápido no caso seletivo (m52: theodb 58ms vs pgvector 17ms
@1%). theodb IGUALA o RECALL (0.973 ≥ 0.967) mas paga QPS. Otimização: expor um `discarded` set resumível do
`traverse` (estado de scan entre chamadas de amgettuple) em vez de re-buscar. Ver `docs/benchmarks/m52-filtered-ann.md § 2`.
Prioridade: MÉDIA (recall já em paridade; é custo, não correção).

## [M52 review LOW] Testes diretos de terminação/rescan do iterative scan
council-index-storage (não-bloqueante): o amgettuple não tem teste unit direto (o módulo scan_heap_tests não
registra no cargo pgrx test — classe pré-existente). Adicionar: (i) teste com `max_scan_tuples=5` provando
terminação por cap, (ii) self-join/nested-loop provando que emitted.clear() evita skip/dup entre rescans,
(iii) exit por ef-ceiling. A prova de terminação é airtight por construção (3 bounds), mas testes diretos
reforçariam. Prioridade: BAIXA.
