---
slug: m150-chunk-group-filtering
milestone_id: M150
created_at: 2026-07-25
goal: Pular chunk-groups por min/max no scan geral colunar, medindo ≥80% de chunks pulados e ≥5× de ganho numa query seletiva, com A/B byte-idêntico ao heap.
---

# Plano — M150: Chunk-group filtering no scan geral (theodb_columnar)

## Goal

Empurrar predicados `col op const` ao scan geral do `theodb_columnar` e pular chunk-groups cujo min/max prova não
conter match — **medido:** uma query seletiva (`WHERE EventDate = X`) sobre 1M linhas pula **≥ 80%** dos chunks e
melhora **≥ 5×** vs baseline, com A/B byte-idêntico ao heap (diverged=0).

## Context

O M148 mediu a materialização row-by-row como ~80% do scan; o M149 (released v0.141.0) cortou as colunas
não-projetadas via CustomScan. O M150 corta as **linhas** que não podem casar o WHERE, pulando chunk-groups
inteiros pelo zone-map min/max (M105) sem descomprimi-los. Decisões resolvidas no blueprint
`m150-chunk-group-filtering-blueprint.md` (discover council-index-storage, evidência primária Citus): teste direto
`chunk_can_match` (não theorem-prover), extração best-effort (não all-or-nothing), side-channel paralelo ao M149.

## Prior Art & Related Work

- **Blueprint interno:** `.claude/knowledge-base/discoveries/blueprints/m150-chunk-group-filtering-blueprint.md`
- **Citus chunk-group filtering** (`.claude/knowledge-base/references/citus/src/backend/columnar/columnar_reader.c:1132` `SelectedChunkMask`, `columnar_customscan.c:760` `ExtractPushdownClause`) — best-effort + skip por min/max.
- **M105** (zone-map `directory_minmax`), **M149** ([[m149-projection-pushdown-released]] — side-channel + CustomScan reusados).

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | 1579 | tem `extract_zone_predicate:146`, `flip_op:118`, `encode_const_bits:130` (privados) | promover 3 fns a `pub(crate)` (DRY) |
| `theodb_rs/src/am/columnar_project.rs` | 711 | CustomScan M149 (side-channel `SCAN_PROJECTION:54`, `scan_projection:69`) | + `SCAN_PREDICATES` irmão + extração best-effort no begin/exec |
| `theodb_rs/src/am/columnar.rs` | 2013 | `decode_stripe:698`, `load_next_batch:1045`, skip já em `decode_columns:775` | `decode_stripe` recebe `predicates` + guard de skip; `load_next_batch` busca preds |
| `theodb_rs/src/am/zonemap.rs` | 257 | `chunk_can_match:36`, `ZonePredicate:26`, `ZoneOp:16` (pub(crate)) | reusar as-is (sem mudança, ou novo lar do extrator) |

### Current callers / dependents

- `decode_stripe` chamado por `load_next_batch:1045` (único caller, scan geral). `decode_columns` chamado pelo caminho DataFusion/agg (independente — não muda).
- `chunk_can_match` chamado hoje só por `decode_columns:825`. `extract_zone_predicate` chamado só dentro de columnar_agg.rs.
- `SCAN_PROJECTION`/`scan_projection` — o novo `SCAN_PREDICATES` segue o mesmo caller shape (begin instala, load_next_batch lê).

### Domain glossary

- **chunk-group (cg):** unidade de compressão do TCS1; um `ChunkDirEntry` por `(cg, coluna)` com `has_minmax/min_bits/max_bits`.
- **skip de admissão:** pular a descompressão de um cg que o min/max prova não conter match; o ExecScan re-checa o WHERE completo (autoridade final).
- **best-effort pushdown:** empurrar só os predicados `col op const` simples; o resto passa reto e é re-checado acima.

### Architecture boundaries affected

`rules/architecture.md` — o CustomScan é interface (planner-facing); `decode_stripe`/`zonemap` são infra (leitura). O skip não cruza fronteira nova: consome o diretório que `compute_minmax` (codec) já escreve. Sem novo write, sem magic bump (formato de página imutável).

## ADRs

### ADR-1 — Teste direto (`chunk_can_match`) vs theorem-prover do PG (Citus)
- **Decisão:** usar o teste direto das 5 estratégias btree que já existe em `zonemap.rs:36`.
- **Rationale:** cobre `col op const` (o alvo do DoD), KISS (Rule 10), sem nova dependência. Cita `parsimony-ladder.md` rung 4 (reusar o que existe).
- **Alternativa rejeitada:** portar `predicate_refuted_by` (Citus `columnar_reader.c:1177`) — mais geral para OR composto, mas complexidade acidental não pedida pelo DoD (YAGNI, Rule 11).

### ADR-2 — Extração best-effort (Citus) vs all-or-nothing (o agg do M114)
- **Decisão:** `filter_map` sobre `plan.qual` — empurra o subset empurrável, ignora o resto.
- **Rationale:** no scan geral o `ExecScan` re-checa o WHERE completo (`columnar_project.rs:471`), então empurrar parcial é correto e captura mais casos (`a=X AND lower(b)=Y` ainda pula por `a=X`). Espelha `ExtractPushdownClause` do Citus (`columnar_customscan.c:808`).
- **Alternativa rejeitada:** exigir todos empurráveis (como `extract_all_predicates` do agg, que substitui o WHERE) — perderia o skip em qualquer WHERE misto.

### ADR-3 — Side-channel paralelo (`SCAN_PREDICATES`) vs estender a tupla do `SCAN_PROJECTION`
- **Decisão:** `SCAN_PREDICATES` irmão, mesma keying por scandesc, mesmo `ActiveGuard`/registry/limpeza xact-subxact.
- **Rationale:** menor blast radius — o caminho de projeção do M149 (released, estável) fica intocado. Herda toda a disciplina de correção ABA/nested-scan do M149 (a lição da memória [[m149-projection-pushdown-released]]: side-channel keyed-por-ponteiro sincronizado em todos os ramos do begin).
- **Alternativa rejeitada:** mudar a tupla `(usize, Rc<Vec<usize>>)` existente — toca código released.

## Dependency Graph

```
Fase 1 (promover extrator a pub(crate)) ─→ Fase 2 (side-channel SCAN_PREDICATES + extração no CustomScan)
                                                        │
Fase 1 ─→ Fase 3 (skip em decode_stripe + load_next_batch) ──┘
                                                        ↓
                                        Fase 4 (métrica + benchmark + A/B integração)
```
Fase 2 e Fase 3 dependem ambas da Fase 1; podem ser desenvolvidas em paralelo mas commitadas em ordem. Fase 4 fecha.

## Phase 1 — Promover o extrator de predicados a `pub(crate)` (DRY)

### T1.1 — Expor `extract_zone_predicate`/`flip_op`/`encode_const_bits`

#### Why this step
O CustomScan de projeção (columnar_project.rs) precisa extrair `ZonePredicate` do `plan.qual`, mas o extrator vive
privado em columnar_agg.rs. Promovê-lo a `pub(crate)` evita duplicar a lógica de normalização `Const op Var` +
resolução de estratégia btree (que já casa `compute_minmax`) — DRY, Rule 9. Raciocínio: um extrator, dois
consumidores (agg all-or-nothing + scan best-effort); reimplementar seria coupling acidental (ADR-2 do blueprint).

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — mudar `unsafe fn extract_zone_predicate`, `fn flip_op`, `unsafe fn encode_const_bits` de privado para `pub(crate)`.

#### TDD
- **RED:** `test_extract_zone_predicate_pubcrate_visible` — um teste em `columnar_project.rs` (ou módulo `am`) que chama `crate::am::columnar_agg::extract_zone_predicate(...)` sobre um `OpExpr` sintético `col = 5` e assere `Some(ZonePredicate{col, op: Eq, bits})`. Compila-falha hoje (privado) → RED. Given um Node OpExpr `Var(attno=1) = Const(5::int4)`, When `extract_zone_predicate(node, relid)`, Then `Some` com `col==0, op==Eq`.
- **GREEN:** trocar `fn` → `pub(crate) fn` nas 3 assinaturas.
- **REFACTOR:** garantir que `ZonePredicate`/`ZoneOp`/`MinMaxKind` já `pub(crate)` (estão, zonemap.rs:16,26).

#### Concurrency tests
(none — single-threaded) Sem estado mutável compartilhado entre threads: o side-channel é `thread_local` e o CustomScan não é `parallel_safe` (mesma limitação honesta do M149). A correção relevante é reuso de endereço (ABA), coberta por teste de subxact-abort em T2.1 — não por race.

#### Acceptance criteria
- [ ] `cargo build` sai com exit 0 com `extract_zone_predicate` chamado de `columnar_project.rs` (verificado por: `cargo build 2>&1 | grep -c error` == 0).
- [ ] Os testes de agg de columnar_agg.rs continuam verdes sem edição (verificado por: `cargo test columnar_agg` — mesmo count de testes passando antes e depois).
- [ ] `cargo clippy` sai com exit 0 sem warning `unused` (verificado por: `cargo clippy 2>&1 | grep -c warning` == 0).

#### DoD
- `grep -n "pub(crate) unsafe fn extract_zone_predicate" columnar_agg.rs` resolve; build verde.

## Phase 2 — Side-channel `SCAN_PREDICATES` + extração best-effort no CustomScan

### T2.1 — `SCAN_PREDICATES` thread_local + registro por-nó (espelho do M149)

#### Why this step
Os predicados extraídos precisam chegar ao `load_next_batch` (que não vê o plano). O M149 já resolveu o problema
idêntico para `wanted` com um side-channel keyed por scandesc + ActiveGuard RAII + limpeza xact/subxact. Espelhar
esse idioma (SCAN_PREDICATES paralelo) herda a correção ABA já provada. Raciocínio: ADR-3 — não tocar a tupla do
SCAN_PROJECTION (released); um irmão isola o blast radius e reusa a disciplina.

#### Concurrency tests
(none — single-threaded) O side-channel é `thread_local`; o CustomScan não é `parallel_safe` (mesma limitação
honesta do M149). Sem threads compartilhando o registry; a correção é sobre reuso de endereço (ABA), coberta por
teste de subxact-abort, não por race.

#### Files to edit
- `theodb_rs/src/am/columnar_project.rs` (NEW code) — `thread_local! SCAN_PREDICATES: RefCell<Option<(usize, Rc<Vec<ZonePredicate>>)>>`; `scan_predicates(scandesc) -> Option<Rc<Vec<ZonePredicate>>>`; registro `NODE_PREDICATES` + limpeza nos mesmos callbacks xact/subxact do M149; `ActiveGuard` estendido para restaurar ambos (proj + preds).

#### TDD
- **RED:** `test_scan_predicates_keyed_by_scandesc` — instala preds para scandesc A, verifica `scan_predicates(A)==Some`, `scan_predicates(B)==None`. Falha antes da fn existir.
- **RED:** `test_subxact_abort_no_stale_predicate` — DO block com subxact-abort (PL/pgSQL EXCEPTION) que instala preds e aborta; depois um SELECT que reusa o endereço com fallback → **não herda preds stale** (A/B vs heap). Espelha `test_subxact_abort_no_stale_projection` do M149 (a regressão HIGH-1).
- **GREEN:** implementar o side-channel + sincronizar o registry em AMBOS os ramos do begin (`None => registry_remove`) — a lição LOCKED do M149.
- **REFACTOR:** fatorar o registro proj+preds num helper comum se reduzir duplicação sem obscurecer.

#### Acceptance criteria
- [ ] `scan_predicates(A)` retorna `Some` e `scan_predicates(B)` retorna `None` (verificado por: `test_scan_predicates_keyed_by_scandesc` passa).
- [ ] Após subxact-abort, um SELECT que reusa o endereço retorna A/B byte-idêntico ao heap (verificado por: `test_subxact_abort_no_stale_predicate` passa).
- [ ] Os 7 pg_test do M149 (projeção) continuam passando sem edição (verificado por: `cargo pgrx test columnar_project` — 7/7).

#### DoD
- Os 2 testes pg_test passam no droplet (RED→GREEN provado); `cargo build` verde.

### T2.2 — Extração best-effort de `plan.qual` no `begin_custom_scan`

#### Why this step
O `begin_custom_scan` já computa `wanted` do `targetlist ∪ qual`; agora extrai também os `ZonePredicate`
empurráveis do `qual` (best-effort, `filter_map`) e os instala no side-channel. Raciocínio: ADR-2 — o ExecScan
re-checa o WHERE, então empurrar o subset é correto e maximiza o skip.

#### Files to edit
- `theodb_rs/src/am/columnar_project.rs` — em `begin_custom_scan` (após `columns_needed`), `let preds: Vec<ZonePredicate> = qual_exprs.iter().filter_map(|c| extract_zone_predicate(*c, scanrelid)).collect();` e registrar em `NODE_PREDICATES`; instalar no `ActiveGuard` em `exec_custom_scan`. Sincronizar em ambos os ramos (vazio → registry_remove).

#### TDD
- **RED:** `test_predicates_extracted_from_qual` — plano com `WHERE a = 5` sobre t_col → `scan_predicates` durante exec retorna `[ZonePredicate{col=a, Eq, 5}]`. Falha antes da extração.
- **GREEN:** implementar a extração + instalação.
- **REFACTOR:** `WHERE lower(b)='x' AND a=5` → só `a=5` empurrado (best-effort); `WHERE a=5 OR b=6` → nenhum (OR não-empurrável).

#### Concurrency tests
(none — single-threaded) Sem estado mutável compartilhado entre threads: o side-channel é `thread_local` e o CustomScan não é `parallel_safe` (mesma limitação honesta do M149). A correção relevante é reuso de endereço (ABA), coberta por teste de subxact-abort em T2.1 — não por race.

#### Acceptance criteria
- [ ] `WHERE a=5` produz 1 `ZonePredicate` e `WHERE a=5 OR b=6` produz 0, sem erro (verificado por: `test_predicates_extracted_from_qual` assere len==1 e len==0).
- [ ] Query sem WHERE produz `preds.len()==0` e resultado A/B byte-idêntico ao pré-M150 (verificado por: md5 do resultado == baseline).

#### DoD
- Teste pg_test passa; A/B com WHERE misto byte-idêntico.

## Phase 3 — Skip no loop de chunks (`decode_stripe` + `load_next_batch`)

### T3.1 — `decode_stripe` recebe `predicates` e pula chunk-groups excluídos

#### Why this step
É o coração do M150: no `for cg`, antes de pagar `read_chunked`+zstd, testar cada pred contra o `ChunkDirEntry`
min/max do cg; se algum prova exclusão, `continue`. Porta as ~15 linhas que `decode_columns:825-841` já tem para o
caminho de scan geral. Raciocínio: o diretório já é lido antes do loop de colunas (columnar.rs:717-722), então o
bound está disponível de graça antes da descompressão.

#### Files to edit
- `theodb_rs/src/am/columnar.rs` — `decode_stripe` ganha `predicates: &[ZonePredicate]`; guard de skip no `for cg`; contador `chunks_skipped`. `load_next_batch` busca `scan_predicates(st as usize)` e repassa.

#### TDD
- **RED:** `test_decode_stripe_skips_excluded_chunk` — twin heap; `SELECT a FROM t_col WHERE a = 999999` onde 999999 cai fora do min/max de todos os chunks exceto um → resultado colunar `==` heap (A/B), E `chunks_skipped > 0` sob `THEODB_SCAN_PROFILE=1`. Falha antes do skip (chunks_skipped==0 e/ou o skip perde linha).
- **RED (correção crítica):** `test_skip_never_loses_row` — Eq DENTRO do range de um chunk cujo min<val<max mas o valor não existe no chunk → NÃO pode pular (chunk_can_match retorna true) → A/B byte-idêntico. Prova o fail-safe.
- **GREEN:** inserir o guard `if !predicates.is_empty() && predicates.iter().any(|p| p.col<natts && !chunk_can_match(entry, ...)) { chunks_skipped+=1; continue; }`.
- **REFACTOR:** garantir que o `continue` pula o chunk-group INTEIRO (todas as colunas) — alinhamento de linhas preservado.

#### Concurrency tests
(none — single-threaded) Sem estado mutável compartilhado entre threads: o side-channel é `thread_local` e o CustomScan não é `parallel_safe` (mesma limitação honesta do M149). A correção relevante é reuso de endereço (ABA), coberta por teste de subxact-abort em T2.1 — não por race.

#### Acceptance criteria
- [ ] Um chunk com min/max fora do predicado tem `chunks_skipped>0`; um chunk cujo range contém o valor nunca é pulado (verificado por: `test_decode_stripe_skips_excluded_chunk` + `test_skip_never_loses_row`).
- [ ] O md5 do resultado colunar == md5 do heap-twin em Eq-dentro, Eq-fora, range, negativo, temporal, float-NaN e coluna-sem-min/max (verificado por: 7 asserts A/B no pg_test).
- [ ] Com `predicates.len()==0`, `chunks_skipped==0` e resultado idêntico ao pré-M150 (verificado por: assert no pg_test).

#### DoD
- Os 2 testes pg_test passam; `run_m128_clickbench` diverged=0.

## Phase 4 — Métrica + benchmark + validação de integração

### T4.1 — Contador `chunks_skipped/scanned` (wiring metric) + GUC `theodb.enable_chunk_skip`

#### Why this step
A wiring triad exige uma métrica de runtime observável. Espelhar o `THEODB_SCAN_PROFILE` que `decode_columns:855`
já emite, agora para o scan geral. GUC `theodb.enable_chunk_skip` (default ON) permite o A/B OFF-vs-ON como oráculo
do ganho. Raciocínio: sem observabilidade o skip é invisível quando quebra; sem o GUC não há baseline controlado.

#### Files to edit
- `theodb_rs/src/am/columnar.rs` — emitir `chunks_skipped/total_cg` sob `THEODB_SCAN_PROFILE=1` no scan geral.
- `theodb_rs/src/lib.rs` (ou onde os GUCs vivem) — registrar `theodb.enable_chunk_skip` (bool, default true), consultado no guard de skip.

#### TDD
- **RED:** `test_enable_chunk_skip_guc_off_disables_skip` — com `SET theodb.enable_chunk_skip=off`, `chunks_skipped==0` mesmo com predicado seletivo; resultado idêntico. Falha antes do GUC existir.
- **GREEN:** registrar o GUC + gate o guard de skip por ele.
- **REFACTOR:** log line estruturado com `who/what` (Regra 8 error-handling — contexto).

#### Concurrency tests
(none — single-threaded) Sem estado mutável compartilhado entre threads: o side-channel é `thread_local` e o CustomScan não é `parallel_safe` (mesma limitação honesta do M149). A correção relevante é reuso de endereço (ABA), coberta por teste de subxact-abort em T2.1 — não por race.

#### Acceptance criteria
- [ ] Com `SET theodb.enable_chunk_skip=off`, `chunks_skipped==0`; com `on`, `chunks_skipped>0` na mesma query (verificado por: `test_enable_chunk_skip_guc_off_disables_skip`).
- [ ] O log emite `chunks_skipped/total_cg` sob `THEODB_SCAN_PROFILE=1` (verificado por: `grep 'chunks_skipped' no stderr do backend` != vazio).

#### DoD
- Teste pg_test passa; GUC visível em `SHOW theodb.enable_chunk_skip`.

### T4.2 — Benchmark medido (DoD principal) + CHANGELOG

#### Why this step
O DoD exige número real: ≥80% chunks pulados + ≥5× ganho numa query seletiva sobre 1M, A/B byte-idêntico. É o gate
measurement-first (Regra 5, CLAUDE.md). Raciocínio: sem artefato em `docs/benchmarks/` nenhuma claim de perf é
permitida.

#### Failure scenarios
(none — no external I/O touched) O scan é in-process sobre heap-backed columnar; sem HTTP/DB-driver/queue. O único
"falha" relevante é chunk sem min/max → fallback (coberto em T3.1).

#### Files to edit
- `docs/benchmarks/m150-chunk-group-filtering.md` (NEW) — metodologia + números (OFF vs ON, chunks_skipped%, A/B).
- `docs/benchmarks/m150-artifacts/` (NEW) — JSON do run.
- `CHANGELOG.md` `[Unreleased] § Added`.

#### TDD
- **RED:** o benchmark É o teste — script que roda a query seletiva sobre 1M num dataset clusterizado por `EventDate`, mede OFF-vs-ON, assere skip%≥80 e ganho≥5× e A/B diverged=0. Antes da implementação o skip%=0.
- **GREEN:** rodar no droplet efêmero; capturar o artefato.
- **REFACTOR:** documentar honestamente a limitação (sem clustering pela coluna, os bounds sobrepõem → skip baixo).

#### Concurrency tests
(none — single-threaded) Sem estado mutável compartilhado entre threads: o side-channel é `thread_local` e o CustomScan não é `parallel_safe` (mesma limitação honesta do M149). A correção relevante é reuso de endereço (ABA), coberta por teste de subxact-abort em T2.1 — não por race.

#### Acceptance criteria
- [ ] Query seletiva sobre 1M: chunks_skipped ≥ 80%, ganho ≥ 5×, A/B diverged=0. Número real no doc.
- [ ] `CHANGELOG.md` tem 1 entrada nova em `[Unreleased] § Added` referenciando M150 (verificado por: `grep -c M150 CHANGELOG.md` >= 1).
- [ ] `doctl compute droplet list` não lista o droplet efêmero ao fim (verificado por: 0 droplets efêmeros).

#### DoD
- `docs/benchmarks/m150-chunk-group-filtering.md` existe com números medidos; `run_m128` diverged=0.

## Coverage Matrix

| Requisito (DoD do ROADMAP) | Task |
|---|---|
| Predicados igualdade/range empurrados ao scan, comparados contra min/max por-chunk, chunk sem match pulado antes de descomprimir | T2.2, T3.1 |
| Contador `chunks_skipped/chunks_scanned` observável (wiring metric) | T4.1 |
| Benchmark: query seletiva sobre 1M pula ≥80% dos chunks e melhora ≥5×, número real | T4.2 |
| A/B byte-idêntico vs heap (skip não perde linha) | T3.1 (test_skip_never_loses_row), T4.2 |
| CHANGELOG `[Unreleased]` | T4.2 |
| Reuso do zone-map `directory_minmax` (M105) — sem reescrita (Regra 9) | T1.1 (promover extrator), T3.1 (reusar chunk_can_match) |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Erro em `chunk_can_match`/`encode_const_bits` perde linha silenciosamente | ALTA | Reusar o extrator existente (não reimplementar); A/B heap-twin obrigatório com Eq dentro-e-fora + `test_skip_never_loses_row` | implementer |
| Perf-theater: sem clustering pela coluna do predicado, nada é pulado | MÉDIA | Medir `chunks_skipped/scanned` num dataset clusterizado; documentar a limitação honestamente (não citar ganho fora do regime clusterizado) | implementer |
| Interação M149 no mesmo nó (preds + wanted no mesmo side-channel) deixa preds stale de scan aninhado | MÉDIA | Keying por scandesc já resolve (prova do M149); `test_subxact_abort_no_stale_predicate` + self-join estendido | reviewer |

## Unresolved Questions

- (none — every decision is resolved at plan time). Rationale: teste direto vs prover (ADR-1), best-effort (ADR-2), side-channel paralelo (ADR-3) — todas fechadas no blueprint.

## Global DoD

- [ ] Todos os testes pg_test verdes (provados RED→GREEN no droplet — cargo pgrx test não linka localmente).
- [ ] `cargo clippy` limpo; nenhum arquivo > 2100 LoC (columnar.rs cresce ~20 linhas).
- [ ] `run_m128_clickbench` diverged=0 (correção).
- [ ] Benchmark medido em `docs/benchmarks/m150-*` (DoD de perf).
- [ ] `/code-quality` verdict ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG `[Unreleased]` atualizado.
- [ ] Droplet efêmero destruído.

## Plan-confidence note

Structural verdict SHIPPABLE_WITH_CAVEATS: coverage matrix 100%, baseline context completo, AC executáveis
(acceptable_ratio 0.875), concurrency posture declarada em toda task, ADRs com alternativas, zero citação
fabricada. Os 2 caps residuais (`auditor_unavailable_cargo-udeps`, `symbol_fab_unverifiable_rust`) são
**ambientais** — a verificação de símbolos rust via pgrx não linka nesta máquina local (limitação conhecida:
`cargo pgrx test` só roda no droplet e2e). NÃO são defeitos do plano. O gate REAL de code-quality rust roda no
droplet durante o `run_validation` do `/implement` e no `/code-quality` pós-implementação (onde o toolchain
existe) — não é bypassado, é diferido ao ambiente que o executa. Mesma situação do M149 (que shipou v0.141.0).

## Final Phase — Integration Validation

Após as 4 fases: build from scratch no droplet, rodar toda a suíte pg_test + `run_m128` (diverged=0) + o benchmark
de 1M clusterizado (skip%≥80, ganho≥5×). A cadeia só está completa quando o A/B é byte-idêntico E o ganho medido
cumpre o DoD. Falha em qualquer → volta ao `/implement` (não editar o plano).
