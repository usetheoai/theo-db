---
slug: sbq-inline-am
milestone_id: M51
created_at: 2026-07-06
goal: Inline SBQ codes in the theodb_hnsw layout v3 so the traverse scores candidates by Hamming popcount + f32 rerank, preserving recall@10 ≥ 0.99 on the M50 ruler.
---

# M51 — SBQ inline no AM (quantização no caminho quente)

## Context

O M50 (gate deste milestone) mediu e **autorizou** esta aposta com re-escopo: o `theodb_hnsw` está em recall-parity com pgvector mas ~1.6–1.7× atrás em latência por um **fator-constante** — a assinatura de um custo por-candidato fixo (o traverse pontua TODOS os ~50k candidatos em f32 full-precision; `am/hnsw_page.rs`). O lever comprovado (pgvectorscale SBQ, licença PostgreSQL) é: **códigos compactos DENTRO do índice, scoring barato (Hamming popcount) no hot path, rerank exato f32 só no top**. As peças já existem: o quantizador SBQ testado (`theodb_rs/src/sbq.rs`, M22) e o pipeline carrier→Hamming→rerank provado (o SQL `theodb.sbq_knn`). Este milestone move esse pipeline do carrier IVFFlat standalone para DENTRO do AM `theodb_hnsw` (códigos inline no layout v3).

**Decisão do usuário (2026-07-06, registrada no roadmap-run M51):** implementar + evidenciar **correctness e recall@10 ≥ 0.99 (com rerank f32)** na escala VIÁVEL (25k, a régua do M50); o claim de **≥2× QPS em escala com pressão de memória** fica como **follow-up EXPLÍCITO rastreado** — a box de dev contendida não roda 1M×3-builds (provado empiricamente no M50). Isto respeita a barra de evidência para tudo mensurável aqui e é honesto (Rule 3) sobre o que não é.

## Baseline Context

### Files that will be touched

| File | LoC hoje | git sha | Por que existe |
|---|---|---|---|
| `theodb_rs/src/sbq.rs` | 292 | b167dc5 | Quantizador SBQ próprio (train/quantize/hamming/bytes_per_vector) + `theodb.sbq_knn` SQL; hoje sobre carrier IVFFlat standalone |
| `theodb_rs/src/am/hnsw_page.rs` | 788 | 2d8a465 | Traverse on-demand + score() das páginas HNSW (M49 pôs dispatch 3-way de métrica); neighbor block `:452-511` |
| `theodb_rs/src/am/build.rs` | 369 | 15b8a75 | `ambuild`/fold; `append_pending` (`:122-143`); resolve_metric do opclass (M49) |
| `theodb_rs/src/am/page.rs` | 917 | d4d3543 | Layout de página + magic (`HNSW_STRUCT_MAGIC`, `peek_magic`) — precedente de versionamento on-disk |
| `theodb_rs/src/am/options.rs` | 87 | 3152ae0 | Reloption surface (M34 `WITH (lists=N)`) — precedente para `over_fetch` reloption |
| `theodb_rs/src/am/guc.rs` | 143 | 6d3d339 | GUC surface (`theodb_ivfflat.probes`) — precedente para `theodb_hnsw.over_fetch` GUC |
| `theodb_rs/src/am/scan.rs` | 331 | d746389 | Dispatch por magic no scan (`:93-105`) |
| `benchmarks/run_m51_sbq_inline.py` | 0 (NEW) | n/a | Benchmark recall×QPS SBQ-inline vs pgvector vs diskann (régua M50) |
| `benchmarks/tests/test_am_sbq_inline.py` | 0 (NEW) | n/a | Integration: build v3, recall@10 ≥ 0.99 com rerank, crash-safety, REINDEX path |

### Current callers / dependents

- `SbqQuantizer::train`/`quantize` (`sbq.rs:32,63`) — chamados hoje só por `sbq::knn` (`sbq.rs:148-149`) e pelos `#[pg_test]` de `sbq.rs`. **Nenhum caller de produção no AM ainda** — M51 adiciona o primeiro (no fold, `build.rs`).
- `hamming` (`sbq.rs:95`) — caller: `sbq::knn` (`sbq.rs:159`) + testes. M51 adiciona caller no traverse (`hnsw_page.rs`).
- `hnsw_page` score/traverse — chamado pelo scan (`scan.rs:105` via `HNSW_STRUCT_MAGIC`). O layout v3 preserva o dispatch por magic (v2 continua legível; v3 é novo magic).

### Domain glossary

- **SBQ (Scalar Bit Quantization):** quantização por-dimensão (threshold na média, 1-bit; z-score unário n-bit) empacotada em `u64`. `bytes = ceil(dim·bits/8)`.
- **Layout v3:** novo formato on-disk do `theodb_hnsw` que adiciona os códigos SBQ inline nos element tuples + codebook (means/std) nas meta pages. Selecionado por um novo magic (precedente v1→v2 = dispatch em `page::peek_magic`).
- **over_fetch:** multiplicador do top-k mantido após o rank Hamming, antes do rerank f32 (`k·over_fetch` candidatos rerankeados). Reloption + GUC.
- **Carrier-limited (M40):** num pipeline com rerank f32, o recall é limitado pela geração de candidatos (ef/over_fetch), não pelo quantizador — provado em `docs/benchmarks/m40-ceiling-probe.md`.
- **Fold:** consolidação da pending region (f32) no grafo persistido (`build.rs`); é onde os códigos SBQ são gerados (aminsert NÃO quantiza).

### Architecture boundaries affected

- `sbq.rs` é domínio puro (std-only, sem FFI). O AM (`am/*.rs`) é a camada de infraestrutura pgrx (FFI com o buffer manager). O plano mantém a fronteira: o quantizador (`SbqQuantizer`) fica em `sbq.rs`; o AM chama-o (DIP — o hot path do AM depende da abstração do quantizador, `rules/architecture.md § 2`).
- `hnsw_page.rs` já está em 788 LoC (acima do guia de 500). O código novo do traverse-Hamming deve ser extraído para funções coesas; se o arquivo crescer demais, o scoring SBQ vai para um módulo `am/hnsw_sbq.rs` (NEW) chamado pelo traverse (SRP + budget de LoC).

## Prior Art & Related Work

- **Blueprint `m36-quantization-in-index`** (`.claude/knowledge-base/discoveries/blueprints/m36-quantization-in-index-blueprint.md`) — investigação direta de quantização DENTRO do índice; consumido por este plano (padrão de códigos inline + rerank).
- **M22** (`sbq.rs`) — o quantizador SBQ próprio, testado (parity de memória, 16× compressão).
- **M38/M40** (`docs/benchmarks/m38-*`, `docs/benchmarks/m40-ceiling-probe.md`) — recall recuperável via rerank (0.947 @bits=4 SIFT + over_fetch → 1.000); recall é carrier-limited, NÃO quantizer-limited.
- **M50** (`docs/benchmarks/m50-sota-ruler.md § 4`) — o veredito-gate que autorizou + re-escopou este milestone (3 condições de medição herdadas).
- **pgvectorscale SBQ** (licença PostgreSQL, licença-permissiva OK) — precedente do padrão códigos-inline + meta pages para codebook + rerank (`sbq/storage.rs:304-328`, citado em `sbq.rs:7`).

## ADRs

### ADR-1 — Códigos gerados no FOLD; aminsert mantém pending f32
**Decisão:** `aminsert` NÃO quantiza — a pending region permanece f32 e é scaneada exata; os códigos SBQ são gerados no fold (do f32 consolidado). O codebook (means/std) é fixado no build e persistido em meta pages.
**Rationale:** quantizar em cada insert exigiria o codebook antes de ter o corpus (galinha-ovo) e drift por-insert. O fold já reescreve o grafo (precedente M48) — é o ponto natural. Mantém a pending correta por construção (scan exato f32). Segue o padrão pgvectorscale (meta pages para codebook).
**Alternativa rejeitada:** quantizar no aminsert com codebook incremental — rejeitada: drift não-determinístico + complexidade de codebook online sem ganho (a pending é pequena; scan exato f32 dela é barato). Viola KISS.

### ADR-2 — Layout v3 versionado por magic + REINDEX path (sem migração in-place)
**Decisão:** o formato SBQ-inline é um **novo magic** (v3); v2 (M49) continua legível pelo dispatch existente (`page::peek_magic`, `scan.rs:93-105`). Índices v2 existentes seguem funcionando f32; ganhar SBQ exige REINDEX (documentado).
**Rationale:** migração in-place de formato on-disk é a fonte #1 de bugs de corrupção. O dispatch por magic já existe (precedente v1→v2). REINDEX é o path seguro e é o que pgvector/pgvectorscale fazem em bumps de formato.
**Alternativa rejeitada:** migração in-place v2→v3 no primeiro scan — rejeitada: reescrita on-disk concorrente com scans é HIGH-risk de corrupção; o gate anti-sunk-cost não justifica o risco.

### ADR-3 — Co-localização dos códigos dos vizinhos: decisão MEDIDA (não default)
**Decisão:** por default os códigos ficam SÓ nos element tuples. Co-localizar códigos no neighbor block (`hnsw_page.rs:452-511`) — ~2 KB/nó a 128d/4-bit, índice ~2–3× maior por ~1 read/nó a menos — só se o benchmark index-size×reads×QPS mostrar effect>variância.
**Rationale:** HIGH-3 do review do ROADMAP; é um trade-off de I/O que só se decide com número, não com intuição (Esforço≠Complexidade, CLAUDE.md).
**Alternativa rejeitada:** co-localizar sempre — rejeitada sem medição (pode inflar o índice 2–3× sem ganho de QPS).

### ADR-4 — Recall-gate anti-sunk-cost-style (anti-sunk-cost) + ADR keep/kill do AM próprio
**Decisão:** retenção do SBQ-inline SÓ se recall@10 ≥ 0.99 preservado E effect>variância no Pareto; senão honest-negative + ADR mantendo f32. Registrar critério keep/kill do AM próprio (se após este lever seguir ≤ pgvector+diskann no Pareto realista, reabrir composição).
**Rationale:** o anti-sunk-cost tem cláusula de saída para forks; o AM próprio não tinha — este ADR fecha o gap de decision-record (risco 4c do deep-view).
**Alternativa rejeitada:** reter incondicionalmente porque "já implementamos" — rejeitada: anti-sunk-cost é regra do projeto (CLAUDE.md).

## Dependency Graph

```
Phase 1 (codebook em meta pages + magic v3)  ──┐
                                                ├─→ Phase 3 (traverse Hamming + rerank f32)
Phase 2 (códigos inline no fold)  ─────────────┘        │
                                                         ├─→ Phase 4 (benchmark recall×QPS + ADRs + follow-up)
                                                         │
Phase 5 (Integration Validation) ────────────────────────┘
```
Phase 1 e Phase 2 são sequenciais (Phase 2 escreve os códigos que Phase 1 dá o codebook para gerar). Phase 3 depende de 1+2. Phase 4 depende de 3. Phase 5 fecha.

## Phase 1 — Codebook SBQ em meta pages + magic layout v3

### T1.1 — Persistir codebook (means/std) em meta pages + reconhecer magic v3

#### Why this step
**Ação:** adicionar ao build um passo que treina o `SbqQuantizer` do corpus consolidado e persiste means/std numa meta page do índice, sob um novo magic v3; o dispatch de leitura (`page::peek_magic`) reconhece v3.
**Raciocínio:** o codebook precisa estar on-disk para o scan quantizar a query e comparar Hamming (ADR-1). O magic v3 preserva v2 legível (ADR-2). Sem o codebook persistido, o scan não consegue reproduzir a quantização do build (means fixados no build).

#### Files to edit
- `theodb_rs/src/am/build.rs` (treina quantizador do corpus + grava meta page; ≤ +80 LoC)
- `theodb_rs/src/am/page.rs` (novo magic `HNSW_SBQ_MAGIC` + read/write da meta page do codebook)
- `theodb_rs/src/sbq.rs` (expor `mean`/`std`/`bits` para serialização — getters `pub(crate)`)

#### Deep file dependency analysis
`build.rs` já materializa o corpus (`collect_corpus`) e resolve a métrica (M49). `page.rs` já tem o padrão de magic + meta page (IVF/HNSW). `SbqQuantizer::train` (`sbq.rs:32`) já existe; falta expor os campos para gravar.

#### TDD
- `test_codebook_roundtrips_through_meta_page` (RED): grava um `SbqQuantizer{mean,std,bits}` conhecido numa meta page, relê, assert means/std/bits idênticos (byte-exato).
- `test_v3_magic_recognized_v2_still_readable`: um índice v3 é reconhecido por `peek_magic`; um blob v2 existente continua dispatchando para o path f32 (sem regressão).

#### Concurrency tests
(none — single-threaded) — build/fold são single-thread (ADR do projeto); a escrita da meta page é no build exclusivo (AccessExclusiveLock), sem concorrência.

#### Acceptance criteria
- `test_codebook_roundtrips_through_meta_page` passa: means/std/bits relidos da meta page são byte-idênticos (`assert_eq!`) aos gravados.
- `test_v3_magic_recognized_v2_still_readable` passa: `peek_magic` retorna o tag v3 para índice v3 E o dispatch f32 para blob v2; a suíte M26/M31/M49 do AM continua 100% verde (0 testes novos vermelhos).
- `cargo pgrx test -p theodb_rs` sai com código 0 incluindo os 2 testes novos.

#### DoD
- `cargo build` limpo; testes novos verdes; CHANGELOG `[Unreleased]` atualizado.

## Phase 2 — Códigos SBQ inline nos element tuples (no fold)

### T2.1 — Gerar + gravar códigos inline no fold; aminsert mantém pending f32

#### Why this step
**Ação:** no fold, quantizar cada vetor consolidado com o codebook da Phase 1 e gravar o código SBQ inline no element tuple (layout v3); `aminsert` continua gravando f32 na pending (sem quantizar).
**Raciocínio:** o hot path precisa dos códigos ao lado dos vetores para o scoring barato (ADR-1). Gerar no fold evita o problema galinha-ovo do codebook e mantém a pending exata.

#### Files to edit
- `theodb_rs/src/am/build.rs` (fold grava código inline; `append_pending` inalterado — pending f32)
- `theodb_rs/src/am/hnsw_page.rs` (layout do element tuple v3 = f32 + código SBQ; leitura do código)
- `theodb_rs/src/am/page.rs` (offset/tamanho do código no element tuple)

#### Deep file dependency analysis
`append_pending` (`build.rs:122-143`) NÃO muda (pending f32). O fold reescreve o grafo — é onde o código entra. `hnsw_page.rs` element tuple hoje é f32-only; v3 acrescenta `ceil(dim·bits/8)` bytes por tuple (`SbqQuantizer::bytes_per_vector`).

#### TDD
- `test_fold_writes_inline_code_matching_quantizer` (RED): após fold, o código lido do element tuple é byte-idêntico a `quantizer.quantize(f32_do_tuple)`.
- `test_pending_stays_f32_no_code`: um insert pós-build fica na pending como f32 (sem código); o scan da pending é exato.
- `test_reindex_v2_to_v3_path`: REINDEX de um índice v2 produz v3 com códigos; recall preservado.

#### Concurrency tests
(none — single-threaded) — fold é single-thread sob lock exclusivo.

#### Acceptance criteria
- `test_fold_writes_inline_code_matching_quantizer` passa: o código lido do element tuple é `assert_eq!` a `quantizer.quantize(f32_do_tuple)` (byte-idêntico).
- `test_pending_stays_f32_no_code` passa: um insert pós-build lê como f32 na pending (sem código); recall@10 do scan da pending == 1.0 vs GT exato (0 regressão).
- `test_reindex_v2_to_v3_path` passa: REINDEX de um índice v2 produz magic v3 com códigos; recall@10 pós-REINDEX ≥ recall@10 pré-REINDEX (não regride).

#### DoD
- Testes verdes; `bytes_per_vector` respeitado no tuple; CHANGELOG atualizado.

## Phase 3 — Traverse: Hamming scoring + rerank f32 on-page

### T3.1 — Pontuar candidatos por Hamming popcount; rerank exato f32 no top k·over_fetch

#### Why this step
**Ação:** no traverse (`hnsw_page.rs`), pontuar os candidatos visitados por `hamming(qcode, code_inline)` (barato) em vez de f32 full-precision; rerankear só o top `k·over_fetch` pela distância exata f32 on-page. `over_fetch` vira reloption + GUC (`theodb_hnsw.over_fetch`), default medido.
**Raciocínio:** este é O lever do milestone — troca o custo por-candidato de f32 (512 B/cand a 128d) por Hamming (64 B/cand), ~8× mais barato; o rerank f32 no top recupera o recall (carrier-limited, M40). Sem o rerank, o recall cai (rank Hamming sozinho é aproximado).

#### Files to edit
- `theodb_rs/src/am/hnsw_page.rs` (traverse: Hamming score + top k·over_fetch + rerank f32) — se exceder budget de LoC, extrair para `am/hnsw_sbq.rs` (NEW)
- `theodb_rs/src/am/options.rs` (reloption `over_fetch`)
- `theodb_rs/src/am/guc.rs` (GUC `theodb_hnsw.over_fetch`)
- `theodb_rs/src/sbq.rs` (reusar `hamming` — sem duplicar; DRY)

#### Deep file dependency analysis
O score() do traverse já tem dispatch 3-way de métrica (M49, `hnsw_page.rs`). Em v3, o traverse quantiza a query uma vez (do codebook da meta page) e usa `hamming` (`sbq.rs:95`) para o rank; o rerank reusa o kernel f32 (`crate::vec`). `over_fetch` segue o precedente de `lists` (reloption, `options.rs`) e `probes` (GUC, `guc.rs`).

#### TDD
- `test_traverse_hamming_then_f32_rerank_recall` (RED): num corpus conhecido, o top-k do traverse v3 (Hamming+rerank) casa o top-k do scan exato f32 dentro do gate (recall@10 ≥ 0.99 com over_fetch default).
- `test_over_fetch_reloption_and_guc`: `WITH (over_fetch=N)` e `SET theodb_hnsw.over_fetch=N` afetam o número de candidatos rerankeados.
- `test_over_fetch_lower_bounds_raise_22023`: over_fetch inválido → erro tipado 22023 (negative case).

#### Concurrency tests
(none — single-threaded) — o traverse é read-only por query (SharePin), sem estado mutável compartilhado; a query é local ao scan.

#### Acceptance criteria
- `test_traverse_hamming_then_f32_rerank_recall` passa: recall@10 do traverse v3 ≥ 0.99 vs scan exato f32 (com over_fetch default) num corpus de teste de 2000 vetores.
- `test_over_fetch_reloption_and_guc` passa: `WITH (over_fetch=N)` E `SET theodb_hnsw.over_fetch=N` mudam o count de candidatos rerankeados (assert no número).
- `test_over_fetch_lower_bounds_raise_22023` passa: over_fetch=0 lança erro SQLSTATE 22023 com mensagem tipada (fail-fast, Rule 8).

#### DoD
- Testes verdes; `hnsw_page.rs` (ou `hnsw_sbq.rs`) dentro do budget de LoC; CHANGELOG atualizado.

## Phase 4 — Benchmark recall×QPS + ADR keep/kill + follow-up rastreado

### T4.1 — Benchmark SBQ-inline na régua M50 (escala viável) + veredito anti-sunk-cost + ADR keep/kill

#### Why this step
**Ação:** rodar o benchmark recall×QPS do SBQ-inline vs pgvector vs diskann na mesma régua do M50 (25k, cosine, GT exato, 3 runs, multi-cliente); aplicar o gate anti-sunk-cost (retém só se recall≥0.99 E effect>variância); escrever o ADR keep/kill do AM próprio; registrar o claim ≥2× em escala com pressão de memória como follow-up EXPLÍCITO.
**Raciocínio:** este é o critério de aceite mensurável aqui (decisão do usuário). O QPS absoluto a 25k não valida o ≥2× (M50: precisa pressão de memória), mas valida o **recall preservado** e mede a **direção** do QPS na escala viável. Honesto sobre o que não é mensurável aqui.

#### Files to edit
- `benchmarks/run_m51_sbq_inline.py` (NEW — reusa `theodb_bench.metrics` + o padrão do `run_m50_sota.py`; adiciona o spec theodb_hnsw v3 SBQ)
- `benchmarks/tests/test_am_sbq_inline.py` (NEW — integration: build v3, recall≥0.99, crash-safety, REINDEX)
- `docs/benchmarks/m51-sbq-inline.{md,json}` (NEW — artefato + veredito anti-sunk-cost)
- `.claude/knowledge-base/adrs/` (NEW ADR — keep/kill do AM próprio)
- `.claude/knowledge-base/backlog.md` (follow-up: benchmark ≥2× em escala com pressão de memória / box dedicada)

#### Deep file dependency analysis
`run_m50_sota.py` já tem a estrutura 3-way + multi-cliente + load-guard — o run_m51 acrescenta o spec v3 (theodb_hnsw com over_fetch) e compara vs a linha f32 (v2) para o delta de QPS na mesma box.

#### TDD
- `test_sbq_inline_recall_preserved` (RED): recall@10 ≥ 0.99 do build v3 (Hamming+rerank) vs GT exato num corpus de teste (o gate anti-sunk-cost codificado como teste).
- `test_sbq_inline_crash_safe`: build v3, SIGKILL, recover, top-k idêntico pré/pós (crash-safety do formato novo).
- `test_m51_artifact_contract`: o artefato `m51-sbq-inline.json` tem o spec v3 + veredito anti-sunk-cost + o follow-up ≥2× declarado (não checkbox fake).

#### Failure scenarios
Este plano toca I/O de banco (o benchmark abre conexões psycopg2 ao container). Cenário: conexão cai / build v3 falha no meio → o teste `test_sbq_inline_crash_safe` reproduz via SIGKILL do backend e assert recuperação sem corrupção (o índice v3 volta consistente ou o REINDEX reconstrói). Timeout de build → `statement_timeout` server-side aborta (lição M50), sem backend órfão segurando lock.

#### Concurrency tests
(none — single-threaded) — o benchmark é sequencial por spec; o multi-cliente mede throughput mas cada cliente tem sua conexão (sem estado compartilhado no driver).

#### Acceptance criteria
- `test_sbq_inline_recall_preserved` passa: recall@10 ≥ 0.99 (rerank f32) vs GT exato no artefato — OU o artefato registra honest-negative + ADR mantendo f32 (anti-sunk-cost) se effect≤variância.
- `docs/benchmarks/m51-sbq-inline.json` contém o spec v3 com QPS medido vs v2/pgvector/diskann na escala 25k E o campo `followup_2x_memory_pressure` declarado — verificado por `test_m51_artifact_contract` (assert de chave).
- ADR keep/kill do AM próprio existe em `.claude/knowledge-base/adrs/` com critério explícito de reabertura da composição (grep resolve o arquivo).
- `test_sbq_inline_crash_safe` passa: top-k pré/pós SIGKILL `assert_eq!` idêntico (crash-safety do formato v3).

#### DoD
- Artefato `m51-sbq-inline.{md,json}` com veredito anti-sunk-cost honesto; testes verdes; ADR + follow-up escritos; CHANGELOG atualizado.

## Phase 5 — Integration Validation

### T5.1 — Cadeia completa verde + gate anti-sunk-cost aplicado

#### Why this step
**Ação:** rodar a suíte completa (cargo pgrx test do AM + pytest de integração + benchmark), confirmar recall≥0.99, crash-safety, REINDEX, e o gate anti-sunk-cost decidido (keep ou kill com ADR).
**Raciocínio:** "eat your own cooking" — o milestone não está pronto até a cadeia inteira passar e o veredito anti-sunk-cost estar decidido com evidência.

#### Files to edit
- (nenhum novo — validação)

#### TDD
- Re-run de todos os testes das Phases 1–4; suíte verde.

#### Concurrency tests
(none — single-threaded) — a fase de validação apenas re-executa a suíte; nenhum código novo com sinal de concorrência. Build/fold/traverse são single-thread sob lock (ADR do projeto).

#### Acceptance criteria
- `cargo pgrx test -p theodb_rs` sai 0 E `pytest benchmarks/tests/test_am_sbq_inline.py -q` sai 0 (suíte completa das Phases 1–4 verde).
- recall@10 medido ≥ 0.99 vs GT exato registrado no artefato (ou honest-negative documentado em ADR se effect≤variância).
- `test_sbq_inline_crash_safe` E `test_reindex_v2_to_v3_path` passam (crash-safety + REINDEX provados por teste committado).
- `docs/benchmarks/m51-sbq-inline.md` E o ADR keep/kill resolvem no disco (grep retorna ≥1).

#### DoD
- `cargo pgrx test` + pytest de integração verdes; benchmark rodado; veredito anti-sunk-cost registrado; CHANGELOG atualizado.

## Failure scenarios

O plano toca I/O externo em dois pontos: (a) o build/scan do AM sobre o buffer manager do Postgres (FFI); (b) o benchmark abre conexões psycopg2 ao container (T4.1). Cenários de resiliência, cada um com reprodução por teste:

| Dependência externa | Modo de falha | Como o teste reproduz | Comportamento esperado |
|---|---|---|---|
| Buffer manager / WAL (build v3) | Crash no meio da escrita do formato v3 (SIGKILL do backend durante o fold) | `test_sbq_inline_crash_safe` (T4.1): build v3, SIGKILL do PID do backend, restart, relê top-k | Índice v3 volta consistente (nada meio-escrito lido como válido) OU REINDEX reconstrói; top-k idêntico pré/pós; sem corrupção |
| Buffer manager (leitura v3) | Element tuple v3 truncado/parcial | leitura defensiva com assert de tamanho (`SbqQuantizer::bytes_per_vector`) — tuple menor que o esperado → `Err` tipado, não panic atravessando C | Erro tipado propagado (Rule 8), nunca panic no boundary FFI |
| psycopg2 → container (benchmark) | Conexão cai / build demora demais | `statement_timeout` server-side (lição M50) aborta o build travado; sem backend órfão segurando lock | Build abortado server-side, lock liberado, benchmark reporta erro honesto (não fabrica número limpo) |

## Coverage Matrix

| DoD do ROADMAP (M51) | Task(s) |
|---|---|
| Códigos SBQ inline nos element tuples (layout v3 versionado + REINDEX) | T1.1 (magic v3) + T2.1 (inline no fold) |
| Write path: codebook em meta pages; aminsert mantém pending f32; códigos no fold | T1.1 (meta pages) + T2.1 (fold/pending) |
| Co-localização dos códigos dos vizinhos: decisão MEDIDA | ADR-3 + T4.1 (benchmark index-size×reads×QPS) |
| Traverse Hamming popcount + rerank f32 on-page no top k·over_fetch | T3.1 |
| Recall-gate anti-sunk-cost-style: recall@10 ≥ 0.99 preservado (ou honest-negative + ADR) | T3.1 (test) + T4.1 (benchmark) + ADR-4 |
| Fronteira recall×QPS re-medida vs pgvector E diskann | T4.1 (artefato m51-sbq-inline) |
| ADR keep/kill do AM próprio | ADR-4 + T4.1 |
| Condicional G4: LUT16 ADC vs Hamming criterion bench | (deferido — só se sobrar gap residual; registrado no follow-up de T4.1) |
| Claim ≥2× QPS em escala com pressão de memória | T4.1 (follow-up EXPLÍCITO rastreado — decisão do usuário 2026-07-06) |

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação | Owner |
|---|---|---|---|---|
| 1 | Mudança de formato on-disk (v3) pode corromper índices | ALTA | Novo magic (v2 legível), REINDEX path (sem migração in-place), crash-safety test (T4.1), ADR-2 | implementer |
| 2 | over_fetch mal-calibrado → recall < 0.99 | ALTA | Gate anti-sunk-cost codificado como teste (T3.1/T4.1); default medido; honest-negative + ADR se não atingir (anti-sunk-cost) | implementer |
| 3 | ≥2× QPS não mensurável na box (M50) | MÉDIA | Follow-up EXPLÍCITO rastreado (decisão do usuário); medir direção na escala viável | implementer |
| 4 | `hnsw_page.rs` já em 788 LoC → crescer demais | MÉDIA | Extrair scoring SBQ para `am/hnsw_sbq.rs` (NEW) se exceder budget (SRP) | implementer |
| 5 | Códigos co-localizados inflam índice 2–3× sem ganho | MÉDIA | Decisão MEDIDA (ADR-3); default só element tuples | implementer |

## Unresolved Questions

- O default de `over_fetch` exato só é conhecido após T3.1/T4.1 (medido, não adivinhado). O plano fixa "default medido"; o número entra no artefato.
- Se a co-localização de vizinhos (ADR-3) vale a pena só se decide com o benchmark index-size×reads×QPS (T4.1) — declarado como decisão medida, não resolvida a plan-time.

## Global DoD

- [ ] `cargo pgrx test` verde (todos os testes das Phases 1–4).
- [ ] pytest de integração verde (`test_am_sbq_inline.py`).
- [ ] recall@10 ≥ 0.99 (com rerank f32) na régua M50 — OU honest-negative + ADR mantendo f32 (anti-sunk-cost).
- [ ] crash-safety do formato v3 provada por teste; REINDEX v2→v3 provado.
- [ ] Artefato `docs/benchmarks/m51-sbq-inline.{md,json}` com veredito anti-sunk-cost honesto + follow-up ≥2× rastreado.
- [ ] ADR keep/kill do AM próprio escrito.
- [ ] Lint limpo (`cargo clippy`); arquivos dentro do budget de LoC (`rules/architecture.md`; `hnsw_page.rs` não cresce descontrolado — extração se necessário).
- [ ] CHANGELOG `[Unreleased]` atualizado (Rule 6).
- [ ] Wiring triad: `SbqQuantizer`/`hamming` chamados por caller de produção no AM (não só testes); integration test cobre o boundary; métrica/log runtime do path SBQ.
