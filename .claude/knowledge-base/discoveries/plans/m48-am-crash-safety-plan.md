# Discovery Plan: M48 — Crash-safety & durabilidade do Index AM (INIT fork WAL, meta-pivot atômico, pending fold, cancelabilidade, custo honesto)

> **Version 1.1** (edge-cases absorvidos — `reviews/m48-am-crash-safety-edge-cases-2026-07-05.md`:
> EC-1 Q4 reformulada+dep Q1; EC-2 Q8 método pinado; EC-3/4/5 checkpoints) —
> Investiga como AMs Postgres maduros (pgvector, GIN/nbtree do core, pgvectorscale)
> garantem crash-safety de índice, para fechar os 5 DoDs do M48: fix #46 (INIT fork de UNLOGGED sem WAL),
> fix #47 (rewrite multi-página não-atômico no VACUUM), pending fold com threshold, build cancelável e
> `amcostestimate` honesto. Referências in-scope: `postgres` (core, REL_17_STABLE — fonte primária de
> GenericXLog/GIN/recovery-tests), `pgvector` (o AM análogo direto), `pgvectorscale` (AM Rust/pgrx — o
> precedente da nossa stack). Blueprint esperado: padrões citáveis file:line + estratégia de teste de
> crash reproduzível em Docker + superfície FFI pgrx necessária.

**Slug:** `m48-am-crash-safety`
**Owner:** paulohenriquevn
**Created:** 2026-07-05
**Time budget:** 8h (postgres core: 4h, pgvector: 2h, pgvectorscale: 1h, pgrx bindings: 1h)

## Context

O deep-view de trajetória (2026-07-05) encontrou dois furos de correctness no AM próprio, filados como
issues **#46** e **#47** (usetheodev/theo-db):

- **#46:** `ambuildempty`/`ambuildempty_hnsw` escrevem o INIT fork via `GenericXLog`
  (`theodb_rs/src/am/build.rs:259-276` → `page.rs:87-99`), mas `GenericXLogStart` seta
  `isLogged = RelationNeedsWAL(rel)` — **false** para relação UNLOGGED → nenhum WAL do INIT fork →
  após crash/failover o main fork resetado vem de um INIT fork vazio → `aminsert` falha
  ("truncated meta page") até REINDEX.
- **#47:** o rebuild-on-vacuum reescreve o índice **in-place, meta primeiro**, um registro GenericXLog
  por página (`page.rs:517-537` IVF; `hnsw_page.rs:387-406` HNSW). GenericXLog não tem atomicidade
  multi-registro → crash mid-fold ⇒ estado misto; pior caso, scan pontua bytes stale como vetores
  (resultado silenciosamente errado).

Mais três gaps do mesmo milestone (ROADMAP § M48): pending region nunca foldada em workload insert-only
(`am/mod.rs:181-183`), build paralelo não-cancelável (`ann/hnsw_parallel.rs:44-54`), e `amcostestimate`
stub (`am/mod.rs:117-140`). O M48 exige as âncoras upstream ANTES de qualquer código (Regra 9 — não
reinventar; `rules/architecture.md § 2` — a fronteira FFI é infra e precisa de contrato claro;
`rules/testing.md § 4.1` — os testes de crash são negative-cases que provam o error handling).
Perfil de rigor: `rules/discover-phd-rigor.md` (R1 SOTA-anchoring: o "SOTA" aqui é o próprio core
Postgres + pgvector; R6 honest-BLOCKED).

## Objective

Produzir um blueprint que permita escrever o plano do M48 sem nenhuma decisão de design em aberto:
cada um dos 5 DoDs com o padrão upstream citado (file:line), a superfície pgrx/FFI verificada, e a
estratégia de teste de crash executável no nosso harness Docker.

- [ ] Todas as research questions respondidas com citações a `.claude/knowledge-base/references/`
- [ ] Comparação cruzada populada para postgres/pgvector/pgvectorscale onde aplicável
- [ ] Recomendação concreta por DoD do M48 (1 decisão proposta por questão técnica)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/postgres/` | `src/backend/access/transam/generic_xlog.c`, `src/backend/access/gin/ginfast.c`, `src/backend/access/nbtree/` (metapage), `src/backend/access/brin/` (só se GIN não bastar), `src/test/recovery/t/`, `src/include/access/generic_xlog.h`, `src/backend/optimizer/util/plancat.c` + `src/backend/utils/adt/selfuncs.c` (costestimate), `src/backend/commands/vacuum.c` (vacuum_delay_point) | Fonte primária: semântica GenericXLog, precedente pending-list (GIN), metapage-pivot (nbtree/GIN), TAP tests de recovery, custo |
| `.claude/knowledge-base/references/pgvector/` | `src/hnswbuild.c`, `src/ivfbuild.c`, `src/hnswvacuum.c`, `src/hnswinsert.c`, `src/hnsw.c`, `src/ivfflat.c`, `test/` | O AM análogo direto — INIT fork fix, manutenção in-place, costestimate, suite de teste |
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/{vacuum.rs,cost_estimate.rs,meta_page.rs,build.rs}` | O precedente Rust/pgrx da nossa stack — como eles resolvem os mesmos callbacks |
| pgrx-pg-sys 0.16.1 (cargo registry local — NÃO é citação de blueprint, é verificação de FFI) | bindings gerados | Verificar que `log_newpage_range`, `RelationGetNumberOfBlocksInFork`, `vacuum_delay_point`, `ProcessInterrupts`/`CHECK_FOR_INTERRUPTS` estão expostos |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/postgres/src/backend/replication/`, `storage/` (exceto o que generic_xlog puxar), `parser/`, `executor/` | Fora do problema: replicação lógica/parser não tocam os 5 DoDs |
| `.claude/knowledge-base/references/postgres/doc/` | Docs de usuário; a fonte de verdade é o código + comentários |
| `.claude/knowledge-base/references/pgvector/{Dockerfile,META.json,*.control}` | Packaging, não crash-safety |
| `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/{distance,graph,labels}/` | Algoritmo ANN — já coberto por blueprints M41/M43/M46; M48 é durabilidade, não busca |
| `vectorchord`, `paradedb`, demais references | AGPL (vectorchord — só leitura conceitual barrada por disciplina D1 no código) / fora do problema |
| Qualquer projeto NÃO clonado em `.claude/knowledge-base/references/` | Cross-Project Rule: nunca afirmar sem ler a fonte |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** postgres core: 4h; pgvector: 2h; pgvectorscale: 1h; pgrx bindings (registry local): 1h.

**Rationale:** o core é a fonte primária de DUAS semânticas que nunca lemos de verdade (GenericXLog,
GIN pending-list) e do padrão de TAP recovery test — maior fatia. pgvector já tem âncoras conhecidas
(review de 2026-07-05 verificou `hnswbuild.c:1137`) — 2h para aprofundar vacuum + testes. pgvectorscale
é confirmatório (mesma stack pgrx). Alternativas: split igual (desperdiça em confirmação), só pgvector
(perde a semântica de generic_xlog que é a CAUSA do #46).

**Stop condition — per question (mandatory):** Fase A vazia após 3 variantes de query (pattern →
kind-based → path alternativo → escopo mais largo) → questão BLOCKED com "Fase A exhausted", seguir
para a próxima. NUNCA preencher com hotspot de outra questão.

**Stop condition — per project (mandatory):** budget do projeto exaurido com N questões pendentes →
todas BLOCKED com "budget exhausted"; se TODOS os projetos ficarem nesse estado, emitir
`<promise>BLUEPRINT_BLOCKED</promise>` com o relatório honesto — nunca `BLUEPRINT_COMPLETE` com
questões bloqueadas.

**Anti-pattern:** fabricar Fase B para fechar questão com Fase A exaurida (Regra 3).

**Consequences:** questões bloqueadas viram seed da próxima discovery; o blueprint as lista em
`## Blocked questions`.

### D2 — Investigation depth

**Decision:** Fase B lê **funções inteiras** (não trechos) nos hotspots de crash-safety
(generic_xlog.c: `GenericXLogStart/Register/Finish/generic_redo` completos; ginfast.c:
`ginHeapTupleFastInsert` + `ginInsertCleanup` completos), porque a correção mora nos invariantes de
ordem-de-escrita e nos comentários — trechos perdem o contexto que explica o *porquê*.
Para costestimate e TAP tests, leitura de exemplares representativos (1 TAP test completo, os 2
costestimates do pgvector completos).

**Rationale:** #47 é exatamente um bug de invariante de ordem — a classe de bug que leitura parcial
não pega. Alternativa (grep de símbolos): suficiente para Q-deps, insuficiente para Q-techniques.

**Consequences:** menos arquivos, mais profundidade; o budget do core (4h) é gasto em ~6 funções lidas
por inteiro.

### D3 — postgres core adicionado às references (decisão desta discovery)

**Decision:** clonado `postgres` REL_17_STABLE (shallow, 179M) em
`.claude/knowledge-base/references/postgres/` — licença PostgreSQL (permissiva, D1-OK).

**Rationale:** GenericXLog/GIN/recovery-tests são a fonte primária dos DoDs #46/#47/pending-fold; sem o
core, as respostas viriam de docs de segunda mão (viola `discover-phd-rigor.md` R2 — fontes primárias).
Alternativa rejeitada: WebFetch em postgresql.org (allowlisted, mas sem grep/ast-grep e sem file:line
estável para o blueprint).

**Consequences:** references ganha um projeto grande (179M shallow); M55 (VACUUM wall) reutiliza.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Qual é a semântica EXATA do GenericXLog — por página ou por registro? limite de páginas por registro (`MAX_GENERIC_XLOG_PAGES`)? o que `isLogged=false` (UNLOGGED) muda em Start/Finish/redo? full-page-image sempre ou delta? | techniques | `.claude/knowledge-base/references/postgres/` | Grep `MAX_GENERIC_XLOG_PAGES\|isLogged\|RelationNeedsWAL` em `src/backend/access/transam/generic_xlog.c` + `src/include/access/generic_xlog.h` | Ler `GenericXLogStart/RegisterBuffer/Finish/Abort` + `generic_redo` INTEIROS (D2) | Contrato citado file:line: atomicidade (N páginas/1 registro), comportamento UNLOGGED, custo (FPI vs delta) — a base dos fixes #46/#47 |
| Q2 | Como o pgvector garante o INIT fork de UNLOGGED no WAL (o fix do #46) — e por que `log_newpage_range` e não GenericXLog? O IVF faz igual ao HNSW? | techniques | `.claude/knowledge-base/references/pgvector/` | Grep `INIT_FORKNUM\|log_newpage_range\|smgrimmedsync` em `src/hnswbuild.c` + `src/ivfbuild.c` | Ler `hnswbuildempty`/`ivfflatbuildempty` + o tail de `BuildIndex` (a janela `RelationNeedsWAL \|\| INIT_FORKNUM`) por inteiro | Padrão exato transplantável p/ `am/build.rs:259-276` (chamadas, ordem, sync), com deltas IVF vs HNSW |
| Q3 | Como o GIN folda a pending list com threshold — quem dispara (`gin_pending_list_limit`), em qual callback (insert? vacuum? autoanalyze?), com que lock, e como o fold é crash-safe página-a-página? | techniques | `.claude/knowledge-base/references/postgres/` | Grep `gin_pending_list_limit\|ginInsertCleanup\|GinPageDeletePage` em `src/backend/access/gin/ginfast.c` | Ler `ginHeapTupleFastInsert` + `ginInsertCleanup` INTEIROS (D2) — invariantes de ordem e comentários | O precedente do nosso pending-fold-com-threshold: trigger, lock, garantia de crash, e o que NÃO copiar (deadlock notes do GIN) |
| Q4 | Qual é o **primitivo** de metapage-update atômico no core — nbtree fast root (`_bt_upgrademetapage`/redo) e GIN meta (pending head) — o que UM registro WAL pode pivotar, e o que fazem com páginas que ficam órfãs? **depends: Q1** (o limite `MAX_GENERIC_XLOG_PAGES` restringe o registro do pivot). Resposta PARCIAL é válida: se não houver precedente de shadow-generation de índice inteiro, a âncora honesta é o primitivo single-record meta update (EC-1) | techniques | `.claude/knowledge-base/references/postgres/` | Grep `metapg\|BTPageSetMeta\|_bt_upgrademetapage\|fastroot` em `src/backend/access/nbtree/nbtinsert.c`,`nbtpage.c`; `GinMetaPageData` em `gin/` | Ler as funções de update de metapage + o redo correspondente; capturar o que o primitivo garante e o que NÃO garante | O desenho do meta-pivot do #47: ordem de escrita, conteúdo do registro único (≤ MAX_GENERIC_XLOG_PAGES da Q1), política de reciclagem (FSM vs órfã até próximo vacuum), e o registro honesto de onde inovamos além do precedente |
| Q5 | Como o pgvector faz manutenção in-place por página no VACUUM (`hnswvacuum.c`) — o que garante que CADA página modificada é individualmente consistente, e por que ele NUNCA precisa de rewrite global? | techniques | `.claude/knowledge-base/references/pgvector/` | Grep `HnswVacuumScan\|RepairGraph\|MarkDeleted` em `src/hnswvacuum.c` | Ler o fluxo de vacuum inteiro (fases, locks, WAL por página) | Contraste honesto com nosso rebuild-fold: o que o in-place compra (sem cliff O(N)-RAM) e o que custa (complexidade de repair) — insumo do M55, e sanity-check de que o meta-pivot é o fix certo AGORA |
| Q6 | Como o core testa recovery/crash de verdade — a anatomia de um TAP test de `src/test/recovery/t/` (kill -9? `pg_ctl stop -m immediate`? asserts pós-restart?) e o que dele é replicável no nosso harness pytest+Docker | tests | `.claude/knowledge-base/references/postgres/` | Glob `src/test/recovery/t/*.pl`; Grep `immediate\|kill\|crash` para escolher 1-2 exemplares | Ler 1 TAP test de crash completo (ex.: o de crash recovery básico) + o helper `PostgreSQL::Test::Cluster` (métodos `stop('immediate')`) | Receita: como derrubar (modo immediate == crash), o que assertar pós-restart, transplantada para pytest+docker (`docker kill` ≈ immediate) |
| Q7 | Como pgvector e pgvectorscale testam insert→delete→vacuum→scan (o ciclo do #47/pending)? Existe QUALQUER teste de crash neles, ou a cobertura é só funcional? | tests | `.claude/knowledge-base/references/pgvector/`, `.claude/knowledge-base/references/pgvectorscale/` | Grep `vacuum\|VACUUM` em `pgvector/test/` (sql/expected) e em `pgvectorscale/pgvectorscale/src/access_method/vacuum.rs` (mod tests) | Ler os testes de vacuum de ambos; anotar honestamente se crash-safety é testada (hipótese: NÃO — só funcional) | Inventário do que os peers testam vs não testam — define onde nosso teste de crash INOVA (e o marca como além-da-paridade) |
| Q8 | A superfície FFI necessária existe no pgrx 0.16.1? `log_newpage_range`, `RelationGetNumberOfBlocksInFork`, `CHECK_FOR_INTERRUPTS` (macro — qual é o equivalente pgrx?), `vacuum_delay_point`, `FreeSpaceMap` (RecordFreeIndexPage/GetFreeIndexPage), `IndexFreeSpaceMapVacuum` | deps | pgrx-pg-sys 0.16.1 (cargo registry local; verificação, não citação) + `.claude/knowledge-base/references/pgvectorscale/` | **Método pinado (EC-2, pré-verificado):** `grep -n '<símbolo>' ~/.cargo/registry/src/*/pgrx-pg-sys-0.16.1/src/include/pg17.rs` — bindings PRÉ-GERADOS existem no registry (3 símbolos-chave já confirmados na edge-case review); E grep de uso em `pgvectorscale/pgvectorscale/src/` | Ler o uso real no pgvectorscale (ex.: `check_for_interrupts!` macro pgrx?) | Tabela símbolo → existe? → como chamar do Rust (com o precedente pgvectorscale citado) → gap se não existir (fallback: extern manual) |
| Q9 | Como injetar crash determinístico no meio do fold no NOSSO ambiente: `pg_ctl stop -m immediate` dentro do container vs `docker kill`; e o PG17 empacotado tem `injection_points` (build flag `--enable-injection-points`) ou precisamos de GUC de teste próprio (`theodb.test_crash_after_pages=N`)? | tools | `.claude/knowledge-base/references/postgres/` + container theodb local (verificação) | Grep `injection_point` em `src/backend/utils/misc/` + `pg_config` no container (`--enable-injection-points`?); listar como os TAP tests param o servidor | Ler `src/test/modules/injection_points/README` (se existir) + decidir o mecanismo viável no Debian-packaged PG17 | Decisão de tooling: mecanismo de crash-injection executável no CI (com fallback GUC próprio se injection_points indisponível) |
| Q10 | Como pgvector implementa `hnswcostestimate`/`ivfflatcostestimate` (as ~30 linhas), o que pgvectorscale faz em `cost_estimate.rs`, e o que de `genericcostestimate` precisamos via FFI? | techniques→(overflow: contado em tools p/ budget? NÃO — ver Coverage: mapeada em **deps**? NÃO) — **corner: techniques é o natural mas está cheio; esta questão é sobre COMO CHAMAR a infra de custo = fronteira deps/techniques; mapeada em **deps** (a resposta é a superfície `selfuncs.h` que consumimos) | deps | `.claude/knowledge-base/references/pgvector/`, `.claude/knowledge-base/references/pgvectorscale/`, `.claude/knowledge-base/references/postgres/` | Grep `costestimate` em `pgvector/src/{hnsw.c,ivfflat.c}`, `pgvectorscale/pgvectorscale/src/access_method/cost_estimate.rs`, `postgres/src/backend/utils/adt/selfuncs.c` (assinatura `genericcostestimate`) | Ler os 3 costestimates completos, lado a lado | Tabela comparada: inputs usados (spc_random_page_cost, tuples, pages), fórmula, e a assinatura FFI mínima p/ nosso `am/mod.rs:117-140` |

> Nota de budget (`discover-phd-rigor.md § 2`): 10 questões, techniques=5 (Q1-Q5, ≥2 ✓, ≤5 ✓),
> tests=2 (Q6-Q7), deps=2 (Q8, Q10), tools=1 (Q9). Total dentro de 6-14.

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q6, Q7 | Covered |
| Dependencies | Q8, Q10 | Covered |
| Tools | Q9 | Covered |
| Techniques | Q1, Q2, Q3, Q4, Q5 | Covered |

**Coverage: 4/4 corners covered (100%)**

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | path declarado na Fase A existe em `.claude/knowledge-base/references/` | Qx BLOCKED "path not found", próxima |
| Per-question Fase A budget | ≥1 hotspot OU 3 variantes tentadas | BLOCKED "Fase A exhausted", próxima |
| After answering Qx | seção do blueprint tem ≥1 citação file:line | Re-iterar Qx (1 retry) |
| Q9 especial | a checagem do container (`pg_config`) roda de verdade, não é suposta | Antes da Q9: `docker start theodb-m48-verify \|\| docker run -d --name theodb-m48-verify -e POSTGRES_PASSWORD=theodb -p 55448:5432 theodb:m48-verify` + `pg_isready` (EC-3). Se ainda indisponível, BLOCKED com reason — nunca "provavelmente não tem" |
| Q5 anti-creep (EC-4) | Q5 termina na tabela de contraste + sanity-check do meta-pivot | Qualquer desenho de manutenção in-place para o theodb é M55 — parar e anotar como seed |
| Q6 exemplar pinado (EC-5) | começar por `src/test/recovery/t/013_crash_restart.pl` (existe — verificado) e `022_crash_temp_files.pl` | Só grep exploratório se os exemplares não bastarem |
| Q4 ordem (EC-1) | Q1 respondida ANTES de Q4 (o limite de páginas/registro entra no desenho do pivot) | Se Q1 BLOCKED, Q4 usa o header `generic_xlog.h:23` diretamente como fonte mínima |
| Per-project time budget | budget não exaurido | Restantes BLOCKED "budget exhausted", próximo projeto |
| Before promising complete | 4 corners populados + 1 recomendação por DoD do M48 | Recusar promise, continuar |

## Acceptance Criteria

- [ ] Todas as questões respondidas OU BLOCKED com razão explícita
- [ ] 4 coverage corners populados no blueprint
- [ ] Toda citação aponta path real em `.claude/knowledge-base/references/{...}`
- [ ] ≥1 seção de ADR no blueprint sintetizando as decisões (uma recomendação por DoD do M48)
- [ ] Time budget respeitado por projeto
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint salvo em `.claude/knowledge-base/discoveries/blueprints/m48-am-crash-safety-blueprint.md`

## Global Definition of Done

- [ ] Todas as fases completadas (plan → edge-cases → plan-confidence → execute → confidence → improve se preciso)
- [ ] Verdict final do `/discover-confidence` registrado no header do blueprint
- [ ] Zero citações fabricadas
- [ ] Coverage Matrix 100%
- [ ] ADRs referenciam ≥1 regra do projeto (`architecture.md § 2` DIP/fronteira FFI; `testing.md § 4.1`
      negative-cases; Regra 9 não-reinventar; `discover-phd-rigor.md` R1/R2/R6)
