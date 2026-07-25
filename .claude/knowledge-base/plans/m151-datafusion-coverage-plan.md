---
slug: m151-datafusion-coverage
milestone_id: M151
created_at: 2026-07-25
goal: Rotear os agregados do ClickBench bloqueados só por `<>` pelo CustomScan DataFusion vetorizado, subindo a cobertura de 6 para ≥12 das 43 queries com A/B byte-idêntico (diverged=0).
---

# Plano — M151: ampliar a cobertura do CustomScan vetorizado (DataFusion)

## Goal

Aceitar o operador `<>` (not-equal) **em colunas numéricas/temporais** no filtro do CustomScan de agregação
DataFusion, roteando os agregados hoje bloqueados por `<>` numérico — **medido:** a fração de queries com
`columnar_customscan=True` sobe acima de 6 (número real no benchmark), com A/B byte-idêntico ao heap
(`result_ab.diverged == 0`). **Escopo revisado (measurement-first):** `<>` em TEXTO (`SearchPhrase <> ''`, a
maioria das queries `<>` do ClickBench) é honest-negative neste milestone — ver ADR-4 (requer serialização de
const-texto pelo caminho agg released, uma fatia própria). Este milestone entrega o `<>` numérico correto +
mede o ganho real.

## Context

O M148 mediu a materialização row-by-row como ~80% do scan; o caminho DataFusion (M100/M114) **nunca**
materializa heap-tuple (batch Arrow) → ataca os 80%. Mas hoje só 6/43 queries roteiam, porque
`extract_zone_predicate` (columnar_agg.rs:146) declina qualquer WHERE que não seja `col op const` com op ∈
btree strategy 1-5 `{<,<=,=,>=,>}`. O discover (`m151-datafusion-coverage-blueprint.md`, council-benchmark)
mediu que `<>` (strategy 6) é o **único** bloqueio em 9 agregados limpos (`{1,7,12,14,30,31,36,37,38}`);
adicioná-lo leva a cobertura de 6→15 (meta conservadora ≥12, a validar no benchmark). Risco semântico BAIXO:
`=`/`<>` são collation-independentes no PG; o `not_eq` do DataFusion casa a semântica SQL 3-valued (NULL).

## Prior Art & Related Work

- **Blueprint interno:** `.claude/knowledge-base/discoveries/blueprints/m151-datafusion-coverage-blueprint.md`
- **DataFusion Exact/Inexact/Unsupported** (`.claude/knowledge-base/references/datafusion/datafusion/expr/src/table_source.rs:37-51`) — `<>` é Unsupported-para-poda / aplicado-no-Filter.
- **MonetDB/X100** (`.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf`) — teoria da vetorização (tuple-at-a-time overhead).
- **M148** ([[m148-flamegraph-released]] — os 80% de materialização), **M100/M114** (o CustomScan DataFusion de agregação), **M105** (o zone-map que a poda reusa).

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/zonemap.rs` | 257 | `enum ZoneOp:16` (Lt/Le/Eq/Ge/Gt), `chunk_can_match:36`, `excluded:69` | + variante `Ne`; `excluded(Ne)=false` (never-prune, fail-safe) |
| `theodb_rs/src/am/columnar_agg.rs` | 1579 | `extract_zone_predicate:146` (strategy 1-5 → op, `_ => None`), `extract_all_predicates:216`, `Admitted:233` | strategy 6 → `ZoneOp::Ne` (deixa de declinar `<>`) |
| `theodb_rs/src/am/df_executor.rs` | 760 | `build_filter_expr:256` (`match p.op` 289-294), `run_columnar_aggs:309`, `run_columnar_grouped_aggs:392` | + braço `ZoneOp::Ne => c.not_eq(val)` |

### Current callers / dependents

- `extract_zone_predicate` (columnar_agg.rs:146, pub(crate) desde M150) — chamado por `extract_all_predicates` (agg) E por `predicates_needed` (columnar_project.rs, M150). **Cuidado:** o M150 usa o mesmo extrator para o skip do scan geral; adicionar `Ne` faz o M150 também empurrar `<>` — mas `chunk_can_match(Ne)=true` (never-prune) garante que o M150 NÃO pula chunk por `<>` (o ExecScan re-checa). Sem regressão no M150.
- `chunk_can_match` (zonemap.rs:36) — chamado pelo skip do agg (`decode_columns:825`) E pelo skip do scan geral (`decode_stripe`, M150). Ambos reusam o novo `excluded(Ne)=false`.
- `build_filter_expr` (df_executor.rs:256) — chamado por `run_columnar_aggs`/`run_columnar_grouped_aggs`.

### Domain glossary

- **btree strategy 6 (`<>`):** o operador not-equal na família btree; hoje `extract_zone_predicate` mapeia só 1-5.
- **poda vs filtro (DataFusion Inexact):** a poda (`chunk_can_match`) exclui chunk-groups por min/max; o filtro (`build_filter_expr`) aplica o predicado sobre o batch Arrow (autoridade final). `<>` entra só no filtro (nunca poda).
- **never-prune:** `chunk_can_match` retorna `true` (must-scan) para `Ne` — um chunk `[min,max]` quase sempre contém valores ≠ const; a poda por `<>` é inútil (e o caso min==max==const é micro-opt YAGNI).

### Architecture boundaries affected

`rules/architecture.md` — `columnar_agg` (admission/planner) é interface; `df_executor` (DataFusion) é infra de execução; `zonemap` é infra de leitura. O `<>` cruza a mesma fronteira já estabelecida (extrai no planner, aplica no executor). Sem novo write, sem mudança de formato de página.

## ADRs

### ADR-1 — `<>` entra só no filtro DataFusion, nunca na poda por min/max
- **Decisão:** `chunk_can_match` retorna `true` (never-prune) para `ZoneOp::Ne` via `excluded(Ne)=false`; o `<>` é aplicado só pelo `build_filter_expr` sobre o batch Arrow.
- **Rationale:** um chunk `[min,max]` com min≠max contém valores ≠ const → `<>` não exclui o chunk. Podar por `<>` seria incorreto (perderia linhas) exceto no caso degenerado min==max==const (YAGNI, Rule 11). Cita DataFusion Inexact (`table_source.rs:37-51`).
- **Alternativa rejeitada:** podar por `<>` no caso min==max==const — micro-otimização não medida como relevante; complexidade acidental.

### ADR-2 — Uma lista de predicados (com Ne) vs duas listas (poda/filtro) do blueprint
- **Decisão:** manter a lista única `Admitted.preds` (o blueprint sugeriu desacoplar em poda-list + filter-list). `chunk_can_match(Ne)=true` já isola o comportamento: Ne está na lista mas nunca poda.
- **Rationale:** KISS (Rule 10) / parsimony-ladder rung 5 — a mesma correção com menos código; `chunk_can_match` fail-safe para Ne evita a bifurcação de listas. Override honesto do ADR-2 do blueprint `m151-datafusion-coverage-blueprint.md`.
- **Alternativa rejeitada:** duas listas (blueprint ADR-2) — mais código, mais superfície de bug, sem ganho de correção sobre `chunk_can_match(Ne)=true`.

### ADR-4 — `<>` numérico agora; `<>` em texto é honest-negative (follow-up)
- **Decisão:** este milestone aceita `<>` só em colunas com `MinMaxKind` (int/float/temporal/bool). `<>` em texto (`SearchPhrase <> ''`) fica fora.
- **Rationale (2 descobertas do implement, corrigindo o blueprint):** (1) **`<>` não é uma estratégia btree** (btree só define 1-5); é detectado como o **negador de `=`** — implementado corretamente em `extract_zone_predicate`. (2) O const de texto **não cabe** no `ZonePredicate` (`const_bits: u64`) nem na serialização `custom_private` (`encode_private` usa `lappend_int` — só ints). Text `<>` exige serializar os bytes do const pelo caminho agg **released** (M114/M115) + um decode pareado — uma fatia própria com risco ao caminho released, não uma adição rushed (viola "sem workarounds" fazer sob build de 25min/iteração). Measurement-first: entregar o `<>` numérico correto + medir; text `<>` (sempre `<> ''`) é o próximo slice bem-especificado.
- **Alternativa rejeitada:** special-case `<> ''` (empty string) — gambiarra que não generaliza a `<> 'foo'`; ou rushar a serialização de texto no caminho released sob build lento — risco desproporcional.

### ADR-3 — Escopo = só `<>` (LIKE opcional) vs rotear as 36 lentas
- **Decisão:** só `<>` neste milestone; COUNT(DISTINCT)/plain-scan/regex/HAVING ficam fora (cada um é milestone próprio).
- **Rationale:** A/B byte-idêntico + measurement-first inviabiliza rotear tudo; `<>` é o maior ganho×menor risco (6→15). Cita blueprint § 5.
- **Alternativa rejeitada:** COUNT(DISTINCT) (ganho incerto, memória), plain-scan (re-materializa, M148), regex (motor PG) — honest-negative.

## Dependency Graph

```
Fase 1 (ZoneOp::Ne + excluded never-prune)  ─→  Fase 2 (extract_zone_predicate strategy 6)
                                                        │
                                              Fase 3 (build_filter_expr not_eq)
                                                        ↓
                                              Fase 4 (A/B 43 queries + cobertura medida + benchmark)
```
Fase 2 e Fase 3 dependem da Fase 1 (a variante do enum). Fase 4 fecha com o benchmark.

## Phase 1 — `ZoneOp::Ne` never-prune (fail-safe)

### T1.1 — Adicionar `ZoneOp::Ne` e `excluded(Ne)=false`

#### Why this step
O `<>` precisa de uma variante no enum `ZoneOp` para fluir do extrator ao filtro. `chunk_can_match` deve tratá-lo
como never-prune (retorna must-scan) — um `<>` nunca prova que um chunk é vazio. Raciocínio: ADR-1 — a poda é só
para ops que estreitam o range; `<>` é um filtro puro (DataFusion Inexact/Unsupported).

#### Concurrency tests
(none — single-threaded) `chunk_can_match`/`excluded` são funções puras sem estado compartilhado; sem threads.

#### Files to edit
- `theodb_rs/src/am/zonemap.rs` — `enum ZoneOp` + variante `Ne`; `excluded` ganha `ZoneOp::Ne => false`.

#### TDD
- **RED:** `test_chunk_can_match_ne_never_prunes` — `chunk_can_match(has_minmax=true, min=0, max=100, I4, ZonePredicate{Ne, const=50})` retorna `true` (must-scan), mesmo com 50 dentro de [0,100]. Falha antes da variante (não compila / não existe Ne).
- **GREEN:** adicionar `Ne` ao enum + `excluded(Ne)=false`.
- **REFACTOR:** confirmar que os testes existentes de `chunk_can_match` (Eq/Lt/etc) não mudam.

#### Acceptance criteria
- [ ] `chunk_can_match(..., Ne, ...)` retorna `true` para qualquer chunk com min/max (verificado por: `test_chunk_can_match_ne_never_prunes` passa).
- [ ] Os testes de zonemap.rs existentes continuam verdes (verificado por: `cargo test -p ... zonemap` — mesmo count).

#### DoD
- Teste passa (RED→GREEN provado no droplet); `cargo build` verde.

## Phase 2 — `extract_zone_predicate` aceita strategy 6

### T2.1 — Mapear btree strategy 6 → `ZoneOp::Ne`

#### Why this step
`extract_zone_predicate:197-204` mapeia strategy 1-5 e `_ => return None` (declina `<>`). Adicionar `6 => ZoneOp::Ne`
faz o extrator aceitar `col <> const`, deixando de declinar a query inteira. Raciocínio: é o único ponto que
bloqueia os 9 agregados (blueprint § 2).

#### Concurrency tests
(none — single-threaded) O extrator roda no planner (single-threaded); sem estado compartilhado.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — `extract_zone_predicate`: `6 => ZoneOp::Ne` no match de strategy.

#### TDD
- **RED:** `test_extract_ne_predicate` — `extract_zone_predicate` sobre um OpExpr `a <> 5` (int4) retorna `Some(ZonePredicate{Ne, 5})`. Falha antes (strategy 6 → None).
- **GREEN:** adicionar o braço `6 => ZoneOp::Ne`.
- **REFACTOR:** `a <> 'x'` (text) — o extrator já checa tipo const==coluna; `<>` em text é aceito (collation-independente, ADR-3 do blueprint).

#### Acceptance criteria
- [ ] `extract_zone_predicate` sobre `a <> 5` retorna `Some(Ne)` (verificado por: `test_extract_ne_predicate` assere op==Ne).
- [ ] Um agregado com `WHERE a <> 0` deixa de declinar e roteia ao CustomScan (verificado por: A/B em T4.1).

#### DoD
- Teste passa; um `SELECT count(*) FROM t_col WHERE a<>0` usa o CustomScan (EXPLAIN mostra `Custom Scan`).

## Phase 3 — `build_filter_expr` aplica `not_eq`

### T3.1 — Braço `ZoneOp::Ne => c.not_eq(val)`

#### Why this step
O `build_filter_expr:289-294` monta o `Filter` DataFusion sobre o batch (`Lt=>lt`, `Eq=>eq`, ...). Adicionar
`Ne=>not_eq` faz o `<>` ser aplicado sobre as linhas decodificadas (autoridade final, ADR-1). Raciocínio: sem
este braço, um predicado `Ne` extraído não teria como ser aplicado → resultado errado.

#### Failure scenarios
(none — no external I/O touched) O DataFusion roda in-process sobre o batch Arrow decodificado do heap-backed
columnar; sem HTTP/DB-driver/queue. Divergência de semântica (o risco real) é coberta pelo A/B em T4.1.

#### Concurrency tests
(none — single-threaded) O `build_filter_expr` monta uma expressão; a execução DataFusion é single-thread neste caminho (o CustomScan não é parallel-aware). Sem estado compartilhado mutável.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` — `build_filter_expr`: `ZoneOp::Ne => c.not_eq(val)`.

#### TDD
- **RED:** `test_ne_filter_ab` (via harness SQL) — `SELECT count(*) FROM t_col WHERE a <> 0` roteado ao CustomScan == heap twin. Falha se o braço Ne faltar (o match seria não-exaustivo → não compila, OU o resultado diverge).
- **GREEN:** adicionar o braço.
- **REFACTOR:** garantir que a coluna do `<>` entra na projeção do batch (`proj.push(p.col)`, df_executor.rs:241) para o Filter relê-la.

#### Acceptance criteria
- [ ] `WHERE a <> 0` roteado ao CustomScan retorna o mesmo que o heap (verificado por: A/B md5 idêntico).
- [ ] O `match p.op` em build_filter_expr é exaustivo (compila com Ne) (verificado por: `cargo build` verde).

#### DoD
- Teste A/B passa; o count com `<>` byte-idêntico ao heap.

## Phase 4 — A/B das 43 queries + cobertura medida + benchmark

### T4.1 — Gate A/B ClickBench + cobertura 6→≥12 + geomean

#### Why this step
O DoD é medido: a cobertura sobe de 6 para ≥12, A/B diverged=0 em TODA query roteada, e o geomean melhora. É o
gate measurement-first (Regra 5). Raciocínio: sem `run_m128 --agg diverged=0` nenhuma claim de cobertura/ganho.

#### Failure scenarios
(none — no external I/O touched) O benchmark é in-process; a única "falha" relevante é divergência A/B numa
classe de tipo (ex.: `<>` em text com collation) → honest-negative (não roteia essa query, documenta).

#### Concurrency tests
(none — single-threaded) `max_parallel_workers_per_gather=0` no harness (regime serial declarado).

#### Files to edit
- `docs/benchmarks/m151-datafusion-coverage.md` (NEW) — cobertura 6→N medida, A/B diverged=0, geomean.
- `docs/benchmarks/m151-artifacts/` (NEW) — JSON do run.
- `CHANGELOG.md` `[Unreleased] § Added`.

#### TDD
- **RED:** o benchmark É o teste — `run_m128_clickbench.py --agg --n 1000000` deve mostrar `columnar_customscan_count ≥ 12` (era 6) E `result_ab.diverged == 0`. Antes da implementação, count=6.
- **GREEN:** rodar no droplet; capturar o artefato.
- **REFACTOR:** listar honestamente quais das 9 queries-alvo roteiam (e se alguma divergir A/B, honest-negative documentado).

#### Acceptance criteria
- [ ] `columnar_customscan_count ≥ 12` das 43 (era 6) (verificado por: o JSON do run).
- [ ] `result_ab.diverged == 0` em toda query roteada (verificado por: o oráculo A/B do run_m128).
- [ ] CHANGELOG `[Unreleased]` atualizado (verificado por: `grep -c M151 CHANGELOG.md` ≥ 1).
- [ ] Droplet destruído ao fim (verificado por: 0 droplets efêmeros).

#### DoD
- `docs/benchmarks/m151-datafusion-coverage.md` com a cobertura medida (6→≥12) + diverged=0; artefato salvo.

## Coverage Matrix

| Requisito (DoD do ROADMAP) | Task |
|---|---|
| CustomScan cobre ≥ N classes adicionais além de agregação simples | T2.1, T3.1 (a classe `<>`) |
| Cobertura medida: fração com columnar_customscan sobe de 6 para ≥ meta | T4.1 |
| A/B byte-idêntico vs heap em TODA query roteada ao DataFusion | T3.1, T4.1 |
| Geomean geral do ClickBench melhora vs baseline, número real | T4.1 |
| CHANGELOG `[Unreleased]` | T4.1 |
| `<>` nunca poda chunk (só filtra) — correção do zone-map | T1.1 (ADR-1) |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Divergência semântica DataFusion vs PG numa classe de tipo (`<>` em text/collation, NULL) | ALTA | A/B byte-idêntico obrigatório (diverged=0); honest-negative por-query se divergir (não roteia) | implementer |
| `<>` acidentalmente poda um chunk (perde linha) via o extrator compartilhado com o M150 | ALTA | `chunk_can_match(Ne)=true` (never-prune) — T1.1 prova; o M150 reusa o mesmo extrator mas nunca pula por `<>` | reviewer |
| Ganho de tempo menor que o esperado (queries de texto largo têm decode real) | MÉDIA | Measurement-first — reportar o geomean real, não prometer um "×"; a cobertura (6→≥12) é o DoD primário | implementer |

## Unresolved Questions

- (none — every decision is resolved at plan time). Rationale: `<>` só no filtro (ADR-1), uma lista (ADR-2 override do blueprint), escopo só `<>` (ADR-3) — todas fechadas no blueprint + baseline.

## Global DoD

- [ ] Todos os testes verdes (provados RED→GREEN no droplet — cargo pgrx test não linka localmente).
- [ ] `cargo clippy` limpo; `match p.op` exaustivo.
- [ ] `run_m128_clickbench --agg` diverged=0 + `columnar_customscan_count ≥ 12`.
- [ ] Benchmark medido em `docs/benchmarks/m151-*`.
- [ ] `/code-quality` verdict ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG `[Unreleased]` atualizado.
- [ ] Droplet efêmero destruído.

## Plan-confidence note

Structural verdict SHIPPABLE_WITH_CAVEATS: coverage matrix 100%, baseline context completo, AC executáveis
(acceptable_ratio 0.9), concurrency posture declarada, drawbacks (3) + unresolved (none) completos, ADRs com
alternativas, zero citação fabricada. Os 2 caps residuais (`auditor_unavailable_cargo-udeps`,
`symbol_fab_unverifiable_rust`) são ambientais — a verificação de símbolos rust via pgrx não linka localmente
(mesma situação do M149/M150 que shipparam). O gate REAL de code-quality rust roda no droplet durante o
`/implement`. Não são defeitos do plano.

## Final Phase — Integration Validation

Após as 4 fases: build no droplet, rodar a suíte de testes + `run_m128 --agg` (diverged=0, count≥12) + o geomean.
A cadeia só está completa quando o A/B é byte-idêntico E a cobertura sobe de 6 para ≥12. Divergência A/B numa
query → honest-negative (documenta, não roteia). Falha → volta ao `/implement` (não editar o plano).
