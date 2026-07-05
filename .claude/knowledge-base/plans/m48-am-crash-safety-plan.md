---
slug: m48-am-crash-safety
milestone_id: M48
created_at: 2026-07-05
goal: Fechar os 5 DoDs de correctness & durabilidade do Index AM (issues #46/#47 + pending fold + cancelabilidade + custo honesto), provado por testes de crash reais contra container e artefato de benchmark.
---

# Plan: M48 — Correctness & durabilidade do Index AM (crash-safety #46/#47, pending fold, cancelabilidade, custo honesto)

> **Version 1.2** (revisão por plan-defect descoberto no /implement iter-2: o mecanismo FSM per-page do
> T2.2 conflita com a realidade dos layouts — TODOS os readers assumem ranges CONTÍGUOS
> (`read_chunked(first,npages)`, dir por cursor `page.rs:459-465`, pending como cauda
> `pstart..nblocks` `page.rs:105-112`); reuso de página avulsa fragmentaria os ranges. GIN/nbtree usam
> FSM porque suas páginas são auto-contidas — as nossas não. Mecanismo substituído por **reuso de
> região contígua (alternação de gerações)** — mesmo outcome do DoD (tamanho estável), zero FSM.
> Divergência do blueprint §Q4 documentada em D2 (o precedente core é verdadeiro, mas inaplicável ao
> layout chunked). Halt-loop pausado e retomado conforme cycle-implement § Stop conditions.)
> **Version 1.1** (edge-cases absorvidos — `reviews/m48-am-crash-safety-plan-edge-cases-2026-07-05.md`:
> EC-1 auto-migrate v1→v2 no fold; EC-2 guard FSM; EC-3 costestimate nunca-erra; EC-4..8 testes) —
> Executa o blueprint SHIPPABLE `m48-am-crash-safety` (99.7): fecha os dois furos de
> crash-safety do AM próprio (#46 INIT fork UNLOGGED sem WAL; #47 VACUUM rewrite não-atômico) com os
> padrões upstream ancorados (pgvector `log_newpage_range`; composição GIN-order + nbtree-meta-full-record
> via GenericXLog ≤4 páginas/registro), adiciona pending fold com threshold, cancelabilidade do build
> paralelo (seam DIP na camada pura), `amcostestimate` real (template pgvectorscale), e o harness de
> teste de crash (GUC de injeção + docker kill) que nenhum peer do corpus tem. Benchmark artifact com
> dados de pending-degradação e WAL volume (insumo M55).

## Goal

Fechar os 5 DoDs do ROADMAP § M48 (fix #46, fix #47, pending fold threshold, build cancelável,
amcostestimate honesto) de modo que **`pytest benchmarks/tests/test_am_crash.py benchmarks/tests/test_am_maintenance.py -q` retorne 100% verde contra o container** (incluindo os 3 testes de crash real: UNLOGGED kill/restart, crash mid-fold pré-pivot, crash pós-pivot) e o artefato `docs/benchmarks/m48-am-maintenance.{md,json}` exista com mean±std ≥3 runs.

## Context

O deep-view de trajetória (2026-07-05) encontrou dois furos de correctness no AM próprio, filados como
issues **#46** e **#47**, mais três gaps operacionais — consolidados no ROADMAP § M48. A discovery
(blueprint `m48-am-crash-safety`, SHIPPABLE 99.7, 10/10 questões, 0 BLOCKED) ancorou cada fix no padrão
upstream com file:line e verificou a superfície FFI completa nos bindings pinados do pgrx 0.16.1.
Decisões já fechadas na discovery (D1–D6 do blueprint) — este plano NÃO reabre nenhuma; ele as executa.

Restrição herdada do ROADMAP § M48 (review de engenharia de BD): o mecanismo meta-pivot é
**layout-agnóstico** — camada de ciclo de vida de páginas, separada do serializer de tuples — para que o
layout v3 do M51 troque só o serializer. E a nota de teste: custo honesto ⇒ seqscan vencer em N pequeno
é o resultado CORRETO.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/am/build.rs` | 293 | `f003a7b` (2026-07-02) | `ambuild`/`ambuildempty`/`aminsert`/`vacuum_rebuild` dos dois AMs | `ambuildempty` já escreve no INIT_FORKNUM (`build.rs:259-276`); assinatura dos callbacks C-unwind intocada |
| `theodb_rs/src/am/page.rs` | 829 | `cca599f` (2026-07-03) | Primitivas de página: blob v1, structured IVF v2, pending region, GenericXLog lifecycle | `META_MAGIC`/`META_VERSION` (`page.rs:13-14`); erros tipados "truncated meta page" (`:363-365`); leitura blob v1 continua funcionando (coexistência) |
| `theodb_rs/src/am/fold.rs` (NEW) | 0 | — | (novo) ciclo de vida do shadow-fold: write-new-gen → meta-pivot → reclaim — **layout-agnóstico** | separação page-primitives (page.rs) vs fold-lifecycle (fold.rs) — SRP + budget 500 LoC |
| `theodb_rs/src/am/hnsw_page.rs` | 800 | `467de15` (2026-07-05) | Layout structured do HNSW (M35) + traverse (M46/FU-1) | `rewrite_structured:387-406` será substituído pelo caminho fold.rs; traverse/scan_core intocados |
| `theodb_rs/src/am/mod.rs` | 213 | `f003a7b` (2026-07-02) | `IndexAmRoutine` wiring; `ambulkdelete`/`amvacuumcleanup`/`amcostestimate` | tokens do amroutine (amcanorderbyop etc.) intocados; `amvacuumcleanup:176-190` ganha fold-por-threshold |
| `theodb_rs/src/am/guc.rs` | 59 | `f003a7b` (2026-07-02) | GUCs `probes`/`ef_search` (padrão GucRegistry) | GUCs existentes intocados; novos seguem o MESMO padrão (`guc.rs:9-46`) |
| `theodb_rs/src/am/lock.rs` | 27 | `3514322` (2026-07-01) | Advisory lock do fold (SHARE scan/insert; EXCLUSIVE fold) | semântica intocada — o fold lock é o que substitui safexid (Blueprint §Q4) |
| `theodb_rs/src/ann/hnsw_parallel.rs` | 249 | `682bb32` (2026-07-03) | Build paralelo M44 (camada PURA — zero pg_sys) | **pureza da camada `ann/` (architecture.md § 1)** — cancelabilidade entra por seam de callback, NUNCA por import de pg_sys |
| `theodb_rs/src/ann/hnsw.rs` | 481 | `31baf39` (2026-07-03) | HnswIndex build (chama parallel acima de 4096) | idem — seam de callback |
| `benchmarks/tests/test_am_crash.py` (NEW) | 0 | — | (novo) testes de crash: UNLOGGED, mid-fold, pós-pivot | — |
| `benchmarks/tests/test_am_maintenance.py` (NEW) | 0 | — | (novo) pending fold threshold, cancelabilidade, costestimate | — |
| `benchmarks/tests/test_index_am.py` | 204 | `d50c7eb` (2026-07-01) | Suíte funcional atual do AM (34 testes verdes hoje) | permanece 100% verde (regressão zero) |
| `benchmarks/run_m48_maintenance.py` (NEW) | 0 | — | (novo) driver do benchmark artifact | — |
| `docs/benchmarks/m48-am-maintenance.md` (NEW) + `.json` (NEW) | 0 | — | (novo) artefato de dados | — |
| `CHANGELOG.md` | — | — | contrato público | entrada `[Unreleased]` por mudança (Regra 6) |

### Current callers / dependents

- **Symbol:** `ambuildempty`/`ambuildempty_hnsw` (`build.rs:259-276`) — **Callers:** `mod.rs:47,64`
  (amroutine wiring). External: chamado pelo core PG para relações UNLOGGED. Tests: nenhum hoje (o gap).
- **Symbol:** `append_pending` (`page.rs:127`) — **Callers (prod):** `build.rs:139` (aminsert).
- **Symbol:** `vacuum_rebuild` (`build.rs:148`) — **Callers (prod):** `mod.rs:168` (ambulkdelete).
  Dispatcha para `vacuum_rebuild_structured` (IVF) / `vacuum_rebuild_hnsw_structured` (`build.rs:162,165`).
- **Symbol:** `rewrite_ivf_structured` (`page.rs:517`) — **Caller:** `build.rs:251`. `rewrite_structured`
  HNSW (`hnsw_page.rs:387`) — **Caller:** `build.rs:217`. Ambos serão substituídos pelo caminho `fold.rs`.
- **Symbol:** `amvacuumcleanup` (`mod.rs:176-190`) — wired no amroutine; hoje early-return.
- **Symbol:** `amcostestimate` (`mod.rs:117-140`) — wired no amroutine; hoje stub 0/0.
- **Symbol:** `HnswIndex::build` (`ann/hnsw.rs`) — **Callers (prod):** `build.rs` (ambuild_hnsw + fold);
  bench FU-1 (`benches/scan_hot_path.rs` via scan_core — NÃO toca build). Ganha parâmetro de callback.

### Domain glossary

- **INIT fork** — fork-template de relação UNLOGGED, copiado para o main fork no crash-recovery reset.
- **GenericXLog** — WAL genérico para extensões; ≤4 páginas/registro; **no-op de WAL quando
  `RelationNeedsWAL()==false`** (a causa do #46).
- **pending region** — páginas após o corpo estruturado onde `aminsert` appenda `(tid, vec)` em O(1);
  escaneadas exatas; foldadas (rebuild) no VACUUM.
- **fold** — o rebuild do índice no VACUUM (`vacuum_rebuild`): corpus vivo re-indexado e reescrito.
- **meta-pivot** — o novo protocolo do fold: geração nova em páginas frescas; troca atômica no bloco 0.
- **shadow generation** — as páginas da geração nova, inertes até a meta apontar para elas.
- **fold lock** — advisory lock (`lock.rs`): SHARE em scan/insert, EXCLUSIVE no fold (substitui safexid).

### Architecture boundaries affected

- `ann/` (domínio puro) **não pode importar** `pg_sys` (`architecture.md § 2`). A cancelabilidade do
  build paralelo cruza essa fronteira **por injeção de dependência**: a camada pura declara o seam
  (`check_interrupt: &dyn Fn()`), o `am/` injeta `pgrx::check_for_interrupts!` (DIP — o domínio define o
  contrato, a infra implementa). Mesmo padrão do `NeighborSource` do FU-1.
- `am/fold.rs` (NEW) fica na camada `am/` (infra de página) — pode usar pg_sys livremente.

## Prior Art & Related Work

- **Blueprint interno:** `knowledge-base/discoveries/blueprints/m48-am-crash-safety-blueprint.md` —
  TODAS as decisões de design deste plano vêm de lá: §"Q1" (GenericXLog: cap 4 págs/registro, no-op
  UNLOGGED), §"Q2" (fix pgvector `hnswbuild.c:1137-1138`), §"Q3" (ordem GIN dados-antes-pivot-depois),
  §"Q4" (primitivo meta-full-record; FSM advisory; sem safexid → fold lock), §"Q6" (receita TAP
  013_crash_restart → pytest+docker), §"Q8" (bindings pgrx verificados), §"Q9" (GUC crash-injection;
  injection_points indisponível), §"Q10" (costestimate template), e §"Cross-cutting Comparison".
- **Reference projects:** `knowledge-base/references/pgvector/src/hnswbuild.c:1137-1138` (fix #46);
  `knowledge-base/references/postgres/src/backend/access/gin/ginfast.c:766-772` (ordem de escrita);
  `knowledge-base/references/postgres/src/backend/access/nbtree/nbtxlog.c:81-130` (meta regenerada no
  redo); `knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/cost_estimate.rs:1-51`
  (template FFI); `knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/build.rs:1078`
  (check_for_interrupts por tupla); `knowledge-base/references/postgres/src/test/recovery/t/013_crash_restart.pl`
  (anatomia do teste de crash).
- **Patterns skills:** (nenhuma `*-patterns` no repo — verificado em Step 0).
- **Issues:** usetheodev/theo-db#46, #47 (evidência file:line nos corpos).

## Objective

- [ ] #46 fechado: índice UNLOGGED sobrevive a `docker kill -s KILL` + restart (INSERT funciona; sem "truncated meta page")
- [ ] #47 fechado: crash em QUALQUER ponto do fold deixa o índice consistente (geração antiga OU nova, nunca mista) — provado por crash-injection determinística
- [ ] Pending fold: workload insert-only tem a pending foldada no VACUUM acima do threshold (scan volta a O(estrutura))
- [ ] Build paralelo cancelável: `pg_cancel_backend` interrompe `CREATE INDEX` em ≤ 1 batch
- [ ] `amcostestimate` real: EXPLAIN escolhe índice em N realista E seqscan em N pequeno (ambos assertados)
- [ ] Artefato `docs/benchmarks/m48-am-maintenance.{md,json}` com pending-degradação, WAL volume do fold (insumo M55) e custo-vs-real — mean±std ≥3 runs

## ADRs

### D1 — Fix #46 via `log_newpage_range` incondicional no fim do buildempty (blueprint D1)

**Decision:** manter a escrita via bufmgr/GenericXLog como está (o plumbing de `INIT_FORKNUM` JÁ existe —
`build.rs:259-276`) e adicionar, ao fim de `ambuildempty`/`ambuildempty_hnsw`:
`pg_sys::log_newpage_range(indexrel, INIT_FORKNUM, 0, RelationGetNumberOfBlocksInFork(indexrel, INIT_FORKNUM), true)`.
**Rationale:** é o padrão literal pgvector (`hnswbuild.c:1137-1138`) e gist — Regra 9 (não reinventar);
GenericXLog é no-op de WAL para unlogged (Blueprint §Q1.3), então o WAL do INIT fork TEM que vir de fora.
**Alternatives considered:** (a) padrão bloom (só BM_PERMANENT/checkpoint) — REJEITADO: frágil, o próprio
blueprint o marca anti-pattern; (b) `smgrimmedsync` + escrita smgr direta — REJEITADO: pgvector moderno
não usa (Blueprint §Q2), mais superfície unsafe sem ganho. **Consequences:** fix de ~8 linhas; o binding
`log_newpage_range` (pg17.rs:37137, verificado) vira o 1º uso Rust no nosso corpus — assinatura validada
contra o header C na implementação.

### D2 — Fix #47 via shadow-fold com meta-pivot atômico em módulo novo `fold.rs`, layout-agnóstico (blueprint D2)

**Decision:** substituir os rewrites in-place (`rewrite_ivf_structured`, `rewrite_structured`) por um
ciclo de vida em 3 passos num módulo NOVO (`am/fold.rs`): (1) escrever a geração nova em páginas
**frescas** (extend após `nblocks` atual), cada página em registro GenericXLog próprio — inertes até o
pivot; (2) **pivot**: reescrever SÓ o bloco 0 (meta) em UM registro com `GENERIC_XLOG_FULL_IMAGE`,
apontando o directory/pending_start para a geração nova; (3) **reclaim**: marcar as páginas da geração
velha como vazias (registros WAL) e `RecordFreeIndexPage` + `IndexFreeSpaceMapVacuum` (advisory — perda
em crash é aceitável, Blueprint §Q4). A meta ganha pointers explícitos (pending_start deixa de ser
posicional) → **META_VERSION bump 1→2** com erro tipado instruindo REINDEX (precedente v1→v2 existente).
O fold.rs recebe `Vec<item-bytes>` prontos do serializer — **não conhece o layout** (restrição
anti-retrabalho M51 do ROADMAP). **Auto-migração v1→v2 (EC-1):** `vacuum_rebuild` já lê o corpus vivo
de forma formato-agnóstica; o fold SEMPRE escreve a geração nova em meta v2 ⇒ o primeiro VACUUM
pós-upgrade migra v1→v2 atomicamente (crash-safe pelo próprio pivot) — os rewrites in-place morrem de
verdade, e o #47 fecha TAMBÉM para índices legados (não só para recém-criados). `extend_page_with_item` permanece intocado (extend puro); o reuso é por REGIÃO no
fold.rs, não por página avulsa.
**Rationale:** composição dos DOIS precedentes core — ordem GIN (dados-antes-pivot-depois,
`ginfast.c:766-772`) + meta-full-record nbtree (`nbtxlog.c:81-130`); cabe no cap de 4 páginas/registro
porque o pivot toca 1 página (Blueprint §Q1.1). SRP/budget: page.rs já tem 829 LoC (>500,
`architecture.md`) — o lifecycle novo NÃO entra lá.
**Alternatives considered:** (a) manutenção in-place à la pgvector — REJEITADO AGORA: é o M55 (complexidade
tombstone/repair/4-passes, Blueprint §Q5); (b) "meta+N páginas novas num único registro" — REJEITADO:
estoura o cap de 4 do GenericXLog (Blueprint §Q1.1); (c) escrever a geração nova NO LUGAR (in-place,
estado atual) — REJEITADO: é o bug #47 (estado misto após crash mid-fold); (d) reclaim via FSM
per-page (RecordFreeIndexPage/GetFreeIndexPage — recomendação original do blueprint §Q4) — REJEITADO
NA v1.2: fragmenta os ranges contíguos que todos os readers assumem (`read_chunked`,
`page.rs:459-465`, `page.rs:105-112`); precedente core vale para páginas auto-contidas, não para
layout chunked.
**Consequences:** pico transitório de disco ~2× durante o fold (velha+nova coexistem até o reclaim);
WAL do fold vira FPIs da geração nova (volume medido no benchmark — insumo M55); leitores protegidos
pelo fold lock EXCLUSIVE existente (sem safexid — Blueprint §Q4).

### D3 — Pending fold por threshold no `amvacuumcleanup` (blueprint D3)

**Decision:** `amvacuumcleanup` (hoje early-return, `mod.rs:176-190`) passa a: se `ambulkdelete` NÃO
rodou nesta passada (stats NULL na entrada — sinal de zero dead tuples) E `pending_pages >
theodb.vacuum_pending_threshold` (GUC int, default 16 páginas, min 1, max 65536), executa o fold
completo (mesmo caminho D2). Insert path permanece O(1) intocado.
**Rationale:** fecha o gap insert-only (scan paga O(pending) para sempre — `mod.rs:181-183`); trigger no
vacuum (não no insert) é divergência CONSCIENTE do GIN (`ginfast.c:458-471`): o fold do GIN é
incremental/barato, o nosso é rebuild O(N) — no insert path seria cliff de latência (Blueprint D3).
**Alternatives considered:** (a) fold no insert acima do threshold (padrão GIN literal) — REJEITADO:
cliff de latência imprevisível no INSERT do usuário; (b) threshold em bytes — REJEITADO: páginas é a
unidade natural do custo de scan (pages_read) e do GIN (`nPendingPages`).
**Consequences:** autovacuum por insert-threshold (`autovacuum_vacuum_insert_threshold`) passa a
disparar o fold naturalmente; VACUUM manual idem. Default 16 é chute educado — o benchmark (Fase 6)
mede a degradação e o valor pode ser recalibrado com dado.

### D4 — Cancelabilidade por seam DIP na camada pura (blueprint D4 + architecture.md § 2)

**Decision:** `HnswIndex::build`/`build_parallel` ganham parâmetro `check_interrupt: &(dyn Fn() + Sync)`
chamado 1×/batch no leader (nunca nos workers — sinal só no backend principal). `am/build.rs` injeta
`|| pgrx::check_for_interrupts!()`; o bench FU-1 e testes puros injetam no-op `|| {}`. Loops de
página do fold/bulkdelete chamam `pg_sys::vacuum_delay_point()` por página (precedente pgvectorscale
`vacuum.rs:94-101`).
**Rationale:** a camada `ann/` é pura (zero pg_sys — invariante de link do FU-1 e `architecture.md § 1`);
importar pg_sys quebraria o bench standalone e a fronteira DIP. O seam de callback é o MESMO padrão do
`NeighborSource` (FU-1). `check_for_interrupts!` pode `ereport(ERROR)` → longjmp — seguro porque os call
sites do am/ estão sob `#[pg_guard]` (Blueprint §Q8).
**Alternatives considered:** (a) importar pg_sys no `ann/` — REJEITADO: quebra a camada pura + o link do
bench; (b) checar interrupt só no fim do build — REJEITADO: não cancela um build de 1M (o gap real);
(c) cancelar dentro dos worker threads — REJEITADO: sinais PG pertencem ao backend (main thread), workers
são PG-free por design (M44).
**Consequences:** assinatura de `build` muda (2 call sites de produção + bench); custo por batch é 1
load atômico (InterruptPending) — desprezível.

### D5 — `amcostestimate` real: template pgvectorscale + ratios pgvector (blueprint D5)

**Decision:** substituir o stub (`mod.rs:117-140`) mantendo o branch "sem orderby → f64::MAX" e, no
branch com orderby: `GenericCosts` zerado + `numIndexTuples` estimado + `pg_sys::genericcostestimate` +
ratio por AM — IVF: `probes/lists` (+ 50% random→seq); HNSW: fórmula pgvector
(`0.55·log(tuples)/(log(m)·(1+log(ef_search)))`, constantes adotadas como estão — paridade, tuning fora
de escopo); `indexStartupCost = indexTotalCost·ratio`; escreve os 5 out-params.
**Rationale:** Regra 9 — `genericcostestimate` faz seletividade/correlação; nunca reimplementar
(Blueprint §Q10). Template FFI provado em produção na MESMA versão pinada de pgrx
(`cost_estimate.rs:1-51`).
**Alternatives considered:** (a) manter stub — REJEITADO: planner escolhe o índice SEMPRE (custo 0),
mesmo quando seqscan é melhor — mente para o planner; (b) modelo próprio sem generic — REJEITADO: Regra 9.
**Consequences:** seqscan passa a vencer em N pequeno (CORRETO — nota do ROADMAP M48); testes de
pushdown migram para assertar em N realista (os 34 testes atuais são auditados na Fase 5).

### D6 — Harness de crash: GUC `theodb.test_crash_after_pages` + `abort()` + docker kill (blueprint D6)

**Decision:** GUC int `theodb.test_crash_after_pages` (default 0=off, min 0, max 1_000_000,
`GucContext::Suset`), **sempre compilado**. No caminho de escrita do fold (fold.rs), após commitar a
N-ésima página da geração nova: `std::process::abort()`. Testes pytest derivados do TAP
`013_crash_restart.pl` (Blueprint §Q6): durabilidade seletiva + writability pós-restart; crash de
postmaster via `docker kill -s KILL` + `docker start` + `pg_isready` poll.
**Rationale:** injection_points indisponível no Debian PG17 (verificado no container — Blueprint §Q9);
`abort()` (SIGABRT) = crash real de backend sem cleanup → postmaster faz crash-restart + WAL replay;
`proc_exit` roda shutdown callbacks (limpo demais). Sempre-compilado = testa o binário SHIPPED
("tests passing ≠ system works").
**Alternatives considered:** (a) feature-flag de build — REJEITADO: testaria binário ≠ shipped;
(b) timing com kill externo — REJEITADO: flaky (fold pode terminar em ms); (c) gdb — REJEITADO: frágil
no CI. **Consequences:** ~10 linhas de scaffolding no binário de produção (mitigado: default-off +
SUSET + nome autoexplicativo; precedente: developer GUCs do core).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Pico de disco ~2× durante o fold (gerações coexistem até reclaim) | Medium | Documentado; reclaim imediato pós-pivot no mesmo VACUUM; FSM reusa nas próximas gerações | implementer |
| META_VERSION bump 1→2 quebra índices existentes (formato) | Medium | Erro tipado instruindo REINDEX (precedente v1→v2 do projeto); CHANGELOG `Changed` com nota BREAKING de formato | implementer |
| WAL volume do fold cresce (FPIs da geração nova) | Medium | Medido no benchmark (Fase 6) como insumo EXPLÍCITO do M55; nenhum claim sem número | implementer |
| Scaffolding de crash-GUC no binário de produção | Low | default 0 + SUSET + só no caminho do fold; precedente developer-GUCs do core | implementer |
| `abort()` num backend com buffers pinados pode deixar warnings de recovery | Low | É exatamente o cenário que o crash-recovery cobre; asserts do teste validam consistência pós-replay | implementer |
| Assinatura de `HnswIndex::build` muda (callback) — toca bench FU-1 | Low | No-op closure no bench; suite FU-1/M46 re-rodada na Integration Validation | implementer |
| Custo honesto muda planos de queries existentes (seqscan em N pequeno) | Low | Comportamento CORRETO (ROADMAP M48); testes auditados para N realista na mesma fase | implementer |

## Unresolved Questions

- Q1 — o default do `vacuum_pending_threshold` (16 páginas) é adequado? → o benchmark da Fase 6 mede a
  degradação por página pendente; recalibrar com o dado é follow-up aceitável (não bloqueia).
- Q2 (v1.2) — a região reusada pode conter lixo de gerações antigas/crashes? → sim; a escrita na
  região re-inicializa incondicionalmente cada página (reinit_page_with_item) — coberto pelo property
  test de free_region + teste de reclaim (T2.2).
- (demais decisões: resolvidas em plan time via blueprint D1–D6 + edge-case review absorvida em v1.1.)

## Dependencies

**Zero dependências novas** (parsimony rung 4 — tudo já instalado):

| Dependency | Version (pinned) | New? | Rule 9 (por que não reescrever/por que esta) | CVE gate |
|---|---|---|---|---|
| `pgrx` / `pgrx-pg-sys` | `=0.16.1` (existente, `theodb_rs/Cargo.toml`) | NÃO | Bindings `log_newpage_range`/FSM/`genericcostestimate`/`check_for_interrupts!` já expostos (Blueprint §Q8 — verificado no registry) — zero `extern "C"` manual | já auditada (deps-audit FU-1) |
| `criterion` | `0.5.1` dev-only (existente — FU-1) | NÃO | intocada por este plano (bench FU-1 só re-linka) | já auditada (FU-1) |
| Python de teste: `psycopg2`, `pytest` | existentes (`benchmarks/requirements.txt`) | NÃO | harness atual; `docker` CLI via subprocess (sem lib nova) | n/a (dev-only, já em uso) |

Nenhuma crate/pacote adicionado ⇒ o audit corre em modo verificação (nada novo a escanear; a superfície
nova é FFI de símbolos já linkados pelo pgrx pinado).

## Dependency Graph

```
Phase 1 (#46 INIT fork) ──▶ Phase 2 (#47 shadow-fold + crash GUC) ──▶ Phase 3 (pending fold threshold)
                                                                            │
Phase 4 (cancelabilidade — independente) ◀──────────── pode paralelizar ────┤
Phase 5 (costestimate — independente) ◀─────────────── pode paralelizar ────┤
                                                                            ▼
                                              Phase 6 (benchmark artifact) ──▶ Final (Integration Validation)
```

Sequencial obrigatório: 1→2→3 (o fold do 3 usa o mecanismo do 2). 4 e 5 são independentes entre si e de
2-3 (tocam arquivos disjuntos), mas rodam em ordem no halt-loop (1 task em voo).

---

## Phase 1: Fix #46 — INIT fork de UNLOGGED no WAL

**Objective:** índice sobre tabela UNLOGGED sobrevive a crash/restart — INSERT pós-recovery funciona.

### T1.1 — `log_newpage_range` no buildempty + teste de crash UNLOGGED

#### Objective
Emitir WAL (FPIs) do INIT fork ao fim de `ambuildempty`/`ambuildempty_hnsw`, e provar por teste de
crash real (docker kill) que o índice UNLOGGED volta utilizável.

#### Why this step (action + reasoning — ReAct discipline)
1. **O que faz:** adiciona a chamada `log_newpage_range(rel, INIT_FORKNUM, 0, nblocks, true)` ao fim dos
   dois buildempty (`build.rs:259-276`) e cria `test_am_crash.py` com o teste RED que reproduz o #46.
2. **Por que agora:** é o fix de menor risco e destrava o harness de crash (container + kill + poll) que
   as fases 2-3 reusam. Padrão literal do blueprint D1 (`hnswbuild.c:1137-1138`); GenericXLog é no-op de
   WAL em unlogged (Blueprint §Q1.3) — sem esta chamada o INIT fork nunca chega ao WAL.

#### Evidence
- Bug: `theodb_rs/src/am/build.rs:259-276` escreve INIT fork só via GenericXLog (no-op p/ unlogged).
- Padrão: `knowledge-base/references/pgvector/src/hnswbuild.c:1137-1138`; comentário-autoridade
  `ivfbuild.c:1046` ("GenericXLog functions do not").
- Binding verificado: `log_newpage_range` pg17.rs:37137 (Blueprint §Q8).

#### Files to edit
```
theodb_rs/src/am/build.rs — log_newpage_range ao fim dos 2 buildempty (+ helper unsafe comum)
benchmarks/tests/test_am_crash.py — (NEW) helpers docker (kill/start/wait_ready) + teste RED do #46
```

#### Deep file dependency analysis
- `build.rs`: hoje `ambuildempty` escreve blob IVF e `ambuildempty_hnsw` escreve structured meta-only no
  INIT fork (Baseline row). Muda: +1 chamada unsafe após cada escrita. Downstream: nenhum caller muda
  (callbacks wired em `mod.rs:47,64`).
- `test_am_crash.py`: novo; usa o mesmo padrão de conexão de `test_ann_index.py:34-36` (PGHOST/PGPORT
  env) + subprocess docker. Runner: pytest já descobre `tests/test_*.py` (verificado — 34 testes atuais).

#### Deep Dives
- **Invariante pós-recovery de UNLOGGED:** main fork = cópia do INIT fork ⇒ índice VAZIO mas VÁLIDO
  (meta com magic/version corretos); `aminsert` appenda pending sobre ele normalmente.
- **Edge:** `RelationGetNumberOfBlocksInFork` DEPOIS de toda escrita (o range cobre tudo);
  `page_std=true` (nossas páginas são standard — têm pd_lower/pd_upper).
- **Por que incondicional:** buildempty SÓ roda para INIT fork; o guard `RelationNeedsWAL || INIT` do
  pgvector cobre o caminho compartilhado com o build normal — o nosso não é compartilhado.

#### Pseudo-code / Signatures
```pseudocode
unsafe fn wal_log_init_fork(rel: Relation):
    let n = RelationGetNumberOfBlocksInFork(rel, INIT_FORKNUM)
    if n > 0: log_newpage_range(rel, INIT_FORKNUM, 0, n, true)
# chamado como última linha de ambuildempty e ambuildempty_hnsw
```

#### Tasks
1. RED: escrever `test_unlogged_index_survives_crash_restart` (falha hoje com "truncated meta page")
2. GREEN: helper `wal_log_init_fork` + 2 call sites
3. Rebuild da imagem + re-run do teste (verde)

#### TDD
```
RED:     test_unlogged_index_survives_crash_restart() — CREATE UNLOGGED TABLE + CREATE INDEX theodb_hnsw
         + INSERT; docker kill -s KILL + docker start + wait_ready; assert SELECT count()==0 (reset
         válido); assert INSERT + ORDER BY <-> LIMIT 1 funcionam (hoje: ERRO "truncated meta page")
RED:     test_unlogged_ivfflat_survives_crash_restart() — idem para theodb_ivfflat
RED:     test_alter_set_unlogged_survives_crash() (EC-7) — tabela LOGGED + índice → ALTER TABLE SET
         UNLOGGED → crash/restart → reset válido + INSERT ok (segundo caminho que chama ambuildempty)
GREEN:   wal_log_init_fork + call sites; docker build; testes passam
REFACTOR: extrair helpers docker p/ módulo compartilhado do arquivo (crash_utils no próprio test file)
VERIFY:  PGPORT=55448 python3 -m pytest benchmarks/tests/test_am_crash.py -q -k unlogged
```

#### Concurrency tests
(none — single-threaded) — buildempty roda num único backend; o crash test cobre o ciclo processo-morte.

#### Acceptance Criteria
- [ ] `pytest benchmarks/tests/test_am_crash.py -q -k unlogged` retorna `2 passed` pós-fix (e `2 failed` pré-fix — evidência RED no log do halt-loop)
- [ ] `pytest benchmarks/tests/test_index_am.py benchmarks/tests/test_ann_index.py -q` retorna `34 passed` (regressão zero)
- [ ] Pass: lint — `docker build` conclui com `warning: 0` novos de cargo no stage 1b (diff vs build baseline)
- [ ] Pass: size — `wc -l theodb_rs/src/am/build.rs` ≤ 500

#### DoD (Definition of Done)
- [ ] `pytest benchmarks/tests/test_am_crash.py -q -k unlogged` verde contra imagem rebuilt
- [ ] `grep -c "#46" CHANGELOG.md` ≥ 1 na seção `[Unreleased] § Fixed`

---

## Phase 2: Fix #47 — shadow-fold com meta-pivot atômico

**Objective:** crash em qualquer ponto do fold deixa o índice consistente (geração antiga OU nova).

### T2.1 — Meta v2 (pointers explícitos) + módulo `fold.rs` (write-new-gen + pivot)

#### Objective
Criar `am/fold.rs` com o ciclo shadow-write → meta-pivot, e migrar a meta para v2 (directory/pending
via pointers, não posicional), com erro tipado de REINDEX para v1.

#### Why this step (action + reasoning)
1. **O que faz:** novo módulo layout-agnóstico `fold(rel, items: &[Vec<u8>]) -> Result` que (a) escreve
   `items` em páginas frescas (extend após nblocks; ou reuso FSM), (b) pivota o bloco 0 num único
   registro `GENERIC_XLOG_FULL_IMAGE` apontando para a geração nova (start_blkno + pending_start).
   Serializers existentes (`structured_page_items` IVF; `pack` HNSW) continuam produzindo os items.
2. **Por que agora:** é o coração do #47 (D2); precisa vir antes do reclaim (T2.2) e dos testes de crash
   (T2.3) que o exercitam. Layout-agnóstico por restrição do ROADMAP M48 (anti-retrabalho M51).

#### Evidence
- Bug: `page.rs:517-537` + `hnsw_page.rs:387-406` reescrevem in-place meta-primeiro (issue #47).
- Padrão: ordem GIN (`ginfast.c:766-772`); meta-full-record nbtree (`nbtxlog.c:81-130`); cap 4
  págs/registro (`generic_xlog.c:326-327`) ⇒ pivot toca SÓ a meta (Blueprint D2).
- Directory-based meta já existe no IVF (`page.rs` `IvfMeta.dir: Vec<(u32,u32,u32)>`) — a indireção
  necessária ao pivot é extensão natural.

#### Files to edit
```
theodb_rs/src/am/fold.rs — (NEW) shadow-write + meta-pivot (layout-agnóstico)
theodb_rs/src/am/page.rs — meta v2 (start_blkno/pending_start explícitos; META_VERSION=2 + erro REINDEX p/ v1); pending_layout lê da meta
theodb_rs/src/am/hnsw_page.rs — write/read structured parametrizados pelo start_blkno da meta
theodb_rs/src/am/build.rs — vacuum_rebuild_* passam a chamar fold::fold(...)
theodb_rs/src/am/mod.rs — mod fold
```

#### Deep file dependency analysis
- `page.rs` (829 LoC, Baseline): ganha campos na meta e perde os rewrites in-place (movidos/absorvidos);
  `pending_layout` (`page.rs:105`) passa a ler `pending_start` da meta (caller: `append_pending:127`,
  `read_pending`). LoC líquido ~estável (remoção compensa).
- `fold.rs` (NEW): consome só primitivas públicas de page.rs (`extend_page_with_item`,
  `reinit_page_with_item`, GenericXLog helpers) — zero conhecimento de layout.
- `build.rs`: `vacuum_rebuild_structured`/`_hnsw_structured` (`build.rs:162,165`) trocam o rewrite
  in-place por `fold::fold` — mesma assinatura de entrada (corpus vivo), caminho novo de escrita.
- Scans (`scan.rs`, `hnsw_page.rs` read paths): leem start via meta — mudança localizada nas fns de
  localização de página (invariante: pages_read O(probes)/O(ef·M) preservado).

#### Deep Dives
- **Estados possíveis pós-crash:** (a) crash durante shadow-write → meta velha aponta geração velha
  intacta; páginas novas órfãs (lixo inofensivo; reclamadas por VACUUM futuro via nblocks-scan ou FSM
  perdido — aceitável, Blueprint §Q4); (b) crash exatamente após o registro do pivot → meta nova aponta
  geração nova completa (o registro só é escrito DEPOIS de todas as páginas novas commitadas — ordem
  GIN); (c) crash durante reclaim (T2.2) → meta nova válida; velhas parcialmente marcadas — inofensivo.
- **Invariante do pivot:** o registro do pivot é o ÚLTIMO write da transição; `GENERIC_XLOG_FULL_IMAGE`
  garante meta torn-page-proof (equivalente do WILL_INIT — Blueprint §Q1.5/Q4).
- **Fold lock:** EXCLUSIVE segurado do início do shadow-write até o fim do reclaim (já é assim no
  caller `mod.rs:146-172`) — nenhum scan concorrente vê a transição (substitui safexid).
- **Meta v2 layout:** `[magic u32, version u32=2, start_blkno u32, nblocks_gen u32, pending_start u32,
  reserved]` + payload específico do AM (dim/metric/directory) — o fold.rs escreve o header; o
  serializer fornece o payload (layout-agnóstico).

#### Pseudo-code / Signatures
```pseudocode
// am/fold.rs — layout-agnóstico
pub(crate) unsafe fn fold(rel, meta_payload: &[u8], items: &[Vec<u8>]) -> Result<(), String>
  old_nblocks = RelationGetNumberOfBlocksInFork(rel, MAIN)
  // 1. shadow-write: cada item numa página fresca (FSM-first, senão extend), registro próprio
  new_blknos = items.map(|it| write_item_fresh_page(rel, it))       // inertes
  // 2. pivot: UM registro FULL_IMAGE no bloco 0
  pivot_meta(rel, meta_payload, start=new_blknos[0], pending_start=after(new_blknos))
  // (crash antes daqui ⇒ geração velha; depois ⇒ nova)
  Ok(())
# invariante: nenhuma página 1..old_nblocks é modificada antes do pivot
```

#### Tasks
1. RED: testes Rust puros do encode/decode da meta v2 + erro tipado v1→REINDEX
2. RED: teste pytest `test_fold_preserves_scan_results` (fold sem crash: resultados idênticos pré/pós)
3. GREEN: meta v2 em page.rs; fold.rs (shadow-write + pivot); rewire build.rs
4. REFACTOR: remover rewrites in-place mortos

#### TDD
```
RED:     test_meta_v2_roundtrip() (Rust #[test] puro) — encode/decode start_blkno/pending_start; assert_eq campos
RED:     test_meta_v1_reads_typed_reindex_error() (Rust) — bytes v1 ⇒ Err contendo "REINDEX"
RED:     test_fold_preserves_scan_results() (pytest) — build+DELETE 30%+VACUUM; ORDER BY <-> LIMIT 10
         byte-idêntico ao esperado do corpus vivo (oráculo exato); pages_read O(estrutura) via
         THEODB_SCAN_PROFILE
RED:     test_fold_empty_corpus() (EC-5) — DELETE 100% + VACUUM; scan retorna 0 rows sem erro; INSERT
         novo funciona (nova geração meta-only válida)
RED:     test_fold_auto_migrates_v1_index() (EC-1) — fixture de índice meta-v1; VACUUM; assert meta
         agora v2 (probe via nova coluna de versão OU comportamento: fold subsequente funciona) e scan
         resultado exato
GREEN:   implementação acima
REFACTOR: rewrites in-place removidos (código morto zero — gate /code-quality)
VERIFY:  cargo test (host: só testes puros) + pytest -q -k fold contra imagem rebuilt
```

#### Concurrency tests
Concurrent test: test_fold_blocks_concurrent_scan() (pytest, 2 conexões) — conexão A: VACUUM (fold)
numa tabela grande; conexão B dispara ORDER BY <-> durante o fold; assert B retorna DEPOIS do fold
(fold lock EXCLUSIVE) e com resultado consistente (nunca erro "truncated"/resultado misto).
Happens-before observation via timestamps das duas conexões.

#### Acceptance Criteria
- [ ] Fold sem crash: `pytest -q -k fold_preserves` retorna `passed` — top-10 do scan `equals` o oráculo exato do corpus vivo
- [ ] Nenhuma página da geração velha modificada antes do pivot — `pytest -q -k crash_mid_fold_pre_pivot` retorna `passed` (T2.3 prova o invariante por crash real)
- [ ] Pass: size — `wc -l theodb_rs/src/am/fold.rs` ≤ 500; `wc -l theodb_rs/src/am/page.rs` ≤ 850
- [ ] `pytest benchmarks/tests/ -q -k "not m48_driver"` sem `failed`

#### DoD
- [ ] `pytest -q -k "meta_v2 or fold"` retorna `passed`; `pytest benchmarks/tests/test_index_am.py -q` sem `failed`; `grep -c "meta v2" CHANGELOG.md` ≥ 1 em `[Unreleased] § Changed`

### T2.2 — Reclaim pós-pivot por região contígua (alternação de gerações) [v1.2]

#### Objective
Após o pivot, re-inicializar a região da geração velha (WAL) e fazer o fold seguinte reusá-la
(lowest-fit contíguo) — tamanho do índice estabiliza sem FSM.

#### Why this step (action + reasoning)
1. **O que faz:** no fim do `fold()`: loop `reinit_page_with_item(b, &[])` sobre a região velha
   (registros WAL próprios). No início do `fold()`: escolhe `base` = menor região contígua livre
   (computada dos pointers da meta atual: tudo que NÃO é bloco 0, nem geração viva, nem pending é
   livre) que caiba `items.len()` páginas; senão `base = nblocks` (extend).
2. **Por que assim (v1.2):** os readers assumem ranges contíguos (`read_chunked(first,npages)`,
   dir-cursor `page.rs:459-465`, pending-cauda `page.rs:105-112`) — reuso FSM por página avulsa
   (desenho v1.1, blueprint §Q4) fragmentaria os ranges. Alternação de regiões dá o MESMO outcome do
   DoD (tamanho estável a partir do 2º fold) com zero máquina nova.

#### Evidence
- Contiguidade load-bearing: `page.rs:459-465` (cursor absoluto por lista), `page.rs:105-112`
  (pending = cauda `pstart..nblocks`), `read_chunked` (ranges).
- Precedente da alternação: o próprio meta-pivot (D2) — a região velha é inerte pós-pivot por
  construção; reusar exige apenas que ela seja computável (pointers da meta v2) e re-inicializada.

#### Files to edit
```
theodb_rs/src/am/fold.rs — free_region(): computa a região livre dos pointers da meta; reclaim no fim do fold
```

#### Deep file dependency analysis
- `fold.rs`: reclaim e escolha de base são passos do MESMO lifecycle (nenhum arquivo novo);
  page.rs intocado neste task (extend puro permanece).

#### Deep Dives
- **Free-region (guard EC-2 reframed):** a computação EXCLUI sempre o bloco 0, a região da geração
  viva e a pending viva — por construção nunca devolve bloco 0/out-of-range (o guard vira uma
  função pura testável, não uma defesa contra FSM stale). Região devolvida é re-init incondicional
  página a página na escrita (pode conter lixo de gerações antigas/crashes).
- **Crash durante o reclaim:** meta nova já pivotada — páginas velhas parcialmente re-inicializadas
  são inertes; o próximo fold recomputa a região livre dos pointers (nada depende do reclaim ter
  completado).
- **Tamanho estável:** fold N+1 cabe na região do fold N-1 quando as gerações têm tamanho
  similar ⇒ alternação low/high; pico ~2× documentado em D2/Drawbacks.

#### Pseudo-code / Signatures
```pseudocode
fn free_region(meta: &MetaPointers, nblocks: u32, need: u32) -> u32:
  // regiões candidatas: [1, live_start) e [live_end, pending_start) — a maior folga contígua
  // que NÃO intersecta {0} ∪ [live_start, live_end) ∪ [pending_start, nblocks)
  if live_start > 1 && live_start - 1 >= need: return 1        // lowest-fit
  return nblocks                                                // extend (sem região que caiba)
// fold(): base = free_region(...); escreve items em base..; pivot; reinit região velha + pending velha
```

#### Tasks
1. RED: teste Rust puro de `free_region` (casos: cabe-na-baixa, não-cabe→extend, exclui bloco 0/viva/pending)
2. RED: `test_fold_reclaims_pages` (pytest) — 2 folds consecutivos: tamanho estável
3. GREEN: free_region + reclaim no fold
4. REFACTOR: None expected

#### TDD
```
RED:     test_free_region_lowest_fit() (Rust #[test] puro) — meta com região baixa livre de N páginas;
         assert_eq free_region(need<=N) == 1; assert_eq free_region(need>N) == nblocks; assert região
         devolvida nunca inclui 0/viva/pending (property nos 3 casos)
RED:     test_fold_reclaims_pages() (pytest) — build N; DELETE 30% + VACUUM (fold 1); anota
         pg_relation_size(index); DELETE mais 30% + VACUUM (fold 2); assert size(fold2) <= size(fold1)
         (alternação comprovada — sem reclaim seria estritamente maior)
GREEN:   implementação
REFACTOR: None expected
VERIFY:  cargo test --lib free_region (builder stage) + pytest -q -k reclaim
```

#### Concurrency tests
Coberto pelo concurrent test de T2.1 (mesmo fold lock EXCLUSIVE; free_region roda sob o lock).

#### Acceptance Criteria
- [ ] 2 folds consecutivos: `pg_relation_size(index)` do fold 2 `<=` do fold 1 (assert do teste `test_fold_reclaims_pages`)
- [ ] Guard EC-2 (reframed): `cargo test free_region` retorna `ok` — região livre nunca inclui bloco 0/geração viva/pending (property test puro)
- [ ] Pass: size — `wc -l theodb_rs/src/am/fold.rs` ≤ 500

#### DoD
- [ ] `pytest -q -k reclaim` retorna `passed`; `cargo test --lib` (builder) `ok`; suíte inteira sem `failed`; CHANGELOG coberto pela entrada do T2.1

### T2.3 — GUC `theodb.test_crash_after_pages` + testes de crash mid-fold (o gate do #47)

#### Objective
Crash-injection determinística no fold + os testes que provam os 3 estados pós-crash.

#### Why this step (action + reasoning)
1. **O que faz:** GUC SUSET (default 0) no padrão `guc.rs:9-46`; no shadow-write do fold.rs, após
   commitar a k-ésima página nova: se `k == guc`, `std::process::abort()`. Testes pytest: crash pré-pivot
   (geração velha íntegra), crash pós-pivot (nova íntegra — GUC alto o suficiente para cair no reclaim),
   VACUUM re-executado completa.
2. **Por que agora:** é o TESTE do #47 — sem ele o fix é claim sem evidência (Regra 5 aplicada a
   correctness). Blueprint D6: injection_points indisponível (verificado); abort() = crash real.

#### Evidence
- Blueprint §Q9 (verificação real do container: sem `--enable-injection-points`, 0 extensões injection).
- Receita do teste: TAP `013_crash_restart.pl` (Blueprint §Q6) — durabilidade seletiva + writability.
- Padrão GUC: `guc.rs:9-46` (GucRegistry, mesmo shape).

#### Files to edit
```
theodb_rs/src/am/guc.rs — GUC test_crash_after_pages (SUSET)
theodb_rs/src/am/fold.rs — ponto de injeção no shadow-write (e um segundo ponto pós-pivot/pré-reclaim)
benchmarks/tests/test_am_crash.py — 3 testes de crash mid-fold
```

#### Deep file dependency analysis
- `guc.rs` (59 LoC): +1 GUC no init() existente — padrão idêntico aos 2 atuais.
- `fold.rs`: 2 pontos de injeção nomeados (após k páginas; após pivot antes do reclaim) — custo zero
  quando GUC=0 (um load + compare).
- `test_am_crash.py`: reusa helpers docker da T1.1; conexão dedicada seta o GUC (SUSET — user postgres).

#### Deep Dives
- **Estados assertados:** (1) `crash_after_pages=2` → restart → scan retorna EXATAMENTE o resultado
  pré-VACUUM (geração velha; deletados ainda visíveis no índice é aceitável? NÃO — deletados foram
  removidos do heap; o scan do índice pode retornar TIDs mortos que o executor filtra por visibilidade
  ⇒ o assert é: scan não ERRA e retorna os vivos corretos); (2) GUC = ponto pós-pivot → restart → scan
  = resultado do corpus vivo (geração nova); reclaim incompleto é inofensivo; (3) após qualquer crash:
  VACUUM re-executado termina e converge.
- **SIGABRT vs postmaster:** abort mata o backend do VACUUM; postmaster derruba os demais e faz
  crash-restart in-place (container fica de pé — verificado no blueprint §Q6); `wait_ready` poll.

#### Pseudo-code / Signatures
```pseudocode
// fold.rs shadow-write loop:
pages_written += 1
if guc::TEST_CRASH_AFTER_PAGES.get() > 0 && pages_written == guc as usize:
    std::process::abort()   // SIGABRT: crash real, sem cleanup
```

#### Tasks
1. GUC + pontos de injeção (GREEN direto — o RED são os testes de crash abaixo, que sem os pontos não
   conseguem sequer injetar; registrado como exceção consciente de ordem: infra de teste primeiro)
2. RED→GREEN: os 3 testes de crash
3. REFACTOR: nomear os pontos de injeção como consts

#### TDD
```
RED:     test_crash_mid_fold_pre_pivot_leaves_old_generation() — build+DELETE; SET
         theodb.test_crash_after_pages=2; VACUUM → conexão morre (assert exceção psycopg2); wait_ready;
         assert scan ORDER BY <-> retorna os vivos corretos SEM erro; assert pg_relation_size cresceu
         (páginas órfãs — evidência do estado (a))
RED:     test_crash_post_pivot_leaves_new_generation() — idem com GUC=999999 e segundo ponto de injeção
         (pós-pivot); assert scan = corpus vivo exato; assert re-VACUUM converge (reclaim das órfãs)
RED:     test_vacuum_after_crash_converges() — após qualquer crash acima: SET GUC=0; VACUUM; assert
         resultado exato + tamanho estável
RED:     test_cancel_vacuum_mid_fold_leaves_old_generation() (EC-4) — pg_cancel_backend no VACUUM
         (responsivo via vacuum_delay_point); assert scan consistente (geração velha), re-VACUUM
         converge — cobre aborto de TRANSAÇÃO sem morte de processo (estado distinto do crash)
GREEN:   pontos de injeção + GUC
REFACTOR: consts nomeadas p/ os pontos
VERIFY:  pytest -q benchmarks/tests/test_am_crash.py
```

#### Concurrency tests
Coberto pelo concurrent test de T2.1; os crash tests deste task são single-connection por natureza
(a morte do processo é o evento, não a interleaving). Cancellation propagation adicional: EC-4
test_cancel_vacuum_mid_fold_leaves_old_generation (aborto de transação sob concorrência de lock).

#### Acceptance Criteria
- [ ] `pytest benchmarks/tests/test_am_crash.py -q -k "mid_fold or post_pivot or converges"` retorna `3 passed`; contra a imagem pré-T2.1 o pre-pivot retorna `failed` (evidência de que o teste detecta o #47 — registrada no log)
- [ ] GUC SUSET: `SET theodb.test_crash_after_pages=1` como usuário não-superuser retorna erro `42501` (assertRaises no teste)

#### DoD
- [ ] `pytest benchmarks/tests/test_am_crash.py -q` retorna `0 failed`; `grep -c "#47" CHANGELOG.md` ≥ 1 em `[Unreleased] § Fixed`

---

## Phase 3: Pending fold por threshold (insert-only workload)

### T3.1 — Fold no `amvacuumcleanup` quando pending > threshold

#### Objective
`amvacuumcleanup` executa o fold quando `pending_pages > theodb.vacuum_pending_threshold` mesmo sem
dead tuples; insert path intocado.

#### Why this step (action + reasoning)
1. **O que faz:** GUC `theodb.vacuum_pending_threshold` (default 16, min 1, max 65536, Userset);
   `amvacuumcleanup` (`mod.rs:176-190`): quando stats==NULL (bulkdelete não rodou), conta pending pages
   (meta v2 tem `pending_start`; count = nblocks - pending_start) e, acima do threshold, roda o MESMO
   fold da Phase 2 (corpus = estrutura + pending vivos).
2. **Por que agora:** fecha o gap G4 do deep-view (scan O(pending) para sempre em insert-only); depende
   do fold (Phase 2). Divergência consciente do GIN registrada em D3.

#### Evidence
- Gap: `mod.rs:181-183` (early-return com stats NULL); custo: `scan.rs:214-220` (O(pending) linear).
- Padrão/threshold: GIN `ginfast.c:39,458-471` (gin_pending_list_limit) — adaptado por D3.

#### Files to edit
```
theodb_rs/src/am/guc.rs — GUC vacuum_pending_threshold
theodb_rs/src/am/mod.rs — amvacuumcleanup: branch de fold por threshold
benchmarks/tests/test_am_maintenance.py — (NEW) testes do threshold
```

#### Deep file dependency analysis
- `mod.rs::amvacuumcleanup`: quando stats NULL, hoje retorna NULL; passa a alocar stats (PgBox alloc0 —
  contrato do AM permite) quando folda, preenchendo num_pages. Caller: core PG (lazy vacuum).
- Fold reusado de `fold.rs` — zero duplicação (DRY).

#### Deep Dives
- **Semântica stats NULL:** `amvacuumcleanup` recebe NULL quando não houve bulkdelete; retornar NULL é
  válido; retornar stats alocado também (nbtree faz p/ cleanup). Foldar exige o fold lock EXCLUSIVE
  (mesmo caminho — `lock.rs`).
- **Edge:** threshold=1 + 1 página pendente com 1 item → folda (mínimo); pending vazia → nunca folda;
  índice v1 blob (formato antigo) → skip com WARN (fold é v2-only; REINDEX migra).

#### Pseudo-code / Signatures
```pseudocode
amvacuumcleanup(vinfo, stats):
  if analyze_only: return stats
  if stats.is_null():
      pending = pending_page_count(rel)          // meta v2
      if pending > guc::VACUUM_PENDING_THRESHOLD.get():
          lock::index_exclusive(rel); fold_from_live(rel)   // mesmo fold Phase 2
          stats = alloc0(); (*stats).pages_deleted = pending
  (*stats).num_pages = nblocks(rel); return stats
```

#### Tasks
1. RED: testes threshold abaixo
2. GREEN: GUC + branch
3. REFACTOR: extrair `pending_page_count` (page.rs)

#### TDD
```
RED:     test_insert_only_vacuum_folds_pending_above_threshold() — build pequeno; INSERT (sem DELETE)
         até pending > threshold (setar threshold=2 p/ teste barato); VACUUM; assert pages_read do scan
         cai (THEODB_SCAN_PROFILE antes/depois) E resultado exato preservado
RED:     test_insert_only_vacuum_skips_below_threshold() — pending=1 página, threshold default; VACUUM;
         assert pending intacta (pages_read inalterado) — não folda à toa
RED:     test_pending_threshold_boundary() (EC-6) — pending == threshold ⇒ NÃO folda (semântica `>`);
         threshold+1 ⇒ folda; assert dos dois lados
GREEN:   implementação
REFACTOR: pending_page_count extraído
VERIFY:  pytest -q benchmarks/tests/test_am_maintenance.py -k pending
```

#### Concurrency tests
Coberto pelo concurrent test de T2.1 (mesmo lock, mesmo fold; nenhum estado novo compartilhado).

#### Acceptance Criteria
- [ ] pages_read pós-fold `<` pages_read pré-fold (números do `THEODB_SCAN_PROFILE` assertados no teste `test_insert_only_vacuum_folds_pending_above_threshold`)
- [ ] Abaixo do threshold: pages_read pré == pós (`assertEqual` no teste `test_insert_only_vacuum_skips_below_threshold`)

#### DoD
- [ ] `pytest -q -k pending` retorna `passed`; suíte sem `failed`; `grep -c "vacuum_pending_threshold" CHANGELOG.md` ≥ 1 em `[Unreleased] § Added`

---

## Phase 4: Cancelabilidade do build paralelo (seam DIP)

### T4.1 — `check_interrupt` callback na camada pura + `check_for_interrupts!` no am/

#### Objective
`pg_cancel_backend` interrompe `CREATE INDEX ... USING theodb_hnsw` em ≤ 1 batch.

#### Why this step (action + reasoning)
1. **O que faz:** `HnswIndex::build`/`build_parallel` ganham `check_interrupt: &(dyn Fn() + Sync)`
   chamado no leader entre batches (`hnsw_parallel.rs` loop de batches; `hnsw.rs` caminho serial idem
   por uniformidade); `am/build.rs` injeta `|| check_for_interrupts!()`; bench/testes puros: `&|| {}`.
   Loops de página do fold/bulkdelete: `vacuum_delay_point()` por página.
2. **Por que agora:** gap G7 do deep-view (build de 1M ignora cancel); D4 fixa o desenho DIP (a camada
   pura NÃO importa pg_sys — mesma disciplina do NeighborSource/FU-1).

#### Evidence
- Gap: `ann/hnsw_parallel.rs:44-54` (thread::scope sem check). Precedentes: pgvectorscale
  `build.rs:1078,1122` (check por tupla); `vacuum.rs:94-101` (delay por página). Macro:
  `check_for_interrupts!` (elog.rs:430) — Blueprint §Q8.

#### Files to edit
```
theodb_rs/src/ann/hnsw_parallel.rs — parâmetro check_interrupt no loop de batches do leader
theodb_rs/src/ann/hnsw.rs — propaga o parâmetro (serial + parallel)
theodb_rs/src/am/build.rs — injeta check_for_interrupts! (ambuild_hnsw + fold path)
theodb_rs/src/am/fold.rs — vacuum_delay_point por página no shadow-write/reclaim
benchmarks/tests/test_am_maintenance.py — teste de cancel
```

#### Deep file dependency analysis
- `hnsw_parallel.rs` (puro): assinatura muda; workers intocados (PG-free por design M44 — sinal
  pertence ao backend). Callers: `hnsw.rs:44-53`; transitivo: `build.rs`, testes puros do ann/.
- `fold.rs`: `vacuum_delay_point` é chamada pg_sys direta (camada am/ — permitido).

#### Deep Dives
- **Por que só no leader:** sinais Postgres são entregues ao backend (main thread); worker threads não
  devem tocar estado PG (invariante M44). O leader checa entre batches ⇒ latência de cancel ≤ 1 batch.
- **ereport ERROR → longjmp:** seguro sob `#[pg_guard]` nos call sites do am/ (Blueprint §Q8); o
  `thread::scope` do leader garante join dos workers antes do unwind atravessar (scope drop).
  **Atenção:** o check NÃO pode rodar DENTRO do scope com workers vivos → checa entre batches, fora do
  scope de cada batch (o desenho atual já batcheia por scope — verificar na implementação; se o scope é
  único, o check entra no ponto de sincronização de batch do leader).

#### Pseudo-code / Signatures
```pseudocode
pub fn build_parallel(..., check_interrupt: &(dyn Fn() + Sync)) -> Self
  for batch in batches:
      thread::scope(|s| { ...workers no batch... })   // join no fim do scope
      check_interrupt()                                // fora do scope: sem workers vivos
```

#### Tasks
1. RED: teste de cancel
2. GREEN: seam + injeção
3. REFACTOR: none expected

#### TDD
```
RED:     test_cancel_interrupts_create_index() — anti-flake EC-8: mede build baseline 1× (sem cancel);
         pytest.skip com WARN se baseline < 10s; senão: thread A: CREATE INDEX; main: cancela em
         baseline/4 via pg_cancel_backend(pid_A); assert A termina com ERRO "canceling statement" em
         < baseline/2 (não roda até o fim); assert índice NÃO existe (pg_class); re-CREATE ok
GREEN:   implementação
REFACTOR: None expected
VERIFY:  pytest -q benchmarks/tests/test_am_maintenance.py -k cancel
```

#### Concurrency tests
Cancellation propagation: test_cancel_interrupts_create_index é o teste de concorrência (sinal →
leader → unwind com workers joinados). Complemento Rust puro (parallel test):
test_build_parallel_interrupt_callback (#[test]) — build_parallel com callback de atomic-counter
invariant; assert contador == n_batches (chamado 1×/batch, nunca durante um scope).

#### Acceptance Criteria
- [ ] Cancel: build interrompido em `< baseline/2` segundos (baseline medido no próprio teste; `assertLess` com os dois números no log)
- [ ] Pureza preservada: `grep -c pg_sys theodb_rs/src/ann/hnsw_parallel.rs theodb_rs/src/ann/hnsw.rs` retorna `0` (gate mecânico)
- [ ] Bench FU-1 continua linkando: `cargo bench --no-run` retorna exit code `0` no stage builder

#### DoD
- [ ] `pytest -q -k cancel` retorna `passed`; suíte sem `failed`; `cargo bench --no-run` exit `0`; `grep -c "cancel" CHANGELOG.md` ≥ 1 em `[Unreleased]`

---

## Phase 5: `amcostestimate` honesto

### T5.1 — Custo real via genericcostestimate + ratio por AM

#### Objective
Planner recebe custo honesto: seqscan vence em N pequeno; índice vence em N realista — ambos provados.

#### Why this step (action + reasoning)
1. **O que faz:** substitui o corpo do stub (`mod.rs:117-140`) pelo template pgvectorscale
   (`GenericCosts` + `genericcostestimate` + out-params) com ratio IVF `probes/lists` (+0.5 seq) e ratio
   HNSW (fórmula pgvector); precisa de 2 variantes (o amroutine é compartilhado — dispatch por
   relopts/magic? NÃO: `make_amroutine` é único p/ os 2 AMs com o MESMO amcostestimate hoje — a variante
   é detectada abrindo a meta (1 página, NoLock, padrão pgvector `hnsw.c:166-168`)).
2. **Por que agora:** independente das fases 2-4 (arquivo/fn disjuntos); gap G6 do deep-view — custo 0
   mente para o planner. D5 fecha o desenho.

#### Evidence
- Stub: `mod.rs:117-140`. Template: `cost_estimate.rs:1-51`; fórmulas: `hnsw.c:197-232`,
  `ivfflat.c:122-150`; bindings `genericcostestimate`/`GenericCosts` verificados (Blueprint §Q8 + Blueprint §Q10).

#### Files to edit
```
theodb_rs/src/am/mod.rs — amcostestimate real (dispatch por meta magic; ratios por AM)
benchmarks/tests/test_am_maintenance.py — testes EXPLAIN nos 2 regimes
```

#### Deep file dependency analysis
- `mod.rs`: só a fn muda; wiring intocado. Leitura da meta no costestimate: 1 página via read path
  existente (NoLock, padrão pgvector) — invariante partial-read preservado.
- Testes existentes com EXPLAIN em N pequeno: auditar `test_index_am.py` (204 LoC) — asserts que
  esperavam índice em tabelas de brinquedo migram para N realista (nota do ROADMAP M48).

#### Deep Dives
- **Sem orderby → f64::MAX** (mantido do stub — pgvector faz igual).
- **tuples==0/meta ilegível/TORN (EC-3):** contrato inviolável — costestimate trata QUALQUER falha de
  leitura da meta (v1, corrupta, torn sob NoLock durante fold concorrente) como fallback silencioso
  `ratio=1.0`; NUNCA `error!` — um erro aqui derruba TODO planejamento de query durante VACUUMs.
- **Números do ratio HNSW:** constantes pgvector adotadas literalmente (0.55…) — paridade; tuning é
  fora de escopo (Blueprint §Limites honestos).

#### Pseudo-code / Signatures
```pseudocode
amcostestimate(root, path, loop_count, out...):
  if indexorderbys.is_null(): out = f64::MAX...; return
  costs = GenericCosts{ numIndexTuples: est_visited(meta, tuples), ..default }
  genericcostestimate(root, path, loop_count, &mut costs)
  ratio = match meta.magic { IVF => probes/lists (com 0.5 seq-adjust), HNSW => formula_pgvector(...) }
  *startup = costs.indexTotalCost * ratio; *total = adjust(costs, ratio); ... (5 out-params)
```

#### Tasks
1. RED: testes EXPLAIN
2. GREEN: implementação
3. REFACTOR: auditar/migrar asserts de N pequeno na suíte existente

#### TDD
```
RED:     test_planner_prefers_seqscan_small_n() — 100 linhas + índice; EXPLAIN ORDER BY <-> LIMIT 5;
         assert plano NÃO usa Index Scan (seqscan+sort vence — comportamento CORRETO novo)
RED:     test_planner_prefers_index_realistic_n() — 50k linhas + índice; EXPLAIN idem; assert Index Scan
         usado (o pushdown continua em N real)
RED:     test_costestimate_never_errors_on_unreadable_meta() (EC-3) — índice em formato v1 (fixture) OU
         meta zerada; EXPLAIN ORDER BY <-> funciona (fallback generic) — assert plano emitido sem erro
GREEN:   implementação
REFACTOR: migração dos asserts existentes (test_index_am.py) p/ N realista onde necessário
VERIFY:  pytest -q -k planner + suíte inteira
```

#### Concurrency tests
(none — single-threaded) — costestimate roda no planner do backend.

#### Acceptance Criteria
- [ ] Ambos os regimes assertados: `EXPLAIN` contains `Index Scan` em N=50k E não-contains em N=100 (outputs no log do teste)
- [ ] `pytest benchmarks/tests/ -q` retorna `0 failed` (asserts migrados p/ N realista listados na mensagem do commit)

#### DoD
- [ ] `pytest -q -k planner` retorna `passed`; suíte `0 failed`; `grep -c "amcostestimate" CHANGELOG.md` ≥ 1 em `[Unreleased] § Changed`

---

## Phase 6: Benchmark artifact (dados do M48 — e insumo do M55)

### T6.1 — `run_m48_maintenance.py` + artefato `docs/benchmarks/m48-am-maintenance.{md,json}`

#### Objective
Quantificar: (a) degradação de scan por pending acumulada e o ganho do fold; (b) WAL volume do
shadow-fold (insumo M55); (c) custo estimado vs comportamento real do planner. Mean±std ≥3 runs.

#### Why this step (action + reasoning)
1. **O que faz:** driver novo (reusa `theodb_bench` harness — Regra 9) que mede: pending 0/8/16/64
   páginas → p50 de scan + pages_read (antes/depois do VACUUM fold); WAL bytes do fold via
   `pg_stat_wal`/`pg_current_wal_lsn` delta em torno do VACUUM; EXPLAIN nos 2 regimes de N. Persiste
   md+json com metodologia e load-guard (lição M46: abortar se controle derivar).
2. **Por que agora:** o goal do usuário exige DADOS; o WAL volume é DoD do M48 (nota D2/ROADMAP) e
   insumo explícito do M55. Roda por último (mede o código final).

#### Evidence
- Harness: `benchmarks/theodb_bench/` (M45 Pareto — isolation + seeds); THEODB_SCAN_PROFILE
  (`hnsw_page.rs:572-575`) para pages_read; `pg_stat_wal` (PG17) para bytes de WAL.

#### Files to edit
```
benchmarks/run_m48_maintenance.py — (NEW) driver
docs/benchmarks/m48-am-maintenance.md — (NEW) artefato (metodologia + tabelas mean±std + caveats)
docs/benchmarks/m48-am-maintenance.json — (NEW) dados brutos por run
```

#### Deep file dependency analysis
- Driver: reusa conexão/seed/isolamento do harness M45; não toca produção.
- Artefato: segue o contrato de honestidade (`public-copy.md` — números com metodologia; box caveat).

#### Deep Dives
- **Load-guard:** pré-flight aborta se load1 > nproc/2; registra load no json (lição M46).
- **WAL delta:** `SELECT pg_current_wal_lsn()` antes/depois do VACUUM → `pg_wal_lsn_diff` = bytes; +
  `pg_stat_wal.wal_bytes` como segundo sinal.
- **Caveat honesto:** números da dev box; sem claim comparativo — é caracterização, não competição.

#### Pseudo-code / Signatures
```pseudocode
for run in 1..=3:
  for pending_pages in [0, 8, 16, 64]:
    setup(N=50k, seed=42); insert_until_pending(pending_pages)
    measure: p50 scan (200 queries seed), pages_read
    lsn0 = wal_lsn(); VACUUM; lsn1 = wal_lsn()
    measure pós-fold: p50, pages_read; record wal_bytes = diff(lsn0, lsn1)
emit json {runs, mean±std por célula} + md
```

#### Tasks
1. Driver + smoke (1 run)
2. 3 runs + artefatos md/json
3. Cross-link: nota no ROADMAP M55 (insumo disponível) — NÃO edita milestone [x]

#### TDD
```
RED:     test_m48_driver_smoke() (pytest, marcado slow) — driver com N=5k/1 run termina e produz json
         com as chaves obrigatórias (schema assert: runs, pending_series, wal_bytes, load)
GREEN:   driver
REFACTOR: None expected
VERIFY:  pytest -q -k m48_driver_smoke; depois python3 benchmarks/run_m48_maintenance.py (3 runs)
```

#### Concurrency tests
(none — single-threaded) — driver é 1 cliente sequencial por desenho; QPS multi-cliente é M50.

#### Acceptance Criteria
- [ ] Artefato: `test -f docs/benchmarks/m48-am-maintenance.md` exit `0`; contém tabela pending→(p50 em ms, pages_read) antes/depois + `wal_bytes` por fold + mean±std de ≥ 3 runs + load da box
- [ ] `python3 -c "import json; json.load(open('docs/benchmarks/m48-am-maintenance.json'))"` exit `0` com chaves `runs`, `pending_series`, `wal_bytes`, `load`
- [ ] `grep -c "caracterização" docs/benchmarks/m48-am-maintenance.md` ≥ 1 (caveat de box explícito)

#### DoD
- [ ] Artefatos commitados (`git ls-files docs/benchmarks/m48-am-maintenance.md` não-vazio); `grep -c "m48-am-maintenance" CHANGELOG.md` ≥ 1; effect>variância declarado no md

---

## Coverage Matrix

| # | Gap / Requirement (ROADMAP § M48 DoD) | Task(s) | Resolution |
|---|---|---|---|
| 1 | #46 — INIT fork UNLOGGED no WAL + teste de regressão crash | T1.1 | log_newpage_range + teste docker kill |
| 2 | #47 — meta-pivot atômico + teste fault-injection | T2.1, T2.2, T2.3 | shadow-fold + FSM reclaim + GUC crash-injection + 3 testes de estado |
| 3 | Pending fold com threshold (insert-only) | T3.1 | GUC + branch no amvacuumcleanup + testes O(pending)→O(estrutura) |
| 4 | Build paralelo cancelável (≤ 1 batch) | T4.1 | seam DIP + check_for_interrupts + teste de cancel |
| 5 | amcostestimate honesto (+ nota de testes N realista) | T5.1 | genericcostestimate + ratios + testes dos 2 regimes |
| 6 | WAL volume do shadow-rewrite registrado (insumo M55 — nota D2 do ROADMAP) | T6.1 | benchmark artifact com wal_bytes/fold |
| 7 | Restrição meta-pivot layout-agnóstico (anti-retrabalho M51) | T2.1 (D2) | fold.rs recebe items opacos; serializer separado |

| 8 | EC-1 — índices legados v1 no VACUUM (#47 para legados) | T2.1 | fold auto-migra v1→v2 no primeiro VACUUM |
| 9 | EC-2 (reframed v1.2) — região de reuso jamais inclui bloco 0/geração viva/pending | T2.2 | `free_region()` pura por construção + property test Rust |
| 10 | EC-3 — torn meta no costestimate durante fold | T5.1 | contrato nunca-erra + teste negativo |

**Coverage: 10/10 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas; halt-loop emitiu IMPLEMENTATION_COMPLETE + run_validation.py exit 0
- [ ] `pytest benchmarks/tests/ -q` 100% verde contra a imagem rebuilt (regressão zero nos 34 atuais)
- [ ] Testes Rust puros verdes (`cargo test` nos módulos sem pg) + imagem builda sem warnings novos
- [ ] Lint: clippy limpo no build da imagem (sem novos #[allow])
- [ ] File-size budget: fold.rs ≤ 500; nenhum arquivo tocado cresce além do baseline +10% sem split
- [ ] CHANGELOG.md `[Unreleased]` atualizado por fase (Fixed #46/#47; Added GUCs+artefato; Changed meta v2+custo)
- [ ] Formato: índices v1 dão erro tipado com instrução REINDEX (backward-compat honesta)
- [ ] **Runtime-metric proof:** pages_read (THEODB_SCAN_PROFILE) observado nos testes de pending
      (não-zero, decrescente pós-fold) — métrica exercitada em workload de integração
- [ ] Issues #46 e #47 comentados com a evidência (testes + sha) e fechados após merge
- [ ] Plan archived após READY_TO_MERGE + merge (mover p/ knowledge-base/plans/completed/)

## Failure scenarios (when I/O external)

O plano é sobre resiliência a falha do PRÓPRIO storage engine — os cenários de falha SÃO o produto:

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| backend do VACUUM (processo) | SIGABRT mid-shadow-write | GUC `test_crash_after_pages=2` (T2.3) | pós-replay: geração velha íntegra; scan correto; re-VACUUM converge |
| backend do VACUUM (processo) | SIGABRT pós-pivot/pré-reclaim | segundo ponto de injeção (T2.3) | geração nova íntegra; órfãs reclamadas no próximo VACUUM |
| postmaster inteiro (power-loss) | SIGKILL do container | `docker kill -s KILL` + `docker start` (T1.1) | crash-recovery; UNLOGGED resetada VÁLIDA (INSERT ok); LOGGED intacta |
| backend do CREATE INDEX | `pg_cancel_backend` mid-build | teste de cancel (T4.1) | ERRO "canceling statement" ≤ 1 batch; sem índice fantasma; re-create ok |
| conexão do teste | morte esperada no crash | asserts de exceção psycopg2 tipada em todos os crash tests | teste distingue morte-esperada de falha-real |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validar a cadeia completa contra a imagem final.

### Execution
```
docker build -t theodb:m48 .                                  # imagem final (clippy/warnings no build)
docker run -d --name theodb-m48-it -e POSTGRES_PASSWORD=theodb -p 55449:5432 theodb:m48
PGPORT=55449 python3 -m pytest benchmarks/tests/ -q           # suíte INTEIRA (34 existentes + novos)
cd theodb_rs && cargo test --lib 2>/dev/null || true          # testes puros (host, sem pg — best effort)
python3 benchmarks/run_m48_maintenance.py                     # 3 runs do artefato
```

### Acceptance Criteria
- [ ] Suíte inteira verde (incl. crash tests — os 5 cenários da tabela acima exercitados)
- [ ] Zero warnings novos de cargo no build da imagem
- [ ] Artefato de benchmark produzido com mean±std e load-guard registrado
- [ ] Bench FU-1 ainda linka (`cargo bench --no-run` no stage builder)

### If Validation Fails
1. Separar falhas causadas pelo plano vs pré-existentes
2. Corrigir todas as causadas pelo plano (validation halt-loop do /implement)
3. Re-rodar a cadeia
4. Pré-existentes: documentar no PR (não bloqueiam, mas são listadas)
