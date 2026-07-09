# ADR 0027 — M68: observabilidade do query vetorial via função diagnóstica (não `amexplain`)

- **Status:** Accepted
- **Date:** 2026-07-09
- **Milestone:** M68 (observabilidade do query vetorial — pilar P4 de operabilidade, ROADMAP-v3)
- **Depends on:** M67 (`theodb.scan_stats` + catálogo `theodb._index_scan_stats` + thread_local `SCAN_PAGES_READ`), M35 (catálogo em heap-page, crash-safe), M52 (iterative scan / filtered ANN)
- **Deciders:** engenharia TheoDB

## Contexto

O DoD do M68 pede: (1) `EXPLAIN` do scan vetorial mostrando índice, ef efetivo, pages-read e
candidatos vistos; (2) métricas runtime do scan (pilar (c) do wiring-triad); (3) doc de operação
para diagnosticar recall-baixo/latência-alta em produção.

O scan ANN é **opaco por natureza** — pgvector/pgvectorscale não expõem por-query quantos nós o beam
navegou nem quantas páginas leu. Um operador com "recall ruim em produção" hoje não tem instrumento:
adivinha o `ef`, tenta valores, reza. M67 já entregou `scan_stats` (pages-read real via thread_local) +
o recomendador `recommend_ef`. M68 fecha o pilar de observabilidade adicionando **candidates_seen** (o
tamanho do pool navegado no beam — o sinal que distingue "grafo caro de navegar" de "I/O pesado") e uma
superfície `EXPLAIN`-shaped.

## Decisão

### D1 — `EXPLAIN` vetorial é uma **função diagnóstica** `theodb.explain_scan(...)`, NÃO um hook `amexplain`

O PostgreSQL **não tem** um hook para um Access Method injetar linhas no `EXPLAIN` do plano
(`amexplain` não existe em PG17/PG18). A única forma de o AM contribuir para o output do `EXPLAIN` do
plano seria um planner/executor hook C — indireção pesada, frágil a cada major, e fora do contrato
`IndexAmRoutine`.

Adotamos o padrão da indústria de vector DBs: **uma função diagnóstica separada**. Qdrant expõe
`/telemetry`; Milvus expõe métricas de segmento; nós expomos `theodb.explain_scan(index_table, vector_col,
query, ef, k)` que retorna, de UM scan real: `index_name`, `ef_effective`, `pages_read`, `candidates_seen`,
`latency_us`, `results`. É portável (só SPI + o coletor thread_local já existente), honesto (não finge ser
o `EXPLAIN` do plano) e suficiente para o diagnóstico.

**Alternativa rejeitada — hook C de executor injetando no `EXPLAIN`:** custo de manutenção alto (privado do
core, muda entre majors), viola KISS, e o ganho sobre uma função diagnóstica é cosmético (aparecer dentro do
`EXPLAIN ANALYZE` vs. uma chamada separada). Registrado honestamente no doc de ops como caveat.

### D2 — `candidates_seen` é capturado no `scan_core` (own-code), não estimado

`ground_search_nodes` (o motor de busca exato/aproximado own-code em `ann/scan_core.rs`) já mantém o set
`visited` do beam. Capturamos `visited.len()` **antes do drop** e o retornamos junto com os nós
(`Result<(Vec<(Node,f64)>, usize), String>`). O call-site em `hnsw_page::traverse` propaga esse número para
o thread_local `SCAN_CANDIDATES` (irmão do `SCAN_PAGES_READ` do M67). É a **verdade do que o scan navegou**,
não um proxy.

Caveat honesto (documentado no ops-doc): no caminho **aproximado** (SBQ/AQ), `candidates_seen` reflete o pool
alargado do walk (`ef · over_fetch`), não o `ef` do resultado. É o número certo (o que o beam de fato tocou),
mas o operador precisa saber lê-lo.

### D3 — a métrica runtime (pilar (c)) é o **catálogo consultável** `theodb._index_scan_stats`, não um histograma Prometheus

O wiring-triad pede uma métrica runtime observável. Em vez de introduzir uma dependência de exporter
(Prometheus/OTel) — que seria escopo de plataforma, fora deste repo (o banco) — a métrica é a coluna
`sum_candidates` (além de `sum_pages_read`, `n_scans`, `sum_latency_us` do M67) no catálogo heap-page
`theodb._index_scan_stats`, agregada por `theodb.index_scan_stats(rel)` (expõe `avg_candidates`). O catálogo
vive em heap (fora das páginas de índice → crash-safe, precedente M35). Um exporter Prometheus por cima disto
é um passo de plataforma trivial e adiado por YAGNI (nenhum consumidor hoje).

## Consequências

**Positivas:**
- Operador ganha 3 instrumentos own-code que pgvector/pgvectorscale não têm: `explain_scan` (por-query),
  `scan_stats` (por-query + persiste), `index_scan_stats` (agregado). Diferencial real de operabilidade.
- `candidates_seen` distingue as duas causas de latência-alta que o `pages_read` sozinho não separa: grafo
  caro de navegar (candidates alto) vs. I/O pesado / spill pra disco (pages alto). O ops-doc mapeia isso.
- Zero dependência nova; zero fork; reusa o coletor thread_local do M67 e o padrão de catálogo do M35.

**Negativas / caveats (honestos):**
- Não é o `EXPLAIN` do plano — é uma função separada. Documentado como o padrão correto e portável, mas um
  operador que espera ver as linhas dentro de `EXPLAIN ANALYZE SELECT ...` não as verá.
- `ef_effective` do `explain_scan` é o ef **passado**, não o ef **crescido pelo iterative scan** (M52, que
  vive no executor real, fora do coletor diagnóstico). Para o ef iterative real, o operador olha `last_ef` em
  `index_scan_stats` após scans reais. Documentado.
- Métrica é catálogo consultável, não histograma time-series. Suficiente para v1; exporter é passo de
  plataforma adiado (YAGNI).

**Validação:** pg_tests (`explain_scan_shows_index_and_candidates`, `scan_stats_records_real_pages_read`
estendido para 4-tupla + `sum_candidates > 0`). M68 é observabilidade → validado por teste funcional
determinístico, **sem benchmark de performance** (não há claim de performance; Regra 5 não se aplica —
nenhuma afirmação "Nx mais rápido" é feita).

## Relação com o North Star

Operabilidade é pilar de produto (P4 do ROADMAP-v3), não o pilar de superioridade vetorial (P0, ainda
aberto — ver `goto-p0-vector-superiority`). M68 não avança o claim de performance; entrega o instrumento que
um operador de produção precisa e que os concorrentes OSS diretos não expõem por-query.
