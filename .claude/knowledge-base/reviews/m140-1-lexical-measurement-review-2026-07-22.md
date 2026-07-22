# Review — M140.1 (medição + decisão de storage lexical) — 2026-07-22

**Verdict:** READY_TO_MERGE

Ciclo: council-benchmark (a lente crítica para um milestone de MEDIÇÃO) auditou adversarialmente →
emitiu **NEEDS_FIXES** (1 HIGH + 3 MEDIUM de rigor; **sem fabricação, sem spin, sem inversão de
decisão**). Todos os achados foram corrigidos com re-medição real; re-verificado verde.

## Hard gates (cycle-review.md) — todos ✅

| Gate | Estado |
|---|---|
| Testes verdes na branch | ✅ 79 passed (suíte `theodb_bench/` inteira; 30 novos M140.1) |
| Sem secrets commitados | ✅ (só `postgres:postgres` do docker local, não é secret) |
| Sem commit direto em `main` | ✅ tudo em `develop` |
| Sem trailer `Co-Authored-By` | ✅ nenhum |
| CHANGELOG atualizado | ✅ `[Unreleased] § Added` M140.1 |
| code-quality | ✅ NOOP (nenhuma linguagem shipada habilitada; harness é dev-only) |

## Auditoria council-benchmark — findings e disposição

| Sev | Finding | Disposição (commit `fix(m140.1)`) |
|---|---|---|
| — | BLOCKER: nenhum (sem fabricação, sem inversão) | — |
| HIGH | H1: `TantivyBM25.index()` fazia append em path persistido → 626KB eram 2 segmentos idênticos (número não-reproduzível) | **CORRIGIDO** — `shutil.rmtree` limpa o path; índice limpo = 313 455 B (reproduzível, = o valor que o revisor mediu) |
| HIGH | H2: storage não apples-to-apples (índice Tantivy vs `pg_total_relation_size` com tsv redundante+pkey+toast) | **CORRIGIDO** — 3 framings honestos (índice-vs-índice 1,7×; footprint enxuto 3,5×; footprint fiel 5,0×); `gin_index_bytes()`/`tsv_column_bytes()` adicionados; ADR-0052 + report re-rotulados |
| MEDIUM | M1: pseudo-replicação (3 seeds = mesmo corpus → 900 obs pareadas não-independentes) | **CORRIGIDO** — significância **por-seed** (300 obs cada; p=[1e-5,5e-5,7e-5]); `flip` exige p<0,05 em todos os 3; sem pooling |
| MEDIUM | M2: assimetria stemming/stopword não declarada no headline BEIR | **CORRIGIDO** — reenquadrado pipeline-vs-pipeline; assimetria (Tantivy default sem stem vs PG english com stem) declarada |
| MEDIUM | M3: `test_..._within_tolerance` não checava tolerância (nome overclaim) | **CORRIGIDO** — `test_beir_ts_rank_reproduces_m138_within_tolerance` agora assere o leg ts_rank ≈ M138 (0,206/0,070) ±0,03 |
| LOW | L1: "termos mais distintivos" (é raridade-no-doc, não IDF) | **CORRIGIDO** — "mais raros no doc" |
| LOW | L2/L3: degeneração de template HDFS / seeds não-homogêneos | ACEITO — L2 já coberto na discussão do artefato + caveat de proxy; L3 mitigado (o fix H1 limpa o seed-0 também; MRR@10 é robusto a duplicatas) |

## INFO (o que o revisor confirmou estar certo)

- **Anti-spin exemplar:** o report descarta explicitamente o ponto mais favorável ao BM25 (m=5 = 5×)
  como ARTEFATO, usando m=1-2 (+9-13%) como magnitude honesta. Oposto de cherry-pick.
- **Sem fabricação:** todos os números do JSON internamente consistentes (W+L+T=n; mean_diff derivado);
  o eixo ts_rank **pula** quando PG ausente em vez de fabricar.
- **Âncora anti-fabricação genuína:** nfcorpus ts_rank 0,20599 ≈ M138 0,206117 (agora travada por teste).
- **ADR segue a evidência:** heap-vs-AM repousa no argumento Rule-9/MVCC-de-graça, robusto ao fator exato.

## DoD do milestone (ROADMAP M140.1) — verificação

| # | Item DoD | Estado | Evidência |
|---|---|---|---|
| 1 | Artefato docs/benchmarks/ BM25 vs ts_rank_cd vs pg_textsearch, nDCG/MRR + teste pareado, bate M138 ou reporta honesto | ✅ | `m140-1-lexical-measurement.md` + `m140-1-data/{beir,logproxy}.json`; BM25 bate ts_rank nos 2 eixos, reproduz M138; magnitude honesta declarada |
| 2 | ADR decidindo storage (heap vs AM) com custo/benefício medido | ✅ | `docs/adr/0052` — heap, com storage apples-to-apples; AM rejeitado |
| 3 | Tamanho do índice + latência de ingest medidos | ✅ | índice 313KB (3 framings) + ingest ~41ms, no corpus de logs |
| Goal metric | `test_m140_1_decision.py` verde sobre JSON real | ✅ | gate offline verde (veredito derivado, não hardcoded; âncora M138 travada) |

## Conclusão

Merge-ready. O núcleo científico é honesto e defensável (a própria auditoria adversarial confirmou:
sem fabricação, sem spin). Os 6 achados de rigor foram corrigidos com re-medição real, não com
racionalização. A direção (BM25 own-engine bate ts_rank_cd em retrieval lexical puro; índice menor
em todos os framings) é robusta. **Gate M140.1 PASSA → M140 segue para M140.2.**

Revisor: `council-benchmark` (auditoria adversarial, 19 tool-uses, reproduziu os pontos-chave empiricamente).
