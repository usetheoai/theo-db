---
slug: inline-filter-pushdown
milestone_id: M90
created_at: 2026-07-12
goal: "Inline label filtering no IVF-AQ via scan-key: recall@10 sob filtro de label ~1% MEDIDO estritamente > M87 post-filter, zero regressão."
---

# Plano — M90 inline filter pushdown (label scan-key, IVF-AQ-native / Approach A)

## Goal

Empurrar o filtro de label (`WHERE labels && '{…}' ORDER BY e <-> q LIMIT k`) para DENTRO da travessia do IVF-AQ via o mecanismo de **scan-key** — com **recall@10 sob filtro de label seletivo (~1%) MEDIDO estritamente maior que o M87 post-filter** num benchmark reproduzível, **zero regressão** (250+ pg_tests GREEN; path sem-label byte-idempotente) e crash-safety no novo layout v7.

## Context

DISCOVER: `knowledge-base/discoveries/blueprints/inline-filter-pushdown-blueprint.md` (deep research Staff-DB web-grounded lendo o pgvectorscale real). Decisão: **Approach A (label scan-key)** — o que o pgvectorscale (permissivo, Rust+pgrx) usa; o Custom Scan Provider (Approach B, arbitrary-WHERE) é YAGNI aqui → M91. Grill: `knowledge-base/grills/inline-filter-pushdown-feature-grill.md`. Achado: o inline do AlloyDB é ScaNN-only-não-IVF, mas o nosso IVF-AQ Stage-1/Stage-2 é encaixe melhor (label nas code-pages → Stage-1 poda antes do rerank).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/am/mod.rs` | ~290 | `d601513` (2026-07-12) | amroutine flags (`amcanmulticol=false` `:82`), opclass DDL vetor (`:253`), support FUNCTION 1 | o opclass do vetor + o handler inalterados; `amcanorderbyop=true` |
| `theodb_rs/src/am/build.rs` | ~1160 | `fea5dfc` (2026-07-12) | `ambuild`/`aminsert` leem só `*values` (o vetor); streaming writers v5/v6 (M89) | o build escalável do M89 (build_owned + streaming) intacto; byte-idempotente sem label |
| `theodb_rs/src/am/page.rs` | ~1750 | `fea5dfc` (2026-07-12) | writers/readers v5/v6 (code-pages `[ids][codes]`) | v5/v6 sem-label byte-idênticos; GenericXLog crash-safe |
| `theodb_rs/src/am/scan.rs` | ~700 | `d601513` (2026-07-12) | `amrescan(_keys ignorado `:99`)`, `scan_ivf_aq_split` (probes/rerank_pool), `amgettuple` iterative M87 | o scan sem-label inalterado; o cursor iterativo M87 |
| `benchmarks/m90_filter_bench.py` (NEW) | 0 | — | (a criar) recall inline vs M87 post-filter | — |
| `docs/benchmarks/m90-inline-filter.md` (NEW) | 0 | — | (a criar) artefato | — |
| `docs/benchmarks/m90-inline-filter.json` (NEW) | 0 | — | (a criar) dados brutos | — |

### Current callers / dependents

- **Symbol:** `amrescan(scan, keys: pg_sys::ScanKey, nkeys, orderbys, norderbys)` em `theodb_rs/src/am/scan.rs:97` — hoje ignora `keys` (`_keys`).
- **Symbol:** `write_ivf_aq_split`/`write_ivf_aq_split_sq8` (`page.rs`), chamados só do `ambuild` (`build.rs`).
- **Callers (tests):** os pg_tests v5/v6 em `build.rs` (via `CREATE INDEX ... theodb_ivfflat`).
- **External:** no — AM interno.

### Domain glossary

- **scan-key** — a repr. interna do Postgres de um `WHERE index_key op const`; passada ao `amrescan` como `ScanKeyData`, implicitamente ANDed.
- **label** — coluna `smallint[]` declarada como 2ª coluna do índice; o filtro `&&` (overlap) é pushado como Index Cond.
- **inline skip** — pular um candidato sem overlap de label na Stage-1 (código) ANTES do rerank Stage-2 (f32/SQ8).
- **xs_recheck** — flag que diz ao executor para re-checar o predicado no heap (correção mesmo se o filtro do índice for lossy).
- **v7** — novo layout de página IVF-AQ com o label co-localizado nas code-pages.

### Architecture boundaries affected

Cruza a fronteira C do Postgres (`am/mod.rs` opclass DDL + `am/scan.rs` lê `ScanKeyData` via `pg_sys`). Mantém o AM como adapter sobre o core PG (per `architecture.md`, DIP). A lógica de overlap de label é código próprio (Regra 9); o scan-key/opclass é mecanismo do core. Sem nova camada.

## Prior Art & Related Work

- **Internal blueprint** — `knowledge-base/discoveries/blueprints/inline-filter-pushdown-blueprint.md` (Approach A vs B, o delta mínimo, o boundary honesto).
- **Reference project** — pgvectorscale `knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/{labels/mod.rs:181-237, scan.rs:189-364, mod.rs:56-317}` (o mecanismo scan-key/label; PostgreSQL License — estudo-de-design, código próprio Regra 9).
- **External** — Postgres index-scanning.html (scan key = `index_key operator constant`, recheck); Filtered-DiskANN dl.acm.org/doi/10.1145/3543507.3583552 (recall-under-filter, `UNBENCHMARKED` interno). AlloyDB = Approach B (fora do escopo M90).
- **M87** (post-filter iterative) + **M89** (build escalável) — a base.

## Objective

Adicionar o layout v7 (label nas code-pages) + o opclass `&&` multicol + o parse do scan-key com inline-skip na Stage-1, medindo o ganho de recall vs M87.

## ADRs

### D1 — Approach A (label scan-key) sobre Custom Scan Provider (B)
Decisão: inline via scan-key/label (pgvectorscale-proven). Rationale: parsimony — o DoD é um filtro de label seletivo; A é o menor delta que produz inline>post medível, provado em extensão Rust+pgrx permissiva. Alternatives considered: (B) Custom Scan Provider — REJEITADO: YAGNI (arbitrary-WHERE não está no DoD), pesado → M91; (C) manter M87 post-filter — REJEITADO: não fecha o gap de recall no regime médio. Consequences: format bump v7 + REINDEX; filtros limitados à coluna de label + `&&` (boundary honesto documentado).

### D2 — label co-localizado nas CODE-pages (Stage-1), não num stream separado
Decisão: guardar o label junto do código AQ na code-page. Rationale: a Stage-1 (código-only) pula não-matching ANTES do rerank Stage-2 (random-read f32/SQ8) → preserva o I/O O(probes). Alternatives considered: stream de label separado — REJEITADO: um random-read extra por candidato derrota o propósito. Consequences: novo magic v7; o widening do code-entry é o delta de formato.

### D3 — `xs_recheck=true` quando há key de label
Decisão: sinalizar recheck ao executor quando um scan-key de label está presente. Rationale: correção garantida mesmo se o filtro do índice for lossy (o core re-checa no heap) — o contrato do Postgres. Alternatives considered: assumir lossless — REJEITADO: frágil a edge-cases de dedup de label. Consequences: um heap-recheck por tupla emitida (barato; o pgvectorscale faz igual).

## Dependencies

Nenhuma dep externa nova. `ScanKeyData`/opclass/`smallint[]` são core Postgres via `pg_sys`. Regra 9. pgvectorscale é estudo-de-design (não linkado).

## Dependency Graph

```
Phase 1 (opclass && + amcanmulticol — o pushdown enabler)
      ▼
Phase 2 (layout v7 — build lê 2ª coluna + writers/readers guardam o label nas code-pages)
      ▼
Phase 3 (scan-key parse + inline-skip na Stage-1 + xs_recheck)
      ▼
Phase 4 (benchmark gate — recall inline vs M87 post-filter, droplet)
```

## Phase 1: opclass `&&` + amcanmulticol (o pushdown enabler)

### T1.1 — função de overlap própria + opclass de label

#### Objective
Fazer o planner empurrar `labels && '{…}'` como Index Cond (não post-filter).

#### Why this step (action + reasoning)
Ação: `#[pg_extern]` `theodb_smallint_array_overlap(smallint[], smallint[]) -> bool` (código próprio — sorted-set overlap) + DDL `CREATE OPERATOR && (…)` + `CREATE OPERATOR CLASS theodb_ivfflat_label_ops FOR TYPE smallint[] USING theodb_ivfflat AS OPERATOR 1 &&`. Raciocínio: o Postgres só pusha um qual como scan-key se o operador é membro do opclass da coluna do índice (index-scanning.html); espelha o mecanismo do pgvectorscale (`mod.rs:243-262`), código próprio.

#### Files to edit
`theodb_rs/src/am/mod.rs` (`amcanmulticol=true`; o `#[pg_extern]` de overlap; o DDL do opclass de label).

#### Concurrency tests
(none — single-threaded)

#### TDD
`label_overlap_pushed_as_index_cond` (pg_test): `CREATE INDEX ... (e, lbl)` + `EXPLAIN` de `WHERE lbl && '{3}' ORDER BY e <-> q` mostra o `&&` como Index Cond (não Filter). E `theodb_smallint_array_overlap('{1,3}','{3,5}')` == true, `('{1}','{2}')` == false.

#### Acceptance Criteria
EXPLAIN mostra `Index Cond: (lbl && '{3}')`; a função de overlap passa os casos +/−.

#### DoD
pg_test GREEN no droplet.

## Phase 2: layout v7 (label nas code-pages)

### T2.1 — build lê a 2ª coluna + guarda o label; writers/readers v7

#### Objective
Persistir o label por-vetor nas code-pages (novo magic v7), byte-idempotente ao v5/v6 quando não há label.

#### Why this step (action + reasoning)
Ação: `ambuild`/`aminsert` leem `*values.add(1)` quando `amcanmulticol` e a 2ª coluna existe → um `LabelSet` sorted-deduped por vetor; os writers v5/v6 ganham uma variante v7 que intercala `[ids][labels][codes]` na code-page; os readers leem o label. Raciocínio: D2 (Stage-1 poda) — o label tem que estar na code-page. Reusa o streaming write do M89. Sem label → v5/v6 inalterados (novo magic só quando há 2ª coluna).

#### Files to edit
`theodb_rs/src/am/build.rs` (read da 2ª coluna, encode do label), `theodb_rs/src/am/page.rs` (writer/reader v7 — `write_ivf_aq_split_v7`/`read_label_at`, magic v7).

#### Concurrency tests
(none — single-threaded)

#### TDD
`v7_build_restart_scan_identical` (pg_test): índice v7 com labels → simula restart → scan retorna idêntico (crash-safety). `v5_v6_byte_identical_without_label`: sem 2ª coluna, o build produz v5/v6 byte-idêntico ao pré-M90.

#### Failure scenarios
label NULL na 2ª coluna → trata como "sem label" (não filtra esse vetor OU o inclui — definir e testar); 2ª coluna de tipo errado → erro tipado no build (fail-fast).

#### Acceptance Criteria
v7 crash-safe (restart-scan-identical); v5/v6 byte-idênticos sem label; label lido de volta correto.

#### DoD
pg_tests GREEN no droplet.

## Phase 3: scan-key parse + inline-skip

### T3.1 — parse do scan-key de label + inline-skip na Stage-1 + xs_recheck

#### Objective
Consumir o `ScanKey` de label e pular candidatos sem overlap na Stage-1 antes do rerank.

#### Why this step (action + reasoning)
Ação: `amrescan` para de ignorar `keys`; lê `keys[i].sk_argument` como o query `LabelSet`; passa ao `scan_ivf_aq_split`/`_sq8`, cuja Stage-1 checa overlap por candidato e PULA os sem-match (o candidato não custa slot do top-k); `amgettuple` seta `xs_recheck=true` (D3). Raciocínio: o `_keys` já chega ao `amrescan` (`scan.rs:99`) — o delta é parsear + threading + o skip. Interage com o M87 (grow-probes recupera recall se a lista probed tem poucos matches).

#### Files to edit
`theodb_rs/src/am/scan.rs` (parse dos keys, threading do LabelSet, inline-skip na Stage-1, `xs_recheck`).

#### Concurrency tests
(none — single-threaded)

#### TDD
`filtered_scan_recall_equals_exact` (pg_test): index-scan com `WHERE lbl && '{k}'` retorna top-k == exact seqscan-filtered top-k (correção). `inline_skip_does_not_consume_topk_slot`: um candidato sem-match não aparece no resultado nem reduz o k emitido.

#### Acceptance Criteria
filtered recall == exact; xs_recheck garante correção; sem-label path inalterado.

#### DoD
pg_tests GREEN no droplet.

## Phase 4: benchmark gate (droplet) — o DoD medível

### T4.1 — recall inline vs M87 post-filter a ~1% seletividade

#### Objective
Provar o DoD: recall@10 sob filtro de label ~1% inline > M87 post-filter, MEDIDO.

#### Why this step (action + reasoning)
Ação: harness que constrói (a) índice v7 com labels + (b) índice v5 sem label; SIFT1M + coluna de label sintética (~1% dos vetores com o label alvo); mede recall@10 do inline (v7, `WHERE lbl && '{alvo}'`) vs o M87 post-filter (v5, mesmo WHERE aplicado pós-scan), same-data. Raciocínio: measurement-first (Regra 5) — o gate D3-style; honest-negative (inline não bate o M87) é terminal válido.

#### Files to edit
`benchmarks/m90_filter_bench.py` (NEW), `docs/benchmarks/m90-inline-filter.{md,json}` (NEW).

#### Concurrency tests
(none — single-threaded)

#### TDD
N/A (medição empírica — a correção é coberta por T3.1; este é o gate de valor).

#### Acceptance Criteria
**recall@10 sob filtro de label ~1% (inline v7) MEDIDO estritamente > M87 post-filter (v5)**, num benchmark reproduzível. Honest-negative fecha honesto.

#### DoD
`docs/benchmarks/m90-*.{md,json}`; sign-off council-benchmark.

## Final Phase: Integration Validation (MANDATORY)

### Execution
`cargo pgrx test pg17` completo no droplet (250+ testes); v5/v6 byte-idênticos sem label; v7 crash-safe; filtered recall == exact; o benchmark de recall inline vs M87.

### Acceptance Criteria
Suite GREEN; recall inline > M87 medido (ou honest-negative documentado); zero regressão; EXPLAIN mostra Index Cond; 3 sign-offs (index-storage, rust-pgrx, benchmark).

### If Validation Fails
Loop de volta ao `/implement` (validation halt-loop); nunca completude sobre falha (Regra 3).

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | `labels && '{…}'` chega ao amrescan como ScanKey (Index Cond) | T1.1 | opclass `&&` + amcanmulticol=true |
| 2 | label persistido nas code-pages (Stage-1 prune), v7 | T2.1 | build lê 2ª coluna + writer/reader v7 |
| 3 | inline-skip na Stage-1 antes do rerank + xs_recheck | T3.1 | parse do scan-key + skip + recheck |
| 4 | recall@10 sob filtro ~1% inline > M87 MEDIDO | T4.1 | benchmark same-data v7 vs v5 |
| 5 | zero regressão (250+ tests, v5/v6 byte-idêntico sem label) | T2.1, T3.1 | novo magic só com 2ª coluna |
| 6 | crash-safety no v7 | T2.1 | restart-scan-identical pg_test |
| 7 | boundary honesto (só label + `&&`; arbitrary-WHERE é M91) | T1.1 | o opclass `&&` define o único filtro pushed (D1); documentado |
| 8 | sign-off council-index-storage + rust-pgrx + benchmark | T4.1 | benchmark + review assinados no gate final |

**Coverage: 8/8 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] `cargo pgrx test` (pg17) green (250+ testes incl. os novos de label/v7)
- [ ] Zero type errors / lint — `cargo clippy` clean
- [ ] File-size budget (delta ≤ ~600 LoC/arquivo; `unsafe` isolado)
- [ ] CHANGELOG `[Unreleased]` atualizado (Regra 6) — `Changed` (format bump v7 + REINDEX story) + `Added` (inline label filter)
- [ ] Backward-compat: v5/v6 sem label byte-idênticos (sem REINDEX para índices sem label); v7 é opt-in (só com 2ª coluna)
- [ ] Plan-specific: recall@10 sob filtro ~1% inline > M87 MEDIDO (`docs/benchmarks/m90-*`)
- [ ] filtered recall == exact seqscan-filtered (correção); EXPLAIN Index Cond
- [ ] Sign-off council-index-storage + council-rust-pgrx + council-benchmark
- [ ] Plan archived após `/review` READY_TO_MERGE + PR merged

## Failure scenarios (I/O externo: Postgres heap scan + páginas)

- **label NULL / 2ª coluna ausente:** trata como "sem label filter" (v5/v6 path), testado em T2.1.
- **scan-key de tipo inesperado:** erro tipado no amrescan (fail-fast), nunca resultado errado silencioso.
- **restart pós-build v7:** scan idêntico (GenericXLog) — T2.1.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Format bump v7 (label nas code-pages) + REINDEX para usar label | High | novo magic só com 2ª coluna (v5/v6 sem label inalterados, sem REINDEX); gate de crash-safety | implementador |
| O inline pode não bater o recall do M87 no regime medido → honest-negative | Medium | o gate T4.1 mede antes de reivindicar; honest-negative é terminal válido | implementador |
| Boundary limitado (só label `smallint[]` + `&&`) frustra quem quer arbitrary-WHERE | Medium | documentado honesto; M91 (Custom Scan) entrega arbitrary-WHERE | implementador |
| Validação exige droplet (recall benchmark) | Low | aceito; destruir ao fim | operador |

## Unresolved Questions

- Q1 — semântica de label NULL: um vetor sem label é excluído de TODO filtro de label, ou incluído? (definir em T2.1 — provável: excluído do resultado quando há filtro, igual pgvectorscale; testar.)
- Q2 — o threshold de "1% seletividade" do benchmark é representativo? (varrer 0.5–5% em T4.1 para robustez; não bloqueia o design.)
