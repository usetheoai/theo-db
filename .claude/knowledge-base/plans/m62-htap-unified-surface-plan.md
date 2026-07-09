---
slug: m62-htap-unified-surface
milestone_id: M62
created_at: 2026-07-09
goal: Ship a lakehouse-materialized HTAP surface (theodb.htap_refresh/olap/htap_freshness) whose columnar OLAP path is measurably faster than the Postgres row executor on a checksum-matched aggregate.
---

# Plan: M62 — Superfície HTAP unificada (lakehouse-materializada)

> **Version 1.0** — Entrega a experiência HTAP do TheoDB como um fluxo HONESTO row-store ↔ colunar, não como "a mesma tabela heap serve OLAP magicamente" (medido honest-negative no M61: `force_execution` sobre heap = 0.63–0.89×). Três funções SQL/plpgsql próprias em `sql/` compõem 100% sobre o `pg_duckdb` já embarcado (M61/ADR-0020, MIT — Regra 9, zero peça nova): `theodb.htap_refresh(regclass)` materializa a tabela row para um snapshot Parquet datado (`COPY … TO … (FORMAT parquet)`), `theodb.olap(regclass)` roteia a agregação para o snapshot colunar via `duckdb.query`/`read_parquet` (~9× medido no M61), e `theodb.htap_freshness(regclass)` retorna o lag do snapshot. Um benchmark de 3 eixos (`docs/benchmarks/m62-htap.{md,json}`) é o gate do milestone: (a) speedup OLAP colunar checksum-matched, (b) freshness lag + custo do refresh, (c) latência OLTP p50/p95 sob OLAP concorrente. Honest-negative aceito. NÃO é HTAP-transparente; a freshness é explícita e datada; o storage é 2× (heap+Parquet).

## Goal

> "Enable a TheoDB user to route an analytical aggregate to a columnar snapshot of a transactional table (`theodb.olap(table)`) so that the query runs measurably faster than the Postgres row executor on the same checksum-matched result, measured by `benchmarks/tests/test_htap.py::test_olap_speedup_checksum_matched` asserting `parquet_over_heap_speedup > 1.0 AND checksum_match == True` at n=1_000_000."

## Context

M62 é o pilar HTAP do roadmap (`ROADMAP.md:1000` — "transacional + analítico na mesma tabela") — a marca do AlloyDB. O achado medido do M61 (`docs/benchmarks/m61-columnar-adoption.md`, `docs/adr/0020-m61-embed-pgduckdb.md:36-38`) restringe o design de forma dura e honesta: o `pg_duckdb` embarcado **NÃO** acelera analytics sobre o heap row-store (`force_execution` = 0.63–0.89×, honest-negative) e **VENCE ~9× a 5M apenas sobre dados já COLUNARES** (Parquet). Sem MotherDuck não há columnstore DuckDB-nativo persistente. Portanto o "HTAP unificado do TheoDB" não pode ser "a mesma tabela heap serve OLTP e OLAP magicamente" — tem que ser um fluxo honesto row-store ↔ colunar materializado.

O blueprint `.claude/knowledge-base/discoveries/blueprints/m62-htap-unified-surface-blueprint.md` (§ ADR "D — Superfície HTAP lakehouse-materializada") recomenda, por ADR com alternativas rejeitadas (moonlink=BSL barrado por D1; MotherDuck=SaaS proprietário; Hydra/Citus=AGPL; `force_execution`=honest-negative; só-doc=não fecha o milestone), a superfície materializada-por-snapshot. Este plano implementa essa recomendação: as funções próprias em SQL/plpgsql + o benchmark-gate de 3 eixos, mantendo `docs/adr/0020-m61-embed-pgduckdb.md:49-50` ("M62 constrói sobre esta adoção — o caminho analítico é o Parquet/read_parquet + o `force_execution` para queries ad-hoc").

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/30-theodb-embed.sql` | 17 | `b6ac534` (2026-07-03) | Bootstrap do schema `theodb` (`CREATE SCHEMA IF NOT EXISTS theodb`); doc do contrato de GUCs do embed | `CREATE SCHEMA IF NOT EXISTS theodb` deve permanecer idempotente; NÃO redefinir `theodb.embed` (é Rust em theodb_rs) |
| `sql/50-theodb-ai.sql` | ~150 | `6f5a01a` (2026-06-30) | Superfície `ai.*` (generate/summarize/agg) em plpgsql late-bound sobre `ai._chat` (Rust) | funções `ai.*` existentes intactas; padrão plpgsql late-bound + `VOLATILE` preservado |
| `sql/85-theodb-htap.sql` (NEW) | 0 | — | (arquivo a criar) — as 3 funções HTAP próprias `theodb.htap_refresh/olap/htap_freshness` | — |
| `Dockerfile` | ~150 | `5ae4f79` (2026-07-08) | Multi-stage build; concat dos `sql/*.sql` em `theodb--1.0.sql` (linha 129-131); `CREATE EXTENSION pg_duckdb` (linha 163) | ordem de concat determinística; `pg_duckdb` já criado; `shared_preload_libraries='pg_duckdb'` intacto |
| `benchmarks/theodb_bench/columnar.py` | 77 | `16421b2` (2026-06-28) | `_AGG` (query de agregação canônica) + `_results_match` (checksum cross-engine com eps) | `_AGG` e `_results_match` reutilizados verbatim (Rule 9) — assinatura preservada |
| `benchmarks/theodb_bench/db.py` | ~260 | `16421b2` (2026-06-28) | Helper `VectorDB` (`_cursor`, `timed_query`, `explain_plan`, `pg_mooncake_available`) | métodos existentes intactos; novo `pg_duckdb_available()` adicionado (aditivo) |
| `benchmarks/run_m62_htap.py` (NEW) | 0 | — | (arquivo a criar) — harness de 3 eixos, reusa `_AGG`/`_results_match` | — |
| `benchmarks/tests/test_htap.py` (NEW) | 0 | — | (arquivo a criar) — testes de integração (round-trip, freshness, concorrência) | — |
| `docs/benchmarks/m62-htap.md` (NEW) | 0 | — | (arquivo a criar) — relatório honesto de 3 eixos | — |
| `docs/benchmarks/m62-htap.json` (NEW) | 0 | — | (arquivo a criar) — dados brutos do benchmark | — |
| `docs/adr/0021-m62-htap-lakehouse-materialized.md` (NEW) | 0 | — | (arquivo a criar) — ADR da superfície HTAP materializada | — |
| `CHANGELOG.md` | ~30 | `78588a4` (2026-07-09) | Contrato público de mudanças | entrada `[Unreleased] § Added` do discover M62 preservada; nova entrada aditiva |

Todo arquivo listado em qualquer `#### Files to edit` abaixo aparece nesta tabela. Linhas `(NEW)` são esperadas.

### Current callers / dependents

As funções `theodb.htap_refresh/olap/htap_freshness` são **novas** — não há caller de produção a preservar (são superfície SQL nova exposta ao usuário final via `SELECT`/`CALL`). Verificação:

- **Symbol:** `theodb.htap_refresh` / `theodb.olap` / `theodb.htap_freshness` (NEW)
- **Callers (produção):** nenhum (superfície nova; o "caller" é o usuário SQL + o teste de integração)
- **Callers (testes):** `benchmarks/tests/test_htap.py` (NEW) — o wiring da superfície
- **External (API pública consumida por outros repos):** sim — é API SQL pública do `theodb`; contrato: assinaturas `theodb.htap_refresh(regclass) RETURNS text`, `theodb.olap(regclass) RETURNS SETOF record`/`RETURNS jsonb`, `theodb.htap_freshness(regclass) RETURNS interval`. Uma vez publicadas, mudanças de assinatura são breaking.

- **Symbol reutilizado:** `_AGG`, `_results_match` em `benchmarks/theodb_bench/columnar.py:11,67`
- **Callers (produção/bench):** `benchmarks/run_m61_columnar_adoption.py:23` (import), `benchmarks/theodb_bench/columnar.py:56-63`
- **Callers (testes):** `benchmarks/tests/test_columnar.py`
- Confirmado via `grep -rn '_AGG\|_results_match' benchmarks/`.

### Domain glossary

- **HTAP** — Hybrid Transactional/Analytical Processing: OLTP (row) + OLAP (colunar) sobre os mesmos dados. No TheoDB: fluxo materializado row→Parquet, não in-memory transparente (aposta lakehouse/D2).
- **`force_execution`** — GUC do pg_duckdb (`SET duckdb.force_execution=true`) que força o executor DuckDB a ler o heap row-store direto. Medido honest-negative no M61 (0.63–0.89×). Fica como fallback ad-hoc fresco, não superfície principal.
- **snapshot** — o arquivo Parquet materializado por `htap_refresh`; um ponto-no-tempo imutável dos dados row. O OLAP sempre lê um estado consistente (nunca write parcial).
- **freshness lag** — o intervalo entre o estado atual do heap e o estado do snapshot Parquet. É contrato explícito e datado (não bug), retornado por `theodb.htap_freshness`.
- **checksum-matched** — uma medição de speedup só conta se o full-scan checksum (DuckDB double vs Postgres numeric, eps relativo) bater. Speedup sobre resultado errado é zero (`_results_match`, M61).
- **regclass** — tipo Postgres para referência de tabela validada; `%I`-quoted em dynamic SQL (injection-safe; padrão de `sql/80-theodb-migrate.sql:3`).

### Architecture boundaries affected (`rules/architecture.md`)

- **interface (SQL surface) → infrastructure (pg_duckdb / filesystem).** As funções `theodb.*` são a camada de interface (SQL pública); elas chamam `pg_duckdb` (`COPY … TO parquet`, `duckdb.query`) — o adaptador de infraestrutura já embarcado. Direção: interface → infra (correta; inner não importa outer). Nenhuma camada de domínio nova; nenhuma abstração especulativa (KISS, `rules/parsimony-ladder.md`).
- **Composition root:** a instalação da extensão concatena `sql/*.sql` no `Dockerfile:129-131` — o novo `sql/85-theodb-htap.sql` entra no concat na ordem correta (após `50-theodb-ai.sql`, mantendo a superfície `theodb`/`ai` já bootstrapada).

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/m62-htap-unified-surface-blueprint.md` § "ADR (intra-blueprint) — Qual superfície HTAP unificada o TheoDB expõe" (recomendação D) e § "Design do benchmark de carga mista (o GATE do M62)" (os 3 eixos). Consumido integralmente.
- **Internal ADR:** `docs/adr/0020-m61-embed-pgduckdb.md` — a peça `pg_duckdb` embarcada e seus limites medidos (heap honest-negative; Parquet ~9×). Linha `:49-50` prevê explicitamente M62 sobre esta base.
- **Internal benchmark:** `docs/benchmarks/m61-columnar-adoption.md` — o número ~9× a 5M sobre Parquet (re-confirmado, não inventado, no eixo-1 do M62).
- **Reference (harness, Rule 9):** `benchmarks/theodb_bench/columnar.py:11` (`_AGG`), `:67` (`_results_match`), e o padrão de `benchmarks/run_m61_columnar_adoption.py:60-110` (warm-up descartado, ≥3 runs mean±std, checksum-matched, `_measure_parquet` com `COPY … TO parquet` + `read_parquet`). Reutilizados verbatim.
- **Reference (dynamic-SQL safety):** `sql/80-theodb-migrate.sql:3-4` — regclass + `%I` quoting, valores bound, identificadores nunca interpolados raw. Padrão copiado para as funções HTAP.
- **External literature:** HTAP survey arXiv:2404.15670 (`https://arxiv.org/abs/2404.15670`) — taxonomia row+column + técnicas de data-synchronization/isolation; ancora o design "sync explícito on-demand" e o eixo-3 de não-interferência. AlloyDB columnar doc (`https://cloud.google.com/alloydb/docs/columnar-engine/about`) — o SOTA in-memory auto-mantido que posicionamos honestamente (nosso é em-arquivo, freshness datada).

## Objective

- [ ] `theodb.htap_refresh(regclass)` materializa a tabela row para um snapshot Parquet datado e registra o timestamp do snapshot.
- [ ] `theodb.olap(regclass)` roteia a agregação `_AGG` para o snapshot colunar via `read_parquet`/`duckdb.query`, retornando o mesmo resultado que o `GROUP BY` no heap fresco (checksum-matched).
- [ ] `theodb.htap_freshness(regclass)` retorna o lag do snapshot (intervalo desde o último refresh); staleness é observável e datada, não bug.
- [ ] Fallback `force_execution` fresco documentado e testado como caminho ad-hoc (não superfície principal).
- [ ] Benchmark de 3 eixos → `docs/benchmarks/m62-htap.{md,json}`: (a) speedup OLAP colunar checksum-matched, (b) freshness lag + custo refresh, (c) latência OLTP p50/p95 sob OLAP concorrente.
- [ ] Imagem (pg_duckdb do M61 + as novas funções theodb.*) carrega; suíte não regride (Integration Validation).

## ADRs

### D1 — Superfície HTAP lakehouse-materializada (snapshot Parquet datado), não HTAP-transparente

- **Decisão:** expor 3 funções próprias (`theodb.htap_refresh`, `theodb.olap`, `theodb.htap_freshness`) que materializam o row-store para um snapshot Parquet local (`COPY … TO … (FORMAT parquet)`) e roteiam a agregação analítica para o snapshot colunar via `duckdb.query`/`read_parquet`, com freshness EXPLÍCITA e datada. `force_execution` sobre heap fica como fallback ad-hoc fresco.
- **Rationale:** compõe 100% sobre a peça permissiva já embarcada (`pg_duckdb`, MIT — Regra 9, zero peça nova); entrega o ganho colunar REAL medido (~9× a 5M sobre Parquet, `docs/benchmarks/m61-columnar-adoption.md`); é honesta sobre freshness (snapshot datado, não "mágico" — Regra 5); alinha com a aposta declarada lakehouse/D2 (não finge ser o in-memory do AlloyDB). É a arquitetura (a) primary-row+column-store do survey [arXiv:2404.15670], materializada em arquivo.
- **Alternativas consideradas:**
  1. **`force_execution` como a superfície HTAP (heap serve OLAP).** Rejeitada: medida honest-negative no M61 (0.63–0.89×, `docs/adr/0020-m61-embed-pgduckdb.md:36`) — vender isto como HTAP seria claim falso (Regra 5). Mantida só como fallback.
  2. **pg_mooncake (mirror Iceberg sub-second via moonlink).** Rejeitada por D1 (moonlink = BSL 1.1, blueprint § Claim D2) + default PG18 (build-blocker M61). Tecnicamente superior mas não permissivamente adotável hoje → Unresolved Q1.
  3. **MotherDuck TAM (columnstore persistente).** Rejeitada: SaaS proprietário + cloud compute — quebra "downloadable, roda em qualquer lugar" (CLAUDE.md) e D1.
  4. **Columnar access method nativo (Hydra/Citus).** Rejeitada: AGPL (D1) ou reescrever motor colunar do zero (Regra 9, PhD-level/anos).
  5. **Só documentar o padrão, sem expor superfície.** Rejeitada: M62 exige superfície medível (o benchmark é o gate); documentar sem API+benchmark não fecha o milestone.
- **Consequências:** habilita analytics colunar ~9× sobre um snapshot sincronizado, permissivo, sem MotherDuck/AGPL/ETL externo. Constrange: freshness datada (snapshot fica atrás entre refreshes); storage 2× (heap+Parquet); roteamento explícito no v1 (não planner-auto). Todos honestos e medidos (não escondidos como o SOTA).

### D2 — SQL/plpgsql próprio em `sql/85-theodb-htap.sql`, não Rust em theodb_rs

- **Decisão:** implementar as 3 funções como SQL/plpgsql em um novo `sql/85-theodb-htap.sql`, concatenado no install script (`Dockerfile:129-131`), no padrão de `sql/50-theodb-ai.sql` (plpgsql late-bound) e `sql/80-theodb-migrate.sql` (regclass + `%I`).
- **Rationale:** as funções só executam SQL dinâmico (`COPY … TO parquet`, `duckdb.query`, consulta de catálogo p/ timestamp) — não há lógica de baixo nível que exija Rust. A parsimony-ladder (`rules/parsimony-ladder.md`, rung 5/6 — "a menor coisa que resolve") diz: plpgsql basta. Late-bound plpgsql permite referenciar objetos do pg_duckdb (criados após theodb) sem falha no CREATE time — exatamente o padrão já usado por `ai.generate` (`sql/50-theodb-ai.sql:21-29`).
- **Alternativas consideradas:**
  1. **Rust em theodb_rs (`#[pg_extern]`).** Rejeitada: adiciona compilação/FFI/unsafe sem ganho (Regra 9/KISS) — nenhum hot-path Rust aqui, só orquestração de SQL. Um `#[pg_extern]` roda na transação do caller e complicaria o `COPY` a arquivo sem benefício.
  2. **Init-script (docker-entrypoint-initdb.d), não extensão.** Rejeitada: M15 ADR decidiu que a superfície theodb ships como EXTENSÃO (concat em `theodb--1.0.sql`), não init-scripts — manter consistência.
- **Consequências:** zero toolchain novo; diff mínimo; consistente com a superfície SQL existente. Constrange: staleness/freshness fica em plpgsql (menos performático que Rust, mas o custo é I/O do `COPY`, não CPU do plpgsql — irrelevante).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| **Freshness vs performance** — o snapshot colunar fica ATRÁS do heap entre refreshes; usuário pode ler stale sem perceber | Alta | Freshness é contrato datado: `theodb.olap` expõe/retorna o timestamp do snapshot; `theodb.htap_freshness` dá o lag; fallback `force_execution` para queries que não toleram lag. Eixo-2 do benchmark mede o lag. | Eng |
| **Custo de materialização** — `COPY → Parquet` de 5M+ linhas tem custo de I/O/CPU; refresh frequente pode dominar | Média | Eixo-2 do benchmark mede o custo do refresh por escala explicitamente (caveat #2 do M61); refresh on-demand/scheduled (não por-write) amortiza. Honest-negative aceito se o custo dominar. | Eng |
| **Storage 2×** — o dado vive no heap + no Parquet; dobra o footprint em disco para tabelas grandes | Média | Materializar só tabelas/colunas analíticas (manual, como o auto-columnarization seletivo do AlloyDB); documentar o trade-off no ADR-0021. É disco (barato), não RAM. | Eng |
| **Interferência OLTP↔OLAP** — uma agregação OLAP concorrente pode degradar a latência de INSERTs OLTP | Alta | Eixo-3 do benchmark mede p50/p95 de INSERT com/sem OLAP concorrente (teste race-aware, T2.1); o snapshot Parquet é read-only e não bloqueia o heap. | Eng |
| **`COPY … TO parquet` de dentro de função** — incerteza: `COPY TO file` pode falhar/ter restrição de permissão dentro de plpgsql | Média | T1.1 valida o mecanismo com um teste RED antes de qualquer wiring; se `COPY TO` falhar de dentro da função, o fallback é `duckdb.query` com `COPY … TO` embutido no SQL DuckDB (o próprio pg_duckdb roda o COPY). Documentado em Failure scenarios. | Eng |

## Unresolved Questions

- Q1 — Reavaliar pg_mooncake/moonlink quando o BSL Change Date (2029-06-03) passar OU se relicenciarem (seria a superfície tecnicamente superior — sub-second Iceberg sync permissivo). Rastreado; não adotável hoje (D1). Fora de escopo M62.
- Q2 — `COPY … TO '<path>' (FORMAT parquet)` funciona de dentro de uma função plpgsql, ou precisa ser roteado via `duckdb.query('COPY …')`? Resolvido empiricamente em T1.1 (teste RED do mecanismo antes do wiring).
- Q3 — Onde os arquivos Parquet vivem (path fixo `/tmp` vs GUC configurável vs `data_directory`)? v1 usa um path derivado do oid da tabela sob um diretório base; multi-tabela/GC de snapshots antigos é follow-up.
- Q4 — Roteamento automático row-vs-colunar via planner-hook (paridade de ergonomia com AlloyDB) — own-code, futuro milestone, fora do escopo v1 (Regra 9/KISS).

## Dependency Graph

```
Phase 1 (funções HTAP) ──▶ Phase 2 (carga mista / não-interferência) ──▶ Phase 3 (benchmark 3 eixos) ──▶ Phase 4 (Integration Validation)
       │                            │                                             │
       └─ T1.1 mecanismo COPY       └─ T2.1 teste race-aware OLTP↔OLAP            └─ reusa _AGG/_results_match
       └─ T1.2 htap_refresh         (depende das funções de P1)                  (depende de P1+P2)
       └─ T1.3 olap + freshness
```

Phase 1 é bloqueante para todas. Phase 2 depende das funções de Phase 1. Phase 3 depende de Phase 1+2 (mede as funções). Phase 4 é a validação final. Dentro de Phase 1, T1.1 (mecanismo) precede T1.2/T1.3.

---

## Phase 1: Funções HTAP (own-code SQL/plpgsql em `sql/`)

**Objective:** entregar `theodb.htap_refresh`, `theodb.olap` e `theodb.htap_freshness` como plpgsql idempotente concatenado na extensão, com round-trip correctness (refresh → olap == heap fresco) e freshness observável.

### T1.1 — Validar o mecanismo `COPY row → Parquet` + `read_parquet` de dentro do Postgres

#### Objective
Provar empiricamente (RED antes de wiring) que uma tabela row pode ser materializada em Parquet e re-lida via DuckDB retornando o mesmo agregado — o mecanismo bruto sobre o qual as 3 funções se apoiam.

#### Why this step (action + reasoning — ReAct discipline)
1. **O que faz:** um teste de integração que faz `COPY (SELECT * FROM t) TO '<path>' (FORMAT parquet)`, depois `SELECT … FROM duckdb.query($$ … read_parquet('<path>') $$)`, e asserta que o resultado bate com o `GROUP BY` no heap (via `_results_match`).
2. **Por que agora:** o blueprint (§ ADR D) e o M61 (`benchmarks/run_m61_columnar_adoption.py:87-110`, `_measure_parquet`) provam o mecanismo NO HARNESS, mas de dentro de uma FUNÇÃO plpgsql há incerteza (Q2/Risk "COPY TO de dentro de função"). Resolver o mecanismo ANTES de escrever as funções evita construir sobre uma premissa falsa (parsimony rung 1 — "isso precisa existir?" só faz sentido se o mecanismo funciona).

#### Evidence
`benchmarks/run_m61_columnar_adoption.py:93` já roda `cur.execute("COPY (SELECT * FROM …) TO '<path>' (FORMAT parquet)")` com sucesso no nível de conexão; `docs/adr/0020-m61-embed-pgduckdb.md:36-38` confirma Parquet ~9×. A incerteza é somente o contexto plpgsql.

#### Files to edit
```
benchmarks/tests/test_htap.py (NEW) — RED: test_copy_to_parquet_roundtrip_matches_heap
benchmarks/theodb_bench/db.py — add pg_duckdb_available() helper (aditivo)
```

#### Deep file dependency analysis
- `benchmarks/theodb_bench/db.py` (baseline: helper `VectorDB` com `_cursor`/`timed_query`/`explain_plan`/`pg_mooncake_available`): adiciona `pg_duckdb_available()` (espelha `pg_mooncake_available:231` — `SELECT 1 FROM pg_extension WHERE extname='pg_duckdb'`). Não altera métodos existentes.
- `benchmarks/tests/test_htap.py` (NEW): usa a fixture `db` do padrão de `test_columnar.py:24-31`, skip-clean se `pg_duckdb` ausente (nunca silent-green).

#### Deep Dives
- Path do Parquet: `/tmp/theodb_htap_<oid>.parquet` (oid da tabela, único por tabela). GC de snapshots antigos é follow-up (Q3).
- Invariante: o resultado do Parquet == resultado do heap dentro de `eps=1e-3` (`_results_match`), cross-engine numeric tolerance.
- Edge case: tabela vazia → Parquet vazio → `_results_match` retorna False para `len==0` (`columnar.py:69`); o teste asserta o comportamento (vazio-válido tratado explicitamente).

#### Tasks
1. Adicionar `pg_duckdb_available()` a `db.py`.
2. Escrever `test_copy_to_parquet_roundtrip_matches_heap` que seed → COPY → read_parquet → asserta match.
3. Rodar contra a imagem M61 (pg_duckdb presente) para confirmar o mecanismo bruto.

#### TDD
```
RED:     test_copy_to_parquet_roundtrip_matches_heap() — seed n=10000; COPY heap→Parquet; agregar via read_parquet; asserta _results_match(heap_agg, parquet_agg) == True. Falha antes de db.pg_duckdb_available existir/mecanismo confirmado.
GREEN:   Implement pg_duckdb_available() em db.py; confirmar o mecanismo COPY/read_parquet no teste.
REFACTOR: Extrair o path-builder do Parquet se reutilizado em T1.2 (else None expected).
VERIFY:  cd benchmarks && python -m pytest tests/test_htap.py -k roundtrip -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test_copy_to_parquet_roundtrip_matches_heap` passa contra a imagem com pg_duckdb (skip-clean sem ele).
- [ ] `db.pg_duckdb_available()` retorna True na imagem M61.
- [ ] Pass: lint — `cd benchmarks && ruff check theodb_bench/db.py tests/test_htap.py` zero warnings.
- [ ] Pass: size — `db.py` e `test_htap.py` ≤ 500 linhas.

#### DoD (Definition of Done)
- [ ] Todas as tasks completas e validadas.
- [ ] `cd benchmarks && python -m pytest tests/test_htap.py -k roundtrip` verde.
- [ ] Zero lint warnings — `ruff check`.
- [ ] File-size budget respeitado.

---

### T1.2 — `theodb.htap_refresh(regclass)` — materializar row → snapshot Parquet datado

#### Objective
Uma função plpgsql que materializa a tabela row para um snapshot Parquet e registra o timestamp do snapshot num catálogo interno (`theodb._htap_snapshots`).

#### Why this step (action + reasoning — ReAct discipline)
1. **O que faz:** `theodb.htap_refresh(t regclass) RETURNS text` faz `COPY (SELECT * FROM t) TO '<path>' (FORMAT parquet)` (ou via `duckdb.query` se T1.1 mostrar necessário), grava `(t, path, now())` em `theodb._htap_snapshots`, e retorna o path/timestamp.
2. **Por que agora:** é o primeiro half do fluxo (materializar) — `theodb.olap` (T1.3) depende do snapshot existir e do timestamp para computar freshness. Cita ADR D1 (superfície materializada) e D2 (plpgsql). O padrão regclass+`%I` vem de `sql/80-theodb-migrate.sql:3`.

#### Evidence
`sql/80-theodb-migrate.sql:3-4` (regclass + `%I` injection-safe); `sql/50-theodb-ai.sql:21-29` (plpgsql late-bound sobre objeto do pg_duckdb). `benchmarks/run_m61_columnar_adoption.py:93` (o `COPY … TO parquet` bruto).

#### Files to edit
```
sql/85-theodb-htap.sql (NEW) — CREATE SCHEMA/tabela _htap_snapshots + FUNCTION theodb.htap_refresh
Dockerfile — adicionar sql/85-theodb-htap.sql ao concat (linha 129-131)
benchmarks/tests/test_htap.py — RED: test_htap_refresh_creates_dated_snapshot
```

#### Deep file dependency analysis
- `sql/85-theodb-htap.sql` (NEW): `CREATE SCHEMA IF NOT EXISTS theodb` (idempotente, como `30-theodb-embed.sql:16`); `CREATE TABLE IF NOT EXISTS theodb._htap_snapshots (rel regclass PRIMARY KEY, parquet_path text, refreshed_at timestamptz)`; `CREATE OR REPLACE FUNCTION theodb.htap_refresh(rel regclass) RETURNS text LANGUAGE plpgsql VOLATILE`.
- `Dockerfile:129-131`: inserir `sql/85-theodb-htap.sql` no `cat` após `sql/50-theodb-ai.sql` (mantém a superfície `theodb` já bootstrapada e antes do migrate).

#### Deep Dives
- Assinatura: `theodb.htap_refresh(rel regclass) RETURNS text`. Dynamic SQL: `format('COPY (SELECT * FROM %I) TO %L (FORMAT parquet)', rel::text, path)` — `%I` para identificador, `%L` para literal (injection-safe).
- Invariante: após o refresh, `theodb._htap_snapshots` tem uma linha para `rel` com `refreshed_at = now()` do momento do COPY; upsert (`ON CONFLICT (rel) DO UPDATE`) — o snapshot mais recente vence.
- Edge case: tabela inexistente → `regclass` cast já falha com erro típico do Postgres (fail-fast, `rules/error-handling.md`); COPY falha → a exceção sobe (não engolida — Regra 8).

#### Pseudo-code / Signatures
```pseudocode
FUNCTION theodb.htap_refresh(rel regclass) RETURNS text:
  path := '/tmp/theodb_htap_' || rel::oid || '.parquet'
  EXECUTE format('COPY (SELECT * FROM %s) TO %L (FORMAT parquet)', rel::text, path)
  INSERT INTO theodb._htap_snapshots(rel, parquet_path, refreshed_at)
    VALUES (rel, path, now())
    ON CONFLICT (rel) DO UPDATE SET parquet_path = EXCLUDED.parquet_path, refreshed_at = now()
  RETURN path

# Example
input:  theodb.htap_refresh('orders')
output: '/tmp/theodb_htap_16421.parquet'  (+ row in theodb._htap_snapshots)
```

#### Tasks
1. Criar `sql/85-theodb-htap.sql` com schema/tabela + `htap_refresh`.
2. Adicionar o arquivo ao concat no `Dockerfile`.
3. Escrever o teste RED que asserta a linha datada + o arquivo existente.

#### TDD
```
RED:     test_htap_refresh_creates_dated_snapshot() — CALL theodb.htap_refresh('t'); asserta que theodb._htap_snapshots tem 1 linha com refreshed_at recente E o path retornado existe. Falha antes da função existir.
GREEN:   Implementar theodb.htap_refresh + a tabela de catálogo.
REFACTOR: Extrair o path-builder para uma função interna theodb._htap_path(regclass) se T1.3 reusar (else None).
VERIFY:  cd benchmarks && python -m pytest tests/test_htap.py -k refresh -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `theodb.htap_refresh('t')` grava linha datada em `theodb._htap_snapshots` e o Parquet existe.
- [ ] `sql/85-theodb-htap.sql` idempotente (re-run não quebra — `CREATE OR REPLACE`/`IF NOT EXISTS`).
- [ ] Dynamic SQL usa `%I`/`%L` (injection-safe, verificável por leitura + `SELECT theodb.htap_refresh('pg_class')` não injeta).
- [ ] Pass: size — `sql/85-theodb-htap.sql` ≤ 500 linhas.

#### DoD
- [ ] `python -m pytest tests/test_htap.py -k refresh` verde.
- [ ] Concat do Dockerfile inclui o novo arquivo (grep confirma).
- [ ] CHANGELOG `[Unreleased] § Added` atualizado.

---

### T1.3 — `theodb.olap(regclass)` + `theodb.htap_freshness(regclass)` — rotear OLAP colunar + expor lag

#### Objective
`theodb.olap(rel)` roteia a agregação para o snapshot Parquet via `read_parquet` (retornando o mesmo resultado que o heap fresco); `theodb.htap_freshness(rel)` retorna o lag do snapshot.

#### Why this step (action + reasoning — ReAct discipline)
1. **O que faz:** `theodb.olap(rel regclass) RETURNS jsonb` lê o `parquet_path` de `theodb._htap_snapshots`, roda a agregação `_AGG`-equivalente via `duckdb.query($$ … read_parquet(path) … $$)`, e retorna o resultado + o `refreshed_at` do snapshot (freshness embutida). `theodb.htap_freshness(rel) RETURNS interval` retorna `now() - refreshed_at`.
2. **Por que agora:** fecha o fluxo (materializar → rotear → medir freshness). Depende de T1.2 (o snapshot + catálogo). O contrato "olap retorna o resultado do snapshot + o timestamp" é o núcleo da honestidade do ADR D1 (Risk "freshness vs performance": o usuário PRECISA ver o timestamp).

#### Evidence
`benchmarks/theodb_bench/columnar.py:11` (`_AGG`), `run_m61_columnar_adoption.py:94-96` (`duckdb.query($$ … read_parquet …$$)`); blueprint § Coverage Corner 1 (freshness assertion: staleness observável e datada).

#### Files to edit
```
sql/85-theodb-htap.sql — add FUNCTION theodb.olap + FUNCTION theodb.htap_freshness
benchmarks/tests/test_htap.py — RED: test_olap_matches_fresh_heap; test_freshness_reflects_lag; test_force_execution_fallback_is_fresh
```

#### Deep file dependency analysis
- `sql/85-theodb-htap.sql`: adiciona `theodb.olap(rel regclass) RETURNS jsonb` (busca path no catálogo; erro típico se sem snapshot — fail-fast) e `theodb.htap_freshness(rel regclass) RETURNS interval`.
- `benchmarks/tests/test_htap.py`: reusa `_AGG`/`_results_match` de `columnar.py` (Rule 9) para o oráculo do round-trip.

#### Deep Dives
- Assinaturas: `theodb.olap(rel regclass) RETURNS jsonb` (`{"snapshot_at": ts, "rows": [...]}`), `theodb.htap_freshness(rel regclass) RETURNS interval`.
- Invariante freshness: após INSERT no heap SEM refresh, `theodb.olap` retorna o snapshot ANTIGO (staleness datada); após `htap_refresh`, retorna o novo. `htap_freshness` cresce monotonicamente entre refreshes.
- Edge case: `theodb.olap` sem snapshot prévio → erro claro `no snapshot for <rel>; call theodb.htap_refresh first` (typed, Regra 8), não NULL silencioso.
- Fallback fresco: `SET duckdb.force_execution=true; SELECT … FROM rel` retorna dado 100% fresco (sem refresh) — testado como caminho ad-hoc correto (mais lento; blueprint § Coverage Corner 1).

#### Pseudo-code / Signatures
```pseudocode
FUNCTION theodb.olap(rel regclass) RETURNS jsonb:
  SELECT parquet_path, refreshed_at INTO path, ts FROM theodb._htap_snapshots WHERE rel = $1
  IF NOT FOUND: RAISE EXCEPTION 'no snapshot for %; call theodb.htap_refresh first', rel
  rows := EXECUTE duckdb.query($$ SELECT category, count(*) c, round(avg(amount),4) a
                                  FROM read_parquet(path) GROUP BY category ORDER BY category $$)
  RETURN jsonb_build_object('snapshot_at', ts, 'rows', rows)

# Example
input:  theodb.olap('orders')  -- after refresh at 12:00, INSERT at 12:05, no re-refresh
output: {"snapshot_at": "2026-07-09T12:00:00Z", "rows": [{"category":"cat0","c":2000,"a":748.5}, ...]}
        -- rows reflect 12:00 state (stale by design; freshness datada)
```

#### Tasks
1. Implementar `theodb.olap` (busca snapshot, roteia via `read_parquet`, retorna resultado+timestamp).
2. Implementar `theodb.htap_freshness` (`now() - refreshed_at`).
3. Escrever os 3 testes RED (match com heap fresco pós-refresh; freshness reflete lag; fallback fresco correto).

#### TDD
```
RED:     test_olap_matches_fresh_heap() — refresh; olap('t').rows == GROUP BY heap (via _results_match). Falha antes de theodb.olap existir.
RED:     test_freshness_reflects_lag() — refresh; sleep; INSERT sem refresh; asserta htap_freshness > 0 E olap retorna o snapshot ANTIGO (staleness datada); após novo refresh, olap reflete o novo estado.
RED:     test_force_execution_fallback_is_fresh() — SET duckdb.force_execution=true; SELECT sobre heap retorna dado 100% fresco (sem refresh); asserta correto.
GREEN:   Implementar theodb.olap + theodb.htap_freshness.
REFACTOR: Extrair a query de agregação para um único local se duplicada com o harness (else None).
VERIFY:  cd benchmarks && python -m pytest tests/test_htap.py -k "olap or freshness or fallback" -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```
Nota: a não-interferência OLTP↔OLAP concorrente é uma task própria (T2.1) com teste race-aware; estas funções em si são chamadas single-thread nos testes de round-trip.

#### Acceptance Criteria
- [ ] `theodb.olap('t').rows` bate com o `GROUP BY` no heap fresco pós-refresh (`_results_match`).
- [ ] `theodb.htap_freshness('t')` cresce entre refreshes; `theodb.olap` retorna snapshot antigo até novo refresh (staleness observável).
- [ ] `theodb.olap` sem snapshot prévio levanta erro tipado (não NULL silencioso).
- [ ] Fallback `force_execution` retorna dado fresco correto.
- [ ] Pass: lint + size — `ruff check tests/test_htap.py`; arquivo ≤ 500 linhas.

#### DoD
- [ ] `python -m pytest tests/test_htap.py -k "olap or freshness or fallback"` verde.
- [ ] Todas as 3 funções carregam na extensão (smoke `SELECT theodb.olap/htap_refresh/htap_freshness` resolve).
- [ ] CHANGELOG atualizado.

---

## Phase 2: Carga mista / não-interferência (OLTP↔OLAP concorrente)

**Objective:** provar, com um teste race-aware, que rodar uma agregação OLAP concorrente não degrada materialmente a latência de INSERTs OLTP.

### T2.1 — Harness/teste de carga mista: INSERTs OLTP concorrentes com agregação OLAP

#### Objective
Um teste que executa INSERTs OLTP em uma thread/conexão enquanto uma query OLAP roda em outra, e asserta que (a) o OLAP lê um snapshot consistente e (b) o INSERT não é bloqueado / a latência não degrada além de um limiar.

#### Why this step (action + reasoning — ReAct discipline)
1. **O que faz:** abre 2 conexões concorrentes — uma faz N INSERTs (OLTP), outra roda `theodb.olap`/agregação (OLAP) — usando `threading` + uma barreira (`threading.Barrier`) para garantir sobreposição real; mede latência p50/p95 dos INSERTs com e sem o OLAP concorrente e asserta que a degradação fica sob um limiar declarado.
2. **Por que agora:** o eixo-3 do benchmark (não-interferência) e o Risk "Interferência OLTP↔OLAP" exigem prova de concorrência. TDD single-thread NUNCA pega isso (`rules/testing.md` § 6, `plan-confidence` concurrency-tests cap): a latência OLTP sob OLAP concorrente É concorrência — precisa de teste race-aware com sobreposição garantida por barreira, não execução sequencial que interleava limpo.

#### Evidence
Blueprint § "Design do benchmark" eixo-3 ("Latência OLTP sob OLAP concorrente (não-interferência) … 1-cliente-OLTP + 1-cliente-OLAP no mínimo"); § Coverage Corner 1 concurrency test ("um cliente rodando INSERT enquanto outro roda theodb.olap() — asserta que o OLAP lê o snapshot consistente e o INSERT não é bloqueado"). arXiv:2404.15670 (isolation entre OLTP/OLAP).

#### Files to edit
```
benchmarks/run_m62_htap.py (NEW) — harness: mixed-load axis (concurrent INSERT + OLAP) reusa _AGG
benchmarks/tests/test_htap.py — RED: test_oltp_latency_not_degraded_under_concurrent_olap (race-aware)
```

#### Deep file dependency analysis
- `benchmarks/run_m62_htap.py` (NEW): função `measure_mixed_load(scales, runs)` que abre 2 conexões, usa `threading.Thread` + `threading.Barrier(2)` para sobrepor INSERT-loop e OLAP-loop, coleta latências dos INSERTs em ambas as condições.
- `benchmarks/tests/test_htap.py`: teste que chama `measure_mixed_load` numa escala pequena e asserta o invariante de não-interferência + consistência do snapshot.

#### Deep Dives
- Race-aware shape: `threading.Barrier(2)` sincroniza o início das 2 threads (happens-before explícito), garantindo que os INSERTs e o OLAP realmente sobreponham — sem barreira, uma thread poderia terminar antes da outra começar (falso-verde).
- Invariante: p95 dos INSERTs sob OLAP concorrente ≤ `LATENCY_DEGRADATION_FACTOR` × p95 baseline (fator declarado, ex. 3×; honest-negative aceito se degradar mais — reportado, não escondido). O snapshot lido pelo OLAP é consistente (um ponto-no-tempo, nunca write parcial).
- Edge case: se o OLAP terminar antes de todos os INSERTs, a barreira + um loop-até-N-INSERTs garantem sobreposição; o teste falha loud se a sobreposição não ocorreu (contagem de eventos concorrentes > 0).

#### Pseudo-code / Signatures
```pseudocode
FUNCTION measure_mixed_load(n_inserts, olap_iters) RETURNS dict:
  barrier := Barrier(2)
  oltp_latencies := []
  def oltp_worker(): barrier.wait(); for i in n_inserts: t0=now(); INSERT; oltp_latencies.append(now()-t0)
  def olap_worker(): barrier.wait(); for i in olap_iters: theodb.olap('t')   # concurrent pressure
  # baseline: oltp alone; then oltp + olap concurrent
  run(oltp_worker) -> baseline_p50, baseline_p95
  run(oltp_worker, olap_worker concurrently) -> mixed_p50, mixed_p95
  return {baseline_p95, mixed_p95, degradation = mixed_p95/baseline_p95, overlap_confirmed: bool}

# Example
output: {baseline_p95: 0.8ms, mixed_p95: 1.1ms, degradation: 1.38, overlap_confirmed: true}
```

#### Tasks
1. Escrever `measure_mixed_load` em `run_m62_htap.py` com `threading.Barrier` (sobreposição garantida).
2. Escrever o teste race-aware que asserta degradação ≤ fator E overlap_confirmed.
3. Confirmar que o snapshot OLAP lido é consistente durante os INSERTs.

#### TDD
```
RED:     test_oltp_latency_not_degraded_under_concurrent_olap() — mede p95 INSERT baseline vs sob OLAP concorrente; asserta degradation <= LATENCY_DEGRADATION_FACTOR AND overlap_confirmed == True. Falha antes de measure_mixed_load existir.
GREEN:   Implementar measure_mixed_load com Barrier.
REFACTOR: Extrair o percentil helper se reusado no benchmark de Phase 3 (else None).
VERIFY:  cd benchmarks && python -m pytest tests/test_htap.py -k concurrent -v
```

#### Concurrency tests (only when applicable)
```
Race-aware — happens-before observation com threading.Barrier(2):
- 2 threads (OLTP INSERT-loop + OLAP olap-loop) sincronizadas por Barrier para GARANTIR sobreposição real.
- Assert overlap_confirmed (contagem de eventos concorrentes > 0) — sem isso, a execução sequencial daria falso-verde.
- Assert p95 INSERT sob OLAP <= LATENCY_DEGRADATION_FACTOR × p95 baseline (não-interferência).
- Assert o resultado do OLAP é um snapshot consistente (nunca write parcial) durante os INSERTs.
Comando: cd benchmarks && python -m pytest tests/test_htap.py -k concurrent -v
```

#### Acceptance Criteria
- [ ] `test_oltp_latency_not_degraded_under_concurrent_olap` passa (degradação ≤ fator declarado, overlap confirmado).
- [ ] O teste FALHA se a sobreposição não ocorrer (overlap_confirmed guard — não pode passar sequencialmente).
- [ ] Pass: lint — `ruff check run_m62_htap.py`.
- [ ] Pass: size — `run_m62_htap.py` ≤ 500 linhas.

#### DoD
- [ ] `python -m pytest tests/test_htap.py -k concurrent` verde.
- [ ] O teste é determinístico (Barrier garante ordem; sem sleep-based flakiness — `rules/testing.md` § 6).
- [ ] CHANGELOG atualizado.

---

## Phase 3: Benchmark de 3 eixos → docs/benchmarks/m62-htap.{md,json}

**Objective:** produzir o artefato-gate do M62 medindo (a) speedup OLAP colunar checksum-matched, (b) freshness lag + custo do refresh, (c) latência OLTP sob OLAP concorrente. Honest-negative aceito.

### T3.1 — Harness de 3 eixos + relatório honesto

#### Objective
`benchmarks/run_m62_htap.py` mede os 3 eixos em ≥3 runs mean±std, checksum-matched, e escreve `docs/benchmarks/m62-htap.{md,json}` com veredito honesto.

#### Why this step (action + reasoning — ReAct discipline)
1. **O que faz:** estende `run_m62_htap.py` (já tem o eixo-3 de T2.1) com o eixo-1 (speedup OLAP Parquet vs heap, reusando `_measure_parquet` do M61) e o eixo-2 (wall-clock do `htap_refresh` por escala + freshness lag); escreve o `.json` bruto e o `.md` de veredito.
2. **Por que agora:** o benchmark é o GATE do milestone (`ROADMAP.md:1006` — "Benchmark HTAP: carga mista → docs/benchmarks/m62-htap.{md,json}"). Reusa `_AGG`/`_results_match` (Rule 9); o correctness gate (checksum) é não-negociável — speedup sobre resultado errado é zero. Cita `docs/adr/0002` (measurement-first) e ADR D1.

#### Evidence
o blueprint `.claude/knowledge-base/discoveries/blueprints/m62-htap-unified-surface-blueprint.md` (os 3 eixos + correctness gate + honest-negative aceito); `benchmarks/run_m61_columnar_adoption.py:87-133` (`_measure_parquet` + o padrão de report `.json`); `docs/benchmarks/m61-columnar-adoption.md` (formato do relatório honesto).

#### Files to edit
```
benchmarks/run_m62_htap.py — add measure_olap_speedup (eixo-1, reusa _measure_parquet pattern) + measure_refresh_cost (eixo-2) + main() que escreve os artefatos
docs/benchmarks/m62-htap.md (NEW) — relatório honesto de 3 eixos
docs/benchmarks/m62-htap.json (NEW) — dados brutos
benchmarks/tests/test_htap.py — RED: test_olap_speedup_checksum_matched (o metric do Goal)
```

#### Deep file dependency analysis
- `run_m62_htap.py`: `measure_olap_speedup` reusa a lógica de `_measure_parquet` (`run_m61_columnar_adoption.py:87`) — `COPY→Parquet` + `read_parquet` vs heap, checksum-matched; `measure_refresh_cost` cronometra `theodb.htap_refresh` por escala e o lag; `main()` agrega os 3 eixos e escreve os artefatos (padrão `json.dump` de `run_m61:144`).
- `docs/benchmarks/m62-htap.md`: segue o formato de `docs/benchmarks/m61-columnar-adoption.md` (metodologia + números + veredito honesto).

#### Deep Dives
- Escalas: 100k/1M/5M, ≥3 runs, warm-up descartado (`run_m61:66-71`).
- Correctness gate: eixo-1 só conta com `checksum_match == True` (`_results_match`/eps relativo, `run_m61:107`).
- Métrica do Goal: `parquet_over_heap_speedup > 1.0 AND checksum_match == True` em n=1_000_000 (esperado ~2–9× do M61, marcado `UNBENCHMARKED` até rodar).
- Honest-negative: se refresh+staleness tornarem a superfície pior que `force_execution` para um workload, o `.md` DEVE dizer (como o M61 disse do heap).

#### Pseudo-code / Signatures
```pseudocode
FUNCTION main():
  axis1 := measure_olap_speedup(scales, runs)     # parquet vs heap, checksum-matched
  axis2 := measure_refresh_cost(scales, runs)     # htap_refresh wall-clock + freshness lag
  axis3 := measure_mixed_load(...)                # from T2.1
  data := {axis1, axis2, axis3, verdict: honest_verdict(axis1, axis2, axis3)}
  json.dump(data, 'docs/benchmarks/m62-htap.json')
  write_markdown(data, 'docs/benchmarks/m62-htap.md')  # methodology + numbers + honest verdict
```

#### Tasks
1. Implementar `measure_olap_speedup` (eixo-1) e `measure_refresh_cost` (eixo-2).
2. `main()` que roda os 3 eixos e escreve `.json` + `.md`.
3. Escrever `test_olap_speedup_checksum_matched` (o metric do Goal).

#### TDD
```
RED:     test_olap_speedup_checksum_matched() — roda measure_olap_speedup em n=1_000_000 (ou escala CI reduzida com marcador); asserta parquet_over_heap_speedup > 1.0 AND checksum_match == True. Falha antes de measure_olap_speedup existir.
GREEN:   Implementar os eixos 1-2 + main() + escrita dos artefatos.
REFACTOR: DRY entre measure_olap_speedup e o _measure_parquet do M61 (reusar, não duplicar — Rule 9).
VERIFY:  cd benchmarks && python -m pytest tests/test_htap.py -k speedup -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```
Nota: o eixo-3 (concorrência) já tem seu teste race-aware em T2.1; T3.1 apenas o agrega ao relatório.

#### Acceptance Criteria
- [ ] `test_olap_speedup_checksum_matched` passa (speedup > 1.0 E checksum_match em n=1M) — o metric do Goal.
- [ ] `docs/benchmarks/m62-htap.{md,json}` existem, com os 3 eixos e veredito honesto (honest-negative permitido e explícito).
- [ ] Correctness gate: nenhum speedup reportado sem `checksum_match == True`.
- [ ] Pass: lint + size — `ruff check run_m62_htap.py`; ≤ 500 linhas.

#### DoD
- [ ] `python -m pytest tests/test_htap.py -k speedup` verde.
- [ ] Artefatos escritos e commitados (`docs/benchmarks/m62-htap.{md,json}`).
- [ ] CHANGELOG `[Unreleased] § Added` cita o benchmark.

---

## Phase 4: Integration Validation

**Objective:** a imagem (pg_duckdb do M61 + as novas funções theodb.*) carrega; suíte não regride.

### T4.1 — Smoke da imagem + suíte não-regressão + ADR-0021

#### Objective
Confirmar que a imagem construída carrega as 3 funções theodb.* + pg_duckdb, o round-trip funciona end-to-end na imagem real, e escrever o ADR-0021.

#### Why this step (action + reasoning — ReAct discipline)
1. **O que faz:** build da imagem (concat inclui `sql/85-theodb-htap.sql`), smoke `SELECT theodb.htap_refresh/olap/htap_freshness` na imagem, roda a suíte `test_htap.py` contra o container, e escreve `docs/adr/0021-m62-htap-lakehouse-materialized.md`.
2. **Por que agora:** é a validação final (wiring triad — caller é o smoke/teste na imagem real; integração é o container; a métrica de runtime é o benchmark). O ADR-0021 registra a decisão D1/D2 formalmente (`docs/adr/` — próximo número livre é 0021, pois 0013–0020 estão ocupados).

#### Evidence
`Dockerfile:129-131` (concat) + `:163` (CREATE EXTENSION pg_duckdb); `docs/adr/0020-m61-embed-pgduckdb.md` (padrão do ADR anterior); blueprint § Coverage Corner 1 (smoke da superfície).

#### Files to edit
```
docs/adr/0021-m62-htap-lakehouse-materialized.md (NEW) — ADR formal (D1 superfície materializada + D2 plpgsql)
benchmarks/tests/test_htap.py — test_htap_surface_loads_on_image (smoke end-to-end)
CHANGELOG.md — entrada [Unreleased] § Added final
```

#### Deep file dependency analysis
- `docs/adr/0021-...md` (NEW): registra D1 (lakehouse-materializado vs alternativas) + D2 (plpgsql vs Rust), consequências (freshness datada, storage 2×), cita ADR-0020 e o blueprint.
- `test_htap.py`: `test_htap_surface_loads_on_image` — smoke que resolve as 3 funções e roda um round-trip mínimo na imagem.

#### Deep Dives
- Invariante: as 3 funções resolvem (`\df theodb.htap_*`); a suíte existente (`test_columnar.py` etc.) não regride.
- Edge case: se pg_duckdb não carregar (shared_preload_libraries), o smoke falha loud (fail-closed, `Dockerfile:113-116`).

#### Tasks
1. Build da imagem; confirmar o concat inclui `85-theodb-htap.sql`.
2. Smoke end-to-end das 3 funções na imagem.
3. Escrever ADR-0021; rodar a suíte completa (não-regressão).

#### TDD
```
RED:     test_htap_surface_loads_on_image() — na imagem: refresh→olap→freshness round-trip resolve E _results_match. Falha se a superfície não carrega na imagem.
GREEN:   Ajustar o concat do Dockerfile se necessário; garantir load.
REFACTOR: None expected.
VERIFY:  cd benchmarks && python -m pytest tests/test_htap.py -v   # suíte HTAP completa contra a imagem
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] As 3 funções resolvem na imagem (`SELECT theodb.htap_refresh/olap/htap_freshness` OK).
- [ ] `python -m pytest tests/test_htap.py` verde na imagem; suíte existente não regride.
- [ ] `docs/adr/0021-m62-htap-lakehouse-materialized.md` existe com ≥1 alternativa rejeitada.
- [ ] Pass: size — ADR ≤ 500 linhas.

#### DoD
- [ ] Suíte completa verde na imagem.
- [ ] ADR-0021 escrito e commitado.
- [ ] CHANGELOG `[Unreleased] § Added` final.

---

## Coverage Matrix

| # | Gap / Requirement (do ROADMAP.md § M62 + blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | `theodb.htap_refresh(table)` materializa row→Parquet datado | T1.1, T1.2 | Mecanismo validado (T1.1) + função + catálogo datado (T1.2) |
| 2 | `theodb.olap(table)` roteia agregação p/ snapshot colunar via DuckDB | T1.3 | `read_parquet`/`duckdb.query`, checksum-matched com heap fresco |
| 3 | `theodb.htap_freshness(table)` retorna o lag do snapshot | T1.3 | `now() - refreshed_at`; staleness observável e datada |
| 4 | Fallback `force_execution` fresco (caminho ad-hoc) | T1.3 | Testado como correto e fresco (não superfície principal) |
| 5 | Carga mista / não-interferência OLTP↔OLAP (eixo-3) | T2.1 | Teste race-aware com Barrier; p95 INSERT sob OLAP concorrente |
| 6 | Benchmark de 3 eixos → `docs/benchmarks/m62-htap.{md,json}` | T2.1, T3.1 | Eixo-1 speedup (T3.1) + eixo-2 freshness/custo (T3.1) + eixo-3 (T2.1) |
| 7 | Speedup OLAP colunar checksum-matched (o metric do Goal) | T3.1 | `parquet_over_heap_speedup > 1.0 AND checksum_match` em n=1M |
| 8 | Veredito honesto vs AlloyDB (lakehouse/D2, freshness datada) | T3.1, T4.1 | `.md` de veredito + ADR-0021 (posicionamento honesto) |
| 9 | Imagem (pg_duckdb + funções theodb.*) carrega; suíte não regride | T4.1 | Smoke end-to-end + suíte completa na imagem |
| 10 | Honestidade: NÃO transparente; storage 2×; número é do M61 re-confirmado | T3.1, T4.1 | Explícito no `.md` e no ADR-0021 (Regra 5) |

**Coverage: 10/10 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas.
- [ ] Todos os testes passando — `cd benchmarks && python -m pytest tests/test_htap.py -v` verde.
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench/ run_m62_htap.py tests/test_htap.py`.
- [ ] File-size budget respeitado (todo arquivo alterado ≤ 500 linhas, `rules/architecture.md`).
- [ ] CHANGELOG.md atualizado sob `[Unreleased] § Added` (Regra 6).
- [ ] Backward compat: funções `ai.*`/`theodb.*` existentes intactas; assinaturas HTAP são API pública nova.
- [ ] Plan-specific: os 3 eixos do benchmark medidos e escritos em `docs/benchmarks/m62-htap.{md,json}`; correctness gate (checksum) respeitado; honest-negative explícito se aplicável.
- [ ] **Runtime-metric proof** — o speedup OLAP e o p95 OLTP são observados nos artefatos do benchmark (não só compilam); o eixo-3 confirma overlap real (não sequencial).
- [ ] **Plan archived** — após `/review` = READY_TO_MERGE E PR merged, mover para `knowledge-base/plans/completed/`.

## Failure scenarios (when I/O external)

O plano toca I/O externo: o driver de DB (psycopg2 → Postgres) e o filesystem (escrita/leitura de Parquet via `COPY`/`read_parquet`).

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `postgres:pg_duckdb` (DB/extension) | `COPY … TO parquet` de dentro de plpgsql falha (permissão/restrição) | T1.1: rodar o COPY de dentro de uma função; se falhar, rotear via `duckdb.query('COPY …')` | fail-fast com erro tipado (Regra 8); fallback documentado (`duckdb.query` roda o COPY) |
| `filesystem:/tmp/*.parquet` (object store local) | snapshot Parquet ausente/corrompido quando `theodb.olap` lê | test: deletar/truncar o Parquet, chamar `theodb.olap` | erro claro `no snapshot / corrupt parquet for <rel>; call theodb.htap_refresh` (não NULL silencioso, não crash) |
| `postgres` (DB) | `theodb.olap` chamado sem `htap_refresh` prévio (snapshot inexistente) | test: `theodb.olap('t')` sem refresh | `RAISE EXCEPTION 'no snapshot for %; call theodb.htap_refresh first'` (typed, fail-closed) |
| `postgres:pg_duckdb` (extension) | pg_duckdb não carrega (shared_preload_libraries ausente) | smoke T4.1 na imagem sem o preload | smoke falha loud (fail-closed, `Dockerfile:113-116`); nunca silent-green |

## Final Phase: Integration Validation (MANDATORY)

> Roda APÓS todas as fases de implementação. O plano NÃO está pronto até a cadeia passar.

**Objective:** validar que a superfície HTAP funciona num workload real (imagem construída), não só como unit tests isolados.

### Execution

```
cd benchmarks && python -m pytest tests/test_htap.py -v          # round-trip + freshness + concurrency + smoke
cd benchmarks && python -m pytest tests/ -v                       # suíte completa (não-regressão)
cd benchmarks && ruff check theodb_bench/ run_m62_htap.py tests/test_htap.py   # zero lint warnings
python benchmarks/run_m62_htap.py --scales 100000,1000000 --runs 3   # gera os artefatos do benchmark
```

Chaos/failure pass (Failure scenarios acima):

```
cd benchmarks && python -m pytest tests/test_htap.py -k "no_snapshot or corrupt or fallback" -v
```

### Acceptance Criteria

- [ ] Todas as suítes verdes (unit + integração contra a imagem).
- [ ] Coverage ≥ 90% nos arquivos alterados (crítico: as 3 funções + os 3 eixos 100%).
- [ ] Zero lint warnings.
- [ ] Runtime-metric proof — speedup OLAP e p95 OLTP observados nos artefatos `docs/benchmarks/m62-htap.{md,json}` (não-zero, medidos).
- [ ] Failure scenarios verdes — cada linha de § Failure scenarios exercitada e o comportamento esperado observado.
- [ ] Overlap real confirmado no eixo-3 (não sequencial).

### If Validation Fails

1. Identificar quais falhas são causadas por este plano vs pré-existentes.
2. Corrigir todas as falhas causadas pelo plano antes de declarar completo.
3. Re-rodar a cadeia de validação.
4. Issues pré-existentes são logados mas não bloqueiam (documentar no PR).
