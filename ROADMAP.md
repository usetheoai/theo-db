# TheoDB — Roadmap (banco real, Postgres-based, código próprio Rust)

> **Este é o roadmap ATIVO** — o path convencional (`ROADMAP.md`) que o cycle-kit lê e o `cycle-release` flipa.
> Origem: **ADR `0006-own-code-postgres-based-rust-go`** (virada de mandato "v2", sign-off CTO, 2026-06-29).
> Substituiu o antigo roadmap v1 (tese de composição, M0–M16 entregues como distribuição). O foco é o banco:
> o engine PostgreSQL + a extensão própria (control-plane/deploy foram removidos do escopo deste repositório).
> Norte: transformar TheoDB de "distribuição que compõe extensões de terceiros" em um **banco de dados real, com
> código PRÓPRIO**, dependendo o **mínimo possível** de bibliotecas externas, **mantendo a engine PostgreSQL**
> (C, wire-compat).

## Visão

TheoDB é um **banco de dados competitivo, open-source, baseado na engine PostgreSQL**, com a superfície de
IA + vetorial + unificação implementada como **código próprio** — **Rust** (extensões `pgrx`, dentro do
engine) e **Go** (control plane / operação). O usuário recebe um banco wire-compatible com Postgres em que as
capacidades-killer são **nossas**, não uma colagem de extensões de terceiros.

## Estratégia (LOCKED por ADR 0006)

1. **Engine PostgreSQL mantido** (C, não-reescrito, wire-compat). Não reescrevemos parser/MVCC/WAL/protocolo —
   isso É o Postgres (ADR 0001 núcleo; engine-do-zero rejeitado em ADR 0001 A3). A engine não é uma
   "dependência a remover" — é a **fundação**.
2. **Código próprio:** Rust (`pgrx`) para o que roda *dentro* do engine (tipos, funções, índices). Deploy,
   orquestração e control-plane (operador K8s / CLI / gateway) estão **fora de escopo** deste repositório —
   o foco é o banco de dados (o engine + a extensão).
3. **Dependências externas mínimas** (o pedido do CTO):
   - Substituir incrementalmente as extensões de terceiros (`pgvector`, `pgvectorscale`, `plpython3u`) por
     **código próprio Rust** — para deixar de depender delas.
   - Em Rust, **stdlib first**; crates externos só os **essenciais e auditados** (`pgrx` é obrigatório; um
     HTTP client mínimo para a camada IA; `serde`/jsonb nativo). Cada crate passa pelo gate de licença (D1) +
     CVE (`/deps-audit`). **Zero-dep dogmático é anti-Regra 9** — não reescrevemos HTTP/serde/crypto do zero.
4. **Measurement-first nos índices (ADR 0002 preservado):** reescrever um índice (pgvector/pgvectorscale)
   próprio só **substitui** o terceiro quando o nosso atingir **paridade medida** (recall@k + latência no
   harness). Se não atingir, **mantemos o terceiro** (anti-sunk-cost / Regra 9). "Depender menos" é a meta —
   nunca ao custo de um índice pior.
5. **Incremental com paridade, não big-bang:** cada feature reescrita usa os **testes atuais** como prova de
   paridade; o produto permanece funcional a cada milestone.
6. **Honestidade (Regra 3/5):** nenhum claim de performance sem benchmark; reescrita só "concluída" quando a
   paridade é provada por teste contra container.

## O que NÃO muda (invariantes)

- Wire-compatibility com PostgreSQL 17 (gate de produto).
- Licença **Apache-2.0**; **AGPL barrada** na distribuição (D1) — nosso Rust/Go é permissivo.
- Honestidade de copy/benchmark (`public-copy.md`).

## Fora de escopo do v2 (honesto)

- **Reescrever o engine PostgreSQL** (ADR 0001 A3 — multi-anos, perde wire-compat/maturidade).
- **Columnar próprio (substituir DuckDB/`pg_mooncake`)** — reescrever um motor colunar vetorizado é PhD-level
  e anos (Regra 9). HTAP colunar permanece via a peça permissiva atual **ou** é deferido; não é candidato a
  "código próprio" no v2 inicial. Reabrir exige ADR.
  > **Exceção permissiva (Regra 9), decidida no M30 / ADR 0013 (2026-07-03):** `pg_mooncake`/`pg_duckdb` (MIT)
  > para columnar/HTAP e `pg_textsearch` (permissivo) para BM25 são **mantidos como exceções explícitas** ao
  > mandato own-code — justificadas por evidência medida (columnar ~14× a 5M (mean±std); BM25 nDCG 0.95 vs 0.51) e por não
  > haver peça own-code permissiva que resolva (Citus/Hydra columnar são AGPL — barrados por D1). Gated para
  > adoção; não embarcados ainda.
- **Reescrever HTTP/serde/crypto/parser genérico** — isso é reinventar a roda (Regra 9). Usamos crates
  auditados mínimos.

---

## Milestones v2

> Numeração contígua ao v1 (M17+) para compatibilidade com o fluxo de release (`flip_milestone_checkbox`).
> Cada milestone roda o ciclo completo (discover→plan→implement→code-quality→review→release) com paridade
> provada. Flip `[ ]` → `[x]` ao concluir.

### M17 — [x] Fundação: extensão própria `theodb` em Rust (pgrx) + 1ª feature com paridade

**Objective:** Criar a extensão PostgreSQL **própria em Rust** (`cargo-pgrx`), buildada na imagem (o toolchain
Rust já existe — a imagem hoje compila pgvectorscale), com CI, e **reescrever a primeira superfície**
(`theodb.embed`, hoje plpython3u) em Rust **com paridade** provada pelos testes atuais — provando o padrão
"plpython3u → extensão Rust própria".

**Definition of done:**

- [ ] Projeto `theodb` (Rust/pgrx) builda e `CREATE EXTENSION theodb` instala a partir do `.so` próprio (não mais via init-scripts SQL para a parte migrada).
- [ ] `theodb.embed` reescrita em Rust passa `benchmarks/tests/test_embed_sql.py` (paridade — mesmos resultados/erros typed) contra o container.
- [ ] HTTP client da `embed` é um crate auditado mínimo (licença D1-OK, CVE limpo via `/deps-audit`) — documentado.
- [ ] `plpython3u` deixa de ser requisito para `theodb.embed` (uma dep externa a menos nessa fatia).

**Dependencies:** ADR 0006. **Risco:** curva pgrx + HTTP em Rust; mitigado por escopo mínimo (1 função).

### M18 — [x] Superfície de IA própria em Rust (`ai.*` generativas)

**Objective:** Reescrever `ai._chat` + `generate`/`if`/`analyze_sentiment`/`summarize`/`rank`/`generate_batch`/
`agg_summarize` de plpython3u → **Rust/pgrx**, com paridade pelos testes M7/M10/M11.

**Definition of done:**

- [ ] Todas as `ai.*` generativas em Rust; `benchmarks/tests/test_ai_sql.py` + agg/batch green (paridade, stub determinístico).
- [ ] `REVOKE … FROM PUBLIC` + SSRF/no-redirect/fail-fast preservados (segurança não regride).
- [ ] Camada de IA não requer mais `plpython3u`.

**Dependencies:** M17.

### M19 — [x] NL→SQL + híbrida + import próprios em Rust (fim do plpython3u)

**Objective:** Reescrever `ai.nl_to_sql`/`nl_query` (allowlist parser-grade), `ai.hybrid_search(_rrf)` e
`theodb.import_pinecone` → Rust, com paridade pelos testes M12/M13/M16. Após este milestone, a extensão
`theodb` é **100% Rust** e **não requer plpython3u**.

**Definition of done:**

- [ ] NL→SQL anti-injection (L1–L4) em Rust, paridade + regressão de injeção bloqueada (22023).
- [ ] híbrida (RRF) + `import_pinecone` em Rust, paridade.
- [ ] `plpython3u` removido do `requires` da extensão (dependência externa eliminada). README atualizado (some a limitação plpython3u em managed PG).

**Dependencies:** M18.

### M20 — [x] Tipo vetorial próprio em Rust (reduzir dependência do pgvector)

**Objective:** Implementar o tipo `vector` próprio + operadores de distância (`<=>`/`<->`/`<#>`) em Rust, com
**paridade numérica** vs pgvector, para deixar de depender do pgvector no tipo/ops.

**Definition of done:**

- [ ] Tipo próprio + 3 operadores em Rust; paridade numérica vs pgvector provada por teste.
- [ ] Decisão honesta de migração (coexistência vs substituição) documentada (compat de dados existentes).

**Dependencies:** M17. **Nota:** measurement-first — só substitui pgvector quando a paridade for provada.

### M21 — [x] Índice ANN próprio em Rust: HNSW + IVFFlat (gated por benchmark)

**Objective:** Implementar índice (access method) HNSW + IVFFlat **próprio em Rust**, substituindo o pgvector
index — **somente** quando atingir **paridade de recall@k** no harness.

**Definition of done:**

- [ ] Índice próprio HNSW/IVF builda + responde `<=>` com recall@k em **paridade** (harness M2/M9), latência aceitável — medido, reproduzível em `docs/benchmarks/`.
- [ ] Se NÃO atingir paridade → ADR honesto mantendo pgvector index (anti-sunk-cost); o milestone entrega a medição, não uma regressão.

**Dependencies:** M20. **Risco:** ALTO (índice ANN é PhD-level); measurement-first é o guard-rail.

### M22 — [x] Escala/quantização própria em Rust (substituir pgvectorscale — gated)

**Objective:** Índice de escala + quantização **próprio em Rust** (alvo: DiskANN/SBQ-quality), substituindo
pgvectorscale — **somente** com paridade de recall **e** memória medida.

**Definition of done:**

- [ ] Índice próprio atinge paridade de recall + perfil de memória vs pgvectorscale (medido) OU ADR honesto mantendo pgvectorscale (anti-sunk-cost).

**Dependencies:** M21. **Risco:** MÁXIMO; o mais caro do v2. Measurement-first rigoroso.

### ~~M23 — Control plane em Go: operador K8s + CLI + gateway~~ (REMOVIDO 2026-07-03 — fora de escopo)

> **Removido do escopo (2026-07-03):** control-plane / operador K8s / CLI / gateway **não fazem parte do
> banco de dados** — este repositório é só o engine + a extensão. O diretório `operator/` (Go) foi apagado.
> Deploy/orquestração são responsabilidade de outra camada, não do TheoDB-engine.

### ~~M24 — Observabilidade + escala em Go (read pools, OTel/Prometheus, MCP)~~ (REMOVIDO 2026-07-03 — fora de escopo)

> **Removido do escopo (2026-07-03):** observabilidade/read-pools/MCP server eram **código Go do
> control-plane** (vivia em `operator/`, agora apagado). Não é o banco de dados. Se observabilidade voltar,
> entra como capacidade do engine (extensão), não como serviço Go externo.

---

### M25 — [x] Craft hardening do engine Rust (theodb_rs) — dívidas da auditoria de arquitetura

**Objective:** Fechar todos os achados MEDIUM/LOW de craft da auditoria FAANG
(`.claude/knowledge-base/audits/theodb_rs-architecture-verdict-2026-07-01.md`), **behavior-preserving**.

**Definition of done:**

- [ ] DRY: `sbq::rerank_dist` eliminado — `Metric::dist` widened p/ `pub(crate)` e reusado (single source).
- [ ] `nl_to_sql` (CCN 19) decomposto (`l2_validate` + `l4_validate_relations`, cada CCN < 10) + **testes Rust rápidos** da composição L2 (multi-statement, relação não-permitida) sem oráculo Python.
- [ ] `run_rrf` (84 NLOC): extrair `resolve_query_vector`; `sbq::knn` adota `Params` struct (remove `#[allow(too_many_arguments)]`).
- [ ] Magic numbers → consts (`http` timeout 30, ivf Lloyd 10); testes Rust p/ parsers puros de `chat`/`embed`.
- [ ] `lib.rs` (721 LoC) dividido: shims `#[pg_extern]` + `extension_sql!` movidos p/ junto do módulo; `lib.rs` vira module-map fino (padrão pgvectorscale=47/paradedb=192).
- [ ] Gate: `cargo clippy` limpo (sem novos `#[allow]`), 0 ciclos mantido, suíte verde no Docker.

**Dependencies:** M24. **Risco:** refactor em superfície de segurança (`nl_to_sql`) — mitigado por TDD (teste antes do extract) + suíte de paridade v1. **Nota:** puramente behavior-preserving.

---

### M26 — [x] Vector Index Access Method próprio (o gap SOTA — função → index engine)

**Objective:** Promover o ANN in-memory (rebuild-por-query) a um **Postgres Index Access Method real**
(`IndexAmRoutine`), fechando o único HIGH arquitetural da auditoria — paridade estrutural com
pgvector/pgvectorscale/vectorchord (todos AMs).

**Definition of done:**

- [ ] `IndexAmRoutine` registrado (`ambuild`/`aminsert`/`ambeginscan`/`amgettuple`/`amendscan`/`ambulkdelete`/`amvacuumcleanup`/`amcostestimate`) via pgrx (C-unwind guards, memory contexts, page/buffer).
- [ ] `CREATE INDEX ... USING theodb_hnsw (embedding …_ops)` persistido em páginas (não rebuild por query).
- [ ] Planner pushdown: `ORDER BY embedding <-> $1 LIMIT k` usa o índice (`amcanorderbyop` + `amcostestimate`), provado por `EXPLAIN`.
- [ ] Manutenção incremental: `INSERT`/`DELETE` mantêm o índice (sem rebuild total); `VACUUM` limpa.
- [ ] **Benchmark reproduzível** (measurement-first): recall@k ≥ paridade com a função atual + latência índice-persistido vs full-scan+rebuild; `docs/benchmarks/`.
- [ ] Coexistência com a função SQL-callable atual mantida (não quebra M20–M22).

**Dependencies:** M25. **Risco (ALTO):** superfície pgrx de baixo nível (FFI/longjmp/WAL) ainda não exercitada — competência que os peers Rust têm; mitigar com spikes de de-risk + estudo dos peers clonados. **Nota:** absorve o antigo deferral M21b.

---

### ~~M27 — Replicação streaming + read-pool real~~ (REMOVIDO 2026-07-03 — fora de escopo)
### ~~M28 — MCP write tools + auth (atrás do edge)~~ (REMOVIDO 2026-07-03 — fora de escopo)
### ~~M29 — Veredito de arquitetura do control plane (operator, Go)~~ (REMOVIDO 2026-07-03 — fora de escopo)

> **Removidos do escopo (2026-07-03):** replicação/read-pool provisionados por operador, write-tools de MCP
> atrás de um edge autenticador, e a auditoria do `operator/` (Go) são **control-plane / deploy / plataforma**,
> não o banco de dados. O `operator/` e o `ha/` foram apagados deste repositório; este repo é só o engine +
> a extensão. Replicação/HA no nível do Postgres, se voltarem, entram como capacidade do engine (não via
> operador K8s), num roadmap futuro.

---

### M30 — [x] Decisão de escopo v1-legacy: columnar (M6) + BM25 (M7) — ADR deprecar-ou-manter

**Objective:** Resolver via ADR se os pilares columnar (M6, `pg_mooncake`/`pg_duckdb`) e BM25 (M7,
`pg_textsearch`) — construídos sob a tese v1 de _composição_ — permanecem no norte v2 (código próprio, deps
mínimas) ou são deprecados. O `## Fora de escopo do v2` já exige "Reabrir exige ADR" para columnar — **este é
esse ADR**.

**Decisão (2026-07-03, CTO): MANTER os dois** como exceções permissivas (Regra 9), gated para adoção futura —
ADR [`0013-v1-legacy-columnar-bm25-scope`](docs/adr/0013-v1-legacy-columnar-bm25-scope.md).

**Definition of done:**

- [x] ADR `0013-v1-legacy-columnar-bm25-scope` (MADR 3.0): decisão = **MANTER** ambos, trade-offs + evidência + alternativa rejeitada (deprecar). *(ADR 0007 já estava ocupado; usado 0013.)*
- [x] Evidência de benchmark validando o KEEP: columnar-at-scale (`docs/benchmarks/m30-columnar-scale.md`) — columnstore vence o row-store **2.99× (100k) → 8.89× (1M) → 13.87× (5M) — mean±std, effect>variância**, correto (match) + `DuckDBScan`; fecha o gap UNBENCHMARKED do M6. BM25: nDCG 0.95 vs 0.51 (m7).
- [x] Nota de exceção permissiva no ROADMAP (§ Fora de escopo do v2 — Regra 9).
- [x] CHANGELOG + `## Relação com o v1` atualizados com a decisão.

**Dependencies:** — (decisão independente). **Risco:** decisão de produto/CTO; sem risco técnico. **Nota:** o
caminho de adoção (embarcar columnar: build PG17 OU bump PG18) é milestone futura — M30 é decisão + evidência.

---

## P0 — Track de superioridade vetorial (CTO GOTO, marcado 2026-07-01)

> **Prioridade máxima, ANTES de M27–M30.** O North Star (ADR 0002) pede **superioridade de performance
> vetorial comprovada por benchmark**. Hoje temos **recall-parity** (M20/M21/M22 medidos) mas **NÃO
> latência-superior**, e **zero head-to-head vs AlloyDB/ScaNN** (memória `goto-p0-vector-superiority`).
> Até fechar este track a honestidade é "paridade OSS + AI-native diferenciado", não "vetorialmente
> superior" (`.claude/rules/public-copy.md`). Estes três milestones rodam antes dos operacionais.

### M31 — [x] Otimização de latência do index AM (leitura parcial de páginas)

**Objective:** Fechar o gargalo O(N)-por-scan do AM do M26 (hoje deseraliza o blob inteiro por query, ADR
0010 §D2/D5) para que o índice persistido **bata a latência de query-time** — não só o rebuild-per-query
(já 16×), mas se aproxime/supere o seqscan maduro do pgvector no mesmo N.

**Definition of done (re-escopado por ADR 0011, CTO 2026-07-01 — o "≤ pgvector" migrou para M31b):**

- [ ] Scan lê **só as páginas necessárias** (meta/centroide + listas probed) — NÃO o blob inteiro por query (structured layout).
- [ ] **Benchmark reproduzível** (measurement-first): Index Scan do `theodb_ivfflat` **bem abaixo do regime O(N)** e **dentro de um band documentado do pgvector** (medido: ~45× vs M26 O(N); ~2.7× atrás do pgvector), recall@k mantido; `docs/benchmarks/m31-am-latency.{md,json}`.
- [ ] Sem regressão: `test_index_am.py` + coexistência M20–M22 verdes; manutenção incremental intacta.
- [ ] ADR 0010 §D2/D5 atualizado (O(N) fechado para IVFFlat, com número; SIMD-parity → M31b).

**Dependencies:** M26. **Risco (ALTO):** buffer/page FFI de leitura parcial. **Nota:** o `≤ pgvector` (paridade de latência) foi honestamente re-escopado para **M31b** — o gap residual é fator-constante (SIMD), não algorítmico.

### M31b — [x] Distância vetorial SIMD (AVX2 + runtime dispatch) — fechar o gap de latência vs pgvector

**Objective:** Fechar o resíduo de fator-constante do M31 (theodb ~2.7× atrás do pgvector por usar distância
escalar/SSE2 4-wide vs a SIMD AVX 8-wide + dispatch de CPU em runtime do pgvector C) — buscar **p50 ≤ pgvector**.

**Definition of done:**

- [ ] Distância l2/cosine/ip com SIMD (AVX2/AVX-512 quando disponível) + **dispatch de CPU em runtime** (crate portável tipo `wide`/`multiversion` OU intrinsics com feature-detect) — sem quebrar portabilidade (fallback escalar).
- [ ] `/deps-audit` da nova crate (CVE + licença D1 permissiva).
- [ ] **Benchmark reproduzível:** `theodb_ivfflat` Index Scan p50 **≤ pgvector** (n≥100k, dim≥128), recall mantido; `docs/benchmarks/`.
- [ ] Sem regressão: paridade de recall dos M20–M22 (a distância é reusada) preservada.

**Dependencies:** M31. **Risco (MÉDIO):** portabilidade do dispatch de CPU; paridade numérica f32 da distância SIMD vs a escalar (M20).

### M32 — [x] Harness de benchmark de escala (1M+ vetores, QPS head-to-head vs pgvector)

**Objective:** Produzir a evidência de escala que hoje é `UNBENCHMARKED` — QPS/recall/latência dos AMs
próprios vs pgvector em dataset real grande (SIFT1M / GloVe / deep1M), reproduzível.

**Definition of done:**

- [ ] Harness roda **≥ 1M vetores** (dataset público, ex. SIFT1M) contra o container; reusa `theodb_bench.recall`.
- [ ] Tabela QPS + p50/p95/p99 + recall@10 + build time + index bytes: `theodb_ivfflat`/`theodb_hnsw` **vs** pgvector `ivfflat`/`hnsw`, mean±std ≥3 runs, hardware citado; `docs/benchmarks/` + `.json`.
- [ ] Veredito honesto por knob (paridade / superior / inferior) — sem cherry-pick; ANN-Benchmarks semantics.

**Dependencies:** M31b (benchmarkar o AM com a distância SIMD já otimizada, senão a comparação de escala é injusta). **Risco (MÉDIO):** custo de infra/tempo do dataset grande; determinismo do QPS.

### M33 — [x] Head-to-head medido vs AlloyDB/ScaNN (o claim de superioridade)

**Objective:** Fechar o pilar do North Star — comparação medida vs o alvo SOTA (AlloyDB ScaNN, ou ScaNN
standalone se o acesso ao AlloyDB for bloqueado), produzindo o artefato reproduzível que sustenta (ou
refuta honestamente) o claim "igual ou superior ao AlloyDB no vetorial".

**Definition of done:**

- [ ] Benchmark vs AlloyDB ScaNN (ou ScaNN OSS) em dataset + hardware comparáveis, metodologia documentada (caveats de disk-backed vs in-memory explícitos, como manda `analysis-golden-rule`).
- [ ] Veredito: **SUPERIOR / PARIDADE / GAP** com número por dimensão (recall@k, QPS, latência, memória); `docs/benchmarks/` + `.json`.
- [ ] `public-copy.md`: só então um claim de performance vetorial vira permitido (com link ao benchmark) — ou fica marcado `UNBENCHMARKED`/`meta` se não alcançado.

**Dependencies:** M32. **Risco (ALTO):** acesso/reprodutibilidade do AlloyDB; comparar baselines comparáveis (honestidade científica).

---

### M34 — [x] theodb_ivfflat QPS a escala — lists/probes configuráveis (reloption + GUC)

**Objective:** Fechar a alavanca de QPS de MAIOR leverage que o M32 mediu (theodb_ivfflat ~8× atrás do pgvector a
1M): tornar `lists` (build) e `probes` (scan) **configuráveis** no `theodb_ivfflat`, hoje fixos em
`DEFAULT_LISTS=100`/`SCAN_PROBES=10`, que sub-particionam a 1M (escaneia ~100k candidatos vs ~10k do pgvector bem
tunado). Padrão comprovado do pgvectorscale (amoptions callback) + pgvector (GUC `ivfflat.probes`). Re-escopo
(2026-07-02): a 2ª alavanca original (scan estruturado do `theodb_hnsw`) foi movida para **M35** — a discovery mediu
que é ~3-4× o esforço/risco do M31 (reescrita de grafo page-native), grande demais para o mesmo ciclo sem re-trabalho.

**Definition of done:**

- [ ] `theodb_ivfflat` aceita `lists` (build) configurável via reloption `WITH (lists=N)` (pgrx `amoptions`) + `probes` (scan) via GUC `theodb_ivfflat.probes` (`GucRegistry`); o default preserva o comportamento atual (sem regressão nos gates M26/M31).
- [ ] Com tuning (lists≈√N, probes ajustável), `theodb_ivfflat` p50 **≤ pgvector** a 1M×128 (recall ≥ paridade), validado por re-run do harness M32 (`benchmarks/run_m32_sift1m.py` → `docs/benchmarks/`).
- [ ] Validação de bordas das opções (lists/probes fora do range → erro tipado, não crash); coexistência M20–M22 verde; benchmark reproduzível + veredito honesto (`public-copy.md`).

**Dependencies:** M32. **Sequência:** estrategicamente PRECEDE M33 (rodar antes do head-to-head vs AlloyDB). **Risco
(MÉDIO):** design da reloption pgrx (amoptions API + validação de bordas) + o build de índice ivfflat a 1M com
`lists` grande é single-thread (custo de tempo, não de correção).

---

### M35 — [x] theodb_hnsw scan estruturado (partial-read page-native, à la M31 para o grafo)

**Objective:** Eliminar o scan O(N) do `theodb_hnsw` (hoje desserializa o blob inteiro por query — ~6.5 GB / ~0.6 s
a 1M, `hnsw.rs:243`) com uma persistência **estruturada page-native**: tuplas por-nó (element) + por-camada
(neighbor) + entry-point na meta, travessia carregando só os nós visitados (o padrão do pgvector `hnsw.h`
HnswElementTupleData/HnswNeighborTupleData). Espelha o que M31 fez para o ivfflat, mas para o grafo.

**Definition of done:**

- [ ] `theodb_hnsw` persiste o grafo em páginas estruturadas (meta + element tuples + neighbor tuples), não um blob único; VACUUM/INSERT/DELETE intactos.
- [ ] O scan lê **O(ef·M) páginas não O(N)** (travessia on-demand); QPS ≥ ~50 a 1M (recall preservado), validado por re-run do harness M32.
- [ ] Integridade de grafo: entry-point fallback, refs stale tratadas, sem regressão de recall; coexistência M20–M22 verde; benchmark reproduzível.

**Dependencies:** M34 (reusa a infra de GUC/reloption + `ef_search` configurável). **Risco (ALTO):** ~3-4× o M31
(discovery 2026-07-02) — o grafo não é partição plana; codec element/neighbor + travessia validada a 1M são a fonte
de risco. Único milestone dedicado por design (evita re-trabalho de cramar no M34).

---

### M36 — [x] Otimização do scan do índice: heap top-K lazy (RE-ESCOPADO por medição)

**Objective:** Reduzir o custo de scan do índice atacando o gargalo `sort` **MEDIDO** (não o suposto). O gate
measurement-first do M36 (`THEODB_SCAN_PROFILE=1`, blueprint
`.claude/knowledge-base/discoveries/blueprints/m36-quantization-in-index-blueprint.md`) FALSIFICOU a premissa
original: a distância full-precision é **~15%** do custo de scan, não o gargalo. Os gargalos reais, estáveis em 3
pontos de probes, são **`reads` (I/O de página) ~44–51%** e **`sort` (ordenar TODOS os candidatos, `am/scan.rs`)
~35–41%**. M36 ataca o `sort` (win de recall-zero-risco); o `reads` foi separado no **M38** (quantização de I/O,
recall-risco — ADR-2: medir o heap antes de comprometer o risco maior). Contribui para o gap de ~25× vs ScaNN
medido no M33 (`docs/benchmarks/m33-scann-headtohead.json`) — honestamente uma fração via `sort`, NÃO "25× da
distância" (ADR-1 do blueprint). North Star: `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`.

**Definition of done:**

- [x] **Pré-requisito medido (concluído):** `THEODB_SCAN_PROFILE=1` mediu a divisão de fases — distância ~15%, reads ~44–51%, sort ~35–41% (200k×128, 3 pontos de probes). A premissa "distância domina" está falsificada; o milestone foi re-escopado para os gargalos reais.
- [x] **Sort → heap min lazy** (ADR-2, recall-zero-risco): `results.sort_by` sobre TODOS os candidatos → heap min lazy (heapify O(C) no `amrescan` + pop O(log C) no `amgettuple` = O(C+k·log C) em vez de O(C·log C)). Top-K **byte-idêntico** (mesma ordem total `(total_cmp, tid)`) → recall inalterado (provado por construção + `#[pg_test]` de ordering + 61 testes de coexistência). Fase sort caiu ~10–13× (profiler).
- [x] Benchmark reproduzível `docs/benchmarks/m36-scan-optimization.{md,json}` (`benchmarks/run_m36_scan.py`): end-to-end ~1.5× band (mean±std, recall idêntico), reconciliado com o teto de Amdahl (`sort` ~37–41% ⇒ teto ~1.5–1.7×). Honesto: fecha o `sort`, não o gap total do ScaNN.

**Dependencies:** M34 (infra reloption/GUC), M35 (scan estruturado). **Risco (BAIXO):** o heap é correção pura de
complexidade (zero risco de recall — top-K byte-idêntico). O `reads` (quantização de I/O, recall-risco) é o M38.

---

### M38 — [x] Investigação do gargalo `reads` (medido: sem lever recall-zero-risco viável — PQ é o real)

**Outcome (honesto, measurement-first):** o milestone entregou uma **MEDIÇÃO** que fechou 3 hipóteses, não um win
de QPS. Blueprint: `.claude/knowledge-base/discoveries/blueprints/m38-io-quantization-blueprint.md`; artefato:
`docs/benchmarks/m38-copy-free-scan.{md,json}`.

1. **SBQ falsificado (recall).** Em SIFT real o SBQ atinge só recall 0.77–0.95 vs 1.0 do baseline (quantização
   escalar perde ranking demais). O gate "recall preservado" do SBQ não é atingível — PQ seria o answer, mas é
   milestone grande (deferido).
2. **A cópia não é o gargalo end-to-end.** O profiler do M36 sugeria `reads` = 44% (dominado pela cópia dupla do
   `read_chunked`). Eliminada a cópia dupla (`read_page_item_into`, uma cópia), o profiler INTERNO caiu ~metade —
   **mas o end-to-end NÃO mostrou win confiável** (ratio 0.94–1.52 entre runs = ruído; efeito < variância de
   medição). Lição: a atribuição do profiler estava **inflada pelo overhead da própria instrumentação**
   (`Instant::now()`); a cópia não é o gargalo real.
3. **Byproduct entregue:** a eliminação da cópia dupla (`read_page_item_into`) é código estritamente melhor (menos
   alocação/tráfego de memória), **recall byte-idêntico** (61 testes de coexistência), merged como refactor de
   code-quality — SEM claim de QPS (o benchmark não sustenta).

**Conclusão:** nenhum lever de `reads` recall-zero-risco produz win end-to-end mensurável; fechar o gargalo
vetorial de verdade exige **quantização de produto (PQ + ADC via LUT — o algoritmo do ScaNN)**, que reduz
candidatos/bytes preservando recall. É PhD-level (codebooks + LUT SIMD + persistência + gate de recall) — um
milestone futuro grande, registrado aqui e no blueprint para quando o North Star exigir.

**Dependencies:** M31b, M34, M35, M36. **Resultado:** measurement + code-quality byproduct (não um win de QPS).

---

### M37 — [x] Sumarização de conteúdo (`ai.summarize`) — já entregue (correção de doc-drift)

**Outcome (honesto, measurement-first):** o milestone descobriu que a feature **JÁ ESTAVA IMPLEMENTADA E TESTADA**
— a auditoria anterior de `docs/features/` (que criou este milestone) foi **incompleta**: grepou só o Rust
(`theodb_rs/src/`) e perdeu a implementação em `sql/50-theodb-ai.sql`. Blueprint:
`.claude/knowledge-base/discoveries/blueprints/m37-ai-summarize-blueprint.md`.

O que já existe (M10 + M18):
- `ai.summarize(content text, model text DEFAULT NULL) RETURNS text` — plpgsql (`sql/50-theodb-ai.sql:32`) que
  chama o `ai._chat` **em Rust** (`theodb_rs/src/chat.rs`).
- `ai.agg_summarize(text)` — agregado que colapsa várias linhas num resumo (`sql/50-theodb-ai.sql:82`).
- **6 testes de contrato verdes** em `benchmarks/tests/test_ai_sql.py` (summarize escalar + agregado + negative
  cases + volatilidade).

**Trabalho do M37 (o único gap real):** `docs/features/11-sumarizacao-conteudo.md` atualizado de "📋 planejado" →
"✅ Entregue" com `file:line` + os 6 testes, validado por `deep-research/validate_citations.py` (PASS). NÃO foi
adicionado código Rust — seria um `ai.summarize` DUPLICADO (conflito). O grounding measurement-first evitou o
duplicado.

**Honestidade (Regra 3):** quando criei o M37, afirmei "genuinamente não implementada" com base num grep Rust-only.
Estava errado — a feature está em `sql/50`. M37 é uma correção de doc-drift, não código novo.

**Dependencies:** M10, M18 (a feature real). **Resultado:** correção de documentação + grounding que evitou um
duplicado.

---

### M39 — [x] Product Quantization (PQ+ADC) — medido: NÃO é o lever de QPS (SBQ_RETAINED)

**Outcome (honesto, measurement-first — 3º negativo da sequência M36/M38/M39):** construímos um `theodb.pq_knn`
próprio, std-only (k-means Lloyd por subespaço + ADC LUT), funcional e testado, e medimos head-to-head vs
`theodb.sbq_knn`. O gate D3 (anti-sunk-cost) deu **SBQ_RETAINED**: a paridade recall, PQ é **~5× mais lento** que
o SBQ. Blueprint: `.claude/knowledge-base/discoveries/blueprints/m39-pq-product-quantization-blueprint.md`;
artefato: `docs/benchmarks/m39-pq.{md,json}`.

1. **Paridade de recall, não vitória.** PQ 0.770 vs SBQ 0.769 (gap 0.001 = ruído); ambos limitados pelo IVFFlat a
   ~0.77 — **nenhum vence o f32 (recall 1.0)**. O gap que importa é vs f32 (0.23), não PQ-vs-SBQ.
2. **PQ ~5× mais lento** (QPS 352 vs 1828). Hamming (XOR/popcount) do SBQ é intrinsecamente rápido; o ADC do PQ
   precomputa LUT `m·k*` + k-means train por-chamada. Para o P0 (QPS/latência), é regressão.
3. **Ganho de memória real mas fora do alvo** (8 vs 32 vs 256 bytes/vetor) — o P0 é latência, não footprint.

**Decisão (D3):** NÃO faz merge como claim de superioridade; NÃO corta release. O gate parou PQ **antes** da cara
integração no index-AM. **Próximo lever (o gap real = recall):** anisotropic loss do ScaNN sobre o mesmo
esqueleto PQ (ataca recall, não QPS) — semente do M40.

**Dependencies:** M22 (SBQ), M34 (theodb_ivfflat), M38 (que apontou PQ). **Resultado:** measurement + um
`theodb.pq_knn` funcional; sem win de QPS, sem release.

---

### M40 — [x] Carrier head-to-head (theodb_hnsw vs theodb_ivfflat) — re-escopado da anisotropic loss

**Outcome (honesto, measurement-first — 5º da sequência M36/M38/M39/M40-ceiling/M40):** o milestone foi pedido como
"ScaNN anisotropic loss", mas a **sonda de teto** (`docs/benchmarks/m40-ceiling-probe.md`) falsificou a premissa
ANTES de construir: no nosso pipeline com rerank f32, o recall é limitado pelo **carrier (probes)**, não pelo
quantizer — a loss anisotrópica não moveria a agulha. Re-escopado (com aval do owner) para o head-to-head dos
carriers próprios. Artefato: `docs/benchmarks/m40-carrier.{md,json}`; harness: `benchmarks/run_m40_carrier.py`.

**Medição (n=50k synthetic):** `theodb_ivfflat` **vence** o trade-off recall×QPS — a QPS igual tem recall
substancialmente maior; o `theodb_hnsw` é **3–5× mais lento a recall igual** (headroom de otimização real no scan
page-native M35 vs o SIMD+heap do ivfflat). **Caveat honesto:** random-gaussian é o pior caso para grafo; o
veredito NÃO generaliza para dados reais estruturados a escala — o head-to-head confiável precisa de SIFT1M.

**Próximo (evidence-based):** (1) otimizar QPS do `theodb_hnsw` (é grafo, deveria ser mais rápido que probing);
(2) rodar este head-to-head em SIFT1M antes de qualquer claim de superioridade de carrier (`public-copy.md`).

**Dependencies:** M34 (theodb_ivfflat), M35 (theodb_hnsw), M39 (ceiling probe). **Resultado:** measurement +
harness reproduzível; theodb_ivfflat é o carrier mais forte nesta escala/dado; sem claim (precisa SIFT1M).

---

### M41 — [x] Otimização de QPS do scan theodb_hnsw (1.2–1.5× a recall idêntico, honesto)

**Outcome (WIN — 1º positivo após 5 negativos measurement-first):** o M40 apontou que o `theodb_hnsw` era 3–5× mais
lento que o `theodb_ivfflat` a recall igual. O discover (blueprint `m41-hnsw-qps`) identificou o gargalo no
`traverse` (`hnsw_page.rs`): custo fixo por-nó (`to_vec` alloc+memcpy + `RelationGetNumberOfBlocksInFork` ×2/nó),
enquanto o ivfflat amortiza o pin/lock sobre uma página inteira com SIMD. A correção pontua/decodifica cada nó
**dentro do pin** (`page::with_page_item`, sem cópia) e cacheia `nblocks` por query. Artefato:
`docs/benchmarks/m41-hnsw-qps.md`.

**Medição A/B rigorosa (n=50k, 4 amostras alternadas mean±std, recall byte-idêntico):** QPS **1.2–1.5×**, crescendo
com ef (ef=10: 1.24×; ef=100: 1.38×; ef=200: **1.46×** com bandas de std separadas → significativo). Recall inalterado
(0.313/0.617/0.809/0.911 idênticos). **Honestidade (Regra 3):** um run único cross-session sugeriu 2.4–3.0×, mas era
variância do CPU throttled (lição M38/M40); o número controlado é 1.2–1.5×. **Gate:** 8/8 `test_index_am.py` verdes.

**Próximo:** rodar em SIFT1M para o veredito confiável de carrier (theodb_hnsw agora competitivo).

**Dependencies:** M35 (theodb_hnsw), M40 (que mediu o gap). **Resultado:** otimização real, recall-preserving,
provada por A/B benchmark. Código de produto (Rust) — candidato a release.

---

### M42 — [x] Veredito de carrier em SIFT1M real (theodb_hnsw M41 vence — 1º sinal de superioridade vetorial)

> **⚠️ Retratado por M45 (2026-07-03):** o "~1.7–2.8× mais rápido que pgvector hnsw" abaixo era best-of-N +
> 200 queries + cache quente e **NÃO se reproduz** sob mean±std rigoroso (500 queries, ≥3 runs, GT exato) —
> veredito real **PARITY** (`docs/benchmarks/m45-pareto-sift1m.md`). theodb_hnsw é **competitivo, não
> superior** vs pgvector hnsw a 1M. Não citar os multiplicadores abaixo como claim.

**Outcome (WIN honesto em dados reais — inverte o synthetic do M40):** rodamos o head-to-head 4-way em **SIFT1M
real (1M×128, GT exato)** na imagem M41-otimizada. O M40 (synthetic random-gaussian) dava vitória ao ivfflat; em
dados **estruturados reais o grafo vence decisivamente**, exatamente como o caveat honesto do M40 previa. Sem
código novo (harness `run_m32_sift1m.py` existente). Artefato: `docs/benchmarks/sift1m-carrier-verdict.md` +
`m32-scale-sift1m.json`.

**Medição (best-of-3, GT exato):** `theodb_hnsw` 0.96 recall @ **278 QPS** vs `theodb_ivfflat` 0.98 @ **28.7 QPS**
→ **~10×**. E vs o **pgvector hnsw** (mesmo framework): ~1.7–2.8× mais rápido a recall igual (ef=40: 0.941/230 vs
0.926/133; ef=100: 0.987/143 vs 0.977/74). Curva Pareto completa no doc.

**Caveats honestos:** build do theodb_hnsw é lento (24min@1M — M41 otimizou o scan, não o build; próximo alvo);
QPS best-of-N single-machine (direção inequívoca, mas margem vs pgvector precisa mean±std + repro independente
antes de claim público — `public-copy.md`); amostra de 200 queries.

**Próximo:** (1) otimização de build-time do theodb_hnsw; (2) mean±std + repro do margin vs pgvector para claim.

**Dependencies:** M35 (theodb_hnsw), M41 (scan otimizado), M40 (que pediu SIFT1M). **Resultado:** 1º sinal real de
superioridade do carrier próprio vs a baseline SOTA permissiva (pgvector hnsw); sem código novo.

---

### M43 — [x] Otimização de build-time do theodb_hnsw (~2.2–2.9× via SIMD, recall paridade)

**Outcome (WIN honesto):** o M42 expôs o build do theodb_hnsw como gargalo (24min@1M). O discover achou a causa: o
build in-memory (`ann/hnsw.rs`) usava distância L2 **escalar** enquanto o scan já era **SIMD** — bilhões de
distâncias escalares em 128-dim. A correção adiciona `crate::vec::l2_distance_simd` (reusa o kernel AVX2+FMA M31b
via reinterpret f32→bytes) e roteia o build para ele (`Metric::dist_simd`), alinhando build e scan ao mesmo SIMD.
Artefato: `docs/benchmarks/m43-hnsw-build.md`; blueprint: `m43-hnsw-build-qps-blueprint.md`.

**Medição A/B rigorosa (3 samples @ 200k, mean±std):** build **2.20×** (m41 200±23s vs m43 91±3s, bandas
separadas), **recall IDÊNTICO** (0.9825=0.9825). @ **1M**: 24min → **8.4min** (~2.86× vs baseline M42), recall
paridade 0.9725. **Gate:** 8/8 `test_index_am.py` verdes.

**Nota:** `l2_distance` (paridade pgvector, operadores/scan-rerank/knn) intocado — só o build aproximado usa SIMD.

**Dependencies:** M35 (theodb_hnsw), M41 (kernel SIMD do scan), M42 (que mediu o gargalo de build). **Resultado:**
build-time real cortado ~2.2–2.9×, recall-preserving; o carrier próprio agora é competitivo em build, scan E recall×QPS.

---

### M44 — [x] Build PARALELO do theodb_hnsw (2.82× @50k / 1.95× @1M, recall paridade)

**Outcome (WIN honesto):** o M42/M43 mostrou o build como gargalo; o M43 cortou 2.2× via SIMD, o paralelismo era o
próximo teto. Discover: o build é CPU-bound + Rust puro (sem chamadas PG no loop) → paralelizável. Implementado com
`std::thread::scope` + `RwLock` por-nó (`ann/hnsw_parallel.rs`), despacho por threshold (small→sequential
determinístico; large→parallel). Sem nova dep. Artefato: `docs/benchmarks/m44-parallel-build.md`; blueprint no plano.

**Medição A/B (m43 sequential vs m44 parallel):** @50k **2.82×** (33±6s→12±3s, 3 samples back-to-back, bandas
separadas), recall paridade (Δ+0.0055). @1M: 8.4min→**4.3min** (1.95×), recall 0.9730. **Honesto:** o speedup
encolhe com escala (contenção de lock cresce); 2.82× é o número rigoroso controlado. **Lineage: 24min→8.4min→4.3min**.

**Custo honesto (Regra 3):** build NÃO-determinístico (racy insert; nenhum teste de determinismo quebra; recall
paridade é o gate). Race-freedom por construção (RwLock), deadlock-free (1 lock por vez), panic-safe (scope join).

**Dependencies:** M35 (theodb_hnsw), M43 (build SIMD). **Resultado:** build paralelo real, recall-preserving,
race-free; o carrier atinge build competitivo (12 cores). Próximo: redução de contenção (lock striping) se preciso.

---

### M46 — [x] theodb_hnsw scan hot-path hygiene — fechar o déficit de QPS no alto recall (recall-neutro, benchmark-gated)

**Objective (V2 — 1º milestone após o ROADMAP V1 completo):** o M45 (Pareto mean±std, SIFT1M) mediu PARIDADE
theodb_hnsw vs pgvector, com déficit no alto recall (0.58× a recall 0.9932, effect>variância) e variância de QPS
explodindo a ef≥200 (±44% vs pgvector ±1.7%). A discovery (3 council agents, código real + peers SOTA) achou a
causa: overhead per-query **acidental** que escala com ef — `visited: HashSet::new()` (SipHash, capacity 0 → ~12
rehashes a ef=200), heaps sem `with_capacity`, `Vec<Addr>` por nó (`hnsw_page.rs:518-520,200`). Complexidade
acidental que pgvector/pgvectorscale não pagam. Achado honesto: parte do "gap" é ruído de medição (ef400 mede mais
rápido que ef200 — impossível p/ custo real → dev box contendida). Fecha via pre-size + fast scratch, recall-neutro,
com re-medição rigorosa (measurement-first).

**Definition of done:**

- [ ] `traverse` (`hnsw_page.rs:518-520`) pre-size das 3 estruturas per-query (`with_capacity(ef*m0*2)` etc.,
  âncora pgvector `tidhash_create(ef*m*2)` + pgvectorscale `with_capacity(search_list_size*neigbors)`) +
  scratch `Vec<Addr>` reusado no ground loop (elimina alloc-por-nó). **Zero nova dep** (hasher default, rung 5).
- [ ] **Recall-neutro provado:** `traverse` retorna resultado byte-idêntico + `pages_read` idêntico antes/depois
  por seed fixa (teste-âncora `assert_eq!` + property `decode_neighbors_into`). Se divergir 1 tid, é BUG.
- [ ] Re-run do Pareto (`benchmarks/run_m46_highrecall.py`, median ≥5 runs + pages_read) → `docs/benchmarks/m46-*.{md,json}`;
  veredito honesto por effect>variância: theodb QPS ≥ pgvector a recall ≥0.993 OU honest-negative com variância
  a ef≥200 reduzida de ~44% p/ <15% (sem cherry-pick, `public-copy.md`).

**Dependencies:** M45 (o Pareto baseline). **Risco (MÉDIO):** ruído da dev box (mitigado por median + back-to-back +
pages_read determinístico). Blueprint: `.claude/knowledge-base/discoveries/blueprints/m46-hnsw-highrecall-qps-blueprint.md`.

---

## Remediação deep-view (2026-07-05) — M47–M55

> Origem: deep-view de trajetória (5 council agents sobre código real — vector-ann, ai-in-db,
> index-storage, benchmark, research-adr; grill: `knowledge-base/grills/deepview-remediation-feature-grill.md`).
> Veredito: **COURSE_CORRECTION_NEEDED** — fundações certas, esforço no asymptote errado. O gap de ~25× vs
> ScaNN (M33) é ~8–12× **quantização de scoring** (512 B/candidato f32 DRAM-bound vs 64 B AH em cache) +
> ~2–3× layout/page-reads + ~1.2–1.4× piso SQL; o piso estrutural inevitável de um AM Postgres é ~2–4×,
> não 25× — **o gap é quase todo endereçável**. Paridade com pgvector (M45) é o TETO da classe
> "f32-em-páginas" (pgvector maduro senta no mesmo ponto: 71.8 vs 77.9 QPS, M33). Ordem: correctness →
> calibrar a régua → mudar de asymptote (quantização) → filtered ANN → híbrida real → lifecycle AI.
> Sequencial gated: **M50 é GATE do M51** (anti-sunk-cost — se o Pareto calibrado mudar o diagnóstico, a
> aposta muda por ADR).

### M47 — [x] FU-1: micro-benchmark same-graph do ground-loop do HNSW scan (formaliza o plano em voo)

**Objective:** Medir de forma limpa (same-graph, box-noise-immune) o custo de alocação que o M46 remove —
o A/B de 2 containers foi invalidado por contenção de box (controle pgvector +122%) e por grafos diferentes
do build paralelo M44. Extrai o ground-loop para a camada pura `ann/scan_core.rs` (trait `NeighborSource` +
`ground_search<S>(…, presize)`; produção via `PageNeighborSource` em `am/hnsw_page.rs`, bench via
`MemNeighborSource`) e bencha `presized` vs `::new()` via criterion sobre grafo seeded fixo.
Plano: `.claude/knowledge-base/plans/fu1-samegraph-scan-microbench-plan.md` (`milestone_id: M47`).

**Definition of done:**

- [ ] `ann/scan_core.rs` PURO (zero `pg_sys`/`crate::` — invariante de link, blueprint Q5); `cargo bench`
  linka standalone sem runtime PostgreSQL.
- [ ] Guard de equivalência: o path benchado == oráculo `brute()` exato no grafo seeded (D3 do blueprint);
  oracle recall-neutro do M46 (`traverse_presize_is_recall_neutral_end_to_end`) verde no path de produção.
- [ ] Duas medições criterion (`presized`/`unsized`) sobre o MESMO grafo (`HnswIndex::build(seed=42)`,
  N≥50k) com CIs reportados; delta persistido em `docs/benchmarks/fu1-samegraph-scan-microbench.{md,json}`.
- [ ] Caveat EC-2 explícito no artefato: micro-bench sem I/O de página magnifica a fração de alocação —
  o delta é **UPPER BOUND** do ganho de produção, não o número de produção (`public-copy.md`).

**Dependencies:** M46. **Risco (BAIXO):** o delta pode ser pequeno (honest-negative aceito — fecha o
veredito M46 de qualquer forma). `criterion` é dev-only (nunca linka no cdylib).

### M48 — [x] Correctness & durabilidade do AM — fechar os furos de crash-safety (issues #46/#47)

**Objective:** O deep-view do AM achou dois furos que violam o invariante de crash-safety do projeto:
(a) **#46** — índice sobre tabela UNLOGGED quebra após crash/failover porque `GenericXLogStart` é no-op
quando `RelationNeedsWAL()==false` → INIT fork nunca vai ao WAL (`am/build.rs:259-276` → `page.rs:87-99`);
(b) **#47** — o rebuild-on-vacuum reescreve o índice in-place, meta (bloco 0) PRIMEIRO, um registro
GenericXLog por página (`page.rs:517-537` IVF; `hnsw_page.rs:387-406` HNSW) — crash mid-fold ⇒ estado misto;
pior caso, scan pontua bytes stale como vetores (**resultado silenciosamente errado**). Mais dois gaps
operacionais: pending nunca foldada em workload insert-only (`amvacuumcleanup` retorna cedo, `mod.rs:181-183`
→ scan paga O(pending) para sempre) e build paralelo não-cancelável (sem `CHECK_FOR_INTERRUPTS` no
`thread::scope`, `hnsw_parallel.rs:44-54`).

**Definition of done:**

- [ ] **#46 fechado:** `log_newpage_range()` forçado quando `forkNum == INIT_FORKNUM` independente de
  `RelationNeedsWAL` (padrão pgvector `hnswbuild.c:1137-1138` / `ivfbuild.c:1047-1048`); teste de regressão:
  UNLOGGED + `docker kill` (sem checkpoint) + restart → `INSERT` OK (não "truncated meta page").
- [ ] **#47 fechado via meta-pivot atômico:** geração nova escrita em páginas frescas (shadow), meta pivotada
  por ÚLTIMO em UM registro GenericXLog; páginas antigas reclamadas depois (FSM). Teste fault-injection:
  crash injetado entre escritas do fold → restart → scan consistente (geração antiga OU nova, nunca mista).
  **Restrição de design (anti-retrabalho M51):** o mecanismo meta-pivot é **layout-agnóstico** — camada de
  ciclo de vida de páginas, separada do serializer de tuples — para que o layout v3 do M51 troque só o
  serializer. Registrar no artefato o volume de WAL do shadow-rewrite (full-page images do índice inteiro
  por VACUUM — insumo do M55).
- [ ] Pending fold em `amvacuumcleanup` quando threshold excedido (workload insert-only deixa de degradar
  sem limite); teste: N inserts sem DELETE + VACUUM → pending foldada, scan O(ef·M).
- [ ] Build paralelo cancelável: `CHECK_FOR_INTERRUPTS` entre batches do leader (`pg_cancel_backend`
  interrompe `CREATE INDEX` em ≤ 1 batch).
- [ ] `amcostestimate` honesto (estilo `genericcostestimate` + `spc_random_page_cost`, âncora pgvector
  `ivfflat.c:86-116`/`hnsw.c:135-164`) — o planner compara índice vs seqscan de verdade (hoje: stub 0/0,
  `am/mod.rs:117-140`). **Nota de teste:** custo honesto → seqscan vence em N pequeno e isso é o resultado
  CORRETO; os testes de pushdown migram para assertar `EXPLAIN` em N realista, não em tabelas de brinquedo.

**Dependencies:** M47. **Risco (MÉDIO):** harness de fault-injection é novo (gdb breakpoint ou fault hook);
meta-pivot muda o ciclo de vida de páginas — REINDEX path e formato versionado já existem como precedente
(v1→v2). **Nota (fora deste milestone):** o muro estrutural VACUUM O(N)-em-RAM sob lock EXCLUSIVE
(`build.rs:196-221` + `lock.rs`) tem casa própria: **M55** (discover fold incremental vs manutenção
in-place à la pgvector) — não entra aqui para não inflar o DoD.

### M49 — [x] Opclasses cosine + inner-product no AM (o gap funcional que bloqueia RAG real)

**Objective:** Os dois AMs só registram L2 (`vector_l2_ops`, `am/mod.rs:192-213`; dispatch `scan.rs:169`).
Embeddings de produção (OpenAI/Cohere/BGE) usam **cosine/IP** — sem essas opclasses o índice não serve ao
workload que o README promete, e nenhum dataset realista (M50) pode ser medido. Atenção à mina já mapeada:
o `score()` do traverse só tem kernel fused para L2 — o caminho cosine/IP hoje alocaria `Vec<f32>` por nó
visitado (`am/hnsw_page.rs:436-447`).

**Definition of done:**

- [ ] `vector_cosine_ops` + `vector_ip_ops` registradas para `theodb_hnsw` e `theodb_ivfflat`
  (`amcanorderbyop`, strategy 1), com `CREATE INDEX … (embedding vector_cosine_ops)` + `EXPLAIN` provando
  pushdown do `<=>`/`<#>`.
- [ ] Kernel fused SIMD para cosine/IP no traverse — **zero alocação por nó** (mesmo contrato do L2;
  extensão de `vec.rs`, runtime dispatch AVX2 preservado).
- [ ] Paridade numérica + recall@10 vs pgvector nas mesmas métricas (harness existente, seed fixa);
  artefato `docs/benchmarks/m49-cosine-ip-opclasses.{md,json}`.
- [ ] Coexistência: suíte L2 (M45/M46) verde sem regressão; erros tipados para opclass×métrica inválida.

**Dependencies:** M48. **Risco (BAIXO-MÉDIO):** kernel IP/cosine SIMD é extensão direta do L2; decisão
normalizar-vs-computar documentada (âncora pgvector `vector_negative_inner_product`). **Caveat MIPS:**
inner product não é métrica (sem desigualdade triangular) — HNSW-sobre-IP funciona empiricamente
(precedente pgvector), mas o plano registra o caveat e o oráculo de paridade cobre IP explicitamente.

### M50 — [ ] Calibração da régua SOTA — pgvectorscale diskann + dataset realista + higiene de artefatos

**Objective:** Toda a evidência vetorial é UM dataset (SIFT1M/128d/L2), UM eixo (read-only, warm, 1 cliente),
numa box que já invalidou 3 medições (M41/M42/M46). E a comparação SOTA justa **nunca foi feita**: o
pgvectorscale `diskann` (StreamingDiskANN + SBQ — o SOTA permissivo apples-to-apples: mesmo processo, mesmo
buffer manager, mesmo piso SQL) **já está instalado na imagem** (`CREATE EXTENSION theodb CASCADE`). Sem
esta calibração, qualquer novo ciclo de otimização mira um asymptote desconhecido. GATE do M51.

**Definition of done:**

- [ ] Pareto M45 re-executado com **+1 spec: pgvectorscale `diskann`** (mesmo harness `run_m45_pareto.py`,
  mesmas queries seed 42, isolamento de índice) → posição honesta do SOTA-em-Postgres registrada.
- [ ] **+1 dataset realista de RAG, dimensionado pela MEMÓRIA da box** (HIGH-1 do review): o build do
  theodb materializa o corpus em RAM (`collect_corpus`, `am/build.rs:28-45`, sem teto) — 1M×1536d ≈ 6.1 GB
  só de corpus, ×3 builds (theodb/pgvector/diskann) numa box de ~15 GB. Escolha default: **cohere 768d×1M
  (~3 GB)** OU subset 250–500k @1536d com caveat explícito no artefato; 1M×1536d fica gated pelo streaming
  build (M55+). Métrica cosine (requer M49), GT exato; fronteira recall×QPS theodb_hnsw vs pgvector vs
  diskann → `docs/benchmarks/m50-sota-ruler.{md,json}`.
- [ ] **Primeiro artefato de QPS-de-banco** (G5/G6 do deep-view): sweep multi-cliente (8/16 conexões,
  p50/p95/p99, mesmo ponto de recall) + degradação de latência de scan com pending acumulada (N inserts
  pós-build, antes do fold) — theodb vs pgvector. QPS a 1 cliente é 1/latência, não throughput de banco.
- [ ] Protocolo de box quieta documentado e aplicado (load-guard **pré-flight contra carga EXTERNA**: medição
  aborta se load>N ou controle deriva >10% — lição M46); mean±std ≥3 runs, effect>variância obrigatório
  para qualquer veredito.
- [ ] Higiene de artefatos (G8 do deep-view): JSON bruto para m41/m43; banner de retração cross-ref em
  `m32-scale-sift1m.md`; nota `(superseded — ADR 0012)` no bullet do M31; números M30 CHANGELOG↔artefato
  reconciliados.
- [ ] Veredito escrito: onde o teto da classe atual está e se o lever do M51 (SBQ inline) continua sendo a
  aposta certa — **este parágrafo é o gate formal do M51**.

**Dependencies:** M47, M49. **Risco (MÉDIO):** box de dev contendida (mitigação: load-guard + janelas
quietas; se inviável, box dedicada vira pré-requisito registrado); download/GT de dataset 1536d é pesado.

### M51 — [ ] SBQ inline no AM — quantização no caminho quente (a aposta que muda de asymptote; GATED por M50)

**Objective:** O lever dominante do gap (~8–12×): hoje o scan pontua TODOS os ~50k candidatos em f32
full-precision (512 B/candidato a 128d → 25.6 MB/query, DRAM-bound; `am/hnsw_page.rs` traverse). O padrão
comprovado (pgvectorscale SBQ, licença PostgreSQL — D1 OK) é: códigos compactos DENTRO do índice, scoring
barato no hot path, rerank exato f32 só no top. Peças prontas: quantizador SBQ testado (M22 parity, 16×
compressão; `sbq.rs`), recall recuperável via rerank provado (0.947 @bits=4 em SIFT real, M38 + over_fetch
→ 1.000, M40). Os honest-negatives M39/M40 NÃO cobrem esta aposta: M39 mediu ADC *escalar* (não Hamming/LUT16
SIMD); M40 provou que quantização não move *recall* — ela existe para baratear o *scoring* (~10×), permitindo
mais ef ao mesmo custo. Impacto estimado 3–8× QPS a recall ≥0.99 (ESTIMATIVA — é exatamente o que este
milestone mede).

**Definition of done:**

- [ ] Códigos SBQ (4-bit, ~64 B a dim=128) inline nos element tuples do `theodb_hnsw` (layout v3, formato
  versionado + REINDEX path, precedente v1→v2).
- [ ] **Write path resolvido** (HIGH-2 do review): codebook/means SBQ persistidos em **meta pages**
  (precedente pgvectorscale); `aminsert` NÃO quantiza — a **pending permanece f32** (`append_pending`,
  `build.rs:122-143`; o scan da pending segue exato) e os códigos são gerados **no fold**; política de
  drift documentada (means fixados no build; advisory de REINDEX quando a distribuição muda).
- [ ] **Co-localização dos códigos dos vizinhos: decisão MEDIDA, não compromisso** (HIGH-3 do review):
  a 128d/4-bit, códigos no bloco de vizinhança custam ~2 KB/nó (m0≈32×64 B) → índice ~2–3× maior por
  ~1 read/nó a menos (`hnsw_page.rs:452-511`). Adotar SÓ se o benchmark index-size×reads×QPS mostrar
  effect>variância; senão, códigos apenas nos element tuples.
- [ ] Traverse pontua candidatos por **Hamming popcount SIMD** sobre os códigos; **rerank exato f32
  on-page** apenas no top `k·over_fetch` (over_fetch reloption/GUC, default medido).
- [ ] **Recall-gate D3-style:** recall@10 ≥ 0.99 preservado no Pareto M50 (SIFT1M E dataset realista);
  retenção SÓ se effect>variância — senão honest-negative + ADR mantendo f32 (anti-sunk-cost).
- [ ] Fronteira recall×QPS re-medida vs pgvector E diskann → `docs/benchmarks/m51-sbq-inline.{md,json}`;
  meta: mover a fronteira ≥2× a recall ≥0.99 vs pgvector (o M50 fixa o número exato do gate).
- [ ] **ADR keep/kill do AM próprio** (risco 4c do deep-view): critério registrado de quando o AM próprio
  deixa de valer a pena (ex.: se após este lever seguir ≤ pgvector+diskann no Pareto realista, reabrir a
  decisão de composição). O D3 tem cláusula de saída para forks; o AM próprio hoje não tem — este ADR fecha
  o gap de decision-record.
- [ ] Condicional (G4): se sobrar gap residual atribuível ao scoring, criterion bench LUT16 ADC vs Hamming
  (1M candidatos, mesmo codebook) — só DEPOIS do veredito principal.

**Dependencies:** M50 (GATE — o veredito escrito do M50 autoriza ou re-escopa esta aposta). **Risco (ALTO):**
mudança de formato on-disk + calibração de over_fetch (recall); mitigado pelo gate D3 e pelo REINDEX path.

### M52 — [ ] Filtered ANN planner-integrado — `WHERE … ORDER BY embedding <=> $1` com recall preservado

**Objective:** O gap CRÍTICO vs o campo: filtered vector search é o flagship do AlloyDB (adaptive filtering),
está no pgvector 0.8 (iterative index scans) e no pgvectorscale (label filtering) — e no TheoDB tem **zero
milestone, zero blueprint** (PRD P2 lista como [SHOULD] e nunca executou). É o único wedge *técnico* coerente
com a tese de unificação (vetor+relacional no mesmo engine): HNSW ingênuo colapsa de recall sob filtro
seletivo — resolver isso planner-integrado é diferencial genuíno e citável. Cobertura de benchmark hoje: zero.

**Definition of done:**

- [ ] Discover primeiro (blueprint): AlloyDB adaptive filtering, pgvector 0.8 iterative scans, ACORN
  (predicate-agnostic HNSW), pgvectorscale labels — estratégia escolhida por ADR com alternativas.
- [ ] Estratégia implementada no `theodb_hnsw` (ex.: iterative scan — continuar a busca até k tuplas
  passarem o recheck do executor; ou pushdown de bitmap de visibilidade) — `EXPLAIN` provando índice usado
  sob `WHERE`.
- [ ] **Benchmark de seletividade** (1% / 10% / 50%): recall@10 + QPS vs pgvector iterative scan, mesmo
  dataset do M50 → `docs/benchmarks/m52-filtered-ann.{md,json}`. Recall sob filtro seletivo ≥ paridade
  pgvector 0.8.
- [ ] Zero regressão no path unfiltered (suíte M45/M50 verde; effect>variância).

**Dependencies:** M51. **Risco (ALTO):** interação com executor/planner (recheck, rescan) é o território
mais sutil da Index AM API; mitigação: discover + âncora nos 2 designs OSS existentes (pgvector iterative,
pgvectorscale labels).

### M53 — [ ] Híbrida de verdade — WHERE relacional + leg BM25 + benchmark BEIR real

**Objective:** Três dívidas da superfície híbrida: (a) o `FUSION_TEMPLATE` (`hybrid.rs:24-46`) **não aceita
filtro WHERE** — a própria tese de unificação não está exposta na API (`api.rs:432-449` sem parâmetro de
predicado); (b) o leg lexical é `ts_rank_cd`, que perdeu de 0.5143 para **0.9546** (nDCG@10) do BM25
(`m7-bm25-vs-tsrank.md`) — medido superior e não shipado (o gate de adoção do `pg_textsearch` da exceção
ADR 0013 nunca rodou); (c) o claim de recall da híbrida **não tem artefato** — a fixture do m7 tem 12 docs
e o próprio doc diz "not decision-grade"; o follow-up BEIR real está aberto desde 2026-06-28.

**Definition of done:**

- [ ] Parâmetro de filtro relacional na fusão (`ai.hybrid_search_rrf(…, filter_sql)`). **Frame de segurança
  honesto (review):** o predicado executa **com o privilégio do chamador** (SECURITY INVOKER — nenhuma
  fronteira de privilégio é cruzada); a garantia do template é **confinamento sintático** (o filtro não
  escapa do WHERE da CTE — mesma disciplina `%I`/parametrização atual). Composição com M52 quando o leg
  vetorial usa índice.
- [ ] Gate de adoção do BM25 executado: leg lexical `pg_textsearch` (BM25) opt-in com fallback `ts_rank_cd`
  preservado; decisão registrada (executa a exceção ADR 0013 — não reabre out-of-scope).
- [ ] **Benchmark BEIR real** (scifact/nfcorpus, embedder real via `.env` de teste, nDCG@10 + recall@100):
  híbrida vs vector-only vs BM25-only → `docs/benchmarks/m53-hybrid-beir.{md,json}` — a híbrida só vira
  claim com este artefato (`public-copy.md`).
- [ ] Idioma do `to_tsvector`/`plainto_tsquery` parametrizável (hoje 'english' hard-coded, `hybrid.rs:34-37`).

**Dependencies:** M19 (híbrida existente), M50 (protocolo de medição). **Risco (MÉDIO):** `pg_textsearch`
na imagem (packaging); embedder real no CI (custo/flakiness — mitigação: subset fixo + cache de embeddings
no fixture).

### M54 — [ ] Vectorizer próprio — auto-embedding declarativo (o lifecycle que define "AI-native" em 2026)

**Objective:** O campo definiu a categoria pelo lifecycle, não pelas funções per-row: pgai vectorizer
(Timescale), Supabase automatic embeddings, AlloyDB — embedding gerado/atualizado em INSERT/UPDATE,
declarativamente. No TheoDB **não existe nada disso** (zero triggers em `sql/`; ADR 0007 fixou per-row
síncrono para a *função* `embed`, não para manutenção de coluna): todo embedding é mantido pela aplicação —
é a lacuna que um dev de RAG sente no minuto 5. Requer revisitar o ADR 0007 por ADR novo (async permitido
para o vectorizer; a função `embed` continua síncrona).

**Definition of done:**

- [ ] Discover (blueprint): padrão pgai vectorizer + Supabase (trigger → fila → worker) mapeado; decisão de
  worker (background worker pgrx vs cron/externo) por ADR com alternativas — o ADR revisita o 0007
  explicitamente.
- [ ] `theodb.create_vectorizer(table, source_col, embedding_col, model)` declarativo: trigger AFTER
  INSERT/UPDATE enfileira (tabela de jobs com estados tipados: pending/processing/done/failed + tentativas);
  worker consome em batch via `embed_batch` existente (`embed.rs:55-124`) — retry bounded reusa `http.rs`.
- [ ] Chunking helper SQL (recursive character split, tamanho/overlap configuráveis) — suficiente para v1;
  chunking avançado fica para depois (YAGNI).
- [ ] Crash-safety da fila: job em `processing` com owner morto volta a `pending` (visibility timeout);
  teste e2e contra stub determinístico: INSERT → embedding aparece; UPDATE → re-embed; falha de endpoint →
  retry bounded + estado `failed` tipado (nunca swallow).
- [ ] Métrica de runtime (wiring triad): contador de jobs processados/falhados consultável (não só LOG).

**Dependencies:** M19. **Risco (ALTO):** background worker em pgrx é território FFI novo (ciclo de vida,
sinais, shutdown limpo); fila crash-safe tem sutilezas de visibilidade — mitigação: discover primeiro,
estados tipados, e o worker nunca segura transação do usuário (async por design).

### M55 — [ ] Decisão: manutenção do índice a escala — fold incremental vs in-place (o muro do VACUUM)

**Objective (milestone-decisão, precedente M30):** o desenho deliberado do ADR-1/M35 (grafo imutável +
rebuild total no VACUUM) tem um muro estrutural: o fold materializa o corpus O(N) em RAM
(`build.rs:196-221`) **sob advisory lock EXCLUSIVE** (`lock.rs`) — a escala North-Star (1M+×768d), isso
significa GBs de RAM e **parada total de queries vetoriais durante o VACUUM** (e uma transação longa com
lock SHARE bloqueia o VACUUM indefinidamente). pgvector faz manutenção in-place por página, sem cliff. O
mesmo mecanismo limita o **build** (`collect_corpus` sem teto — o gate de dataset do M50). É dívida
classe-bloqueador para qualquer claim de produção; a decisão precisa de discover próprio, não de um fix
apressado.

**Definition of done:**

- [ ] Discover (blueprint): fold incremental (gerações parciais sobre o meta-pivot do M48) vs manutenção
  in-place à la pgvector (`hnswvacuum.c`) vs híbrido (in-place para DELETE, fold para compaction) —
  trade-offs com evidência dos peers (pgvector, pgvectorscale).
- [ ] Medição do estado atual como baseline: RAM de pico + duração do lock EXCLUSIVE no fold a 100k/500k/1M
  (SIFT + 768d) + volume de WAL (insumo já coletado no M48) → `docs/benchmarks/m55-vacuum-wall.{md,json}`.
- [ ] **ADR (MADR 3.0)** com a decisão, alternativas rejeitadas e o plano de milestone(s) de implementação;
  inclui o teto de memória do BUILD (`collect_corpus` streaming ou batched) no escopo da decisão.
- [ ] Trigger registrado: a implementação da decisão é **pré-requisito de qualquer claim v1.0/produção**
  (`public-copy.md` § 3) — o ADR fixa isso explicitamente.

**Dependencies:** M48 (o meta-pivot é a fundação de qualquer fold incremental; o volume de WAL medido lá é
insumo). **Risco (BAIXO — é decisão+medição, não implementação):** a implementação decorrente será ALTO e
ganha milestone próprio via `/roadmap-feature` após o ADR.

> **Insumo do M48 já disponível (2026-07-05):** o volume de WAL do fold shadow-write está medido em
> `docs/benchmarks/m48-am-maintenance.{md,json}` — ~12,3 MB para reescrever um índice de 50k (mean±std de 3
> runs); é exatamente esse custo que o fold incremental desta milestone busca reduzir.

---

## Sequência e paralelismo

```
M17 (fundação Rust) ──▶ M18 (ai.*) ──▶ M19 (nl/híbrida/import — fim do plpython3u)
   │
   └──▶ M20 (tipo vetorial) ──▶ M21 (índice HNSW/IVF, gated) ──▶ M22 (escala/quantização, gated)
                                                                      │
M19 ──────────────────────────────────────────────▶ M23 (control plane Go) ──▶ M24 (observabilidade Go)
```

- M18→M19 elimina `plpython3u` (independência da camada IA).
- M20→M22 reduz/elimina `pgvector`/`pgvectorscale` — **cada passo gated por paridade medida** (sem regressão).
- M23→M24 (Go) podem começar após M19 (o banco próprio já coeso).
- **M26 ──▶ M31 (leitura parcial, O(N) fechado) ──▶ M31b (distância SIMD) ──▶ M32 (escala/QPS) ──▶ M33 (head-to-head AlloyDB)** — o **track P0** de
  superioridade vetorial roda **antes** de M27–M30 (operacionais). É o pilar do North Star ainda não fechado.
- **Remediação deep-view (2026-07-05): M47 (FU-1 régua) ──▶ M48 (correctness #46/#47) ──▶ M49 (cosine/IP) ──▶ M50 (calibração SOTA) ══GATE══▶ M51 (SBQ inline — muda de asymptote) ──▶ M52 (filtered ANN) ──▶ M53 (híbrida real) · M54 (vectorizer, deps M19) · M55 (decisão VACUUM-wall, deps M48)** —
  correctness antes de performance; nenhuma aposta grande sem a régua calibrada (anti-sunk-cost). Emendas
  do review de engenharia de BD (2026-07-05) absorvidas nos DoDs: dataset-por-memória e QPS-multi-cliente
  (M50), write-path SBQ e co-localização-medida (M51), meta-pivot layout-agnóstico (M48), M55 criado.

## Gate de dependências (transversal — o pedido "depender o menos possível")

- Toda nova crate Rust / módulo Go passa por `/deps-audit` (CVE) + gate de licença D1 (permissiva).
- Regra: **stdlib/pgrx/native-jsonb first**; crate externo só quando reescrever seria reinventar a roda
  (Regra 9). Cada dep registrada com justificativa (ADR curto) — "por que não stdlib".
- Substituir terceiros (pgvector/pgvectorscale/plpython3u) por código próprio é o **objetivo**; substituir
  utilitários maduros (HTTP/serde) por código caseiro é **anti-objetivo** (complexidade acidental).

## Relação com o v1

- `ROADMAP.md` (v1): M0–M16 entregues (distribuição-composição). Permanece como histórico + base funcional
  (os testes do v1 são a **prova de paridade** da reescrita do v2).
- ADRs: `0006` é o norte; `0001` núcleo mantido; `0002/0004/0005` supersedidos/reabertos em parte (ver notas).
- **Columnar (M6) + BM25 (M7)** — os dois pilares v1 de composição foram **mantidos** (M30 / ADR `0013`,
  2026-07-03) como exceções permissivas (Regra 9), com evidência medida (columnar ~14× a 5M (mean±std); BM25 nDCG 0.95 vs
  0.51). Gated para adoção; o leg lexical shipado segue o `ts_rank_cd` nativo.
