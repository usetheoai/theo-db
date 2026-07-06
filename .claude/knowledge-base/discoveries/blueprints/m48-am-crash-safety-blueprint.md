# Blueprint: M48 — Crash-safety & durabilidade do Index AM

> **Discovery:** `m48-am-crash-safety` (plan v1.1) · **Executed:** 2026-07-05 · **Verdict:** **SHIPPABLE (99.7)** — `/discover-confidence` 2026-07-05, zero hard caps
> Fontes primárias lidas por inteiro: postgres REL_17_STABLE (generic_xlog, GIN fast path, nbtree meta,
> TAP recovery), pgvector (buildempty/vacuum/costestimate/testes), pgvectorscale (vacuum/cost/FFI pgrx
> 0.16.1 — mesma versão pinada que a nossa), bindings pgrx-pg-sys 0.16.1 verificados no registry, e o
> container real `theodb:m48-verify` (PG 17.10 Debian) para as checagens de tooling.
> Questões: 10/10 respondidas, 0 BLOCKED. Consumidor: plano do M48 (issues #46, #47 + 3 DoDs).

## Context

O deep-view 2026-07-05 encontrou dois furos de crash-safety no AM próprio (issues #46/#47) e três gaps
operacionais (pending nunca foldada, build não-cancelável, `amcostestimate` stub) — ROADMAP § M48. Este
blueprint responde COMO os AMs maduros resolvem cada um, com evidência file:line, para que o plano do
M48 não tenha nenhuma decisão de design em aberto.

## Objective

Entregar ao plano do M48 uma decisão fechada por DoD: o padrão upstream citado (file:line), a superfície
FFI verificada nos bindings pinados, e a estratégia de teste de crash executável no harness Docker —
zero decisões de design em aberto quando o `/to-plan` rodar.

---

## Coverage Corner 1 — Integration Tests

### Q6 — Anatomia do TAP recovery test do core (a receita do nosso teste de crash)

**Resposta.** `013_crash_restart.pl` mata um **backend individual** (`pg_ctl kill QUIT/KILL $pid`,
linhas 104/189) com `restart_after_crash=1` (linhas 27-31) e o postmaster executa crash-restart + WAL
replay. Setup: dado commitado + transação ABERTA não-commitada (linhas 64-81); asserts pós-restart:
(1) reconexão via poll, (2) **durabilidade seletiva** — commitado sobrevive, in-progress some (223-225),
(3) writability — INSERT novo funciona (227-232), (4) restart ordenado re-assertado (235-246).
`022_crash_temp_files.pl` adiciona a técnica de **congelamento determinístico por lock heavyweight** +
polling em `pg_locks WHERE NOT granted` (81-131) — funciona para tuple locks SQL-visíveis, **não** para
buffer locks do fold (limite honesto). Helper: `Cluster.pm` `kill9` (SIGKILL no postmaster, 1155-1167),
`stop('immediate')` (1184-1213).

**Verificado no container real:** o padrão 013 reproduzido em `theodb-m48-verify` (PG 17.10) —
`kill -9 <backend>` → crash-restart in-place; e `docker kill -s KILL` + `docker start` → log real
`automatic recovery in progress` / `redo starts/done`. **`docker kill -s KILL` ≈ kill9 do postmaster
(power-loss); é mais fiel que `pg_ctl stop -m immediate`** (SIGQUIT roda handlers).

- Evidência: `.claude/knowledge-base/references/postgres/src/test/recovery/t/013_crash_restart.pl:27-31,64-81,104,189,223-246`;
  `.claude/knowledge-base/references/postgres/src/test/recovery/t/022_crash_temp_files.pl:81-131`;
  `.claude/knowledge-base/references/postgres/src/test/perl/PostgreSQL/Test/Cluster.pm:1155-1279`.

### Q7 — O que pgvector/pgvectorscale testam (e a lacuna que herdamos de NÃO copiar)

**pgvector:** zero testes de crash (grep `kill|crash|restart|immediate` em `test/t/` = vazio). Cobertura:
funcional de vacuum (`002_ivfflat_vacuum.pl` reuso de espaço; `011_hnsw_vacuum.pl:30-43` tamanho ≤1.02×
pós-ciclo; `014/022/026/030_*_vacuum_recall.pl`) e **réplica streaming como proxy de WAL-correctness**
(`001_ivfflat_wal.pl:86-97`, `010_hnsw_wal.pl` — 10 ciclos DELETE→VACUUM→INSERT comparando primary vs
réplica). A suíte SQL cobre unlogged só como create+query (`test/sql/hnsw_vector.sql:80-88`) — **sem
restart, o INIT fork nunca é testado** (a regressão do #46 deles não tem teste).
**pgvectorscale:** zero crash tests; funcional via scaffolds (`vacuum.rs:169-271` delete→vacuum→reuse
assert count==303; `:278-372` VACUUM FULL) com o padrão **"mock test + client externo + Mutex"**
(`vacuum.rs:165-178,460-464`) porque VACUUM não roda em SPI — **precedente direto para nossos #[pg_test]
de vacuum**.

**Implicação:** nosso harness de crash (docker kill + restart + asserts de durabilidade seletiva) é
**trabalho novo, sem precedente no corpus** — é onde o M48 vai além da paridade. O teste de réplica do
pgvector é um segundo oráculo barato a considerar no futuro (2 nós).

- Evidência: `.claude/knowledge-base/references/pgvector/test/t/011_hnsw_vacuum.pl:30-43`;
  `.claude/knowledge-base/references/pgvector/test/sql/hnsw_vector.sql:80-88`;
  `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/vacuum.rs:159-465`.

---

## Coverage Corner 2 — Dependencies

### Q8 — Superfície FFI no pgrx 0.16.1: 100% coberta, zero extern manual

Bindings verificados em `pg17.rs` do registry (pgrx-pg-sys 0.16.1 — mesma versão pinada do
pgvectorscale, `Cargo.toml:31,42`):

| Símbolo | Binding (pg17.rs) | Precedente pgvectorscale | Nota |
|---|---|---|---|
| `log_newpage_range(rel, forknum, startblk, endblk, page_std)` | :37137 ✓ | nenhum (1º uso Rust é nosso; precedente semântico é o C do pgvector) | o fix do #46 |
| `RelationGetNumberOfBlocksInFork` | :37718 ✓ | `vacuum.rs:38,150` | |
| `vacuum_delay_point()` (0 args no PG17) | :41377 ✓ | `vacuum.rs:94-101` (com cfg pg18: 1 arg `bool`) | copiar o cfg se suportarmos pg18 |
| `RecordFreeIndexPage` / `GetFreeIndexPage` / `IndexFreeSpaceMapVacuum` | :45767/:45766/:45769 ✓ | nenhum (DiskANN não usa FSM) | reclaim do meta-pivot |
| `ProcessInterrupts` | :36527 ✓ (não chamar direto) | — | |
| `CHECK_FOR_INTERRUPTS` | macro `pgrx::check_for_interrupts!` (`elog.rs:430-439`) | `build.rs:1078,1122` (1×/tupla no ambuild) | cancelabilidade; só sob `#[pg_guard]` |
| `genericcostestimate` + `GenericCosts` | :50132 / :33131 (struct com `Default`) | `cost_estimate.rs:40` | o fix do costestimate |

`INTERRUPTS_PENDING` não existe como símbolo (o flag é `InterruptPending`, :36513) — usar a macro.

- Evidência: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/vacuum.rs:38,94-101,150`;
  `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/build.rs:1078,1122`;
  `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml:31,42`;
  bindings: cargo registry local (verificação, não citável como reference — line numbers registrados acima).

### Q10 — Modelo de custo: `genericcostestimate` + escala por fração-visitada

**pgvector** (`hnsw.c:135-232`, `ivfflat.c:86-150`): ambos chamam `genericcostestimate` do core e só
ajustam por cima. (1) `indexorderbys == NIL` → custo infinito (índice inutilizável sem ORDER BY de
distância); (2) HNSW: `ratio = (entryLevel·m + layer0TuplesMax·layer0Selectivity)/tuples` com
`layer0Selectivity = 0.55·log(tuples)/(log(m)·(1+log(ef_search)))`; IVF: `ratio = probes/lists` + 50% do
custo random→seq (listas sequenciais); (3) `indexStartupCost = indexTotalCost · ratio` — startup≈total é
a forma canônica de ANN ordered scan; (4) selectivity/correlation ficam os do genérico; leitura do
metapage no costestimate = 1 página (compatível com nosso invariante partial-read).
**pgvectorscale** (`cost_estimate.rs:1-51`): o template FFI Rust exato — `#[pg_guard]`
`unsafe extern "C-unwind"`, null-check de `indexorderbys` → `f64::MAX`, `GenericCosts { numIndexTuples:
tuples/100., ..Default::default() }` (chute de 1% com TODO honesto), `genericcostestimate`, escreve os 5
out-params. 51 linhas.

- Evidência: `.claude/knowledge-base/references/pgvector/src/hnsw.c:135-232`;
  `.claude/knowledge-base/references/pgvector/src/ivfflat.c:86-150`;
  `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/cost_estimate.rs:1-51`.

---

## Coverage Corner 3 — Tools

### Q9 — Crash-injection determinístico: GUC de teste próprio (injection_points indisponível)

**Verificado no container real** (`theodb-m48-verify`, PG 17.10 Debian `17.10-1.pgdg12+1`):
`pg_config --configure` **sem** `--enable-injection-points`; `pg_available_extensions LIKE '%injection%'`
→ 0 rows. O framework (`injection_point.h:17-20` — macro vira no-op sem a flag; `injection_points.c` —
callbacks nomeados + wait por condition variable) exigiria recompilar o PG — fora do nosso modelo de
distribuição.

**Recomendação (decidida):** GUC `theodb.test_crash_after_pages` — **sempre compilado, default 0 (off),
`GucContext::Suset`**, mesmo padrão dos GUCs existentes (`theodb_rs/src/am/guc.rs`). No ponto do fold,
após N páginas: `std::process::abort()` (SIGABRT — crash de backend SEM cleanup → postmaster faz
crash-restart + replay; `pg_sys::proc_exit` roda shutdown callbacks = crash "limpo demais").
**Rejeitados:** feature-flag de compilação (testaria binário ≠ shipped — "tests passing ≠ system works");
timing com `pg_ctl -m immediate` (flaky para mid-fold); gdb (frágil no CI). Complemento: 1 teste
coarse-grained `docker kill -s KILL` + `docker start` (power-loss do postmaster, já provado funcionando).
Trade-off honesto: ~10 linhas de scaffolding no binário de produção — mitigado por default-off + SUSET;
precedente: developer GUCs do próprio PG.

- Evidência: output real de `pg_config --configure` no container; `.claude/knowledge-base/references/postgres/src/include/utils/injection_point.h:17-20`;
  `.claude/knowledge-base/references/postgres/src/test/modules/injection_points/injection_points.c:1-40` (README não existe neste checkout — verificado, não fabricado).

---

## Coverage Corner 4 — Techniques

### Q1 — Semântica exata do GenericXLog (a física dos fixes)

1. **Cap estrutural: 4 páginas por registro** — `MAX_GENERIC_XLOG_PAGES = XLR_NORMAL_MAX_BLOCK_ID = 4`
   (`generic_xlog.h:23` → `xloginsert.h:27`); 5º buffer = `elog(ERROR)` (`generic_xlog.c:326-327`);
   GenericXLog **não expõe** `XLogEnsureRecordSpace`.
2. **Atômico no replay:** todas as páginas de um registro aplicadas com locks segurados até o fim
   (`generic_redo`, `generic_xlog.c:477-533` — solta tudo só no loop final 527-532); leitor de hot
   standby nunca vê meio-registro.
3. **UNLOGGED (`isLogged=false`):** Finish faz memcpy + MarkBufferDirty, **ZERO WAL, ZERO PageSetLSN**
   (`generic_xlog.c:412-431`) — a causa exata do #46. Durabilidade do INIT fork é responsabilidade do
   caller: btree usa `use_wal = RelationNeedsWAL || forknum == INIT_FORKNUM` (`bulk_write.c:90-92` +
   `smgrimmedsync` :214); gist usa `log_newpage_buffer(_, true)` incondicional (`gist.c:133-150`).
   **Anti-pattern identificado: bloom** escreve INIT fork via GenericXLog (`blinsert.c:164-168`) —
   durabilidade repousa só em `BM_PERMANENT`/checkpoint (`bufmgr.c:1749-1756`) — NÃO copiar.
4. **Delta vs FPI:** default = delta byte-a-byte (`computeDelta`, :227-263); `GENERIC_XLOG_FULL_IMAGE`
   (:369-390) força FPI — usar quando a página é nova/reescrita inteira (precedente bloom
   `blinsert.c:54`; metapage `blutils.c:465-466`).

- Evidência: `.claude/knowledge-base/references/postgres/src/backend/access/transam/generic_xlog.c:21-47,277,298-330,336-436,477-533`;
  `.claude/knowledge-base/references/postgres/src/include/access/generic_xlog.h:23`;
  `.claude/knowledge-base/references/postgres/src/include/access/xloginsert.h:21-28`;
  `.claude/knowledge-base/references/postgres/src/backend/storage/smgr/bulk_write.c:90-92,214`;
  `.claude/knowledge-base/references/postgres/src/backend/access/gist/gist.c:133-150`;
  `.claude/knowledge-base/references/postgres/contrib/bloom/blinsert.c:46-58,164-168`.

### Q2 — O fix do #46 no pgvector (transplante direto)

`hnswbuildempty` = build normal apontado ao INIT fork (`hnswbuild.c:1164-1171`) via **shared buffers**
(`ReadBufferExtended(index, forkNum, P_NEW, …)`, `hnswutils.c:182-188`) + o tail:
`if (RelationNeedsWAL(index) || forkNum == INIT_FORKNUM) log_newpage_range(index, forkNum, 0,
RelationGetNumberOfBlocksInFork(index, forkNum), true)` (`hnswbuild.c:1137-1138`). **Sem
`smgrimmedsync`** (zero hits — durabilidade via FPI do log_newpage_range + checkpoint). O comentário do
IVF é a autoridade sobre o porquê: *"Write WAL for initialization fork since GenericXLog functions do
not"* (`ivfbuild.c:1046`). Delta IVF vs HNSW: IVF WAL-loga página-a-página no build (GenericXLog
FULL_IMAGE, `ivfutils.c:154-168`) e só precisa do range para INIT (`ivfbuild.c:1047`); HNSW faz bulk-WAL
único no fim (build sem WAL por página).

**Transplante para o theodb:** manter bufmgr; (1) plumbing de `forkNum` no caminho de init/extend
(`am/page.rs` hoje assume MAIN); (2) `log_newpage_range(0..nblocks, page_std=true)` no fim do
buildempty, incondicional para INIT fork. O GenericXLog no buildempty unlogged é o bug, não a proteção.

- Evidência: `.claude/knowledge-base/references/pgvector/src/hnswbuild.c:1137-1138,1164-1171`;
  `.claude/knowledge-base/references/pgvector/src/ivfbuild.c:1046-1047,1078-1085`;
  `.claude/knowledge-base/references/pgvector/src/hnswutils.c:182-188`;
  `.claude/knowledge-base/references/pgvector/src/ivfutils.c:131-168`.

### Q3 — GIN pending-list: a ordem de escrita canônica + o lock

Gatilhos do fold: insert quando `nPendingPages·FREESIZE > gin_pending_list_limit·1024`
(`ginfast.c:458-471`, cleanup FORA da critical section), vacuum bulkdelete/cleanup
(`ginvacuum.c:593-594,716-721`), autoanalyze (:702-708), SQL `gin_clean_pending_list()` (:1080).
**Lock:** heavyweight `LockPage(index, GIN_METAPAGE_BLKNO, ExclusiveLock)` (`ginfast.c:807-828`) —
serializa cleaners SEM bloquear inserts; insert path usa `ConditionalLockPage` e desiste se há cleaner.
**Crash-safety (o comentário canônico, `ginfast.c:766-772`):** (1) inserir tudo na estrutura principal
primeiro (cada passo WAL-logged e inofensivo se órfão); (2) SÓ DEPOIS remover da pending — `shiftList`
emite UM registro cobrindo meta+páginas deletadas (:553-671; precisa `XLogEnsureRecordSpace` — indisponível
p/ GenericXLog, reforça o cap de 4). Crash entre (1) e (2) = re-inserção idempotente (duplicatas
toleradas pelo consumidor GIN). Redo da meta é **incondicional, sem olhar LSN** ("full-page image ... to
avoid torn page hazards", `ginxlog.c:536-546`).
**Não copiar:** perseguição de cauda (:843-888), reprocessamento sob exclusivo (:943-961), prova de
concorrência bespoke no replay (`ginxlog.c:693-706`) — nosso fold-por-rebuild evita tudo isso por
construção (idempotência por descarte da pending inteira).

- Evidência: `.claude/knowledge-base/references/postgres/src/backend/access/gin/ginfast.c:39,448-471,553-671,766-1025`;
  `.claude/knowledge-base/references/postgres/src/backend/access/gin/ginvacuum.c:584-595,698-722`;
  `.claude/knowledge-base/references/postgres/src/backend/access/gin/ginxlog.c:528-546,675-723`.

### Q4 — Primitivo de metapage-pivot atômico (a âncora do #47)

**Não existe precedente de shadow-generation de índice inteiro no core** (hipótese honesta confirmada —
EC-1). O primitivo que EXISTE: **meta + página(s) nova(s) em UM registro, com a meta reconstruída no
redo a partir do payload (nunca delta)** — nbtree `_bt_newlevel` (`nbtinsert.c:2443-2609`: root novo
WILL_INIT + meta WILL_INIT no mesmo registro; redo `_bt_restore_meta` regenera a meta campo a campo,
`nbtxlog.c:81-130`) e GIN `XLOG_GIN_UPDATE_META_PAGE` (meta block 0 WILL_INIT + tail no mesmo registro,
`ginfast.c:427-437`; redo incondicional `ginxlog.c:528-617`). Garantia composta: registro atômico no
WAL + meta regenerada por inteiro ⇒ **o pivot lógico muda atomicamente**.
**Órfãs/reciclagem:** deleção é WAL-logged, FSM NÃO é — FSM é cache advisory
(`RecordFreeIndexPage`/`IndexFreeSpaceMapVacuum`, `ginfast.c:667-668,1014-1020`; nbtree
`nbtpage.c:2994-3055`); consumidor re-verifica (`_bt_allocbuf` conditional lock + re-check
`nbtpage.c:868-988`); página perdida em crash = perdida até próximo vacuum ("no big problem").
**Reciclagem adiada por leitores:** nbtree usa safexid (`nbtpage.c:3023-3052`) — **não temos**; nosso
fold lock EXCLUSIVE (`am/lock.rs`) cumpre o papel (nenhum scan concorrente durante pivot+reclaim).

- Evidência: `.claude/knowledge-base/references/postgres/src/backend/access/nbtree/nbtinsert.c:2426-2609`;
  `.claude/knowledge-base/references/postgres/src/backend/access/nbtree/nbtpage.c:231-312,859-988,2941-3110`;
  `.claude/knowledge-base/references/postgres/src/backend/access/nbtree/nbtxlog.c:81-130,937-996`.

### Q5 — pgvector in-place vacuum: o contraste que valida o rebuild-fold (e alimenta o M55)

4 passes (`hnswvacuum.c`): RemoveHeapTids (:36-165, GenericXLog por página) → RepairGraph (:370-490,
re-busca vizinhos por elemento afetado + barreiras UPDATE_LOCK) → ConfirmRepaired (:495-573, pode
`elog(ERROR "hnsw graph not repaired")` :562) → MarkDeleted (:578-713, tombstone + version 4-bit +
`LockBufferForCleanup` :617). Nunca lock de relação inteira; consistência entre-páginas por ordem de
fases + tolerância dos leitores a tombstones. Custo: 4 varreduras, re-busca grau-ef_construction por
elemento, máquina tombstone/version vazando para scan/insert, assert de invariante em runtime.
Teste deles prova reuso: tamanho ≤1.02× pós delete-all+vacuum+reinsert (`test/t/011_hnsw_vacuum.pl:30-43`).
**Contraste com nosso rebuild-fold:** disponibilidade contínua vs janela de rebuild; zero-custo-por-delete
vs O(N) sempre; 5 mecanismos de complexidade a menos no nosso. **Anti-creep EC-4: o desenho in-place
para o theodb é M55 — este blueprint para aqui.**

- Evidência: `.claude/knowledge-base/references/pgvector/src/hnswvacuum.c:36-781`;
  `.claude/knowledge-base/references/pgvector/test/t/011_hnsw_vacuum.pl:30-43`.

---

## Cross-cutting Comparison

Como cada fonte resolve cada problema do M48 — e o que o theodb adota:

| Problema | postgres core | pgvector | pgvectorscale | theodb (decisão) |
|---|---|---|---|---|
| INIT fork UNLOGGED no WAL | gist: `log_newpage_buffer` incondicional (`gist.c:133-150`); btree: `use_wal \|\| INIT_FORKNUM` (`bulk_write.c:90-92`); bloom: GenericXLog (frágil — anti-pattern) | `log_newpage_range` com `RelationNeedsWAL \|\| INIT_FORKNUM` (`hnswbuild.c:1137`) | n/a (não lido p/ este eixo) | pgvector/gist (D1) |
| Atomicidade de mudança estrutural | meta+páginas novas em 1 registro, meta regenerada no redo (nbtree `nbtxlog.c:81-130`; GIN `ginxlog.c:528-617`); ordem dados-antes-pivot-depois (GIN `ginfast.c:766-772`) | mutação por página GenericXLog; sem rewrite global (in-place) | mutação por página; sem FSM | composição GIN-order + meta-full-record via GenericXLog FULL_IMAGE, pivot só-meta ≤4 págs (D2) |
| Pending/deferred maintenance | GIN: fold no insert acima do threshold + vacuum (`ginfast.c:458-471`) | n/a (sem pending region) | n/a | fold no `amvacuumcleanup` acima de threshold — divergência consciente: nosso fold é O(N) (D3) |
| Cancelabilidade | `CHECK_FOR_INTERRUPTS` idiomático em loops longos | idem (C) | `check_for_interrupts!` 1×/tupla (`build.rs:1078`); `vacuum_delay_point`/página (`vacuum.rs:94`) | pgvectorscale (D4) |
| Cost model | `genericcostestimate` (`selfuncs`) | generic + ratio por AM (`hnsw.c:197-232`, `ivfflat.c:122-150`) | generic + 1% chute, 51 linhas (`cost_estimate.rs`) | template pgvectorscale + ratios pgvector (D5) |
| Teste de crash | TAP recovery (`013_crash_restart.pl`) — o único que testa de verdade | zero (réplica como proxy) | zero (funcional via client externo) | pytest+docker derivado do TAP + GUC de crash-injection (D6) — além da paridade |

## ADRs (uma recomendação por DoD do M48)

### D1 — Fix #46: `log_newpage_range` incondicional no INIT fork (padrão pgvector/gist; NÃO bloom)

**Decisão:** buildempty continua via bufmgr; adicionar plumbing de `forkNum` em `am/page.rs` e fechar
com `pg_sys::log_newpage_range(rel, INIT_FORKNUM, 0, RelationGetNumberOfBlocksInFork(...), true)`
(binding :37137 verificado). **Alternativas rejeitadas:** manter GenericXLog (é o bug — Q1.3); padrão
bloom (durabilidade só por checkpoint — frágil, Q1.3); `smgrimmedsync` (pgvector moderno não usa — Q2).
**Regra do projeto:** Regra 9 (padrão upstream literal, não reinventar).

### D2 — Fix #47: meta-pivot atômico = composição GIN-order + nbtree-meta-full-record

**Decisão:** (1) geração nova escrita em páginas FRESCAS, cada página em registro GenericXLog próprio
(inofensiva enquanto a meta não aponta — ordem GIN Q3); (2) pivot em UM registro tocando SÓ a metapage
com `GENERIC_XLOG_FULL_IMAGE` (≤4 páginas/registro — Q1.1; meta-full-record — Q4); (3) reclaim
pós-pivot: marcar velhas deletadas (WAL-logged) → `RecordFreeIndexPage` + `IndexFreeSpaceMapVacuum`
(advisory; perda em crash = aceitável — Q4); (4) leitores protegidos pelo fold lock EXCLUSIVE existente
(substitui safexid — Q4); (5) meta ganha campo de geração/head ⇒ **format version bump + REINDEX story**
(precedente v1→v2 nosso). O mecanismo é **layout-agnóstico** (restrição anti-retrabalho M51 do ROADMAP).
**Alternativas rejeitadas:** in-place à la pgvector (M55 — complexidade de repair/tombstone, Q5);
"meta+N páginas num registro" (estoura o cap de 4 — Q1.1); confiar em FSM como verdade (é cache — Q4).

### D3 — Pending fold com threshold no `amvacuumcleanup` (NÃO no insert path)

**Decisão:** fold quando `pending_pages > threshold` (GUC, default a medir) em `amvacuumcleanup` —
mesmo quando `ambulkdelete` não rodou (workload insert-only). Insert path permanece O(1) append.
**Divergência consciente do GIN** (que folda no insert, Q3): o fold do GIN é incremental/barato; o nosso
é rebuild O(N) — no insert path seria um cliff de latência imprevisível para o usuário. **Anti-pattern
GIN não importado:** perseguição de cauda, reprocessamento sob exclusivo (Q3).

### D4 — Cancelabilidade: `pgrx::check_for_interrupts!` por batch + `vacuum_delay_point` por página

**Decisão:** leader do build paralelo chama `check_for_interrupts!` entre batches (precedente
pgvectorscale `build.rs:1078,1122` — Q8); caminhos de vacuum chamam `vacuum_delay_point` por página
(precedente `vacuum.rs:94-101`). Só em código sob `#[pg_guard]` (longjmp-safety — Q8).

### D5 — `amcostestimate` real: template pgvectorscale (51 linhas) + fórmulas pgvector

**Decisão:** null-check `indexorderbys` → `f64::MAX`; `GenericCosts` + `pg_sys::genericcostestimate`
(bindings :50132/:33131 — Q8); `ratio` específico por AM (IVF: `probes/lists` + 50% seq; HNSW: fórmula
pgvector Q10); `indexStartupCost = indexTotalCost · ratio`. Nota de teste (ROADMAP M48): custo honesto ⇒
seqscan vence em N pequeno é o resultado CORRETO — asserts de pushdown migram para N realista.
**Regra:** Regra 9 — nunca reimplementar seletividade.

### D6 — Tooling de crash-test: GUC `theodb.test_crash_after_pages` (SUSET, default 0) + `abort()`

**Decisão:** GUC sempre compilado (testar o binário SHIPPED — "tests passing ≠ system works"),
`std::process::abort()` no ponto do fold (SIGABRT = crash real de backend; `proc_exit` é limpo demais);
+ 1 teste coarse `docker kill -s KILL`+`docker start` (power-loss). Receita pytest derivada do TAP 013
(Q6): durabilidade seletiva + writability pós-restart. **Rejeitados:** injection_points (indisponível no
Debian PG17 — verificado, Q9), feature-flag de build, timing, gdb.

---

## Recommendations — por DoD do M48 (mapa direto para o plano)

| DoD do M48 | ADR | Padrão âncora (file:line) |
|---|---|---|
| #46 INIT fork | D1 | pgvector `hnswbuild.c:1137-1138`; gist `gist.c:133-150` |
| #47 meta-pivot | D2 | GIN `ginfast.c:766-772` (ordem) + nbtree `nbtxlog.c:81-130` (meta-full) + cap 4 (`generic_xlog.c:326`) |
| Pending fold threshold | D3 | GIN `ginfast.c:458-471` (trigger; divergência consciente p/ vacuumcleanup) |
| Build cancelável | D4 | pgvectorscale `build.rs:1078` |
| Costestimate honesto | D5 | pgvectorscale `cost_estimate.rs:1-51` + pgvector `hnsw.c:197-232`/`ivfflat.c:122-150` |
| (teste de tudo) | D6 | TAP `013_crash_restart.pl` transplantado p/ pytest+docker |

## Blocked questions

(nenhuma — 10/10 respondidas)

## Limites honestos declarados

- Assimetria bloom-unlogged (Q1.3): não confirmada como bug do bloom neste tree — irrelevante desde que
  copiemos gist/btree.
- `log_newpage_range`/FSM: primeiro uso Rust será o nosso (sem precedente no corpus pgrx) — o precedente
  semântico é C; validar assinaturas contra o header C na fase de implement.
- Fórmula HNSW de costestimate usa constantes empíricas do pgvector (0.55…) — adotamos como está
  (paridade), tuning é fora de escopo do M48.
- WAL volume do shadow-rewrite: medido só na fase de benchmark do M48 (insumo M55) — nenhum número
  citado aqui (UNBENCHMARKED até lá).
