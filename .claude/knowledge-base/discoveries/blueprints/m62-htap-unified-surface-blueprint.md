# M62 — Superfície HTAP unificada (OLTP + OLAP nos mesmos dados) — blueprint

**Cycle:** DISCOVER · **Milestone:** M62 · **Date:** 2026-07-09 · **Rigor:** discover-phd-rigor (R0 busca web obrigatória)
**Prior art medido:** `docs/benchmarks/m61-columnar-adoption.{md,json}` + `docs/adr/0020-m61-embed-pgduckdb.md`
**Blueprint anterior:** `.claude/knowledge-base/discoveries/blueprints/m61-columnar-htap-adoption-blueprint.md`

## Context

M62 é o pilar HTAP: "transacional (OLTP) + analítico (OLAP) nos mesmos dados, sem ETL manual" — a marca do AlloyDB.
O achado medido do M61 restringe o espaço de design de forma dura e honesta: o `pg_duckdb` embarcado **NÃO** acelera
analytics sobre o heap row-store (`force_execution` = 0.63–0.89×, honest-negative), e **VENCE ~9× a 5M apenas sobre
dados já COLUNARES** (Parquet/Iceberg). Sem MotherDuck não há columnstore DuckDB-nativo persistente. Portanto o
"HTAP unificado do TheoDB" **não pode** ser "a mesma tabela heap serve OLTP e OLAP magicamente" — tem que ser um
**fluxo honesto row-store ↔ colunar**, e este blueprint decide *qual* fluxo, com evidência web do SOTA e das peças.

## Objective

Recomendar, por ADR com alternativas, a **superfície HTAP unificada REALISTA e medível** para o TheoDB dada a
restrição D1 (só permissivo) e o achado M61; e desenhar o benchmark de carga mista que é o **gate** do M62 (este
discover NÃO mede — measurement-first: o benchmark vem no /implement).

---

## Evidência web (R0) — >= 2 fontes primárias por claim

> R0 (`.claude/rules/discover-phd-rigor.md`): busca web ativa citada. Todas as URLs abaixo foram abertas via `curl`
> (WebFetch/WebSearch indisponíveis nesta sessão; fallback curl autorizado pela tarefa). Cada claim >= 2 fontes.

### Claim A — O SOTA HTAP é "uma tabela lógica, dois stores físicos (row + column), auto-sincronizados". A taxonomia acadêmica confirma 4 arquiteturas de storage.
- **[A1] Zhang, Li, Zhang, Zhang, Feng — "HTAP Databases: A Survey", arXiv:2404.15670 (2024-04-24).** URL: `https://arxiv.org/abs/2404.15670`. Extração (abstract, verbatim): *"HTAP databases typically process the mixed workloads … in a unified system by leveraging both a row store and a column store … we classify state-of-the-art HTAP databases according to four storage architectures: (a) Primary Row Store and In-Memory Column Store; (b) Distributed Row Store and Column Store Replica; (c) Primary Row Store and Distributed In-Memory Column Store; (d) Primary Column Store and Delta Row Store. … key techniques … data organization, data synchronization, query optimization …"*. → **O row+column é o padrão universal; a diferença é onde vive o column store e como sincroniza.**
- **[A2] AlloyDB columnar engine — doc oficial Google, `https://cloud.google.com/alloydb/docs/columnar-engine/about` (aberta).** Extração (verbatim): *"column store that contains table and materialized-view data for selected columns, reorganized into a column-oriented format"*; *"auto-columnarization, which analyzes your workload and automatically adds columns"*; *"By default, the columnar engine is set to use 30% of your instance's memory"*. → AlloyDB é a arquitetura **(a) Primary Row + In-Memory Column Store**, auto-mantido.

### Claim B — AlloyDB é in-memory columnar auto-mantido; TiDB/TiFlash é replica colunar assíncrona (Raft Learner) com Snapshot Isolation. Ambos escondem a sincronização atrás do planner.
- **[B1] AlloyDB (A2 acima).** In-memory, auto-columnarization, 30% da RAM, refresh automático. **In-memory** = freshness alta mas **efêmero** (reconstruído no restart) e **acoplado à RAM da instância**.
- **[B2] TiFlash overview — doc oficial PingCAP, `https://raw.githubusercontent.com/pingcap/docs/master/tiflash/tiflash-overview.md` (aberta).** Verbatim: *"the columnar replicas are asynchronously replicated according to the Raft Learner consensus algorithm … Snapshot Isolation level of consistency is achieved by validating Raft index and MVCC"*; *"TiDB can automatically choose to use TiFlash (column-wise) or TiKV (row-wise), or use both … Currently, data cannot be written directly into TiFlash. You need to write data in TiKV and then replicate it"*. → Arquitetura **(b) Distributed Row + Column Replica**. **Trade-off explícito: replicação assíncrona ⇒ o colunar fica atrás (staleness), resolvido por validação de progresso antes de ler.**

### Claim C — O pg_duckdb (peça já embarcada, MIT) tem lakehouse-read (Parquet/Iceberg/Delta) e um caminho heap (`force_execution`), MAS o columnstore DuckDB-nativo persistente exige MotherDuck.
- **[C1] pg_duckdb README — repo oficial, `https://raw.githubusercontent.com/duckdb/pg_duckdb/main/README.md` (aberta).** Verbatim: *"Read/write* Parquet, CSV, JSON, Iceberg & Delta Lake from S3, GCS, Azure & R2"*; *"No data export required: You do not need to export your data to Parquet … works directly with your existing PostgreSQL tables"* (este é o caminho `force_execution`, medido honest-negative no M61); `iceberg_scan(...)`, `delta_scan(...)` (community extensions).
- **[C2] pg_duckdb functions.md + motherduck.md — repo oficial (ambas abertas).** `functions.md` (`https://raw.githubusercontent.com/duckdb/pg_duckdb/main/docs/functions.md`) lista **apenas scans read-only** de arquivos externos (`read_parquet`, `iceberg_scan`, `iceberg_metadata`, `iceberg_snapshots`, `delta_scan`) — **não há função de WRITE de um heap para Iceberg**. `motherduck.md` (`https://raw.githubusercontent.com/duckdb/pg_duckdb/main/docs/motherduck.md`): tabelas colunares persistentes DuckDB-nativas (`CREATE TABLE … USING duckdb` TAM) **só existem com MotherDuck habilitado**. → **Confirma M61: sem MotherDuck, o único colunar persistente é arquivo (Parquet/Iceberg) materializado por `COPY … TO (FORMAT parquet)`.**

### Claim D — pg_mooncake dá um "columnstore mirror em Iceberg com sub-second freshness sem MotherDuck", MAS seu motor de sync (moonlink) é BSL 1.1 → barrado por D1; e o pacote GA (v0.1.2) é pré-moonlink e default PG18.
- **[D1] pg_mooncake README — repo oficial, `https://raw.githubusercontent.com/Mooncake-Labs/pg_mooncake/main/README.md` (aberta).** Verbatim: *"creates a columnstore mirror of your Postgres tables in Iceberg, enabling fast analytics queries with sub-second freshness … Real-time ingestion powered by moonlink … accelerated by DuckDB, ranking top 10 on ClickBench"*; API: `CALL mooncake.create_table('trades_iceberg','trades')` (mirror que "stays in sync"). É a arquitetura **(a/b) row + column-replica**, permissive-parecendo (extensão MIT).
- **[D2] moonlink LICENSE — repo oficial, `https://raw.githubusercontent.com/Mooncake-Labs/moonlink/main/LICENSE` (aberta).** Verbatim: *"Business Source License 1.1 … Additional Use Grant: … you may not offer the Licensed Work or any derivative as a managed service (such as a database, data warehouse, stream processing, or data lake service) to third parties without a separate commercial license … Change Date: … 2029-06-03"*. + **Makefile (`.../main/Makefile`, aberta): `PG_VERSION ?= pg18`** (o exato build-blocker PG17 do ADR-0013/0020) e `requires = 'pg_duckdb'` (control file). + **Release GA = v0.1.2 (2025-02-12)** — pré-moonlink; o "mirror Iceberg sub-second" vive em `main`, ainda não-GA. → **A capacidade mais atraente do campo tem licença BSL barrada por D1 e maturidade/PG17 não-prontos.** Honesto: a extensão `pg_mooncake` é MIT, mas o valor (sync sub-second) está no moonlink BSL — não é adotável na distribuição TheoDB hoje.

### Claim E — Hydra columnar e Citus columnar (o padrão "columnar table nativo no PG") são AGPLv3 → barrados por D1 (informativo do padrão apenas).
- **[E1] Hydra `columnar/LICENSE` (clone local `.claude/knowledge-base/references/hydra/columnar/LICENSE`): `GNU AFFERO GENERAL PUBLIC LICENSE Version 3`.**
- **[E2] Citus `LICENSE` (clone local `.claude/knowledge-base/references/citus/LICENSE`): `GNU AFFERO GENERAL PUBLIC LICENSE Version 3`.** → Confirma ADR-0020 alternativa #3. O padrão "columnar access method nativo" existe no ecossistema PG mas só sob AGPL — reforça a aposta **lakehouse D2** (Parquet/Iceberg via DuckDB) como a rota permissiva.

---

## ADR (intra-blueprint) — Qual superfície HTAP unificada o TheoDB expõe

**Decisão (recomendada): D — "Superfície HTAP lakehouse-materializada": uma função/rotina `theodb.htap_refresh(table)` que materializa o row-store para um snapshot Parquet/Iceberg local (via `COPY … TO (FORMAT parquet)`), e uma VIEW/roteamento `theodb.olap(table)` que serve a query analítica do snapshot colunar via `duckdb.query`/`read_parquet` — com freshness EXPLÍCITA (o snapshot tem timestamp), e o caminho `force_execution` disponível como fallback ad-hoc para queries que exigem dado 100% fresco.** Compõe 100% sobre a peça já embarcada (pg_duckdb, MIT, ADR-0020) — Regra 9, zero peça nova. É a arquitetura do survey **(a) primary row + column store** materializada em arquivo (não in-memory), i.e. a aposta lakehouse/D2, honesta.

### Alternativas rejeitadas (com razão)

1. **A — `force_execution` como "a superfície HTAP" (a mesma tabela heap serve OLAP via DuckDB).** Rejeitada: **medida honest-negative no M61 (0.63–0.89×)** — DuckDB perde sobre row-format. Vender isto como HTAP seria claim falso (Regra 5). *Mantida só como fallback ad-hoc fresco*, não como a superfície principal.
2. **B — pg_mooncake (mirror Iceberg sub-second via moonlink).** Rejeitada por **D1 (moonlink = BSL 1.1, Additional Use Grant proíbe serviço de data-lake a terceiros; Change Date 2029)** + maturidade (mirror não-GA, default PG18 = build-blocker M61). É a melhor superfície do campo tecnicamente, mas não é permissivamente adotável hoje. *Reavaliar quando o Change Date passar OU se a Mooncake relicenciar.* → **Unresolved Question rastreada.**
3. **C — MotherDuck TAM (`CREATE TABLE … USING duckdb`, columnstore persistente sincronizado).** Rejeitada: MotherDuck é **SaaS proprietário + cloud compute** — quebra "downloadable, roda em qualquer lugar, model/infra-agnostic" (CLAUDE.md) e D1 (não é peça permissiva embarcável). *Pode ser um conector opcional*, nunca a superfície default.
4. **E — Columnar access method nativo (estilo Hydra/Citus/pg_mooncake-v0.1).** Rejeitada: AGPL (E1/E2) ou reescrever motor colunar vetorizado do zero (Regra 9, PhD-level/anos — ADR-0013/0020).
5. **"Só documentar o padrão, não expor superfície".** Rejeitada: M62 exige uma superfície medível (o benchmark é o gate). Documentar sem entregar API+benchmark não fecha o milestone.

**Por que D vence:** é a única que (i) compõe sobre peça permissiva já embarcada (Regra 9, D1), (ii) entrega o ganho colunar REAL medido (~9× a 5M sobre Parquet, M61), (iii) é honesta sobre freshness (snapshot datado, não "mágico"), (iv) alinha com a aposta declarada lakehouse/D2 (não finge ser o in-memory do AlloyDB). Esforço ≠ Complexidade: o esforço de materializar+rotear é essencial ao problema; nenhuma abstração especulativa.

---

## Coverage Corner 1 — Integration Tests

- **Smoke da superfície:** `theodb.htap_refresh('orders')` gera um snapshot Parquet/Iceberg; `theodb.olap('orders')` agrega via DuckDB e o checksum full-scan (`_results_match` de `benchmarks/theodb_bench/columnar.py`) bate com o `GROUP BY` do row-executor do Postgres (correctness-matched, como M61).
- **Freshness assertion:** após um `INSERT` no row-store SEM refresh, `theodb.olap()` retorna o snapshot ANTIGO (staleness observável e datado); após `htap_refresh()`, retorna o novo. O teste ASSERTA o gap — freshness é contrato explícito, não bug.
- **Fallback fresco:** `SET duckdb.force_execution=true; SELECT … FROM orders` retorna o dado 100% fresco (sem refresh) — o teste confirma o fallback existe e é correto (mesmo sendo mais lento; caminho ad-hoc).
- **Concurrency test:** um cliente rodando `INSERT` (OLTP) enquanto outro roda `theodb.olap()` (OLAP) — asserta que o OLAP lê o snapshot consistente e o INSERT não é bloqueado (isolamento snapshot-vs-row, análogo ao Raft-index-validate do TiFlash [B2], mas via snapshot de arquivo).

## Coverage Corner 2 — Dependencies

- **pg_duckdb v1.1.0 (MIT)** — JÁ embarcado (ADR-0020). `read_parquet`, `duckdb.query`, `COPY … TO (FORMAT parquet)`, `iceberg_scan` (community ext). Zero dependência nova. `/deps-audit` das transitivas já feito no M61 (`libcurl4`).
- **Iceberg extension do DuckDB** — community extension; `duckdb.allow_community_extensions=off` no default TheoDB (ADR-0020 segurança). Se a superfície usar Iceberg (não só Parquet), precisa habilitar a ext auditada OU ficar em Parquet puro (mais simples, KISS). **Recomendação: Parquet puro no v1 da superfície; Iceberg como follow-up.**
- **BARRADO D1:** moonlink (BSL 1.1, D2), MotherDuck (SaaS proprietário, C2), Hydra/Citus columnar (AGPL, E1/E2). Nenhum entra na distribuição.

## Coverage Corner 3 — Tools

- **Harness reutilizado (Regra 9):** `benchmarks/theodb_bench/columnar.py` (`_AGG`, `_results_match`) e o padrão de `benchmarks/run_m61_columnar_adoption.py` (2 superfícies, >=3 runs mean±std, warm-up descartado, correctness-matched). O bench M62 estende: adiciona a dimensão **carga mista concorrente** (OLTP INSERT + OLAP agg simultâneos) e **freshness lag** (tempo entre INSERT e visibilidade no snapshot).
- **Ambiente:** droplet DigitalOcean (padrão dos benches P0, ver `m57-bench-droplet` na memória) — box dedicada, não a dev saturada (aprendizado M46).
- **`COPY … TO (FORMAT parquet)`** (nativo pg_duckdb) para materializar; `EXPLAIN` para confirmar que a query OLAP usa o executor DuckDB sobre Parquet (não o row-executor).

## Coverage Corner 4 — Techniques

- **Materialização row→colunar por snapshot (data synchronization do survey [A1]).** O TheoDB usa **sync explícito on-demand/scheduled** (não streaming CDC, que exigiria moonlink-BSL). Trade-off consciente: freshness pior que o sub-second do pg_mooncake/AlloyDB, mas permissivo e sob controle do usuário.
- **Roteamento OLTP-row vs OLAP-colunar.** O SOTA (AlloyDB [B1], TiFlash [B2]) esconde o roteamento atrás do planner ("automatically choose"). O TheoDB v1 usa **roteamento explícito** (`theodb.olap()` vs SELECT normal) — KISS/honesto; roteamento automático via planner-hook é own-code PhD-level (Regra 9), fica como Unresolved/futuro.
- **Consistência snapshot.** Análogo ao Snapshot Isolation do TiFlash [B2] mas materializado: o snapshot Parquet é um ponto-no-tempo imutável — o OLAP sempre lê um estado consistente (nunca um write parcial). A staleness é o preço, medida no benchmark.
- **Colunar+vetorizado do DuckDB é onde o ganho vive (medido M61: 1.56×→8.78× de 100k→5M).** A técnica só materializa se os dados estiverem em formato colunar — a razão de a superfície materializar em Parquet, não servir o heap.

---

## Design do benchmark de carga mista (o GATE do M62 — medido no /implement, NÃO aqui)

Measurement-first: este discover só *desenha*; o número é o gate do milestone. Medir honestamente **3 eixos**:

1. **Speedup OLAP colunar (o ganho).** Agregação `GROUP BY` servida pela superfície (Parquet via DuckDB) vs o mesmo `GROUP BY` no row-executor do Postgres. Escalas 100k/1M/5M, >=3 runs mean±std, checksum-matched (reusa M61). **Esperado (direção do M61): ~2×→~9×**, cresce com N. Marcado `UNBENCHMARKED` até o /implement rodar.
2. **Freshness / staleness lag (o custo de honestidade).** Tempo de wall-clock do `htap_refresh()` por escala (custo de materializar row→Parquet — o trade-off que o M61 caveat #2 apontou como não-medido) + o "lag" entre um INSERT e sua visibilidade no snapshot. Reportar como distribuição, não um número. É a métrica que AlloyDB/TiFlash escondem e nós expomos.
3. **Latência OLTP sob OLAP concorrente (não-interferência).** p50/p95 de INSERTs OLTP com e sem uma query OLAP concorrente rodando. Asserta que a superfície analítica **não degrada o transacional** (o ponto do "isolation" no survey [A1] e no Raft-Learner do TiFlash [B2]). 1-cliente-OLTP + 1-cliente-OLAP no mínimo; multi-cliente como follow-up.

**Correctness gate (não-negociável):** toda medição de speedup só conta se o checksum full-scan bater (DuckDB double vs Postgres numeric, comparação por eps relativo — o método M61). Um speedup sobre resultado errado é zero.

**Honest-negative aceito:** se o custo de `htap_refresh()` + a staleness tornarem a superfície pior que só rodar `force_execution` para um dado workload, o benchmark DEVE dizer isso (como o M61 disse do heap). O veredito honesto é o entregável, não um número bonito.

---

## Riscos honestos

1. **Freshness vs performance (a tensão central, do survey [A1] "data synchronization").** O snapshot colunar fica ATRÁS do row-store entre refreshes. AlloyDB/TiFlash mascaram isto (in-memory refresh / async Raft com validate-before-read); a superfície D expõe. **Mitigação:** freshness é contrato datado + fallback `force_execution` fresco para queries que não toleram lag. Risco residual: usuário lê stale sem perceber → a API DEVE retornar/expor o timestamp do snapshot.
2. **Custo de materialização (não medido no M61 — caveat #2).** `COPY → Parquet` de 5M+ linhas tem custo de I/O e CPU; se o refresh for frequente, pode dominar. **Mitigação:** o benchmark eixo-2 mede isto explicitamente; refresh scheduled/on-demand (não por-write) mantém o custo amortizado.
3. **Duplicação de storage.** O dado vive 2× (heap + Parquet). Para tabelas grandes, dobra o footprint em disco. **Mitigação:** materializar só as tabelas/colunas analíticas (como o auto-columnarization seletivo do AlloyDB [A2], mas manual); documentar o trade-off. Não é in-memory (não come RAM como AlloyDB), o custo é disco (mais barato).
4. **(secundário) Roteamento manual.** v1 exige o usuário escolher `theodb.olap()` — menos ergonômico que o planner-auto do SOTA. Aceito por KISS; planner-hook é futuro.

## Posicionamento honesto vs AlloyDB (Regra 5)

- **AlloyDB:** column store **in-memory, auto-mantido, auto-columnarization, planner-transparente** (arquitetura (a) do survey [A1], confirmado [A2]). Freshness alta, mas efêmero, acoplado à RAM, e proprietário/cloud.
- **TheoDB (superfície D):** column store **em arquivo (Parquet/Iceberg lakehouse, aposta D2 declarada)**, materializado explicitamente, freshness datada e sob controle do usuário, roteamento explícito no v1. Ganho colunar REAL medido (~9× a 5M, M61), permissivo, roda em qualquer lugar, sem lock-in de compute.
- **Não é paridade** com o in-memory do AlloyDB — é uma aposta **diferente e honesta** (CLAUDE.md North Star Opção α: lakehouse ≠ cópia do AlloyDB). Nenhum claim de "HTAP transparente"; o claim é "analytics colunar sobre um snapshot sincronizado dos seus dados transacionais, sem MotherDuck, sem AGPL, sem ETL manual externo". Todo número fica `UNBENCHMARKED` até o benchmark-gate do /implement.

## Prior Art

- Survey acadêmico HTAP (taxonomia + técnicas de sync/freshness): arXiv:2404.15670 [A1].
- SOTA anchor AlloyDB columnar (in-memory auto): doc oficial Google [A2/B1].
- SOTA HTAP replica async: TiFlash/TiDB doc oficial [B2].
- Peça embarcada (M61/ADR-0020) e seus limites medidos: `docs/benchmarks/m61-columnar-adoption.md`, pg_duckdb docs [C1/C2].
- Alternativa mais-próxima-mas-barrada: pg_mooncake+moonlink [D1/D2].
- Clones locais estudados: `.claude/knowledge-base/references/{duckdb,pg_mooncake,hydra,citus,paradedb}`.

## Unresolved Questions

1. **Reavaliar pg_mooncake/moonlink quando o BSL Change Date (2029-06-03) passar OU se relicenciarem** — seria a superfície tecnicamente superior (sub-second Iceberg sync permissivo). Rastreado; não é adotável hoje (D1).
2. **Roteamento automático row-vs-colunar via planner-hook** (paridade de ergonomia com AlloyDB/TiFlash) — own-code, futuro milestone, fora do escopo D-v1 (Regra 9/KISS).
3. **Iceberg (vs Parquet puro) na superfície** — habilita time-travel e interop multi-engine (`iceberg_scan` [C1]), mas exige community extension auditada (ADR-0020 segurança). Follow-up se a demanda de interop aparecer.
4. **Dataset analítico realista** (não sintético 5-categorias) — o M61 caveat #1; o benchmark M62 deveria usar um dataset tipo ClickBench/TPC-H para absolutos defensáveis.

## Drawbacks & Risks (resumo)

Ver § Riscos honestos (freshness-lag, custo-materialização, storage-2×, roteamento-manual). O maior é a **tensão
freshness×performance**: a superfície D é honesta sobre ela (snapshot datado + fallback fresco) enquanto o SOTA a
esconde — é uma escolha de honestidade, não uma falha, mas o usuário PRECISA ver o timestamp do snapshot.
