---
slug: m46-hnsw-highrecall-qps
milestone_id: M46
created_at: 2026-07-04
goal: Fechar o déficit de QPS do theodb_hnsw no alto recall (SIFT1M 1M×128, ef≥200) tornando as três estruturas per-query do scan pre-sized e eliminando a alocação-por-nó (recall-neutro), com veredito por re-run do Pareto mean±std (effect>variância).
---

# Plano — M46: fechar o gap de QPS do theodb_hnsw no alto recall (scan hot-path hygiene)

## Goal

Fechar o déficit de QPS do `theodb_hnsw` no alto recall (SIFT1M 1M×128, ef≥200) tornando as três
estruturas per-query do scan **pre-sized** e eliminando a **alocação-por-nó**, sem mudar recall — meta
observável: **theodb_hnsw QPS ≥ pgvector a recall ≥ 0.993 no Pareto re-medido (mean±std, effect>variância)
OU honest-negative documentado com a variância de QPS a ef≥200 reduzida de ~44% para < 15%.**

Métrica única: o veredito do `benchmarks/run_m45_pareto.py` re-rodado pós-mudança (recall@10 idêntico ±0,
QPS mean±std, gate effect>variância) — persistido em `docs/benchmarks/m46-*.{md,json}`.

## Context

Disparado por `docs/benchmarks/m45-pareto-sift1m.md` (mean±std, exact GT): theodb_hnsw = PARIDADE com
pgvector, com déficit no alto recall (0.58× a recall 0.9932, effect>variância=sim). Blueprint da discovery:
`.claude/knowledge-base/discoveries/blueprints/m46-hnsw-highrecall-qps-blueprint.md` (3 council agents leram
o código real + peers SOTA). Decisões resolvidas na discovery:
- **Causa-raiz (código):** overhead per-query **acidental** que escala com ef — `visited: HashSet::new()`
  (SipHash + capacity 0 → ~12 rehashes a ef=200), heaps sem `with_capacity`, `Vec<Addr>` por nó expandido.
  É memory-bound; a distância já é SIMD (M41). Complexidade acidental que os peers SOTA não pagam.
- **Achado de medição (honestidade):** o ponto ef=200 do M45 é parcialmente **ruído** — theodb ef=400
  (44.8 QPS) é mais rápido que ef=200 (43.5 QPS), fisicamente impossível para custo real → outlier de dev
  box contendida. O sinal confiável é o mid-band (ef=100: theodb 139.9±2.8 vence pgvector 108.6±1.6). Por
  isso `measurement-first` (re-medição endurecida) é parte do DoD (ADR-2 do blueprint).

## Prior Art & Related Work

- Blueprint da discovery M46 (acima) — Coverage Corners 1-4 + ADRs.
- **pgvector** `references/pgvector/src/hnswutils.c` — `tidhash_create(CurrentMemoryContext, ef*m*2, NULL)`
  (`:675`), murmur `hash_tid` (`:54`), stack `indextids[HNSW_MAX_M*2]` + `unvisited` palloc-once (`:799,834`).
- **pgvectorscale** `references/pgvectorscale/.../access_method/graph/mod.rs:109-111` — `HashSet::with_capacity`
  (hasher **default**), `BinaryHeap::with_capacity`, `Vec::with_capacity`, todos dimensionados a
  `search_list_size·neigbors`. **Prova que pre-size com hasher default (zero nova dep) é padrão SOTA.**
- TheoDB IVF path — `scan.rs:42-44,53,76` já reusa o heap via ScanState (padrão interno para o next-seed C).

## Baseline Context

### Files that will be touched

| File | LoC today | Last commit (sha) | Why / Invariants |
|---|---|---|---|
| `theodb_rs/src/am/hnsw_page.rs` | 643 | `66c05b9` 2026-07-03 | `traverse:479-552` (estruturas per-query `:518-520`), `decode_neighbors:200`, `neighbors_of:461-474`. Invariante: recall-neutro (ordem de visita preservada). |
| `theodb_rs/src/am/scan.rs` | 334 | `8e2bdab` 2026-07-02 | `scan_hnsw_structured:115` (único caller de `traverse`). Só leitura de contexto — não editado no código, apenas referência. |
| `benchmarks/run_m46_highrecall.py` | (NEW) | — | driver de re-medição Pareto endurecido (median ≥5 runs + pages_read). |
| `benchmarks/tests/test_run_m46_highrecall.py` | (NEW) | — | teste de estrutura do driver + gate integration. |
| `docs/benchmarks/m46-highrecall-qps.{md,json}` | (NEW) | — | o artefato de dado validante. |
| `CHANGELOG.md` | — | — | entry `[Unreleased] § Changed`. |
| `ROADMAP.md` | — | — | novo milestone M46. |

### Current callers / dependents
`traverse` (`hnsw_page.rs:479`) é chamado por `scan_hnsw_structured` (`scan.rs:115,131`) — único caller de
produção; `amgettuple`/`amrescan` consomem o resultado. `neighbors_of`/`decode_neighbors` são internos ao
`hnsw_page.rs` (chamados em `:501,529` e `:471`). Nenhum caller cross-repo. A mudança é **local ao scan
path**; build/insert/vacuum (`hnsw_page.rs::write_structured`, `rewrite_structured`) intocados.

### Domain glossary
- **ef_search**: tamanho da lista de candidatos no ground layer (mais alto → mais recall, mais trabalho).
- **visited/cands/result**: as 3 estruturas per-query (`hnsw_page.rs:518-520`) — hashset de dedup, min-heap
  a expandir, max-heap dos ef melhores.
- **m0**: grau máximo de vizinhos no ground layer (meta.m0); m no upper.
- **recall-neutro**: a mudança preserva a ordem/resultado byte-exato por seed fixa (só muda alocação).

### Architecture boundaries affected
`hnsw_page.rs` é infra (adapter do Index AM). A mudança não cruza fronteira (não toca domínio/SQL/api). Sem
nova dep → sem mudança em `Cargo.toml` (parsimony rung 5 — âncora pgvectorscale usa hasher default).

## ADRs

### ADR-1 — Escopo = pre-size (A) + eliminar alloc-por-nó (B), ambos recall-neutros zero-dep
**Decisão:** L1-A (pre-size das 3 estruturas per-query) + L1-B (scratch `Vec<Addr>` reusado no ground loop
em vez de um novo por nó). Zero nova dependência.
**Rationale (cita `parsimony-ladder.md` rung 4-5, `discover-phd-rigor.md` R1-R2):** ambos são complexidade
**acidental** medida (a discovery provou), recall-neutros, cirúrgicos (`hnsw_page.rs:518-520` + `:200/529`).
Atacam a causa #1 (rehash super-linear + allocator churn = a variância) com âncora SOTA de 2 fontes.
**Alternativas rejeitadas:**
- **FxHashSet/ahash (nova dep):** ganho per-op marginal sobre pre-size; pgvectorscale prova que pre-size com
  hasher **default** já é SOTA. Rejeitada por parsimony (rung 4 — não adicionar dep redundante). Fica como
  sub-lever se a re-medição mostrar o hash per-op como gargalo (improvável — distância domina).
- **Hoist para ScanState (lever C, cross-query reuse):** muda a assinatura de `traverse`; ganho marginal
  (só a alocação inicial, não o rehash). Next-seed.
- **Prefetch (L4) / SBQ-in-graph (L5):** essenciais mas — L4 só ajuda sob pressão de cache (ataca variância,
  não throughput warm); L5 tem risco de recall + esforço ~M22. Next-seeds documentados no blueprint.

### ADR-2 — Measurement-first é gate do DoD (anti-sunk-cost)
**Decisão:** re-medir o baseline ANTES da mudança (Task 1) e re-medir DEPOIS (Task 3), ambos com metodologia
endurecida (median de ≥5 runs, drop de outliers, `THEODB_SCAN_PROFILE=1` p/ `pages_read` determinístico) e
veredito por **effect>variância**.
**Rationale (cita `analysis-golden-rule.md` rigor + `public-copy.md`):** o ponto ef=200 do M45 é ruidoso
(ef200<ef400). Sem re-medição limpa, otimizar seria perseguir um artefato = re-trabalho. `pages_read` prova
o recall-neutro (deve ser idêntico) independente do wall-clock ruidoso.
**Alternativa rejeitada:** confiar no M45 e só implementar — rejeitada (perseguiria variância).

## Dependencies

### Existing — use as-is
| Package | Version | Ecosystem | Why |
|---|---|---|---|
| (nenhuma nova) | — | rust | pre-size usa `std::collections` (hasher default) — âncora pgvectorscale |

### New — to be introduced
| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | — | — | FxHashSet/ahash/smallvec avaliados e **rejeitados** por parsimony (pre-size zero-dep basta) | — |

### Removed
| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency Graph

```
Task 1 (baseline re-medição) ──┐
                               ├─→ Task 3 (re-medição pós + veredito)
Task 2 (L1-A + L1-B code) ─────┘
```
Task 1 e Task 2 podem paralelizar (medição vs código). Task 3 depende de ambas (precisa do baseline + do
código otimizado buildado). Task 4 (integration validation) fecha.

## Phase 1 — Implementação recall-neutra + medição

### Task 2.1 — L1-A: pre-size visited/cands/result + L1-B: scratch de neighbors reusado

#### Why this step
**Ação:** em `hnsw_page.rs:518-520`, dimensionar `visited = HashSet::with_capacity(ef*m0*2)`,
`cands = BinaryHeap::with_capacity(ef*m0)`, `result = BinaryHeap::with_capacity(ef+1)`; e no ground loop
(`:529`) usar um `scratch: Vec<Addr>` reusado (clear+preenche) em vez do `Vec` novo por nó de
`decode_neighbors` (`:200`).
**Raciocínio:** a discovery (3 agents) mediu que essas alocações fresh + os ~12 rehashes SipHash a ef=200
são a complexidade acidental que injeta a variância de 44% e o custo super-linear em ef (ADR-1). pgvectorscale
`graph/mod.rs:109-111` faz exatamente esse pre-size com hasher default; pgvector reusa o scratch de neighbors
(`hnswutils.c:834`). É recall-neutro por construção (só muda alocação, não a ordem de visita).

#### Files to edit
- `theodb_rs/src/am/hnsw_page.rs` (`traverse:518-520` pre-size; `:524-543` scratch reusado;
  `decode_neighbors:195-210` variante que escreve num `&mut Vec<Addr>`).

#### Deep file dependency analysis
`traverse` é chamado só por `scan_hnsw_structured` (`scan.rs:131`). `decode_neighbors` é chamado por
`neighbors_of` (`:471`) — se eu adicionar uma variante `decode_neighbors_into(&mut Vec)`, `neighbors_of`
passa o scratch. O resultado de `traverse` (Vec<(tid,dist)> ordenado, `:551`) é inalterado. Nenhum outro
consumidor.

#### TDD
- **RED:** `test_traverse_presize_is_recall_neutral` (pg_test em `hnsw_page.rs` ou `am/mod.rs`): construir um
  índice pequeno determinístico (seed fixa), rodar `traverse` com ef alto, capturar o `Vec<(tid,dist)>`
  ANTES da mudança (baseline hard-coded no teste), asseverar que APÓS a mudança a saída é **byte-idêntica**
  (mesma ordem de tids + mesmas distâncias). Assertion: `assert_eq!(result_after, result_expected)`.
- **RED (borda/negativo):** `test_traverse_ef_zero_is_clamped` — ef_search=0 → `ef.max(1)` (já em `:490`),
  sem panic, retorna ≤1 resultado (erro tipado, não crash — `testing.md §4.1`).
- **RED (equivalência L1-B, EC-1 do edge-case review):** `test_decode_neighbors_into_matches_original` — p/
  um buffer de neighbor tuple fixo, `decode_neighbors_into(&mut v)` produz `Vec<Addr>` byte-idêntico ao
  `decode_neighbors` original. Pega o bug do scratch-não-limpo na unidade (não só no end-to-end).
- **GREEN:** aplicar o pre-size (3 linhas) + o scratch reusado (variante `_into`), mínimo que passa.
- **REFACTOR:** extrair `ef*m0*2` num binding nomeado com comentário citando a âncora pgvector `ef*m*2`.

#### Concurrency tests
(none — single-threaded) — `traverse` opera sobre estruturas per-query stack-local; o `index_shared` lock
(`scan.rs`) já serializa contra VACUUM e é intocado. Nenhum estado compartilhado novo.

#### Acceptance criteria
- `traverse` retorna `Vec<(tid,dist)>` byte-idêntico antes/depois por seed fixa (`assert_eq!` no teste-âncora).
- `pages_read` (THEODB_SCAN_PROFILE) reporta valor idêntico antes/depois ao mesmo ef (`assert_eq!` prova ordem de visita inalterada).
- `test_traverse_ef_zero_is_clamped` verifica que ef_search=0 retorna `≤ 1` resultado sem panic (erro/clamp tipado).
- `cargo pgrx test` sai com `0` falhas (suíte pg_test + coexistência M20–M22).

#### DoD
- `cargo build` no container OK; os pg_tests do scan verdes.
- Diff ≤ ~40 linhas em `hnsw_page.rs` (mudança cirúrgica; file continua ≤ budget 500? hnsw_page é 643, já
  acima — a mudança **não aumenta** materialmente; não é o alvo de split deste milestone, documentar).

### Task 1.1 — Baseline re-medição endurecida (measurement-first)

#### Why this step
**Ação:** criar `benchmarks/run_m46_highrecall.py` que reusa o harness Pareto (`m45_pareto`/`m45_report`)
mas com metodologia endurecida: ≥5 runs, reporta **median** além de mean±std, drop de outliers, e captura
`pages_read` por ef (via THEODB_SCAN_PROFILE). Rodar no HEAD atual (baseline) antes da mudança.
**Raciocínio:** ADR-2 — o ponto ef=200 do M45 é ruidoso; preciso do baseline confiável para o effect>variância
do veredito e para quantificar quanto do "gap" some com metodologia limpa. `pages_read` é o sinal
determinístico que a discovery apontou (`hnsw_page.rs:547`).

#### Files to edit
- `benchmarks/run_m46_highrecall.py` (NEW) — reusa `theodb_bench.recall` + `m45_pareto`.
- `benchmarks/tests/test_run_m46_highrecall.py` (NEW).

#### Deep file dependency analysis
Reusa `m45_pareto.py`/`m45_report.py` (existentes, do M45). Não toca código de engine. `theodb_bench.recall`
já provê o cálculo de recall@10 vs GT exato.

#### TDD
- **RED:** `test_run_m46_emits_median_and_pages_read` — o driver, rodado em modo estrutura (sem 1M), emite um
  dict com chaves `ef, recall, qps_mean, qps_std, qps_median, pages_read, effect_gt_variance`.
- **GREEN:** implementar o driver reusando o M45.
- **REFACTOR:** deduplicar contra `m45_report` (DRY — reusar o render de tabela).

#### Concurrency tests
(none — single-threaded) — driver Python sequencial.

#### Failure scenarios
- **DB down / container ausente:** o driver falha-rápido com erro claro (não trava) — reusa o handling do
  harness M45 (que já trata isso). Explícito: `(external I/O = o container Postgres; failure = conexão
  recusada → erro tipado do psycopg, não hang)`.

#### Acceptance criteria
- Baseline persistido: `docs/benchmarks/m46-highrecall-qps.json` com o baseline pré-mudança (ef sweep,
  median + mean±std + pages_read).

#### DoD
- Driver roda no container; JSON baseline gerado; teste de estrutura verde.

## Phase 2 — Veredito

### Task 3.1 — Re-medição pós-mudança + veredito honesto (effect>variância)

#### Why this step
**Ação:** re-rodar `run_m46_highrecall.py` sobre o binário com L1 aplicado; comparar contra o baseline da
Task 1; gate effect>variância; escrever `docs/benchmarks/m46-highrecall-qps.md` com o veredito por knob.
**Raciocínio:** o Goal é medido, não opinado (`public-copy.md`). O veredito é honesto: (a) QPS ≥ pgvector a
recall ≥0.993, OU (b) honest-negative com variância reduzida <15% → next-seed L4/L5.

#### Files to edit
- `docs/benchmarks/m46-highrecall-qps.{md,json}` (final, com baseline vs pós).

#### TDD
- **RED:** `test_m46_verdict_present_and_grounded` — o `.md` contém a tabela baseline-vs-pós, o effect>variância
  por ef, e um veredito explícito (SUPERIOR/PARIDADE/INFERIOR + variância antes/depois). (doc-check, não integration).
- **GREEN:** gerar o report.

#### Acceptance criteria (o DoD do milestone)
- **Baseline (T1.1) e pós (T3.1) medidos back-to-back na mesma sessão** (EC-4 do edge-case review) — o ruído
  da dev box afeta ambos igualmente; documentar isso explicitamente no report.
- recall@10 **idêntico** baseline vs pós (recall-neutro provado — se divergir, é BUG, não otimização).
- `pages_read` idêntico por ef (ordem de visita preservada).
- Veredito honesto por knob com mean±std + median + effect>variância; caveat de hardware explícito.
- Meta batida OU honest-negative documentado com variância a ef≥200 reduzida de ~44% p/ <15%.

## Coverage Matrix

| # | Gap / Requirement (Goal/blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Pre-size das 3 estruturas per-query (L1-A) | T2.1 | `with_capacity` em `hnsw_page.rs:518-520` |
| 2 | Eliminar alloc-por-nó (L1-B) | T2.1 | scratch `Vec<Addr>` reusado no ground loop |
| 3 | Recall-neutro provado (ordem byte-exata + pages_read idêntico) | T2.1 | teste-âncora + property `decode_neighbors_into` |
| 4 | Bordas (ef=0/1 → clamp/erro tipado, não crash) | T2.1 | teste `test_traverse_ef_zero_is_clamped` |
| 5 | Measurement-first: baseline endurecido (median, pages_read) | T1.1 | driver `run_m46_highrecall.py` |
| 6 | Re-medição pós + veredito effect>variância | T3.1 | comparação baseline-vs-pós |
| 7 | Artefato de dado `docs/benchmarks/` + `.json` | T1.1 | `m46-highrecall-qps.{md,json}` |
| 8 | Veredito honesto (SUPERIOR/PARIDADE/INFERIOR, sem cherry-pick) | T3.1 | seção veredito por knob |
| 9 | Coexistência M20-M22 verde | T2.1 | DoD da task (suíte pg_test) |
| 10 | CHANGELOG `[Unreleased]` + milestone M46 no ROADMAP | T3.1 | Global DoD |

**Coverage: 10/10 gaps covered (100%)**

## Drawbacks & Risks

| Risco | Sev | Mitigação | Owner |
|---|---|---|---|
| Ruído da dev box limita o sinal (o próprio motivo do M46 existir) | MÉDIO | median ≥5 runs + drop outliers + `pages_read` determinístico + caveat de hardware explícito no report | eng |
| L1 pode não mover o throughput warm (se o gap era mesmo ruído) | MÉDIO | honest-negative aceito (ADR-2): a vitória alternativa é variância <15% + win de mid-band preservado; L4/L5 viram next-seed | eng |
| Regressão de recall (mudança de ordem) | BAIXO→nulo | recall-neutro por construção; teste-âncora byte-exato + pages_read idêntico bloqueiam qualquer divergência | eng |
| `hnsw_page.rs` já > budget 500 LoC (643) | BAIXO | a mudança é cirúrgica (≤40 linhas), não aumenta materialmente; split é fora de escopo (não é regressão nova) | eng |

## Unresolved Questions

(none — every decision is resolved at plan time: escopo A+B zero-dep (ADR-1), measurement-first (ADR-2),
FxHash/prefetch/SBQ explicitamente next-seeds. A única incerteza — quanto do gap é ruído — é resolvida
empiricamente pela Task 1/3, que é o próprio DoD.)

## Failure scenarios

- **Container Postgres indisponível durante o benchmark:** o driver falha-rápido (psycopg connection refused
  → erro tipado, não hang) — herda o handling do harness M45. Coberto em T1.1.
- **Índice vazio / dim=0:** `traverse` já short-circuita (`hnsw_page.rs:485`) → `Ok(vec![])`, sem panic.
- (Sem outra I/O externa — o único I/O é o buffer manager interno do PG, cujo error path via `with_page_item`
  → `Result` é intocado pela mudança.)

## Global DoD

- Todos os pg_tests + integration verdes no container; `cargo build` limpo.
- Recall@10 idêntico baseline vs pós (recall-neutro).
- Benchmark reproduzível em `docs/benchmarks/m46-*.{md,json}` com veredito honesto.
- `/code-quality` ∉ {FAIL_HARD, INVALID}; `/review` READY_TO_MERGE.
- CHANGELOG `[Unreleased]` atualizado; milestone M46 no ROADMAP; sem `Co-Authored-By`.
- File-size: mudança cirúrgica, não piora o budget.

## Final Phase — Integration Validation

Rodar a suíte completa no container (pytest integration + pg_tests) + o benchmark end-to-end (baseline →
mudança → re-medição → veredito). O plano NÃO está completo até: (a) recall idêntico provado, (b) veredito
honesto escrito com dados, (c) suíte verde. "Eat your own cooking": se o recall divergir 1 tid, é BUG e o
milestone falhou.
