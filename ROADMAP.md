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
