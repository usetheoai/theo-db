# Code Review — theodb_rs (loop-code-review, modo full)

**Alvo:** `/home/paulo/Projetos/usetheo/theo-data/theo-db/theodb_rs` — extensão PostgreSQL 18 em Rust/pgrx, 90 arquivos, ~38,7k LOC (78 Rust / 11 shell / 1 outro)
**Data:** 2026-07-23 · **Fonte de verdade:** `code-review-output/code-review.db` (todos os números abaixo vêm de queries SQL; nenhum é estimado)
**Fases executadas:** 1 inventário → 2 completude → 3 análise profunda → 4 auditoria de testes → 5 relatório · **Gates:** fase 2 = 0.92, fase 3 = 0.87, fase 4 = 0.92 (todos `passed`, tabela `quality_gates`)
**Cobertura:** 90/90 arquivos inspecionados (100%); 0 amostrados, 0 excluídos, 0 pendentes.

---

## 1. Sumário executivo

Este é um codebase **de nível sênior**, e o resultado da revisão reflete isso: **0 findings CRITICAL** em 100 findings (18 HIGH, 35 MEDIUM, 44 LOW, 3 INFO). Pontos fortes verificados no código, não presumidos: o pilar http/egress é endurecido (bloqueio de endereços privados com CC alto porque a lista de casos é essencial — `src/egress.rs:23`), o vectorizer tem a **suíte negativa mais forte do repositório** (22 pg_tests com lease/fencing/dead-letter — `test_audit`), os stubs de erro tipado são deliberados e documentados, e os "espelhos" (build_arrow ↔ arrow_value_to_datum, AggSpec) foram verificados como corretos e explicitamente marcados como *não-abstrair* (finding #75, `src/am/df_executor.rs:198`, INFO). A fase 2 registrou `no_completeness_issues` para a resolução da superfície SQL (finding #8).

Os 3 maiores riscos, em ordem:

1. **Cadeia de upgrade congelada em 1.1.0** — a superfície lakehouse do M143 (`public.read_parquet`/`write_parquet`/`olap`) é inalcançável via `ALTER EXTENSION theodb_rs UPDATE`, contrariando a promessa do README (finding #1, HIGH, `sql/theodb_rs--1.0.0--1.1.0.sql:1`). Qualquer usuário que fizer upgrade in-place é atingido.
2. **`symqg_spike_bench` executável por PUBLIC** — função `#[pg_extern]` de benchmark de spike, compilada sem cfg-gate e criada no SQL shipado (`sql/theodb_rs--1.0.0--1.1.0.sql:340`) **sem** `REVOKE FROM PUBLIC`, permitindo leitura de path arbitrário do filesystem por não-superuser (finding #2, HIGH, `src/bench_symqg.rs:48`) — ao contrário de toda outra função de I/O do codebase, que tem REVOKE.
3. **Delete engolido no vectorizer (PII)** — os dois braços de `_vectorizer_process_delete` descartam o `Result` do SPI (`let _ =`); um DELETE/UPDATE que falha é marcado `done` e o embedding do dado apagado **permanece pesquisável** (finding #76, HIGH, `src/vectorizer.rs:460`).

Escopo de LOC (~38,7k, codebase inteiro) está muito acima do teto FAANG de ~400 LOC por review — este loop é um audit de codebase, não um review de PR; a granularidade de acionamento está no plano de remediação (§6).

---

## 2. Top findings por severidade e categoria

19 findings são **blocking** (coluna `blocking=1`); 81 são não-blocking (`[Nit:]`). Fontes de severidade: 61 `default` (limiares lizard/radon), 38 `heuristic`, 1 `consensus`.

### 2.1 HIGH — correção/segurança/completude (3, todos blocking)

| # | Finding | Arquivo:linha | Categoria | Fonte |
|---|---|---|---|---|
| 1 | Cadeia de upgrade parada em 1.1.0 (M137, commit 1505a34); 16 commits de src depois, incluindo a superfície parquet default-on do M143 — inalcançável via `ALTER EXTENSION UPDATE` | `sql/theodb_rs--1.0.0--1.1.0.sql:1` | completeness | heuristic |
| 2 | `symqg_spike_bench(sift_dir,...)` PUBLIC-executável: lê paths arbitrários do FS; único `#[pg_extern]` de I/O sem REVOKE (criado em `sql:340`) | `src/bench_symqg.rs:48` | completeness | heuristic |
| 76 | `let _ = Spi::run_with_args(...)` nos dois braços de delete (linhas 460 e 469) — falha de delete vira job `done`; embedding de linha apagada persiste (impacto PII) | `src/vectorizer.rs:460` | error_handling | heuristic |

### 2.2 HIGH — complexidade (15, medidos por lizard; fonte `default`)

Os piores, admitidos sem eufemismo:

| Função | Arquivo:linha | CC | NLOC |
|---|---|---|---|
| `admit` | `src/am/columnar_agg.rs:250` | **59** | 170 |
| `theodb_embed_worker_main` | `src/vectorizer.rs:797` | **41** | 217 |
| `write_parquet_impl` | `src/parquet.rs:174` | **35** | 117 |
| `main_index_pages` | `src/am/page/mod.rs:562` | 34 | 131 |
| `gather_symqg_candidates` | `src/am/scan.rs:334` | 31 | 139 |
| `directory_minmax` | `src/am/columnar.rs:868` | 30 | 92 |
| `decode_columns` | `src/am/columnar.rs:707` | 29 | 103 |
| `traverse` | `src/am/hnsw_page/search.rs:175` | 29 | 129 |
| `scan_ivf_aq_split_v7` | `src/am/scan.rs:847` | 27 | 122 |
| `begin_custom_scan` | `src/am/columnar_agg.rs:885` | 26 | 153 |
| `is_blocked_addr` | `src/egress.rs:23` | 26 | 31 |
| `ambuild` | `src/am/build.rs:132` | 21 | 195 |
| `init` (GUC, long_function) | `src/am/guc.rs:270` | CC=3 | 238 |
| `m109_bench_crossover_sweep` (long_function) | `src/graph.rs:933` | CC=14 | 154 |
| `is_word_char` (long_function, tabela) | `src/nl.rs:247` | CC=2 | 198 |

Honestidade sobre complexidade essencial vs. acidental: ver §6 P2 — nem tudo aqui merece refactor.

### 2.3 MEDIUM — destaques não-complexidade (13 de 35)

| # | Finding | Arquivo:linha | Categoria |
|---|---|---|---|
| 80 | **[consensus, blocking]** `sanitize_error_text`: `to_lowercase()` muda comprimento (ex.: U+0130 → 2 chars) e dessincroniza o scan paralelo — credencial (`bearer …`) pode escapar da redação | `src/vectorizer.rs:742` | security |
| 89 | WRITE_STATES pendentes não purgados em DROP TABLE — flush PRE_COMMIT faz `relation_open` de OID dropado e **aborta o commit inteiro** | `src/am/columnar.rs:193` | error_handling |
| 77 | `[Nit:]`* Erro SPI no claim-batch do worker vira batch vazio silencioso | `src/vectorizer.rs:860` | error_handling |
| 79 | Retry de fila sem delay — outage transiente do endpoint vira dead-letter permanente | `src/vectorizer.rs:285` | error_handling |
| 78 | Leituras SPI caem em valores mágicos (`job_id=0`, pk vazio) em vez de falhar | `src/vectorizer.rs:238` | contract |
| 86 | `build_csr` trunca node ids para u32 sem checagem + aloca O(max_id) — grafo errado / OOM | `src/graph.rs:314` | contract |
| 91 | `write_parquet` materializa a tabela inteira na RAM do backend — o bound de work_mem do módulo não cobre o caminho de escrita | `src/parquet.rs:247` | contract |
| 85 | IndexCache por-backend sem evicção — índices Tantivy inteiros acumulam pela vida do backend | `src/lexical/engine.rs:27` | performance |
| 88 | `load_next_batch` libera `(*st).current` antes de trabalho falível — janela de dangling pointer em erro de decode | `src/am/columnar.rs:1009` | code |
| 70 | `nearest_centroid` byte-idêntico duplicado (lógica de negócio, viola DRY) | `src/pq.rs:90` + `src/vec/aq.rs:234` | code |
| 3 | README hero afirma BM25 híbrido "provado no binário shipado" — a imagem default não shipa nenhum caminho BM25 | `src/hybrid.rs:157` | completeness |
| 95 | Gap de teste: nenhum teste negativo para o delete engolido (par do #76) | `src/vectorizer.rs:460` | test |
| 96 | Gap de teste: INSERT columnar pendente + DROP TABLE na mesma txn (par do #89) | `src/am/columnar.rs:193` | test |

\* Convenção `[Nit:]` = `blocking=0`; os demais MEDIUM acima também são `blocking=0` exceto o #80. Mais 22 MEDIUM de complexidade (CC 16–24: `amrescan` scan.rs:114 CC=24, `from_bytes` ann/hnsw.rs:444 CC=24, família `read_ivf_*_meta` em page/ivf.rs, `run_rrf` hybrid.rs:92 CC=20, etc.).

### 2.4 LOW (44) e INFO (3) — resumo

44 LOW: 24 de complexidade (CC 11–15 / funções longas), e 20 pontuais — destaques: retry com `thread::sleep` sem tratamento de interrupt (`src/http.rs:108`), `HeldInterrupts` segurado por todo o `block_on` incluindo I/O de arquivo (`src/parquet.rs:69`), worker hardcoded no DB `postgres` (`src/vectorizer.rs:662`), `bm25_search` engole erro de parse como resultado vazio (`src/lexical/engine.rs:190`), `write_parquet` sem fsync no commit temp+rename (`src/parquet.rs:293`), `assign_callback` pula linhas com dimensão errada sem sinal (`src/am/build_stream.rs:192`), caches sem evicção (`src/graph.rs:23`), `#![allow(dead_code)]` blanket obsoleto (`src/vec/aq.rs:18`), harness de isolation apontando pgrx 17.10 num crate PG18-only (`isolation/run.sh:6`). Os 3 INFO (abaixo do threshold `low`) ficam registrados só no DB: #7 FIXME em pg_test (`src/am/build.rs:1845`), #8 `no_completeness_issues`, #75 padrão "não-abstrair espelhos" (`src/am/df_executor.rs:198`).

---

## 3. Findings por arquivo

Query: `SELECT file, count(*), sum(severity='high'), … FROM findings GROUP BY file`.

| Arquivo | Total | H | M | L | I |
|---|---|---|---|---|---|
| `src/vectorizer.rs` | 13 | 2 | 5 | 6 | 0 |
| `src/am/scan.rs` | 10 | 2 | 7 | 1 | 0 |
| `src/am/columnar.rs` | 9 | 2 | 4 | 3 | 0 |
| `src/am/page/ivf.rs` | 8 | 0 | 4 | 4 | 0 |
| `src/am/columnar_agg.rs` | 6 | 2 | 2 | 2 | 0 |
| `src/graph.rs` | 6 | 1 | 1 | 4 | 0 |
| `src/am/build.rs` | 4 | 1 | 1 | 1 | 1 |
| `src/parquet.rs` | 4 | 1 | 1 | 2 | 0 |
| `src/am/df_executor.rs` | 4 | 0 | 1 | 2 | 1 |
| `src/lexical/engine.rs` | 4 | 0 | 1 | 3 | 0 |
| `src/bench_symqg.rs` | 2 | 1 | 0 | 1 | 0 |
| `sql/theodb_rs--1.0.0--1.1.0.sql`, `src/am/guc.rs`, `src/am/hnsw_page/search.rs`, `src/am/page/mod.rs`, `src/egress.rs`, `src/nl.rs` | 1 cada | 1 | 0 | 0 | 0 |
| `src/graph_extract.rs`, `src/hybrid.rs` | 2 cada | 0 | 2 | 0 | 0 |
| `src/am/customscan.rs` | 2 | 0 | 1 | 1 | 0 |
| `src/am/hnsw_page/store.rs`, `src/ann/hnsw.rs`, `src/pq.rs` | 1 cada | 0 | 1 | 0 | 0 |
| Demais 13 arquivos (1–2 LOW/INFO cada) | 16 | 0 | 0 | 15 | 1 |

Hotspots claros: **vectorizer.rs** (worker + PII + segurança de redação), **am/scan.rs + columnar\*** (complexidade concentrada no motor de scan/agregação) — consistente com serem os módulos de maior densidade de decisão do engine.

---

## 4. Matriz de risco

![Matriz de risco](figures/risk_matrix.svg)

`figures/risk_matrix.svg` — 53 findings ≥ MEDIUM bucketizados por severidade × probabilidade de disparo em operação (julgamento do analista, fase 5): o único HIGH de alta probabilidade é a cadeia de upgrade (#1 — todo upgrade in-place dispara); os 15 HIGH de CC são risco latente de manutenção, não defeito em runtime; os MEDIUM de alta probabilidade são os dois de crescimento de memória sem teto (#85, #91).

---

## 5. Auditoria de testes (fase 4)

**Números (tabela `test_audit`):** 52 arquivos com testes, **400 funções de teste**. Gate 0.92 passed com spot-checks exatos (hnsw_page/tests.rs 45, vectorizer.rs 22, graph.rs 13, ah_tests.rs 12).

**Veredito da pirâmide — meio-pesada, honesta sobre a restrição:**
- **Base (unit `#[test]` puros, rodam na dev box):** fina — codec (`columnar_codec.rs` 12), cost model (`cost.rs` 12), kernels SIMD (`ah_tests.rs` 12). A maior parte da lógica só é exercitada in-PG.
- **Meio (in-PG `pg_test`):** dominante e forte — 45 testes de página/WAL/VACUUM/MVCC em `hnsw_page/tests.rs` (a suíte mais profunda do repo), 22 no vectorizer com lente negativa forte (lease/fencing/dead-letter), oráculos filtered-search-igual-seqscan em `customscan.rs`.
- **Topo (e2e):** vive em harnesses fora de processo (`isolation/`, harnesses de crash no droplet) — restrição real e documentada: **`cargo pgrx test` não linka nas dev boxes** (símbolos PG; memória do projeto confirma), então a validação e2e é A/B in-PG + droplet.

**6 gaps (findings 95–100), todos amarrados a findings da fase 3:**

| # | Gap | Par fase-3 |
|---|---|---|
| 95 (M) | Sem teste negativo do delete engolido | #76 `vectorizer.rs:460` |
| 96 (M) | Sem teste de INSERT pendente + DROP TABLE mesma txn | #89 `columnar.rs:193` |
| 97 (L) | Sem teste de case-mapping Unicode que muda comprimento na redação | #80 `vectorizer.rs:742` |
| 98 (L) | Sem teste de backoff/pacing para jobs persistentemente falhando | #79 `vectorizer.rs:285` |
| 99 (L) | Sem teste de borda para os casts `as u32` do build_csr | #86 `graph.rs:314` |
| 100 (L) | Asserts com gate de relógio de parede são não-determinísticos | `graph.rs:763` (m108 bench) + `vec/ah_tests.rs` (`t_avx <= t_scalar*1.2`) |

**Flakiness:** só 2 arquivos com score > 0 — `graph.rs` (0.3) e `ah_tests.rs` (0.2), ambos por asserts de wall-clock (finding #100). Sob carga da máquina, falham sem bug.

---

## 6. Plano de remediação (priorizado, esforço S/M/L)

### P0 — corrigir antes do próximo release (correção/segurança)

| Item | Esforço | Por quê |
|---|---|---|
| #1 Gerar `theodb_rs--1.1.0--1.2.0.sql` (parquet + REVOKEs), bump `default_version`, re-rodar o oracle `schema_snapshot.sql` | M | Todo usuário que faz upgrade in-place recebe um binário M143 com catálogo M137 — a promessa do README quebra. |
| #2 Cfg-gate `symqg_spike_bench` (ou no mínimo `REVOKE ALL … FROM PUBLIC` no `extension_sql!` + upgrade script) | S | Leitura de path arbitrário do FS por não-superuser; único I/O `#[pg_extern]` sem REVOKE. |
| #76 Propagar o `Err` dos dois braços de delete (mesmo padrão do upsert em :447) | S | Falha de delete não pode virar `done` — embedding de dado apagado persistindo é risco PII/LGPD. |

### P1 — mesma milestone (integridade + os 2 gaps MEDIUM)

| Item | Esforço | Por quê |
|---|---|---|
| #89 Guardar o flush PRE_COMMIT com `try_relation_open` (+ purge em invalidation) | S | Um `DROP TABLE` legítimo aborta o commit inteiro do usuário. |
| #96 Teste do cenário acima (via `isolation/`, já que pg_test faz rollback) | S | RED antes do fix — regra de regressão (`testing.md §3`). |
| #80 Redação: comparar case por-char na janela original em vez de indexar string lowercased separada | S | Bypass de redação de credencial = vazamento em log; é o único finding `consensus`. |
| #95 Teste negativo do delete (target dropado → erro tipado ou `last_error` registrado) | S | Prova o contrato do fix #76. |
| #79 + #98 Backoff no retry de fila + teste de pacing | M | Outage transiente hoje vira dead-letter permanente — perda de dado operacional. |
| #88 Reordenar `load_next_batch` (trabalho falível antes de liberar `(*st).current`) | S | Janela de dangling pointer em unsafe é inaceitável mesmo se improvável. |

### P2 — complexidade: refatorar vs. aceitar (honesto)

**Vale refatorar (acidental/decomponível):**
- `admit` CC=59 (`columnar_agg.rs:250`) — dispatcher de admissão de agregados; decompor por AggSpec-kind é mecânico e o maior CC do repo não deveria estar num gate de correção de planos. (M)
- `theodb_embed_worker_main` CC=41 (`vectorizer.rs:797`) — loop do worker mistura claim/lease/process/report; extrair fases alinharia com o split 3-fases do M122 já existente. (M)
- `write_parquet_impl` CC=35 (`parquet.rs:174`) — junto com #91 (materialização total em RAM), uma passada de streaming resolve dois findings. (L)
- #70 Deduplicar `nearest_centroid` (pq.rs:90 = vec/aq.rs:234) — duplicação de lógica de negócio, violação DRY real. (S)

**Aceitar como complexidade essencial (documentar, não mexer):**
- `is_blocked_addr` CC=26 (`egress.rs:23`) — enumeração de ranges bloqueados; CC alto É a feature de segurança.
- `is_word_char` NLOC=198/CC=2 (`nl.rs:247`) e `guc::init` NLOC=238/CC=3 — tabelas/registro lineares; quebrar só adiciona indireção.
- Família `scan_ivf_*` (CC 16–31) e `traverse` (CC=29) — hot paths de ANN medidos por benchmark; refactor aqui arrisca regressão de QPS sem ganho de correção. Reavaliar apenas com criterion same-graph antes/depois.
- Família `read_ivf_*_meta` (page/ivf.rs, CC=16–17) — parsers de layout de página versionados; o formato dita a forma.

### P3 — LOW/cosmético (oportunista)

fsync no commit do `write_parquet` (`parquet.rs:293`, S); interrupt-aware sleep no retry http (`http.rs:108`, S); escopo do `HeldInterrupts` (`parquet.rs:69`, S); worker multi-DB ou documentar limitação `postgres`-only (`vectorizer.rs:662`, M); erro tipado no `bm25_search` parse (`engine.rs:190`, S); sinal/contagem para linhas dim-mismatch (`build_stream.rs:192`, S); remover asserts de wall-clock ou movê-los para bench-only (#100, S); limpar `#![allow(dead_code)]` blanket (aq.rs:18/ah.rs, S); PGINST 18 no `isolation/run.sh:6` (S); evicção ou teto nos caches por-backend (`engine.rs:27`, `graph.rs:23`, M).

---

## 7. O que NÃO foi revisado

- **Comportamento em runtime** — nenhum PostgreSQL vivo neste loop: nada de execução de queries, medição de QPS/recall, ou validação dos harnesses de crash/isolation em execução. Findings de concorrência/MVCC são análise estática de código.
- **Correção dos benchmarks** — a metodologia dos artefatos em `docs/benchmarks/` não foi re-auditada aqui (números citados no README foram checados só quanto a *presença* da superfície, ex.: finding #3).
- **Execução no droplet** — os harnesses e2e (droplet 165.227.121.20) não foram executados.
- **Auditoria de licenças** — loop separado (`loop-check-licence`); D1/AGPL fora deste escopo.
- **Dependências fora da árvore** — crates de terceiros (tantivy, DataFusion/Arrow, pgrx) não foram inspecionados; só o uso que o código faz deles.
- Não houve amostragem: cobertura estática foi 100% (90/90), então não há limitação de amostra a declarar.

---

## 8. Metodologia e reprodução

**Pipeline:** 5 fases com gate de qualidade por fase (avaliador `quality-evaluator`; 1 iteração cada, todas `passed` — fase 3 com 0.87 por spot-check de 4 findings de 4 agentes contra o fonte, todos exatos).

**Ferramentas e comandos:**
- Complexidade: `lizard -l rust --csv src lexical_core/src` (cwd=theodb_rs; exit 0; CSV bruto em `audit/lizard_rust.csv`). 61 findings de complexidade têm `cc_value` medido — nenhum estimado.
- Baseline: `baseline/architecture_map.md`, `baseline/component_inventory.md`; findings por agente em `findings/{code,completeness,test}/`.
- DB: `code-review-output/code-review.db`. Queries-chave:
  - `SELECT severity, count(*) FROM findings GROUP BY severity;` → high=18, medium=35, low=44, info=3
  - `SELECT severity, category, count(*) FROM findings GROUP BY severity, category;`
  - `SELECT blocking, count(*) FROM findings GROUP BY blocking;` → blocking=19, nit=81
  - `SELECT count(*), sum(inspection_status='inspected') FROM files_inventoried;` → 90, 90
  - `SELECT count(*), sum(test_count) FROM test_audit;` → 52, 400
  - `SELECT phase, score, status FROM quality_gates;` → (2, 0.92), (3, 0.87), (4, 0.92), todos passed

**Como ler os tiers de severidade (`severity_source`):**
- **consensus** (1 finding: #80) — regra inegociável (credencial escapando de redação = OWASP-classe); blocking.
- **default** (61) — limiares padrão de ferramenta (lizard CC>15/20/25, NLOC>100); é o que um CI com SonarQube/lizard bloquearia.
- **heuristic** (38) — julgamento do revisor (padrões, contratos, DRY); default para `[Nit:]` salvo impacto de correção demonstrado (ex.: #1, #2, #76 são heuristic **e** blocking por impacto concreto).

**Componentes:** 16 na tabela `components`; todos têm arquivos cobertos por findings ou por registros de verificação da fase 2 (o mapeamento finding→componente é por path de arquivo; o registro `no_completeness_issues` #8 cobre a raiz de composição).
