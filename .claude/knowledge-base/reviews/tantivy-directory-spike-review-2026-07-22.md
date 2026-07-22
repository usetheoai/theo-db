# Review — M139 (spike: Directory do Tantivy sobre PG) — 2026-07-22

**Verdict:** READY_TO_MERGE. Spike com veredito **GO**, todos os 4 gates medidos em PG18 real; auditoria pgrx
confirma que o claim central de thread-safety não tem furo.

## DoD — verificação item a item (cada gate MEDIDO)

| # | Item DoD (ROADMAP M139) | Estado | Evidência |
|---|---|---|---|
| 1 | Protótipo indexa N docs + busca, sem filesystem | ✅ | `pg_directory.rs` — 5/5 testes standalone; `roundtrip('lazy')=1` em PG real |
| 2 | MVCC: snapshot anterior não vê txn não-commitada; após commit, vê | ✅ | cross-session em PG18: B=0 durante txn A aberta, B=1 após COMMIT (`GATE2_MVCC_OK`) |
| 3 | Crash real: SIGABRT + replay do WAL, índice consistente | ✅ | `scripts/m139-lexical-crash-smoke.sh`: `search=1` antes/depois, "WAL replay ocorreu" (`GATE3_CRASH_OK`) |
| 4 | Medição de custo vs pg_textsearch no mesmo corpus | ✅ | head-to-head 2000 docs: índice 68K vs 192K (2,8× menor), busca 33 vs 40 ms |
| 5 | ADR com veredito GO/NO-GO | ✅ | ADR 0051 → Aceito, **GO** |
| 6 | Fork? | ✅ | **NÃO** — Tantivy MIT stock + Directory custom resolveu (ADR-2, anti-sunk-cost) |

## Revisor

- `council-rust-pgrx` (a lente crítica para este código): audita se a arquitetura buffer-then-flush é segura.

## Veredito do revisor

**Sem furo no claim de thread-safety; o GO se sustenta.** O único `SegmentStore` é o `MemStore` (memória pura),
então o caminho que o Tantivy executa em 4 threads é comprovadamente PG-free (SPI/`pg_sys` só nas funções
`#[pg_extern]` de main-thread). `panic="unwind"` (ambos os perfis) é pré-requisito de segurança — um panic de
worker desenrola e vira `Err` de `commit()`, tratado na main thread; não há longjmp cross-thread nem abort.
Zero `unsafe`, sem injeção (path é bind param), `commit()`+`Drop` juntam as threads antes do flush.

## Findings / disposição

| Sev | Finding | Disposição |
|---|---|---|
| — | Nenhum BLOCKER/HIGH | — |
| M140 | `panic="unwind"` deve permanecer (abort → crash de worker panic) | rastreado em **#153** |
| M140 | o futuro `pg_page_store.rs` não pode tocar PG no caminho das threads; probe→CI | rastreado em **#153** |
| M140 | consistência flush-sob-merge em escala | rastreado em **#153** |

## Fronteira honesta

É um **spike** atrás da feature `spike-lexical` (não entra no build shipado; o default e os gates de CI não o
compilam). Naive per-query reload, single-threaded, sem merge/VACUUM — declaradamente fora do escopo (a produção
é o M140). O spike responde a pergunta de viabilidade (GO) e descobre a arquitetura correta (buffer-then-flush,
forçada pelo probe de threads) — em uma sessão, medido, em vez de trimestres.

## Conclusão

Merge-ready. A pergunta central do M139 ("o Tantivy pode viver no PG com MVCC+WAL+crash?") está respondida
**SIM**, com evidência medida em todos os gates e a arquitetura de segurança validada. M140 (produção) rastreado
em #153.
