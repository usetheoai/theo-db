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

## [M52 review HIGH-2] Controle multi-seed + ON/OFF formal no harness M52
council-benchmark: o delta theodb-vs-pgvector a 10%/50% é pequeno e oscila de sinal entre runs (variância). Para
CONFIRMAR (não supor) que é ruído e que o iterative dispara: estender `run_m52_filtered_ann.py` com (a) um loop
multi-seed (ex.: [42,99,7]) reportando mean±std do delta por seletividade, e (b) uma varredura `max_scan_tuples ∈
{0, 20000}` do theodb por seletividade (prova o trigger). Committar o json regenerado. Numa versão anterior do
artefato esses controles foram citados como "medidos" em prosa sem código/raw — retirados; este é o débito de
torná-los reproduzíveis. Prioridade: MÉDIA (o gate 1% já é medido e passa; isto fecha o "por que 10%/50%").

## [M53 review — council-security F1] Filtro estruturado fail-closed p/ ai.hybrid_search_rrf
council-security: o parâmetro `filter_sql` (M53) é SQL cru interpolado (`%5$s`) sob SECURITY INVOKER. O guard
rejeita `;` e comentários (`--`/`/*`/`*/`), mas NÃO é um parser — uma subquery de leitura ainda compõe
(caller-privilege, por design). Payload que passa o guard: `filter_sql => '(SELECT count(*) FROM t) >= 0'`.
Não é escalonamento hoje (INVOKER + read-only SPI + REVOKE FROM PUBLIC se sustentam), MAS vira BLOCKER latente
sob qualquer wrapper SECURITY DEFINER ou GRANT a role isolado (colide com o modelo de tenant do theo-data).
Fixes JÁ APLICADOS: doc/COMMENT corrigidos (removida a falsa garantia "injection-safe" no path filter_sql;
declarado SQL cru caller-privilege) + guard estendido p/ comentários + teste negativo `hybrid_filter_rejects_sql_comment`.
FOLLOW-UP (ADR): expor filtro estruturado (coluna/operador/valor com `%I` + bind de valor) como alternativa
fail-closed ao predicado cru — a única defesa realmente fail-closed p/ input não-confiável. Prioridade: MÉDIA
(hoje seguro sob INVOKER; sobe p/ ALTA se `ai.hybrid_search_rrf` for exposto no data-plane multi-tenant).

## [M53 review — council-benchmark] Teste de significância pareado hybrid vs vector (BEIR)
O edge +0.004 nDCG@10 da híbrida vs vector-only (scifact) é determinístico entre runs mas NÃO testado p/
significância entre as 300 queries. O harness já coleta arrays per-query (`return_per_query=True`). Adicionar
bootstrap/paired t-test p/ decidir se o edge é significativo ou ruído. Também: híbrida-com-BM25 (leg opt-in)
vs híbrida-com-ts_rank_cd (exige rebuild da imagem bm25 c/ theodb_rs do M53); cross-check pytrec_eval; nfcorpus.
Prioridade: MÉDIA (o DoD "não regride" já está cumprido; isto qualificaria um claim de superioridade).

## [M54 review — deferidos honestos (v1 single-worker OK; endereçar antes do multi-worker)]
- **council-index HIGH-1 (async-embed):** o embed HTTP roda SÍNCRONO dentro de uma txn (embed lê GUCs via SPI). Sob endpoint saudável é ~100ms (negligível); sob endpoint pendurado prende o xmin horizon até ~90s (bounded pelo timeout), atrasando VACUUM. Fix completo: 3-fases real (txn A lê content+cfg+commit → embed sem txn via um run_batch que recebe cfg resolvido → txn B escreve+marca). Exige split de embed.rs (resolve_cfg público + run_batch_resolved sem GUC). Prioridade: MÉDIA (mitigado por timeout; async é o correto).
- **council-index M-2 (latest-wins):** sem ordenação garantida por source_pk entre jobs; sob multi-worker/update-storm um embed stale pode sobrescrever o mais novo. Fix: coluna version/enqueued_at no alvo (write-if-newer) OU coalescer jobs por (vectorizer_id, source_pk) pegando o mais recente no claim. Prioridade: MÉDIA (v1 single-worker processa in-order).
- **council-rust L-1 (target UPDATE fencing):** o UPDATE do embedding no alvo não é owner-fenced (só mark_done é). Janela teórica de stale-write se o conteúdo mudou entre fetches sob multi-worker. Idempotente/last-writer-wins hoje. Prioridade: BAIXA.
- **Multi-worker + multi-DB:** hoje 1 worker, DB fixo (`WORKER_DBNAME='postgres'`). N workers (o SKIP LOCKED já suporta) + launcher por-DB são o próximo passo de throughput/portabilidade. Requer os fixes acima primeiro. Prioridade: MÉDIA.
- **Chunking recursivo separator-aware:** `theodb.chunk_text` v1 é janela de caracteres; o splitter recursivo (parágrafo→frase→palavra→char, à la LangChain) é o upgrade. Prioridade: BAIXA (v1 suficiente).

## [M67/M68 review — council-rust-pgrx LOW (pré-existente M67)] Quotar identificadores nas funções scan-stats
`exact_topk`/`recall_at_ef`/`scan_stats` (`theodb_rs/src/am/autotune.rs`) interpolam `tbl`/`vec_col`/`query`
direto no SQL via `format!` em vez de parametrizar / `quote_ident`. Mitigado hoje: as funções `theodb.scan_stats`
/`theodb.explain_scan`/`theodb.recommend_ef` são `REVOKE ALL ... FROM PUBLIC` (privilégio), e `tbl` chega via
`regclass::text` (nome já resolvido/quotado pelo cast). Mas `vec_col`/`query` são `text` livres. NÃO é regressão
do M68 (o M68 só adicionou `sum_candidates` à query já parametrizada `$1..$5` de `record_scan_stat`, correta).
Fix: `quote_ident` para os identificadores (col) + bind de valores onde possível. Prioridade: BAIXA (funções
admin, REVOKE FROM PUBLIC; sobe se expostas a role não-privilegiado).

## [M70 review — council-index-storage] Migração byte-level (sem reescrita) de instalações com pgvector
O M70 entrega o tipo `public.vector` own-code. A migração de bancos que já têm colunas `vector` do pgvector
usa hoje um intermediário `real[]` (`docs/ops/pgvector-migration.md`): ALTER COLUMN→real[]→DROP pgvector→
CREATE theodb→ALTER COLUMN→vector + REINDEX. É correto e seguro mas **reescreve o heap** (não é O(1)) e exige
janela de manutenção. Uma migração byte-level (aproveitando o layout byte-idêntico do M69) exigiria instalar
o tipo próprio num schema temporário (`theodb.vector`) durante a transição + `ALTER TYPE … SET SCHEMA public`
após dropar o pgvector — NÃO implementado (o tipo é fixo em `public.vector`). Otimização: prover um modo de
instalação schema-qualified do theodb_rs para a transição. Prioridade: BAIXA (o caminho `real[]` funciona;
greenfield — o caso primário — não precisa de migração). Origem: review M70 B1 (corrigido no doc — a alegação
de byte-cast direto era falsa para instalações com pgvector).
