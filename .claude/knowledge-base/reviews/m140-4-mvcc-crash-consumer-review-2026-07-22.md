# Review — M140.4 (MVCC/VACUUM/crash + consumidor theo-lens) — 2026-07-22

**Verdict:** READY_TO_MERGE

Auditoria adversarial `council-rust-pgrx` (a lente de robustez/MVCC/pgrx). Veredito inicial: **NEEDS_FIXES — 1
HIGH (claim D3 falso) + 1 MEDIUM (defeito de produção) + 2 LOW (gates frouxos).** **Todos corrigidos e
re-validados no binário shipado.** O achado HIGH foi um erro real e importante que o review pegou antes do merge.

## Hard gates (cycle-review.md) — todos ✅

branch=develop · sem `Co-Authored-By` · sem secrets · CHANGELOG atualizado · núcleo pgrx-free (13 testes, zero-pgrx).

## Findings e disposição

| Sev | Finding | Disposição |
|---|---|---|
| — | BLOCKER: nenhum (feature pré-cutover; o dano do HIGH era latente, não UB/corrupção) | — |
| **HIGH** | O claim D3 ("`Spi::get_one` e `load` são read-only → mesmo snapshot → straddle fechado") era **FALSO**. Em pgrx 0.19 `Spi::get_one`/`get_one_with_args` = `connect_mut`/`update` → `mark_mutable` → `read_only=false` → abre snapshot fresco por statement (o straddle **continuava aberto**: um leitor RR fixo em gen G pegaria cache-hit servindo conteúdo G+1). O eixo MVCC RC não exercitava isso → false-pass. | **CORRIGIDO** — `read_generation` reescrito para `Spi::connect(\|c\| c.select(...))` (read-only genuíno, sem `mark_mutable`). Agora `read_generation` e `load` reusam o ActiveSnapshot da statement → tag==conteúdo sob RC e RR. Re-validado: 0 erros, ROBUSTNESS_OK, smoke 9/9 sem regressão |
| **MEDIUM** | Mesma raiz: `bm25_search` marcava a txn mutável → **quebra em read replica** ("cannot assign TransactionId during recovery") + queima um XID por busca (um caminho de LEITURA) | **CORRIGIDO** pelo mesmo fix (`c.select` read-only não marca mutável → roda em replica sem burn de XID) |
| MEDIUM (M2) | O crash gate só checava `BEFORE==AFTER==2` — um restart LIMPO satisfaria (não prova "sobreviveu a um crash via recovery") | **CORRIGIDO** — o gate agora exige `DOWN` (o crash pegou) E `REPLAYED` (recovery no restart), não só data-presente; o claim "via WAL replay" virou "durável através de crash+recovery" (honesto) |
| LOW (L1) | O VACUUM gate `DEAD_AFTER<=DEAD_BEFORE` passava em igualdade (um VACUUM no-op passaria) | **CORRIGIDO** — exige `DEAD_BEFORE>0 && DEAD_AFTER<DEAD_BEFORE` (recuperação real) |
| LOW (L2) | O probe assertava `thread_count>=1` (trivial) com `threads=1` — não demonstrava o hazard multi-thread | **CORRIGIDO** — build multi-thread (4 threads, heap escala) assere `thread_count>1`. Nota honesta mantida: a GARANTIA estrutural é o gate zero-pgrx do CI (`lint-rust.yml`), o probe demonstra o hazard |
| INFO | MVCC RC axis é a parte mais sólida (a asserção positiva RC=1 exige o cache invalidar na geração nova); consumidor honesto (boundary proof-agora/cutover-M141 explícito, sem overclaim) | aprovados |

## Validação (binário shipado, e2e-runner pgrx 0.19 + PG18) — pós-fix

| Gate | Resultado |
|---|---|
| `cargo check --features "pg18 spike-lexical"` | ✅ 0 erros (o fix read-only compila) |
| `m140-4-lexical-robustness.sh` (gates endurecidos) | ✅ **CRASH_OK** (crash-pegou+recovery gateados) + **VACUUM_OK** (24→0, recuperação real) + **MVCC RR/RC** |
| `m140-3-bm25-smoke.sh` | ✅ 9/9 (sem regressão do fix read-only) |
| `m140-4-consumer-theolens.sh` | ✅ CONSUMER_OK |
| `cargo test -p theodb_lexical probe` | ✅ probe multi-thread (>1 thread) + zero-pgrx |
| clippy `-D warnings` (baseline M136) | ✅ Finished (0 warnings) |
| theo-lens `trace-bm25-search.test.ts` | ✅ 4/4 |

## DoD do milestone (ROADMAP M140.4) — verificação

| # | Item DoD | Estado | Evidência |
|---|---|---|---|
| 1 | MVCC/VACUUM/crash provados pelas suítes contra o binário shipado | ✅ | `robustness-evidence.txt` (CRASH_OK+VACUUM_OK+MVCC RR/RC, gates honestos pós-review) |
| 2 | Disciplina #153: probe no CI + nenhum toque em pg_sys/SPI no caminho das threads + panic=unwind gateado | ✅ | probe multi-thread (>1 thread) + gate zero-pgrx (`lint-rust.yml`) + panic=unwind (M140.2) |
| 3 | theo-lens consome BM25 (migra de ts_rank) — primeiro consumidor com evidência; alimenta M141 | ✅ | CONSUMER_OK (shape real do theo-lens) + wiring `trace-bm25-search.ts` testada (4/4); cutover=M141 |
| 4 | Hardening do M140.3 LOW (straddle SPI) | ✅ | fechado de verdade (`c.select` read-only, o M140.4 review corrigiu o mecanismo — não `get_one`) |

## Conclusão

Merge-ready **após corrigir o HIGH**. O valor da auditoria adversarial ficou evidente: o claim D3 (a base do
"fecha o straddle") repousava numa premissa FALSA sobre a semântica de SPI do pgrx 0.19 — um leitor RR poderia ser
servido conteúdo de uma geração que seu snapshot não deveria ver, E `bm25_search` não rodaria em read replica. O
fix (`c.select` read-only) fecha os dois de verdade, re-validado no binário shipado. Os gates de crash/VACUUM/probe
viraram provas honestas (gateiam o sinal, não só o resultado). **Gate M140.4 PASSA → FECHA O M140** (M140.1→M140.4
todos released). O cutover de produção do theo-lens + os 30 dias são o **M141** (dogfood `running`).
