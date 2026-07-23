---
slug: cc-hotspots-refactor
milestone_id: M145
created_at: 2026-07-23
goal: Decompor os 4 hotspots de CC refactor-worthy para CC ≤ 25 (medido por lizard), comportamento preservado.
---

# Plan — M145 Refactor dos hotspots de CC

## Goal

Reduzir a complexidade ciclomática dos 4 hotspots refactor-worthy de `theodb_rs` para **CC ≤ 25 cada** (medido por `lizard <arquivo> -l rust`), com comportamento **byte-idêntico preservado** (suíte verde + A/B in-PG para o `admit`).

## Context

O loop-code-review de `theodb_rs` (2026-07-23, lizard 1.23.0 sobre 1359 fns; `code-review-output/audit/lizard_rust.csv`) julgou 4 dos 15 hotspots CC>25 como refactor-worthy; os outros 11 são complexidade essencial de engine (aceita — Esforço≠Complexidade, `CLAUDE.md`). Gated M144 (v0.134.0) — os fixes e seus testes de regressão em `vectorizer.rs`/`parquet.rs` já estão no lugar, evitando conflito (anti-pattern cycle-roadmap de 2 milestones no mesmo módulo). Grill: `.claude/knowledge-base/grills/cc-hotspots-refactor-feature-grill.md` (requisitos resolvidos). Metodologia A/B byte-idêntico herdada do M115 (Agg-swap). Cumpre `rules/parsimony-ladder.md` (anti-sunk-cost — válvula honest-negative), `rules/testing.md` (comportamento, não linhas), `rules/architecture.md` (SRP), `rules/git-safety.md` (develop).

## Baseline Context

### Files that will be touched

| Arquivo | Fn alvo | Linhas | CC hoje | Último commit | Fix |
|---|---|---|---|---|---|
| `theodb_rs/src/parquet.rs` | `write_parquet_impl` | 174-301 | 35 | M144 (`a915f5f`) | T1.1 |
| `theodb_rs/src/vectorizer.rs` | `theodb_embed_worker_main` | 825-1099 | 41 | M144 (`ae5249f`) | T1.2 |
| `theodb_rs/src/am/page/mod.rs` | `main_index_pages` | 562-710 | 34 | pré-M144 | T1.3 |
| `theodb_rs/src/am/columnar_agg.rs` | `admit` | 250-446 | 59 | pré-M144 | T1.4 |

### Current callers / dependents

- `admit` (`columnar_agg.rs:250`): chamado pelo `planner_hook` do M115 (Agg-swap) que decide qual plano trocar; teste `test_admit_groupby_single_key_is_customscan_and_matches_heap` (`columnar_agg.rs:1145`) exercita o caminho. Byte-idêntico é o contrato M115.
- `theodb_embed_worker_main` (`vectorizer.rs:825`): entrypoint do bgworker (registrado em shared_preload); chama `_vectorizer_claim_batch`/`_vectorizer_process_delete`/`_vectorizer_process_upsert`/`_vectorizer_mark_*` (todos com testes M122/M144).
- `write_parquet_impl` (`parquet.rs:174`): chamado por `write_parquet` (`#[pg_extern]`, REVOKEd em M144); `enum Col` é fn-local (215-223) → precisa subir a módulo.
- `main_index_pages` (`am/page/mod.rs:562`): parser **read-only** de meta-page que computa onde começa a pending region (NÃO aloca páginas nem escreve WAL); chamado no scan/vacuum dos AMs de índice. Invariante = exatidão de byte-offset por versão de formato.

### Domain glossary

- **CC (cyclomatic complexity)** — contagem lizard: `if`/`match`-arm/`&&`/`||`/`?`/`for` ≈ +1; closures Rust contam para a fn envolvente (promover closure a fn livre baixa o número medido).
- **Agg-swap byte-idêntico (M115)** — a saída do plano trocado pelo `planner_hook` deve ser byte-a-byte igual ao heap para o mesmo input; `admit` decide a admissão.
- **honest-negative valve** — se um alvo não ganhar legibilidade real ao decompor (só mover CC de lugar), aceita-se a CC como essencial com justificativa no log (anti-sunk-cost).
- **verbatim-move** — extração mecânica cut-and-move de um bloco contíguo, zero edição aritmética (diff mostra só indentação/assinatura).

### Architecture boundaries affected

- `rules/architecture.md` § 3 (SRP): cada helper = uma responsabilidade coesa.
- `rules/parsimony-ladder.md`: extração NÃO é DRY forçado — `main_index_pages` NÃO deve unificar os blocos por-versão (offsets/strides genuinamente diferentes = complexidade essencial).
- Fronteira `unsafe`/pgrx: nenhum FFI/deref é PARTIDO por um seam; helpers que derefam `*mut` permanecem `unsafe`.
- Zero mudança de superfície SQL: mesmas assinaturas `#[pg_extern]`.

## Prior Art & Related Work

- **M115** (`.claude/knowledge-base/plans/` — Agg-swap byte-idêntico): metodologia A/B in-PG para o `admit`, reusada no DoD.
- **M114** (agg colunar): a matriz de type-mapping (ADR-N1/MM1) que `parse_agg_kind` encapsula.
- Análise de seams (2026-07-23): subagente Staff-Engineer produziu os helpers concretos + riscos + CC estimada por função (base deste plano).
- `rules/parsimony-ladder.md` (anti-sunk-cost), `rules/architecture.md` (SRP), Refactoring (Fowler) Extract Function.

## ADRs

### ADR-1 — Extração de helpers privados, comportamento-preservado (não reescrita)

**Decisão.** Cada hotspot é decomposto por **Extract Function**: blocos coesos viram helpers privados na MESMA módulo/arquivo, o corpo principal os chama. Zero mudança de lógica, ordem de decisão ou assinatura pública.

**Rationale.** `rules/architecture.md` § 3 (SRP) + Refactoring Extract Function. A CC medida cai porque cada helper carrega parte dos ramos; a legibilidade sobe (helper auto-documenta). Comportamento preservado = suíte verde + A/B (M115).

**Alternativa rejeitada.** Reescrita/simplificação da lógica — viola comportamento-preservado (o DoD do grill) e arrisca regressão; fora de escopo.

### ADR-2 — `main_index_pages`: 4 helpers verbatim por-versão, NUNCA unificados

**Decisão.** Extrair um helper `pending_start_v{N}` por bloco de versão (v4, v5/7, v6/8, v2/3), cada um **cut-and-move verbatim**; o dispatcher vira um `match ver`. **NÃO** unificar num parser parametrizado.

**Rationale.** Os offsets/strides diferem genuinamente por versão de formato (12B vs 20B stride; campos em posições distintas). Unificar seria complexidade ACIDENTAL onde um bug de conflação de stride/offset se esconderia — o invariante crítico é exatidão de byte-offset (misparse → linhas INSERTed silenciosamente perdidas). `rules/parsimony-ladder.md` (KISS + anti-sunk-cost apontam juntos). O ganho é naming + CC do main, não simplificação de lógica.

**Alternativa rejeitada.** Parser único parametrizado por stride/offset — mais complexo, esconde bugs de conflação; rejeitado.

### ADR-3 — Sequência risco-ascendente + válvula honest-negative

**Decisão.** Ordem: T1 parquet (risco mínimo — sem unsafe/byte-idêntico) → T2 worker (sem unsafe, cuidado com txn boundaries) → T3 main_index_pages (verbatim mecânico, cuidado com offsets) → T4 admit (maior blast-radius — M115 byte-idêntico). Cada alvo que não render ganho real de legibilidade → honest-negative aceito no log.

**Rationale.** Provar a disciplina de extração no alvo mais barato primeiro; deixar o byte-idêntico crítico por último com a disciplina já calibrada. `rules/parsimony-ladder.md` (anti-churn).

**Alternativa rejeitada.** Ordem por CC decrescente (admit primeiro) — expõe o alvo mais arriscado antes da disciplina estar provada; rejeitado.

## Dependency Graph

```
T1.1 (parquet) ─▶ T1.2 (worker) ─▶ T1.3 (main_index_pages) ─▶ T1.4 (admit) ─▶ T2.1 (Integration Validation)
```

Sequencial por disciplina crescente (ADR-3). Cada T1.x é um commit atômico com re-run de lizard como aceite mecânico. Arquivos disjuntos entre tasks, mas a ordem impõe a calibração de risco.

## Phase 1 — Refactors

### T1.1 — `write_parquet_impl` CC 35 → ≤ 25 (parquet.rs)

#### Why this step
Alvo de menor risco (sem `unsafe` salvo `MyProcPid` isolado, sem byte-idêntico) — prova a disciplina de Extract Function. Três matches paralelos de 7 vias são o motor de CC; extraí-los para helpers baixa o main a ~10.

#### Files to edit
- `theodb_rs/src/parquet.rs` — hoist `enum Col` (215-223) para escopo de módulo; helpers `col_builder_for(name,oid)->Result<(Field,Col),String>` (227-243), `append_row(&mut [Col], &SpiHeapTupleData)->Result<(),String>` (254-262), `finish_arrays(Vec<Col>)->Vec<ArrayRef>` (269-280), `atomic_write_parquet(&RecordBatch,SchemaRef,&str)->Result<(),String>` (285-299).

#### Concurrency tests
(none — single-threaded) — refactor num único backend; o comportamento é preservado, sem novo modelo de concorrência.

#### TDD
- RED shape: `lizard theodb_rs/src/parquet.rs -l rust` mostra `write_parquet_impl` CC=35 (> 25) ANTES. assert cc_before == 35.
- GREEN: após extração, `lizard` mostra `write_parquet_impl` CC ≤ 25 E cada helper CC ≤ 25. assert cc_after <= 25.
- Comportamento: smoke A/B no droplet — `write_parquet` de uma tabela com as 7 tipos-coluna produz Parquet byte-idêntico (md5) ao pré-refactor; tipo não-suportado ainda dá `Err` (fail-closed). test_write_parquet_roundtrip verde.

#### Acceptance criteria
- `lizard` re-run: `write_parquet_impl` CC ≤ 25 (medido, não estimado).
- `write_parquet` mesma assinatura `#[pg_extern]` (zero mudança SQL).
- Parquet output md5-idêntico ao pré-refactor para o mesmo input; OID não-suportado → `Err` preservado.

#### DoD
- `cargo check --features pg18,pg_test --tests` exit 0.
- `lizard theodb_rs/src/parquet.rs -l rust | grep write_parquet_impl` → CC ≤ 25.
- Smoke A/B md5-idêntico no droplet.
- CHANGELOG `[Unreleased]`.

### T1.2 — `theodb_embed_worker_main` CC 41 → ≤ 25 (vectorizer.rs)

#### Why this step
Sem `unsafe` (salvo `MyProcPid` que fica no main), mas os limites de transação (`BackgroundWorker::transaction`/`in_subtxn_msg`) são load-bearing (M122 xmin, H-1 poison isolation, H1 fencing). Promover as 2 closures + extrair o 3-phase group loop baixa o main a ~13 sem tocar os txn boundaries.

#### Files to edit
- `theodb_rs/src/vectorizer.rs` — helpers `claim_batch(owner)->Vec<(i64,i32,String,String)>` (874-900), `process_one(owner,job_id,vid,pk,is_delete)->bool` (promove closure 921-966), `renew_lease(owner,job_id)` (promove closure 910-917), `process_group(owner,vid,group)->(i64,i64)` (985-1089), `reap_and_purge()` (860-871).

#### Concurrency tests
Os limites de transação/subtxn são preservados intactos (move-block, nunca fundir/partir um txn). A semântica de `sigterm_received()` break (linha 1052 quebra o loop externo; dentro de `process_group` só `return` → o re-check em 982 quebra no próximo group-boundary, terminação equivalente) é explicitamente documentada no implementation log para o reviewer não ler como mudança de comportamento. Reusa o concurrent test M122/M140.4 (crash/VACUUM/MVCC + probe >1 thread #153) verde pós-refactor, e verifica a cancellation propagation do sigterm (parada limpa do worker) preservada — o refactor não muda o modelo de concorrência (nenhum novo mutex/atomic; move-block dos txn boundaries intactos).

#### TDD
- RED shape: `lizard theodb_rs/src/vectorizer.rs -l rust` mostra `theodb_embed_worker_main` CC=41 ANTES. assert cc_before == 41.
- GREEN: `lizard` mostra CC ≤ 25 no main E cada helper ≤ 25. assert cc_after <= 25.
- Comportamento: os testes existentes do vectorizer (claim/mark/process/retry/backoff, M122/M144) verdes; smoke no droplet — worker processa um job upsert e um delete corretamente (estado da fila igual ao pré-refactor).

#### Acceptance criteria
- `lizard`: `theodb_embed_worker_main` CC ≤ 25 (medido).
- Zero mudança nos txn boundaries (diff mostra só extração); semântica sigterm-break documentada.
- Suíte vectorizer verde; smoke worker (upsert+delete) igual ao baseline.

#### DoD
- `cargo check --features pg18,pg_test --tests` exit 0.
- `lizard` CC ≤ 25 no main.
- Smoke worker no droplet (fila igual ao baseline).
- CHANGELOG `[Unreleased]`.

### T1.3 — `main_index_pages` CC 34 → ≤ 25 (am/page/mod.rs)

#### Why this step
Parser read-only de byte-offset; extração verbatim de 4 helpers por-versão (ADR-2) baixa o main a ~12. Invariante = exatidão de offset (misparse → linhas perdidas). NÃO unificar os blocos (complexidade essencial).

#### Files to edit
- `theodb_rs/src/am/page/mod.rs` — helpers `unsafe fn pending_start_v4(rel,m)->Result<u32,String>` (578-600), `pending_start_v5_v7` (602-630), `pending_start_v6_v8` (632-663), `pending_start_v2_v3(rel,m,ver)` (665-692, dono do reject de versão desconhecida); dispatcher IVF vira `match ver`.

#### Concurrency tests
(none — single-threaded) — refactor num único backend; o comportamento é preservado, sem novo modelo de concorrência.

#### TDD
- RED shape: `lizard theodb_rs/src/am/page/mod.rs -l rust` mostra `main_index_pages` CC=34 ANTES. assert cc_before == 34.
- GREEN: `lizard` mostra CC ≤ 25 no main E cada helper ≤ 25. assert cc_after <= 25.
- Comportamento: A/B por-versão no droplet — para um índice IVF construído de CADA formato (v3/v4/v5/v6), o `pending_start` computado pós-refactor == pré-refactor (mesmo valor u32); diff da extração mostra só indentação/assinatura (verbatim). Suíte de índice verde.

#### Acceptance criteria
- `lizard`: `main_index_pages` CC ≤ 25 (medido).
- Byte-offsets/strides/guards idênticos por versão (diff verbatim); os 4 blocos NÃO unificados (ADR-2).
- `pending_start` idêntico ao baseline para cada versão de formato construída.

#### DoD
- `cargo check --features pg18,pg_test --tests` exit 0.
- `lizard` CC ≤ 25 no main.
- A/B `pending_start` por-versão idêntico no droplet.
- CHANGELOG `[Unreleased]`.

### T1.4 — `admit` CC 59 → ≤ 25 (am/columnar_agg.rs)

#### Why this step
Maior blast-radius (M115 byte-idêntico). O target-walk loop é o motor de CC (~35); extraí-lo para `classify_target_node` (+`parse_agg_kind`) e a decisão de modo para `build_admission` (+`heap_cache_admits`) baixa o main a ~15. Deixado por último com a disciplina de extração já provada em T1-T3.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — `enum TargetSlot { Group(i32,u32), Agg(ParsedAgg) }`; helpers `parse_agg_kind(name,vartype)->Option<i32>` (352-379, safe), `unsafe fn classify_target_node(node,relid,grouped)->Option<TargetSlot>` (289-387), `unsafe fn build_admission(rte,input_rel,relid,grouped,aggs,group_cols,layout)->Option<Admitted>` (393-445) com `heap_cache_admits(rte,aggs)->bool` (420-444) aninhado.

#### Concurrency tests
(none — single-threaded) — refactor num único backend; o comportamento é preservado, sem novo modelo de concorrência.

#### TDD
- RED shape: `lizard theodb_rs/src/am/columnar_agg.rs -l rust` mostra `admit` CC=59 ANTES. assert cc_before == 59.
- GREEN: `lizard` mostra `admit` CC ≤ 25 E cada helper ≤ 25. assert cc_after <= 25.
- Comportamento (M115 byte-idêntico): `test_admit_groupby_single_key_is_customscan_and_matches_heap` verde + A/B in-PG no droplet — para `GROUP BY key, count/sum/avg/min/max` a saída do plano trocado é byte-a-byte igual ao heap; a ORDEM de decisão (guards → loop → modo columnar-antes-de-heap) e todo ponto de `None`/`?` preservados.

#### Acceptance criteria
- `lizard`: `admit` CC ≤ 25 (medido).
- Ordem de decisão e todo `None`/`?` idênticos (byte-idêntico M115 preservado); `classify_target_node` retorna `None` exatamente para os mesmos nós que os ramos inline rejeitavam (incl. `aggsplit != AGGSPLIT_SIMPLE`); o check `grouped && group_cols.is_empty()` fica no main após o loop.
- A/B byte-idêntico in-PG para o caminho Agg-swap.

#### DoD
- `cargo check --features pg18,pg_test --tests` exit 0.
- `lizard` `admit` CC ≤ 25.
- A/B byte-idêntico no droplet (count/sum/avg/min/max × group key).
- CHANGELOG `[Unreleased]`.

## Phase 2 — Integration Validation

### T2.1 — Validação integrada

#### Why this step
"Eat your own cooking": os 4 alvos com CC ≤ 25 medido + suíte verde + A/B, num só gate.

#### Files to edit
- (nenhum — validação)

#### Concurrency tests
(none — single-threaded) — refactor num único backend; o comportamento é preservado, sem novo modelo de concorrência.

#### TDD
- RED shape: antes de T1-T4, `lizard` reporta 4 fns com CC>25 (59/41/35/34). assert count_cc_over_25_targets == 4.
- GREEN: após T1-T4, `lizard` reporta os 4 alvos com CC ≤ 25. assert count_cc_over_25_targets == 0.

#### Acceptance criteria
- `cargo check --features pg18,pg_test --tests` exit 0.
- `lizard theodb_rs/src/ -l rust` → os 4 alvos ≤ 25; nenhuma NOVA fn com CC>25 introduzida pelos helpers.
- Todos os A/B (parquet md5, worker fila, pending_start por-versão, admit byte-idêntico) idênticos ao baseline no droplet.

#### DoD
- Todos os gates de T1-T4 verdes.
- Zero mudança de superfície SQL (mesmas assinaturas `#[pg_extern]`).
- CHANGELOG `[Unreleased]` com a entrada do refactor.

## Coverage Matrix

| # | Requisito (DoD do grill) | Task | Prova |
|---|---|---|---|
| 1 | `write_parquet_impl` CC ≤ 25 | T1.1 | lizard medido + A/B md5 |
| 2 | `theodb_embed_worker_main` CC ≤ 25 | T1.2 | lizard medido + smoke worker |
| 3 | `main_index_pages` CC ≤ 25 | T1.3 | lizard medido + A/B pending_start por-versão |
| 4 | `admit` CC ≤ 25 | T1.4 | lizard medido + A/B byte-idêntico M115 |
| 5 | Comportamento preservado (suíte + A/B) | T2.1 | suíte verde + A/B por alvo |
| 6 | Zero mudança de superfície SQL | T2.1 | assinaturas `#[pg_extern]` inalteradas |
| 7 | Válvula honest-negative | T1.4 | análise: não-acionada; registrada no log se qualquer alvo não render ganho |
| 8 | CHANGELOG | T2.1 | entrada `[Unreleased]` |

100% dos DoDs do grill mapeados a task + prova.

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `lizard` | 1.23.0 | python (tool) | métrica de CC do audit (mesma ferramenta/comando) — já instalada |
| `pgrx` | =0.19.0 | rust | já declarada; refactor não adiciona dep |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | | | | Refactor interno — zero dep nova (parsimony rung 4) |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Refactor do `admit` regride o Agg-swap byte-idêntico do M115 | HIGH | A/B in-PG obrigatório (T4 DoD); deixado por último com disciplina provada; preservar ordem de decisão e todo `None`/`?` | eu |
| `main_index_pages`: erro de byte-offset na extração → linhas INSERTed silenciosamente perdidas | HIGH | verbatim-move (diff só indentação/assinatura); A/B `pending_start` por-versão; NÃO unificar (ADR-2) | eu |
| Churn sem valor (mover CC de lugar sem ganho de legibilidade) | MEDIUM | válvula honest-negative (ADR-3) — aceita CC essencial com justificativa | eu |
| Helper introduz nova fn com CC>25 (complexidade só migrou) | MEDIUM | T5 verifica: nenhuma NOVA fn CC>25; cada helper ≤ 25 medido | eu |
| Quebra de txn boundary no worker (M122 xmin/H-1/H1) | MEDIUM | move-block intacto; nunca fundir/partir txn; suíte concorrência M122/M140.4 verde | eu |

## Unresolved Questions

(none — every decision is resolved at plan time: a análise de seams definiu os 4 alvos, helpers, riscos e CC estimada; a válvula honest-negative cobre o caso de um alvo não render ganho.)

## Failure scenarios

(none — no external I/O touched. O refactor não adiciona I/O externo; parquet/worker já existentes, comportamento preservado. Os A/B rodam contra o `.so` instalado no droplet, não contra rede.)

## Global Definition of Done

- Os 4 alvos com CC ≤ 25 medido por `lizard <arquivo> -l rust` (mesmo comando do audit); nenhuma NOVA fn CC>25.
- `cargo check --features pg18,pg_test --tests` exit 0; suíte relevante verde.
- A/B por alvo idêntico ao baseline no droplet (parquet md5, worker fila, pending_start por-versão, admit byte-idêntico M115).
- Zero mudança de superfície SQL (mesmas assinaturas `#[pg_extern]`).
- Válvula honest-negative: qualquer alvo sem ganho real → CC aceita com justificativa no implementation log (nunca decomposição forçada).
- CHANGELOG `[Unreleased]` atualizado; arquivos ≤ 500 LoC de delta; zero dep nova.
