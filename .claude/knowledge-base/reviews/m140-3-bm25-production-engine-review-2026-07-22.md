# Review — M140.3 (engine BM25 de produção own-code) — 2026-07-22

**Verdict:** READY_TO_MERGE

Duas auditorias adversariais em paralelo (as duas superfícies de risco do milestone):
`council-rust-pgrx` (cache/MVCC/thread-safety) + `council-benchmark` (latência/nDCG). Nenhum BLOCKER de
integridade; o único HIGH (poison do `Mutex`) foi corrigido e re-validado; os caveats de rigor de prosa foram
aplicados ao report.

## Validação com toolchain real (e2e-runner, pgrx 0.19.0 + PG18)

| Gate | Resultado |
|---|---|
| Núcleo `cargo test -p theodb_lexical` (cache incl., stock) | ✅ 11 verdes; zero-pgrx |
| Smoke SQL `scripts/m140-3-bm25-smoke.sh` (build/search/geração/vazio/rebuild + **MVCC 2-sessões**) | ✅ 9/9 OK |
| `cargo check --features "pg18 spike-lexical"` + default | ✅ 0 erros |
| clippy `-D warnings` (baseline M136) | ✅ RC=0 |
| Latência cache-vs-reload + nDCG in-PG | ✅ medido (`docs/benchmarks/m140-3-bm25-engine.md`) |

## Hard gates (cycle-review.md) — todos ✅

branch=develop · sem `Co-Authored-By` · sem secrets · CHANGELOG atualizado · núcleo pgrx-free (sem regressão M140.2).

## council-rust-pgrx — cache/MVCC/thread-safety — NEEDS_FIXES → corrigido

Veredito: o núcleo MVCC está **correto** (isolamento por-backend fecha o buraco cross-backend; nunca serve um
build mais novo a um snapshot mais antigo); thread-safety #153 **preservada**; `quote_ident` **safe** contra
injeção; sem reentrância/deadlock.

| Sev | Finding | Disposição |
|---|---|---|
| HIGH | `bm25_search` (engine.rs:157) tomava o `Mutex` guard através do closure de build; um panic (heap corrompido/SPI) envenenaria o `CACHE` static por-backend → **toda busca futura da sessão falharia** (num pool, 1 erro transitório envenena o backend) | **CORRIGIDO** — `CACHE.lock().unwrap_or_else(\|e\| e.into_inner())`; o `HashMap` fica intacto num panic pré-insert (`get_or_build` avalia `build()` antes do `insert`), recuperar é seguro. Re-validado (smoke 9/9) |
| LOW | `.expect()` genérico nos caminhos que consomem bytes do heap (`open_from_heap`, `searcher.search`, `searcher.doc`) | **CORRIGIDO** — `error!` typed com contexto de `index_id` (`error-handling.md`) |
| LOW | `read_generation` e `load` são 2 statements SPI separados → sob READ COMMITTED (se os selects forem read_only=false) a tag do cache pode straddlear o heap | **RASTREADO p/ M140.4** — sob REPEATABLE READ é airtight (provado pelo smoke); sob RC **auto-cura** (a linha do tempo do backend é linear; nunca serve errado a um leitor legítimo `gen=N`). O M140.4 (prova MVCC/VACUUM/crash a fundo) é o lugar certo p/ co-localizar as 2 leituras num snapshot e verificar o flag `read_only` do pgrx |
| INFO | thread-safety #153, poison não-disparável por worker, `bm25_build` atômico (DELETE+flush+bump na mesma txn), geração-decrescente reachable via rollback e bem tratada | aprovados |

## council-benchmark — latência/nDCG — HONEST-DEFENSIBLE → caveats aplicados

Veredito: **sem fabricação, sem cherry-pick material, sem spin que inverta a conclusão.** Exoneração-chave: o
nDCG `0,6611` **NÃO é número copiado** — o revisor re-executou o eixo in-PG do zero (fresh initdb → CREATE
EXTENSION → bm25_build reindexando scifact → 300 queries) e obteve `0,6611434636029909` nos 16 dígitos: é
determinismo correto (D3, ranking independe do storage), a evidência mais forte do artefato.

| Sev | Finding | Disposição |
|---|---|---|
| MEDIUM | cache-vs-reload compara 2 motores distintos (produção id+body vs spike body-only), não o mesmo motor cache-on/off | **CORRIGIDO no report** — declarado como atribuição **mecanística** (o reload é a única diferença de custo estrutural); os confounders são **conservadores** (o cache faz MAIS trabalho e ganha) |
| MEDIUM | "linear"/"~flat" imprecisos — o scaling é sub-linear (3 pontos, sem curve-fit) | **CORRIGIDO** — descrição fiel ("reload cresce mais rápido que a busca cacheada: 0,62→0,36→0,22") |
| MEDIUM | k=30 só mean; ratio de 2k oscila 0,55–0,66 na fronteira do gate; a query 'lazy' casa 100% do corpus (selectividade máxima) não-declarada | **CORRIGIDO** — variância 0,55–0,66 e selectividade-máxima de 'lazy' declaradas; `~5k` marcado como estimativa |
| LOW | "paridade" era eufemismo p/ déficit sistemático de ~4% vs pg_textsearch (não significance-tested) | **CORRIGIDO** — "~4% ABAIXO do pg_textsearch (não significance-tested), NÃO superioridade" em todo o report |
| LOW | micro-inconsistência entre os 2 JSON (2k ratio 0,62 k=30 vs 0,66 k=50) | sem consequência material (ambos >0,5 → gate não atingido em 2k sob os dois); registrado |

## DoD do milestone (ROADMAP M140.3) — verificação

| # | Item DoD | Estado | Evidência |
|---|---|---|---|
| 1 | Superfície BM25 own-code com cache — latência não paga reload-por-query | ✅ | `bm25_build`/`bm25_search` + `IndexCache`; medido (ratio 0,22 @ 50k, gate <50% em N≥~5k) |
| 2 | Bate a baseline do M138 em nDCG@10 | ✅ (honesto) | nDCG in-PG 0,6611 (reproduz M140.1); **bate ts_rank** (0,072, o baseline shipado); ~4% abaixo do pg_textsearch (declarado, não superioridade) |
| 3 | ADR-2 supersede a exceção do ADR-0013 | ✅ | ADR-0054 (own-code é a superfície BM25; plano de saída do pg_textsearch) |
| 4 | MVCC-correto | ✅ | smoke 2-sessões (A snapshot antigo NÃO vê build de B) + IndexCache unit test; council-rust-pgrx aprovou o core |

## Nota de infraestrutura (honestidade)

O `cargo pgrx test` **não linka** neste ambiente (o problema conhecido do M139 — undefined `PG_exception_stack`).
Por isso a camada pgrx é validada via **extensão instalada + SQL** (`cargo pgrx install` + `m140-3-bm25-smoke.sh`),
o mesmo padrão do CI `cassert-sql-safety` e do M139 crash-smoke — no droplet e2e-runner (pgrx 0.19+PG18 real). O
núcleo pgrx-free (`IndexCache`) roda stock. Os benchmarks foram medidos e **reproduzidos independentemente** pelo
council-benchmark no mesmo box.

## Conclusão

Merge-ready. A engine BM25 de produção own-code opera com cache MVCC-correto (o crux provado por duas sessões e
aprovado pela auditoria pgrx), mata o reload-por-query no regime realista (medido + reproduzido), e a qualidade
in-PG reproduz o M140.1 (determinismo verificado do zero). O HIGH de disponibilidade (poison) foi corrigido; os
caveats de rigor viraram prosa honesta. **Gate M140.3 PASSA → M140.4** (que também herda o LOW de co-localização
SPI a endurecer). Own-code permissivo + cache + índice menor + moat — não superioridade de ranking.
