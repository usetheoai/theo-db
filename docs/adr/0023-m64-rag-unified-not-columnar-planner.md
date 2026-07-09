# ADR 0023 — M64 RAG-over-SQL: unified retrieval é planner-integrado; processamento columnar permanece statement-level

**Status:** Accepted · **Data:** 2026-07-09 · **Milestone:** M64 · **Owner:** Eng
**Relacionado:** blueprint `.claude/knowledge-base/discoveries/blueprints/m64-rag-over-sql-unified-blueprint.md`,
plan `.claude/knowledge-base/plans/m64-rag-over-sql-unified-plan.md`, ADR `0021` (pg_duckdb proíbe DuckDB em função),
ADR `0022` (helper `theodb.vector_join` rejeitado — precedente parsimony), ADR `0002` (North Star / measurement-first),
`.claude/rules/public-copy.md §4` (performance é claim, não opinião), Unbreakable Rule 9 (não reinventar).

## Contexto

O DoD do M64 (ROADMAP § M64) pede: (1) "query de referência `WHERE <filtro> ORDER BY <vetor> LIMIT k` +
**join com agregação columnar, planner-integrado**, recall + latência medidos"; (2) doc do padrão RAG-nativo;
(3) veredito honesto vs pgvector + app-layer. A discovery (blueprint, R0 web-citado, ≥2 fontes por claim)
concluiu que **as peças de retrieval já existem e são planner-integradas** (M52 filtered ANN, M53 híbrida RRF,
M63 vector JOIN, embed/chat in-SQL) — o M64 é **composição + medição + documentação**.

## Achado arquitetural (a premissa incorreta do DoD)

O requisito original ("planner-integrated columnar aggregation") pressupunha um **plano híbrido único** entre
row-store e columnar. A investigação mostrou que essa capacidade depende de um **planner único controlando
ambos os mecanismos de armazenamento**. O TheoDB, por construção, combina PostgreSQL (executor HNSW row-store)
e DuckDB (Parquet OLAP) através de **duas engines independentes, com planners independentes e sem mecanismo de
planejamento conjunto**. O impedimento **não é ausência de engenharia no TheoDB** — decorre da arquitetura de
duas engines. Portanto, a única implementação tecnicamente correta consiste em:

- **Path 1:** uma SQL planner-integrada sobre PostgreSQL (filtro + retrieval + context-assembly);
- **Path 2:** dois statements reutilizando o padrão M62 (retrieval + `theodb.olap_sql()` que o cliente executa).

Esta ADR **não reduz escopo** — corrige uma premissa incorreta do DoD (um plano híbrido row+columnar não
existe porque não existe um planner único).

### Três níveis de "unificado" (para evitar ambiguidade futura)

| Nível | Existe no TheoDB? |
|---|---|
| Uma única SQL enviada ao servidor | **sim** (Path 1) |
| Um único plano do planner PostgreSQL | **sim** (Path 1) |
| Um único plano envolvendo PostgreSQL + DuckDB | **não** (duas engines, dois planners) |

"Planner-integrado" nesta ADR refere-se aos dois primeiros níveis (o retrieval); o terceiro (row+columnar num
plano só) é o que **não** existe.

## Decisão D1 — Entregar Path 1 (uma query, row-store); Path 2 columnar documentado como DOIS statements

**Decisão:** a query de referência RAG unificada é o Path 1 —
`WITH retrieved AS (SELECT id, content FROM t WHERE cat = $c ORDER BY emb <=> $q LIMIT k) SELECT string_agg(content, …) FROM retrieved` —
planner-integrada, row-store, **uma** ida ao servidor. A leg columnar (Path 2) é documentada honestamente como
**dois statements** (o retrieval + o `SELECT theodb.olap_sql()` que o cliente roda), NÃO um plano híbrido único.

**O achado de honestidade (o BLOCKER que o DoD literal esconde).** A cláusula "agregação columnar
planner-integrada" é **estruturalmente inalcançável** no TheoDB hoje:
- **pg_duckdb proíbe execução DuckDB dentro de função** (ADR-0021, medido: `ERROR: DuckDB execution is not
  supported inside functions`; sem GUC). Por isso a superfície columnar do M62 é codegen statement-level.
- O índice `theodb_hnsw` é **row-store** (Index Scan planner-integrado em LATERAL, ADR-0022); o Parquet
  columnar vive no **DuckDB**. São **duas engines**; um planner Postgres não as unifica num plano híbrido.
- O SOTA que faz isso first-class (AlloyDB in-memory columnar; TiDB/TiFlash) o consegue porque tem **uma
  engine + um planner** dono de ambos os stores ([AlloyDB columnar](https://cloud.google.com/alloydb/docs/columnar-engine/about),
  [TiFlash](https://github.com/pingcap/docs/blob/master/tiflash/tiflash-overview.md), [HTAP survey arXiv:2404.15670](https://arxiv.org/abs/2404.15670)).

**Dois caminhos honestos (public-copy.md §4 — não mascarar):**
- **Path 1 (uma query, real, row-store):** filtro + retrieval + context-assembly numa SQL. A agregação (se
  houver) corre no executor PG sobre as k linhas do top-k — para agregação sobre apenas k linhas do
  retrieved-set **não há trabalho analítico suficiente para justificar a participação da engine columnar**;
  chamar isso de "columnar RAG" seria desonesto (Regra 5).
- **Path 2 (agregar o retrieved-set contra um fato columnar Parquet grande):** **dois statements** — o
  retrieval + o `theodb.olap_sql()` que o cliente executa. Reusa M62. NÃO é um plano único.

**Alternativas rejeitadas:**
- **(A) Fingir uma query columnar planner-integrada** — desonesto (a agregação sobre k linhas roda no
  executor PG, a engine columnar não participa). Viola Regra 5.
- **(B) Custom scan que una row-store + Parquet num plano** — PhD-level, exigiria reescrever o planner;
  exige engine única (fora de escopo D2). Complexidade acidental ("Esforço ≠ Complexidade").

## Decisão D2 — NÃO construir `theodb.rag_query()`; padrão documentado + benchmark

**Decisão:** zero código de produção novo. O RAG unificado é um padrão de query (CTE + `string_agg`) que o
usuário escreve; o M64 entrega o guia + o benchmark + a prova de correção (pg_test), não uma função helper.

**Rationale:** rung-1 da parsimony-ladder ("isto precisa existir?") — precedente **ADR-0022** (M63 rejeitou
`theodb.vector_join` porque o raw idiom já é first-class e o SQL dinâmico do helper arriscaria o pushdown/recall).
O mesmo se aplica: um `theodb.rag_search(filter, qtext, k)` seria açúcar SemVer para zero ganho de capacidade
(YAGNI), e a query varia por schema (uma view fixa não generaliza).

**Alternativas rejeitadas:**
- **(A) `theodb.rag_search()` função** — SQL dinâmico arriscaria recall/pushdown; contrato SemVer para açúcar.
- **(B) VIEW canônica** — não generaliza (nomes de coluna/filtro variam); documentar o padrão é mais KISS.

## O que o M64 entrega (o valor real — a lacuna que o campo não publica)

- **Prova de correção (pg_test):** `rag_unified_query_preserves_recall` (a query composta recupera EXATAMENTE
  o top-k filtrado do oráculo exato — compor não degrada o recall) + `rag_unified_read_your_writes` (linha
  INSERTada na txn é recuperável na MESMA SQL e no MESMO snapshot MVCC via pending region). Nota de rigor: um
  cliente app-layer também obtém read-your-writes se abrir uma transação explícita (`BEGIN; retrieve; hydrate;
  COMMIT`); o diferencial do Path 1 não é a visibilidade em si, mas **fazê-la numa SQL única, num snapshot MVCC
  único, sem coordenação adicional de múltiplas chamadas pelo cliente**.
- **Benchmark unified-vs-app-layer:** o head-to-head **"1 SQL vs N app-calls"** que ninguém publica —
  round-trips economizados a recall-igualado. (números em `docs/benchmarks/m64-rag-over-sql.{md,json}`.)
  **O que o benchmark NÃO mede (anti-interpretação-errada):** ele **não** demonstra superioridade algorítmica
  de retrieval — **ambos os braços usam exatamente o mesmo top-k** (recall idêntico por construção; o
  recall-match gate confirma). Mede APENAS a diferença estrutural entre compor a query in-SQL (1 round-trip,
  1 snapshot) vs compor no cliente (N round-trips, coordenação de múltiplas chamadas).

## Evidência (medida)

- **2 `#[pg_test]` GREEN** (`cargo pgrx test pg17 rag_unified` — 2 passed, contra o stack real theodb_rs +
  vector + vectorscale + theodb): `rag_unified_query_preserves_recall` (a query composta recupera o set
  idêntico ao oráculo exato + `string_agg` concatena exatamente K docs) + `rag_unified_read_your_writes`
  (linha INSERTada na txn recuperável na mesma SQL/snapshot MVCC).
- **Benchmark (n=5000, dim 128, k 10, 3 runs × 50 reps, droplet c-8):** braço A (unified) **1 round-trip**
  p50 **6.721 ms** vs braço B (app-layer) **2 round-trips** (retrieve + hydrate PK-served) p50 **7.284 ms**.
  **recall-match gate PASS** (jaccard **1.0**, mesmo top-k por construção). Latência UNIFIED_FASTER mas
  **modesto co-located** (~8%) — o custo do 2º round-trip é pequeno no mesmo host; a economia **amplifica
  sobre rede real**. A vitória estrutural é round_trips (1 vs 2). `docs/benchmarks/m64-rag-over-sql.{md,json}`.
- **15 pytest GREEN** (aritmética: round_trip_delta, recall_match_gate, verdict honesto por-eixo), ruff clean.
- **Correção de honestidade aplicada:** a tabela do benchmark ganhou `PRIMARY KEY (id)` — sem ela, o hydrate
  do braço B (`WHERE id = ANY(...)`) faria seqscan de 5000 linhas (um straw-man que inflava B a ~5×); com a PK
  (que toda tabela real tem) o hydrate é index-served e o gap cai para os 8% honestos do round-trip.

## Consequências

- **O RAG unificado do TheoDB é first-class para retrieval** (filtro + vetor + híbrida + context-assembly numa
  SQL), com recall herdado provado (M52/M53/M63) + consistência transacional (read-your-writes).
- **A leg columnar NÃO é planner-integrada** — é o padrão M62 de dois statements (honesto, não mascarado).
- **Rerank de 2ª ordem (cross-encoder)** é M65 (`ai.rerank`, não existe ainda); o rerank disponível hoje é o
  score RRF (M53) ou `ai.rank` (LLM-scoring por linha). Documentado honestamente.
- **Zero código de produção novo** (composição — Regra 9).

## Caveats honestos

Dados sintéticos (gaussian-mixture, `cat` filter); o round-trip economizado é **estrutural** (1 vs 2) — o
CUSTO em latência é pequeno co-located e cresce com a latência de rede (reportado nos dois cenários, não só o
favorável). Sem claim de paridade com AlloyDB. Os números são os medidos (public-copy.md §4).
