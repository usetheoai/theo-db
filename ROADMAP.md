# TheoDB — Roadmap (banco real, Postgres-based, código próprio Rust/Go)

> **Este é o roadmap ATIVO** — o path convencional (`ROADMAP.md`) que o cycle-kit lê e o `cycle-release` flipa.
> Origem: **ADR `0006-own-code-postgres-based-rust-go`** (virada de mandato "v2", sign-off CTO, 2026-06-29).
> Substituiu o antigo roadmap v1 (tese de composição, M0–M16 entregues como distribuição), agora arquivado em
> [`docs/history/ROADMAP-v1.md`](docs/history/ROADMAP-v1.md) como histórico do que foi provado + base de paridade.
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
2. **Código próprio:** Rust (`pgrx`) para o que roda *dentro* do engine (tipos, funções, índices); Go para o
   que roda *fora* (operador K8s, CLI, gateway, control plane).
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

### M23 — [x] Control plane em Go: operador K8s + CLI + gateway

**Objective:** Construir a camada de produto/operação em **Go** (código próprio): operador Kubernetes (modelo
cloudnative-pg), CLI, gateway — o que torna TheoDB deployável/gerenciável (caminho para managed).

**Definition of done:**

- [ ] Operador K8s provisiona/gerencia um cluster TheoDB (CRD + reconciliation); CLI; deploy reproduzível.
- [ ] Código próprio Go com testes; sem dep externa além do ecossistema K8s padrão.

**Dependencies:** M19 (banco próprio coeso). **Nota:** absorve o antigo v1-M5.

### M24 — [x] Observabilidade + escala em Go (read pools, OTel/Prometheus, MCP)

**Objective:** Observabilidade e escala de leitura em **Go**: métricas OTel/Prometheus, read pools, MCP server.

**Definition of done:**

- [ ] Métricas runtime expostas (Prometheus/OTel); read pools; MCP server — código próprio Go com testes.

**Dependencies:** M23. **Nota:** absorve o antigo v1-M8.

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

### M27 — [ ] Replicação streaming + read-pool real

**Objective:** Dar significado real ao read-Service `<name>-ro` do M24: replicação streaming Postgres
(primary + réplicas) + roteamento de leitura para réplicas (o read-scale que hoje é só endpoint-level).

**Definition of done:**

- [ ] Operador provisiona réplicas com replicação streaming (primary + N réplicas, slots/`pg_basebackup`).
- [ ] Read-Service `<name>-ro` seleciona só pods réplica — read-scale real, não pods independentes.
- [ ] Promoção de réplica (failover) integrada ao `ha/` (Patroni já existe) OU decisão honesta de deferir.
- [ ] Read-pool: ADR PgBouncer (cnpg Pooler) vs Service L4.
- [ ] Evidência real-cluster (kind): réplica recebe writes do primary; read no `-ro` retorna dado replicado.

**Dependencies:** M23, M26. **Risco:** replicação é fonte clássica de bugs de consistência — testes de lag/split-brain. **Nota:** absorve o deferral M24 ADR-2 (read-pool real).

---

### M28 — [ ] MCP write tools + auth (superfície de agente mutável, atrás do edge)

**Objective:** Estender o MCP server (M24, read-only) com write tools protegidos por auth — a superfície
mutável que o M24 ADR-3 deferiu por precisar da história de auth primeiro.

**Definition of done:**

- [ ] Tools `apply_cluster` / `delete_cluster` (write) com validação de input + typed errors.
- [ ] Auth: `-http` deixa de ser unauthenticated — integra o edge autenticador (Traefik ForwardAuth / Model B, padrão theo-memory) OU exige token; stdio segue p/ spawn local confiável.
- [ ] RBAC least-privilege: verbos de write na SA do MCP só com auth presente.
- [ ] Testes: write tool cria/deleta CR real (envtest); tool sem auth no `-http` → 401; toda mutação logada.

**Dependencies:** M24. **Risco (segurança):** IA que muta estado de cluster — edge autenticador obrigatório antes de expor (CWE-441/Model-B do theo-data). **Nota:** absorve o deferral M24 ADR-3.

---

### M29 — [ ] Veredito de arquitetura + hardening do control plane (operator, Go)

**Objective:** Rodar a mesma auditoria FAANG de 7 dimensões no `operator/` (Go) que rodou no `theodb_rs`,
e fechar achados de craft — fechando o veredito dos dois codebases.

**Definition of done:**

- [ ] Auditoria de arquitetura do `operator/` (estrutura/naming/SOLID/coupling+ciclos/patterns) com métricas medidas (gocyclo/gocognit) + comparação SOTA (cloudnative-pg).
- [ ] 0 ciclos; findings HIGH/MEDIUM fechados ou com ADR de aceite; relatório em `.claude/knowledge-base/audits/`.
- [ ] Gate mantido: `golangci-lint` 0, `deadcode` none, `make test` verde.

**Dependencies:** M24. **Risco:** baixo — operator já passou por 12 agentes nos ciclos M23/M24; provável PASS com poucos ajustes. **Nota:** fecha o veredito FAANG dos dois engines (Rust + Go).

---

### M30 — [ ] Decisão de escopo v1-legacy: columnar (M6) + BM25 (M7) — ADR deprecar-ou-manter

**Objective:** Resolver via ADR se os pilares columnar (M6, `pg_mooncake`/`pg_duckdb`) e BM25 (M7,
`pg_textsearch`) — construídos sob a tese v1 de _composição_ — permanecem no norte v2 (código próprio, deps
mínimas) ou são deprecados. O `## Fora de escopo do v2` já exige "Reabrir exige ADR" para columnar — **este é
esse ADR**.

**Definition of done:**

- [ ] ADR `0007-v1-legacy-columnar-bm25-scope` (MADR 3.0): manter / deprecar-e-remover / reescrever-próprio, com trade-offs + evidência.
- [ ] Se **deprecar**: plano de remoção com trilha (CI jobs `columnar-measure`/`ai-sql`-bm25, Dockerfiles throwaway, superfície SQL, docs) — como ciclo próprio, não delete solto.
- [ ] Se **manter**: nota explícita no ROADMAP de que columnar/bm25 são exceção permissiva ao mandato own-code (justificativa Regra 9).
- [ ] CHANGELOG + `## Relação com o v1` atualizados com a decisão.

**Dependencies:** — (decisão independente; pode rodar em paralelo). **Risco:** decisão de produto/CTO; sem risco técnico. **Nota:** alinha com `## Fora de escopo do v2` ("Columnar próprio… Reabrir exige ADR").

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

### M36 — [ ] Otimização do scan do índice: sort top-K + I/O quantizada (RE-ESCOPADO por medição)

**Objective:** Reduzir o custo de scan do índice atacando os gargalos **MEDIDOS** (não os supostos). O gate
measurement-first do M36 (`THEODB_SCAN_PROFILE=1`, blueprint
`.claude/knowledge-base/discoveries/blueprints/m36-quantization-in-index-blueprint.md`) FALSIFICOU a premissa
original: a distância full-precision é **~15%** do custo de scan, não o gargalo. Os gargalos reais, estáveis em 3
pontos de probes, são **`reads` (I/O de página) ~44–51%** e **`sort` (ordenar TODOS os candidatos, `am/scan.rs:109`)
~35–41%**. M36 ataca esses dois. Contribui para o gap de ~25× vs ScaNN medido no M33
(`docs/benchmarks/m33-scann-headtohead.json`) — honestamente uma fração via sort+reads, NÃO "25× da distância"
(ADR-1 do blueprint). North Star: `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`.

**Definition of done:**

- [x] **Pré-requisito medido (concluído):** `THEODB_SCAN_PROFILE=1` mediu a divisão de fases — distância ~15%, reads ~44–51%, sort ~35–41% (200k×128, 3 pontos de probes). A premissa "distância domina" está falsificada; o milestone foi re-escopado para os gargalos reais.
- [ ] **Sort → heap top-K limitado** (ADR-2, primeiro slice, zero risco de recall): substituir `results.sort_by` sobre TODOS os candidatos (`am/scan.rs:109` ivf + `:188` hnsw-pending) por um heap/partial-sort de tamanho K → O(C·log K) em vez de O(C·log C). Resultado top-K **idêntico** (o heap não muda o ranking, só o custo). Ganho medido via `THEODB_SCAN_PROFILE` + benchmark.
- [ ] **Reads → códigos quantizados menores no scan + rerank f32** (reusa `theodb_rs/src/sbq.rs`, M22): persistir códigos SBQ nas páginas de lista (`am/page.rs` + build) para cortar bytes/candidato lidos (16 B vs 512 B em dim=128), pontuar por Hamming/assimétrico, rerank f32 do top over_fetch. **Recall preservado (≥ baseline no ponto casado)** como gate; se SBQ-1bit regredir, escalar via ADR.
- [ ] Benchmark reproduzível `docs/benchmarks/m36-scan-optimization.{md,json}` (reusa `benchmarks/theodb_bench/`), mostrando o ganho de QPS medido a recall preservado vs o baseline pré-M36, e quanto do gap do M33 fecha — honesto (sort+reads, não distância). Delta medido, não suposto.

**Dependencies:** M22 (`sbq.rs`), M34 (infra reloption/GUC), M35 (scan estruturado). **Risco (MÉDIO):** o heap
top-K é correção pura de complexidade (zero risco de recall); a quantização de I/O tem risco de recall (SBQ-1bit
teta ~0.86 no protótipo) — mitigado por rerank f32 + gate de recall, entregue como segundo slice após o heap ser
medido. Deltas de QPS **UNBENCHMARKED** até o `m36-*.json` existir.

---

### M37 — [ ] Sumarização de conteúdo (`ai.summarize`) — fechar a última feature documentada ausente

**Objective:** Entregar a sumarização de conteúdo via SQL — a única feature em `docs/features/` genuinamente NÃO
implementada (`docs/features/11-sumarizacao-conteudo.md`; nenhuma função `summarize` no código hoje). Espelha
exatamente o padrão já entregue de `ai.analyze_sentiment` / `ai.rank` (`theodb_rs/src/chat.rs`, modelo síncrono
por-linha via LLM, ADR `docs/adr/0007-synchronous-per-row-model-http.md`).

**Definition of done:**

- [ ] Função `ai.summarize(content text, model text DEFAULT NULL) RETURNS text` (superfície SQL em `theodb_rs/src/api.rs`, lógica em `theodb_rs/src/chat.rs`, espelhando `ai_sentiment`/`ai_rank`), com erro tipado em saída malformada.
- [ ] Teste de contrato em `benchmarks/tests/test_ai_sql.py` (happy path + negative case de saída malformada → erro tipado), no padrão dos testes de sentiment/rank.
- [ ] `docs/features/11-sumarizacao-conteudo.md` atualizado de "📋 planejado" → "✅ Entregue" com `file:line` + teste (validado por `deep-research/validate_citations.py`). **Nota de honestidade:** qualidade depende do LLM configurado; sem benchmark de qualidade de sumarização.

**Dependencies:** M18 (superfície `ai.*` + `chat.rs` existem). **Risco (BAIXO):** é uma cópia estrutural de
`ai.rank`/`ai.analyze_sentiment` já entregues — sem novo mecanismo, só um novo prompt + parse.

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
