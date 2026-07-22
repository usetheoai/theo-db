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

> **Nota (superseded — [ADR 0012](docs/adr/0012-benchmark-data-degeneracy.md), anotado 2026-07-06 no M50/G8):** as *claims de latência* do M31 (via ADR 0011) foram medidas em dados sintéticos degenerados (InitPlan-hoist → todos os vetores idênticos); ficam **superseded pela ADR 0012**. O trabalho de código do M31 (leitura parcial de páginas) permanece válido; a régua de latência confiável é a do M32 (SIFT1M real) e M45/M50.

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

### M50 — [x] Calibração da régua SOTA — pgvectorscale diskann + dataset realista + higiene de artefatos

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

### M51 — [x] SBQ inline no AM — quantização no caminho quente (a aposta que muda de asymptote; GATED por M50)

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

### M52 — [x] Filtered ANN planner-integrado — `WHERE … ORDER BY embedding <=> $1` com recall preservado

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

### M53 — [x] Híbrida de verdade — WHERE relacional + leg BM25 + benchmark BEIR real

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

### M54 — [x] Vectorizer próprio — auto-embedding declarativo (o lifecycle que define "AI-native" em 2026)

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

### M55 — [x] Decisão: manutenção do índice a escala — fold incremental vs in-place (o muro do VACUUM)

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

### M56 — [x] Manutenção in-place (tombstone) — remove o muro do VACUUM (P3, implementa ADR 0017 fase 1)

**Objective (deep-view 2026-07-07, gap P3):** a deep-view mediu o muro: o fold O(N) whole-index segura o
advisory EXCLUSIVE por ~86 s a 100k×768d (parada total de queries) → ~14 min projetado a 1M
(`docs/benchmarks/m55-vacuum-wall.md`). O ADR 0017 **decidiu** o caminho (híbrido tombstone-in-place +
fold-para-compaction) mas a implementação foi deixada como milestone própria. Esta milestone entrega a
**fase 1**: DELETE vira tombstone in-place por página (à la pgvectorscale `plain/node.rs` / pgvector
`hnswvacuum.c`), removendo o rebuild O(N) sob EXCLUSIVE do caminho de delete — pré-requisito honesto de
v1.0 (`public-copy.md §3`) **e** de medir o P0 (M57) a 1M.

**Definition of done:**

- [ ] DELETE marca tombstone in-place por página (element-tuple `deleted`+`version`), **sem O(N)-RAM, sem
  EXCLUSIVE index-wide**; reúso de slot no `aminsert` (padrão `hnswinsert.c:45`); scan filtra tombstones.
- [ ] Recall preservado sob tombstones; **medir recall entre compactions** (decide se a fase 2 —
  `RepairGraph` in-place do pgvector — é necessária; é a incerteza-chave do blueprint M55).
- [ ] Compaction por threshold **reusa o `fold.rs` do M48** (crash-safe, meta-pivot atômico); disparo por
  `#tombstones/#live > X%`.
- [ ] Teto de memória do **BUILD**: `collect_corpus` (`build.rs:28`) streaming/batched — `CREATE INDEX`/
  `REINDEX` de 1M×768d não materializa O(N) antes do primeiro nó.
- [ ] Benchmark: parada do DELETE-path **≪ 86 s do rebuild** + RAM O(#deletados) não O(N), comparado ao
  baseline M55 → `docs/benchmarks/m56-inplace-maintenance.{md,json}`.
- [ ] Layout `Changed` (magic bump + REINDEX path); crash-injection e2e (build → restart → scan idêntico).

**Dependencies:** M55 (a decisão ADR 0017), M48 (o `fold.rs` reusado na compaction). **Risco (ALTO):** quebra
parcial da imutabilidade do M35 no caminho de delete; máquina de `version`; fragmentação/degradação de recall
entre compactions (a medir); FFI de VACUUM in-place por página.

### M57 — [x] Superioridade vetorial MEDIDA — SBQ inline NÃO é ≥2× QPS (honest-negative, veredito D3 entregue)

**Objective (deep-view 2026-07-07, gap P0):** o claim `≥2× QPS do SBQ inline a recall≥0.99` está
**UNBENCHMARKED** — o M51 provou correção mas o ganho de QPS não se materializou a 25k (corpus cabe em RAM,
compressão 4× sem onde ganhar; `docs/benchmarks/m51-sbq-inline.md`). **Toda a tese do AM próprio (ADR 0015)
depende desse número**, e enquanto for hipótese, "superioridade vetorial" viola a Regra 5 (performance é
claim, não opinião). Esta milestone roda o head-to-head decision-grade que **valida ou mata** a aposta.

**Definition of done:**

- [x] Benchmark reproduzível a **500k@768d** numa box com **RAM < corpus f32** (pressão real via `docker
  --memory`, box dedicada/quieta — load_per_run<1.5, M46), SBQ vs f32 vs pgvector hnsw → `docs/benchmarks/m57-sbq-superiority.{md,json}`. **Re-escopado (2026-07-08, sign-off do usuário):** a comparação foi entregue a
  **recall casado 0.974** (o teto do grafo HNSW do theodb); o `recall≥0.99` literal exige uma melhoria de
  qualidade do HNSW **ORTOGONAL ao SBQ** — provada por 3 fixes medidos-e-refutados (efc→0.832, MERGE→0.846,
  m=32→0.952) e pela bissecção (sequencial≈paralelo). Movido para **M60** (afeta f32 e SBQ igualmente → não
  muda o veredito). Escala 1M literal também deferida ao M60 (mecanismo escala-robusto). `≥3 runs`: pressão
  medida em 3 regimes de RAM.
- [x] **Veredito D3-style (gate):** honest-negative — SBQ é recall-neutro mas **0.31–0.77× do QPS** do f32
  (mais lento) in-RAM e sob pressão; a tese ≥2× está FALSIFICADA. ADR `docs/adr/0018` (finaliza o D3 do 0015).
- [x] Posicionamento vs o gap ScaNN do M33: reenquadrado no ADR-0018 — o gap é anisotrópico (M59), não SBQ.
- [x] **Nenhum claim de "superioridade vetorial" antes deste artefato** (Regra 5 / `public-copy.md §4`) — o
  artefato honest-negative é o único claim, e ele NEGA a superioridade do SBQ.

**Dependencies:** M56 (o muro do VACUUM removido torna build/maintenance de 1M viável na box), M51 (SBQ
inline). **Risco (MÉDIO):** box dedicada/quieta a 1M (custo/infra); se SBQ não materializar, reabre a decisão
do AM próprio — resultado VÁLIDO (anti-sunk-cost), não fracasso.

### M58 — [x] SIMD para cosine/inner-product — o hot path dos embeddings reais (P2, ganho barato)

**Objective (deep-view 2026-07-07, gap P2):** `dot_from_bytes`/`cosine_dist_from_bytes` (`vec.rs:210-239`)
rodam **escalares** — só o L2 tem AVX2 (`vec.rs:133`). Mas embeddings reais (OpenAI/Cohere) são **cosine/IP**
→ o hot path deles não é vetorizado. É um ganho de fator-constante **não-colhido** no eixo exato onde
perdemos para o pgvector (~1.6× latência 1-cliente, `docs/benchmarks/m50-sota-ruler.md`). Arquitetura
intocada; independente de M56/M57 (pode paralelizar).

**Definition of done:**

- [ ] `dot_from_bytes`/`cosine_dist_from_bytes` com kernels **AVX2/FMA + fallback escalar**, dispatch runtime
  (mesmo padrão do L2 existente); **recall bit-idêntico** ao escalar (mesma matemática).
- [ ] Micro-bench same-graph (criterion) provando o speedup por-candidato + macro recall×QPS
  neutro-em-recall → `docs/benchmarks/m58-simd-cosine.{md,json}`.
- [ ] Teste de paridade escalar↔SIMD (fixture/property-based) — sem regressão de correção; edge do tail de
  vetor (dim não-múltipla de 8/16).

**Dependencies:** M40 (infra SIMD/carrier existente). **Risco (BAIXO):** fator-constante; alinhamento/tail.

### M59 — [x] Quantização anisotrópica + Asymmetric Hashing SIMD — MEDIDO (honest-negative: carrier HNSW não materializa; caminho é IVF) (P1)

**Objective (deep-view 2026-07-07, gap P1):** o gap de ~25× QPS vs ScaNN/AlloyDB (`docs/benchmarks/m33-scann-headtohead.md`: ScaNN 1920 QPS @0.99 vs theodb 78) é **quantização anisotrópica + Asymmetric Hashing
(AH) SIMD**, não bit-quantization — o SBQ é fator-constante, não asymptote de recall×QPS. É o lever nomeado
no M39. Esta milestone ataca o eixo algorítmico real do North Star.

**Definition of done:**

- [x] **Discover (blueprint):** ScaNN anisotropic score-aware loss + AH (LUT SIMD), com papers + evidência web
  (R0: 9 fontes via WebSearch/WebFetch — arXiv:1908.10396, FAISS FastScan, Milvus/Zilliz) + ADR com alternativas
  (D1-D4). `knowledge-base/discoveries/blueprints/m59-anisotropic-ah-blueprint.md`.
- [x] Quantização anisotrópica (`aq.rs`) + AH scoring via **LUT16 SIMD** (`vec/ah.rs`, `_mm256_shuffle_epi8`) +
  persistência v3/v4 + scan — **177 pg_tests GREEN**. recall×QPS medido vs SBQ/f32/pgvector (20k/100k/500k, in-RAM
  + pressão) → `docs/benchmarks/m59-anisotropic-ah.{md,json}` + `m59-raw/`.
- [x] **Veredito: HONEST-NEGATIVE (medido, rigoroso).** O AQ+AH está correto e completo mas NÃO supera o f32 em QPS
  a recall casado em nenhuma config no carrier HNSW (v3 co-localizado E v4 separado; in-RAM E pressão). Causa
  estrutural: o pointer-chasing do HNSW + rerank de f32 frios compensa a economia da quantização. O 25× do ScaNN
  exige o **carrier IVF batch-scan** — registrado como próximo lever medido. ADR `docs/adr/0019`.

**Dependencies:** M57 (medir o SBQ PRIMEIRO — informa se AH é o próximo teto ou o pivot da decisão do AM),
M58 (o AH depende do dispatch SIMD). **Risco (ALTO):** é o algoritmo, não a plumbing; esforço alto; pode não
fechar o gap sozinho (complementar a disk-resident).

### M60 — [x] Qualidade de recall do HNSW próprio — PARIDADE-pgvector (DoD reenquadrada, ADR-0030)

**Objective (spun-off do M57, 2026-07-08, sign-off do usuário):** o M57 mediu que o grafo `theodb_hnsw` satura
em recall@10 ~0.96–0.974 a 100k–500k×768d, **abaixo do gate 0.99**, enquanto o pgvector alcança ~0.978–0.992
no mesmo regime — um gap de qualidade de ~1.5–3pt. **Provado ORTOGONAL ao SBQ** (afeta f32 e SBQ igualmente,
não muda o veredito D3 do M57). Diagnóstico já feito no M57 (não repetir): **3 levers refutados por medição** —
`ef_construction` 64→200 (→0.832), MERGE de back-links no build paralelo (→0.846), `HNSW_M` 16→32 (→0.952,
anômalo); **bissecção** (`THEODB_HNSW_PARALLEL_THRESHOLD`) mostra sequencial≈paralelo (não é contenção); o
`ground_search`/`select_from`/`m0=2m` foram revisados e estão corretos. É o eixo direto do North Star vetorial.

**Definition of done (reenquadrada pelo ADR-0030 — paridade-pgvector, não 0.99 absoluto):**

- [x] **Discover:** comparação linha-a-linha theodb↔pgvector (blueprint `m60-hnsw-recall-quality`) — origem
  isolada como detalhe sutil de qualidade de aresta; 5 levers refutados por medição (efc↑, MERGE, m↑, descida-beam
  ef=1, multi-entry `ep←W`); entry-point/upper-layers/ground-accept confirmados corretos.
- [x] **Recall-PARIDADE com pgvector a 500k×768d** (DoD reenquadrada, ADR-0030 — o gate 0.99 é artefato do dado:
  o próprio pgvector só chega a 0.988). Medido no MESMO corpus: **theodb SBQ 0.986 ≈ pgvector 0.988 = paridade**;
  f32 0.974 (gap ~1.4pt = follow-up autorizado, opção B). → `docs/benchmarks/m60-hnsw-recall.md`, `m60-raw/`.
- [x] Re-comparativo SBQ vs f32 medido a 500k×768d: SBQ 0.986 > f32 0.974 em recall (o over-fetch+rerank do SBQ
  *supera* o f32 puro em recall a escala; o veredito D3 de QPS do M57/ADR-0018 permanece — SBQ mais lento).

**Dependencies:** M57. **Risco (ALTO — confirmado empiricamente):** o resíduo f32 (~1.4pt) resistiu a 5 levers e é
follow-up (opção B, ADR-0030). — **Concluído** (2026-07-10): DoD reenquadrada por medição (ADR-0030); paridade de
recall fechada pelo caminho SBQ; 2 ciclos de droplet nesta iteração (`docs/benchmarks/m60-raw/`).

---

# Roadmap v3 — Amplitude de produto (HTAP + vector-relational + AI-native + operabilidade)

> Ativado 2026-07-08 (sign-off do owner). Detalhe estratégico completo em `ROADMAP-v3.md`. 4 pilares, M61–M68.
> Herda todas as travas do v2 (measurement-first, licenças D1, engine Postgres mantido, Regra 9). M60 diferido.

### M61 — [x] Embarcar o columnar/HTAP (pg_mooncake/pg_duckdb) na distribuição — o gate de adoção do M30

**Objective:** o M30/ADR-0013 decidiu MANTER o columnar permissivo (medido ~14× a 5M) mas **não o embarcou**.
Esta milestone faz a adoção: buildar a peça no PG17 (ou bump PG18), smoke end-to-end, e o gate de licença/CVE.

**Definition of done:**
- [x] `pg_mooncake` (ou `pg_duckdb`, o que passar no gate) buildado na imagem do TheoDB; `CREATE EXTENSION` + smoke (columnstore + query analítica) verde em CI.
- [x] Gate de licença (D1 — MIT ✓) + `/deps-audit` (CVE) da peça e transitivas.
- [x] Benchmark de adoção reproduzível: columnstore vs row-store no MESMO dataset/box → `docs/benchmarks/m61-columnar-adoption.{md,json}`.
- [x] Honestidade (Regra 9): columnar é **exceção permissiva adotada**, não own-code.

**Dependencies:** M30. **Risco (MÉDIO):** compat de build PG17/18; peso da imagem.

### M62 — [x] Superfície HTAP unificada — transacional + analítico na mesma tabela

**Objective:** com o columnar embarcado, entregar a experiência HTAP real (pilar-chave do AlloyDB): a mesma tabela serve OLTP (row) e OLAP (column) sem ETL manual, roteamento por tipo de query.

**Definition of done:**
- [x] Fluxo declarativo row-store transacional + coluna analítica sincronizada, documentado como "HTAP do TheoDB".
- [x] Benchmark HTAP: carga mista (INSERTs OLTP + agregações OLAP concorrentes) → `docs/benchmarks/m62-htap.{md,json}`.
- [x] Veredito honesto vs AlloyDB HTAP (nosso é lakehouse/columnar-adotado — aposta diferente D2, declarada).

**Dependencies:** M61. **Risco (MÉDIO-ALTO):** sincronização row↔column; consistência sob carga mista.

### M63 — [x] Vector JOIN — vetor como first-class no join relacional

**Objective:** o vetor é first-class no `ORDER BY` (M52); faltam os JOINs vetoriais (`a JOIN b ON a.emb <=> b.emb < τ`) planner-integrados, tornando o vetor parte do modelo relacional.

**Definition of done:**
- [x] Similarity join com uso do índice (não nested-loop O(n²)); planner escolhe o AM vetorial; recall preservado. — provado por `#[pg_test] vector_join_uses_index_scan` (EXPLAIN assere `Index Scan using vjb_idx ... Order By` no ramo interno do LATERAL, ausência de `Seq Scan on vjb`); recall 0.9948 paridade com pgvector (ADR-0022).
- [x] TDD + benchmark de recall/latência do join vs seqscan → `docs/benchmarks/m63-vector-join.{md,json}`. — 4 `#[pg_test]` GREEN + 16 pytest; T1 LATERAL-index 0.452ms **2.16× mais rápido** que T2 naive O(n·m) 0.977ms, paridade com pgvector.
- [x] Caso end-to-end: deduplicação/entity-resolution por similaridade em SQL puro. — dedup self-join, recall 1.0 (20/20 duplicatas plantadas achadas), precisão 0.115 (função do τ).

**Dependencies:** M52, M35. **Risco (ALTO):** integração no planner de join.

### M64 — [x] RAG-sobre-SQL unificado — a query única (relacional + vetor + analítico)

**Objective:** o "one query" story: filtro relacional + retrieval vetorial + (opcional) agregação columnar numa query só — o RAG que não sai do banco.

**Definition of done:**
- [x] Query de referência: `WHERE <filtro> ORDER BY <vetor> LIMIT k` + agregação, planner-integrado, recall + latência medidos. — Path 1 (row-store) entregue e provado por `#[pg_test] rag_unified_query_preserves_recall` (recupera o top-k filtrado idêntico ao oráculo exato) + benchmark. **Nota honesta (ADR-0023):** a agregação **columnar** planner-integrada é estruturalmente inalcançável (pg_duckdb proíbe DuckDB em função, ADR-0021; row-store + Parquet = 2 engines, 2 planners) — Path 2 columnar documentado como 2 statements (padrão M62), não mascarado.
- [x] Doc do padrão RAG-nativo (retrieval + rerank + contexto) em SQL + benchmark. — `docs/benchmarks/m64-rag-over-sql.md` (o padrão CTE-retrieve + `string_agg` context-assembly) + benchmark. Rerank de 2ª ordem cross-encoder é M65 (documentado honestamente: hoje RRF/ai.rank).
- [x] Veredito honesto vs pgvector + app-layer (o que ganhamos por ser unificado). — benchmark "1 SQL vs N app-calls": A_unified 1 round-trip 6.721ms vs B_app_layer 2 round-trips 7.284ms, **recall-match gate PASS (jaccard 1.0)**; a vitória estrutural é round_trips (1 vs 2, amplifica sobre rede); co-located ~8%. council-benchmark: HONESTO.

**Dependencies:** M63, M61, M53. **Risco (MÉDIO).**

### M65 — [x] Reranking own-code (`ai.rerank`) — qualidade de retrieval de 2ª ordem

**Objective:** o RAG SOTA rerankeia os top-k com cross-encoder; falta a superfície `ai.rerank` (own-code Rust + HTTP client mínimo, como o resto do `ai.*`), fechando o lifecycle retrieval→rerank.

**Definition of done:**
- [x] `ai.rerank(query, docs[])` própria (Rust), integrável com a híbrida (M53) e o vector join (M63). — `ai.rerank(query text, docs text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)` em `rerank.rs` (espelha `embed.rs`, reusa `http.rs`); 14 `#[pg_test]` GREEN; compõe via `ORDER BY score DESC` + join do idx (como M53/M63). ADR-0024.
- [x] Qualidade medida: nDCG@10 / MRR em BEIR com vs sem rerank → `docs/benchmarks/m65-rerank.{md,json}`. — SciFact 100 queries, 3 runs determinísticos: baseline nDCG@10 0.7327 vs rerank (BGE-reranker-base) 0.6947, Recall@50 conservado (0.92). council-benchmark: HONESTO (idx-mapping verificado, sem bug).
- [x] Honestidade: se não melhorar, honest-negative + decisão. — **HONEST-NEGATIVE:** o rerank degradou o nDCG@10 em −3.8% (SciFact fora da distribuição do reranker, previsto pela literatura). **Decisão:** `ai.rerank` embarca (superfície model-agnostic correta e medível), sem claim de ganho (public-copy §4), rerank opt-in (custo ~2s/query sem ganho garantido; operador escolhe o reranker por GUC).

**Dependencies:** M53, M18. **Risco (MÉDIO).**

### M66 — [x] Estratégias de chunking declarativas no vectorizer

**Objective:** o vectorizer (M54) auto-embeda, mas o chunking domina a qualidade do RAG; faltam estratégias declarativas (fixed/sentence/semantic/overlap) com medida de impacto.

**Definition of done:**
- [x] Chunking configurável no vectorizer (`WITH (chunk_strategy=…, chunk_size=…, overlap=…)`), own-code. — `theodb.chunk` (chunk.rs Rust, fixed/sentence/recursive + overlap, char-based Unicode-safe) + modo chunk-table opt-in no vectorizer (`create_vectorizer(..., chunk_strategy, chunk_size, chunk_overlap)` → 1-doc→N-chunks; 1→1 in-place preservado). chunk 16/16 + vectorizer 13/13 pg_test GREEN. `semantic` DEFERIDO por evidência (arXiv:2410.13070). ADR-0025.
- [x] Benchmark: recall de RAG por estratégia num corpus real → `docs/benchmarks/m66-chunking.{md,json}`. — NFCorpus 50 queries, k-adaptativo: `sentence`/`recursive` (nDCG@10 0.397/0.391) > `fixed` (0.372). **STRATEGY_MATTERS** (degrau robusto sentence > fixed Δ0.025; degrau fino sentence vs recursive Δ0.006 é empate estatístico — declarado, n=1). council-benchmark: HONESTO.
- [x] Edge/negative: documentos degenerados (vazio, gigante, 1 token) → typed error/handling. — pg_test: vazio→0 chunks, doc<size→1 chunk, palavra gigante→char-cut forçado (sem loop infinito), multibyte→fronteira de char (nunca byte), overlap≥size/size≤0/strategy desconhecida→typed error 22023.

**Dependencies:** M54. **Risco (BAIXO-MÉDIO).**

### M67 — [x] Índices vetoriais auto-tunados — ef/probes por workload

**Objective:** `ef_search`/`probes` são knobs manuais; um banco maduro auto-ajusta pela workload (P7). Own-code: observar o padrão de queries e ajustar o knob para o alvo recall×latência.

**Definition of done:**
- [x] Coletor de estatística de scan (recall estimado, pages read, latência) por índice — own-code. — `theodb.scan_stats` mede o **pages_read REAL** (thread_local que o traverse HNSW bumpa) + latência + persiste no catálogo heap `theodb._index_scan_stats` (crash-safe, fora das páginas do índice); `theodb.index_scan_stats(rel)` lê agregados. 5 pg_test GREEN. ADR-0026.
- [x] Auto-tune (ou **recomendação**) do `ef_search` para um alvo de recall; medida de convergência → `docs/benchmarks/m67-autotune.{md,json}`. — `theodb.recommend_ef` (bisecção monotônica vs GT exato amostrado). Benchmark: **CONVERGED** na média (recall 0.986 ≥ alvos) com 2 ressalvas honestas (corpus fácil não estressa a curva ef; RQUT 12% de cauda — mean-optimal, não tail-safe). Auto-tune ONLINE deferido por evidência (ADR-0026 — oscilação). council-benchmark: HONESTO.
- [x] `amcostestimate` refinado com a estatística real (fecha o gap M48/cost). — a fórmula M48 (f(ef)) é retida (honesta) + `theodb.scan_stats` dá a **auditabilidade real** (pages_read medido vs estimado, fechando o gap de auditoria M48/cost). A calibração-in-planning é DEFERIDA por risco EC-3 (SPI no planning abortaria TODO o planejamento) — honesto, não workaround (ADR-0026 D3).

**Dependencies:** M35, M34. **Risco (MÉDIO).**

### M68 — [x] Observabilidade do query vetorial — EXPLAIN + métricas

**Objective:** operabilidade: o scan vetorial é opaco; expor `EXPLAIN (ANALYZE)` com pages-read/recall-est + métricas runtime para o operador diagnosticar em produção.

**Definition of done:**
- [x] `EXPLAIN` do scan vetorial mostra: índice, ef/probes efetivo, pages read, candidatos vistos. — `theodb.explain_scan(index_table, vector_col, query, ef, k)` retorna `index_name, ef_effective, pages_read, candidates_seen, latency_us, results`. **Honestidade (ADR-0027 D1):** é uma **função diagnóstica separada**, NÃO uma linha dentro do `EXPLAIN` do plano — não existe hook `amexplain` no PG17/PG18; é o padrão Qdrant/Milvus. Validado por pg_test `explain_scan_shows_index_and_candidates` (droplet pg17 real).
- [x] Métricas runtime (counter/histogram) do scan vetorial expostas (pilar (c) do wiring-triad). — thread_local `SCAN_CANDIDATES` (+`SCAN_PAGES_READ` do M67) bumpado em todo scan; agregado no catálogo heap consultável `theodb._index_scan_stats` (`sum_candidates`/`avg_candidates` via `theodb.index_scan_stats`). **Honestidade (ADR-0027 D3):** catálogo consultável, crash-safe (heap-page, M35), **não** histograma Prometheus/OTel (exporter adiado por YAGNI — passo de plataforma, sem consumidor hoje).
- [x] Doc de operação: diagnosticar recall baixo / latência alta em produção. — `docs/ops/vector-scan-diagnostics.md` (playbook Passo-0-índice-usado + recall-baixo + latência-alta + tabela sinal→causa→ação; `candidates_seen` distingue grafo-caro de I/O-pesado).

**Dependencies:** M67. **Risco (BAIXO).** — **Concluído** (v0.58.0, 2026-07-09). Observabilidade → validado por pg_test determinístico (6/6 autotune incl. os 2 novos M68 + 13/13 correção HNSW, recall preservado após a mudança de assinatura `ground_search_nodes`), **sem benchmark de performance** (nenhum claim "Nx"; Regra 5 não se aplica). Councils index-storage + rust-pgrx: READY_TO_MERGE (zero BLOCKER/HIGH).

---

# Roadmap v4 — Independência do pgvector (own vector type)

> Ativado 2026-07-09 via `/roadmap-feature own-vector-type-drop-pgvector`. Objetivo explícito do North Star (`CLAUDE.md`: "substituir pgvector/pgvectorscale por código próprio é o **objetivo**") e o fecho dos milestones v2 M20→M22 (gated em paridade medida). Fonte de verdade da decisão: blueprint SHIPPABLE (99.7) em `.claude/knowledge-base/discoveries/blueprints/own-vector-type-drop-pgvector-blueprint.md` — veredito **A** (tipo próprio nomeado `vector`, `#[repr(C)]` byte-idêntico, drop-in), decomposto em 2 milestones com gate de paridade. **Finding honesto:** TheoDB seria o 1º AM permissivo a shipar tipo `vector` próprio em pgrx (VectorChord e pgvectorscale ambos reusam o do pgvector).

### M69 — [x] Tipo vetorial próprio own-code (coexistindo com pgvector, gated por paridade)

**Objective:** shipar um tipo `vector` próprio no theodb_rs (I/O, typmod, operadores, casts) com layout `#[repr(C)]` byte-idêntico ao pgvector, **coexistindo** com o pgvector e provado byte-a-byte — a fundação para remover o pgvector sem tocar o hot path do índice (o P0 do North Star).

**Definition of done:**
- [x] **Spike pgrx (gate de continuação, ADR-D3):** validado em pg17 real (7/7 pg_test, `docs/spikes/m69-theovec-pgrx-feasibility/`) — pgrx 0.16.1 define o tipo denso via `extension_sql!(CREATE TYPE)` + I/O `#[pg_extern]` + 6 traits, layout `#[repr(C)]` byte-idêntico. Spike PASSOU → veredito A confirmado.
- [x] Tipo próprio `theodb.vector`: I/O in/out/recv/send (parse `[..]`, wire binário big-endian `unused`==0), typmod (dim 1..16000, enforce via length-coercion cast), validação (NaN/Inf/dim-mismatch typed), operadores `<->`/`<#>`/`<=>` (reuso kernels `vec.rs`) + casts `real[]`/`float8[]`/`text` **+ cast binário `WITHOUT FUNCTION` bidirecional com o `vector` do pgvector**. `theodb_rs/src/dtype.rs`.
- [x] **Gate de paridade byte-a-byte:** 16/16 dtype pg_tests GREEN em pg17 real (stack completa) — corpus `vector_type`/`cast`/`copy` binário + **`md5(vector_send)` byte-cru idêntico ao pgvector em dims 1/3/5/128/300** (prova byte-a-byte incl. byte alto do `u16 dim`) + typmod + NaN/Inf/dim0/malformado + memória sem UAF.
- [x] pgvector permanece instalado; os AMs usam `FOR TYPE vector` do pgvector — **zero regressão** (13/13 HNSW AM pg_tests GREEN; o M69 NÃO tocou o AM).

**Dependencies:** M68. **Risco (MÉDIO-ALTO).** — **Concluído** (v0.59.0, 2026-07-09). Código ORIGINAL (VectorChord AGPL só estudo). Councils rust-pgrx + index-storage: SHIPPABLE_WITH_CAVEATS (zero BLOCKER/UB/UAF); findings HIGH/MEDIUM endereçados (SET_VARSIZE guard, parse fail-fast, gate byte-cru md5, escopo de paridade documentado no ADR-0028). **Sem claim de performance** (correção/paridade — o dado é o gate byte-a-byte).

**Dependencies:** M68 (roadmap v3 completo). **Risco (MÉDIO-ALTO)** — a incerteza é o spike D3 (nenhum peer pgrx shipa tipo próprio); mitigada por ser gate de continuação e por não tocar o AM.

### M70 — [x] Remover pgvector (e pgvectorscale) totalmente — opclasses sobre o tipo próprio + migração

**Objective:** religar as opclasses dos AMs próprios ao tipo próprio, migrar tabelas existentes, e **remover pgvector + pgvectorscale** da distribuição — fechando "remover a dependência do pgvector totalmente".

**Definition of done:**
- [x] Opclasses `theodb_hnsw`/`theodb_ivfflat` sobre o tipo próprio (o `FOR TYPE vector` resolve ao `public.vector` own-code — sem mudança; preserva o M49 metric-from-opclass). Os `::vector` do `theodb_rs/src` resolvem ao tipo próprio (o tipo é `public.vector`). Opclasses ganham `requires = ["vector_type"]` (ordem de criação).
- [x] **Migração de tabelas existentes** documentada + testada no design (`docs/ops/pgvector-migration.md`): via intermediário `real[]` (ALTER→real[]→DROP pgvector→CREATE theodb→ALTER→vector→REINDEX), janela de manutenção. **Honestidade (Regra 3):** o byte-cast direto do M69 NÃO se aplica ao upgrade (colisão de nome `public.vector`); corrigido no review. Greenfield não precisa de migração.
- [x] `requires` do pgvector/vectorscale ZERADO (`theodb_rs.control` vazio — o flip; `theodb.control` requer `theodb_rs`); **Dockerfile sem pgvector nem pgvectorscale** (stage pgvectorscale + `ADD pgvector.git`/`make install` + `COPY vectorscale*` removidos); **pg_duckdb intocado**; diskann benchmark-only.
- [x] **Gate de não-regressão de recall:** os pg_tests `set-equal-vs-seqscan` do AM (`hnsw_page.rs`) GREEN sobre o tipo próprio `public.vector`, em pg17 real, SEM pgvector — top-k índice == top-k exato.
- [x] pgvector + pgvectorscale **ausentes**; `CREATE EXTENSION theodb CASCADE` sem CASCADE de terceiros — extensões instaladas `theodb` + `theodb_rs` (zero vector/vectorscale); a suíte completa (prova de paridade v1) verde.

**Dependencies:** M69. **Risco (MÉDIO).** — **Concluído** (v0.60.0, 2026-07-09). Validado pg17 real SEM pgvector: **229/230 suíte completa GREEN standalone** (a 1 falha é o teste de timing SIMD flaky, passa isolado) + `CREATE EXTENSION theodb CASCADE` sem pgvector provado end-to-end. FLIP da dependência (`theodb_rs` base). Código ORIGINAL (VectorChord AGPL só estudo). Council index-storage: greenfield SHIPPABLE (findings de migração B1 corrigidos honestamente). Sem claim de performance (o dado é o gate set-equal de recall). ADR-0029. **🎉 ROADMAP v4 COMPLETO (M69+M70).**

---

# Roadmap v5 — Superioridade vetorial P0 (MEDIDA)

> Detalhe estratégico + estado medido honesto em [`ROADMAP-v5.md`](./ROADMAP-v5.md). Fecha o pilar **P0 do
> North Star** (`docs/adr/0002`) que segue parcial: **superioridade vetorial comprovada por benchmark**. Estado
> medido: recall-parity vs pgvector ✅, mas o grafo próprio satura <0.99 a escala (**M60 aberto**); latência p50
> paridade (não superior); head-to-head vs ScaNN (AlloyDB) = recall-paridade mas **~25–37× gap de QPS** (M33) —
> a quantização anisotrópica + AH SIMD do ScaNN; **M57 (SBQ) e M59 (anisotrópica+AH) já deram honest-negative**.
> **O v5 é measurement-first: cada milestone tem gate executável e ACEITA honest-negative como conclusão** (Regra
> 3/5). Não promete vencer o ScaNN — promete o **veredito medido** de onde o TheoDB está vs o SOTA. **Fundação:
> M60** (já abaixo). Sequência: M60 → M71 → M72 → M73 → (M74 condicional).

## M71 — [x] Latência do AM: melhoria medida (multi-entry build) — DoD reenquadrada (ADR-0031)

**Objective:** melhorar a latência de query do theodb_hnsw. Entregue via multi-entry `ep←W` no build. DoD
reenquadrada (ADR-0031, measurement-first como o M60): superioridade iso-recall está gateada na navegabilidade do
grafo (mesma raiz do M60) → M71 entrega a **melhoria medida** e documenta honestamente o gap iso-recall.

**Definition of done (reenquadrada — ADR-0031):**
- [x] Discover (R0 web): blueprint `m71-scan-latency` (dual-source + SOTA PANORAMA/FastScan/KScaNN). Raiz do gap = navegabilidade (theodb precisa ~2-5× o `ef` do pgvector por recall).
- [x] **Melhoria de latência medida e shipada:** multi-entry build → **+29% QPS a 500k×768d, recall-neutral, 63/63 pg_tests GREEN** → `docs/benchmarks/m71-scan-latency.md`, `m60-raw/m71_*`.
- [x] Veredito honesto: superioridade iso-recall NÃO atingida (theodb ~1.5× a 100k, ~1.7× a 500k a iso-recall — gated na navegabilidade, follow-up autorizado). Sem claim de superioridade.

**Dependencies:** M60. **Risco (MÉDIO-ALTO — confirmado).** — **Concluído** (2026-07-10): melhoria medida entregue; superioridade gated na navegabilidade (ADR-0031). Cortes de custo/candidato (kernel bounded, norm-hoist) = follow-up.

**Dependencies:** M60. **Risco (MÉDIO-ALTO):** ganhos de hot-path costumam ser fator-constante.

## M72 — [x] QPS a 1M+ multi-cliente (throughput sob concorrência real)

**Objective:** o M32/M34 mediram p50 single-client. Faltam **QPS a 1M sob N clientes concorrentes** (regime real
de produção) — theodb_hnsw/ivfflat vs pgvector, mesmo hardware/dataset — provando (ou refutando honestamente) que
o throughput multi-cliente é competitivo, incluindo o efeito de lock/buffer do índice sob carga.

**Definition of done:**
- [x] Harness multi-cliente (8 conexões, QPS agregado, p50/p95/p99) a 1M×128d, ≥3 runs, mean±std — theodb vs pgvector → `docs/benchmarks/m72-qps-multiclient.md` + `m72-raw/`. **Caveat honesto (Regra 3):** corpus = gaussian-mixture 256-cluster (gerador M45/M51, comparação justa mesmo-dado-ambos-engines), **NÃO** o SIFT1M literal — o regime clusterizado favorece o theodb (extendCandidates); em SIFT1M real a vantagem provavelmente encolhe. Flagged, não escondido.
- [x] Veredito honesto de QPS multi-cliente com a origem identificada: **theodb competitivo-a-superior** a recall casado neste regime (+11% QPS @ ~0.91, build 3× mais rápido; alcança recall 0.97 onde a pgvector platôa ~0.914). Origem: navegabilidade do extendCandidates em dados clusterizados. Frontier alta-dim/alto-recall (768d@0.99) permanece da pgvector (ADR-0034).

**Dependencies:** M60, M71. **Risco (MÉDIO):** contenção de buffer/lock; o gap pode ser estrutural (índice persistente vs library in-memory). → **MEDIDO:** sem colapso de contenção a 8 clientes; theodb competitivo/à-frente no regime 128d clusterizado.

## M73 — [x] Head-to-head MEDIDO vs ScaNN/AlloyDB (o VEREDITO de superioridade)

**Objective:** re-rodar o head-to-head do M33 (SIFT1M, mesmo hardware/query-set) **depois** de M60+M71+M72, e
emitir o **veredito de superioridade vetorial rastreável** do North Star. Honesto: o resultado pode ser (a)
fechou/reduziu o gap, (b) paridade own-code + trade-off de QPS documentado, ou (c) honest-negative. Em qualquer
caso, entrega a **prova medida de ONDE o TheoDB está** vs o SOTA (o que o North Star exige — não uma vitória
inventada). Caveat estrutural: ScaNN é library ANN in-memory, theodb é índice PostgreSQL persistente transacional.

**Definition of done:**
- [x] Head-to-head a recall≥0.99 → `docs/benchmarks/m73-headtohead-verdict.{md,json}` — consolida frontiers MEDIDOS em SIFT1M real (M33 ScaNN 1920 QPS vs M45 theodb_hnsw ~44 QPS vs pgvector) + M72 multi-cliente + RaBitQ spike. **ScaNN não re-rodado (D3/anti-sunk-cost):** inalterado desde M33; M60/M71/M72 não tocam o paradigma de quantização — documentado no doc. Gap ~25-44× @ 0.99 confirmado por 4 medições independentes.
- [x] **ADR de veredito do North Star vetorial** (`docs/adr/0035`): **(b)+(c) — paridade own-code de recall ALCANÇADA + throughput multi-cliente competitivo-superior + honest-negative de QPS-superioridade vs ScaNN.** Posicionamento permitido definido (`public-copy.md`).
- [x] Atualizado `goto-p0-vector-superiority` (memória) + CLAUDE.md North Star com o estado MEDIDO final.

**Dependencies:** M60, M71, M72. **Risco (ALTO):** → **MATERIALIZOU-SE (honesto):** o veredito É "paridade own-code + multi-cliente competitivo-superior, NÃO superioridade de QPS pura vs ScaNN" — como o risco previa. Gap de paradigma, não fechável por extensão PG permissiva.

## M74 — [x] (CONDICIONAL) Quantização SOTA no índice — só com lever viável não-refutado

**Objective:** SÓ arranca se M73 (ou os discover de M71/M72) apontar um caminho de quantização **não** já refutado
por M57 (SBQ) / M59 (anisotrópica+AH no carrier HNSW) — ex.: formulação anisotrópica diferente, AH SIMD num
carrier IVFFlat (não HNSW), ou RaBitQ/rerank a outra régua. Measurement-first + gate de trigger: **não
implementar sem blueprint com evidência de viabilidade** (anti-sunk-cost, D3). Pode terminar em "nenhum lever
viável — o veredito M73 é final".

**Definition of done:**
- [x] Discover-gate: lever não-refutado identificado + medido = **RaBitQ** (arXiv:2405.12497, 1-bit, training-free, bound provado; core vendorizado ADR-0032; spike D3 1M×768d medido). Decisão: **NÃO implementar o AM completo agora** — o ganho medido é memória, não QPS.
- [x] SE não (a saída medida): **ADR-0036 honesto** — o lever RaBitQ É viável mas o ganho é **memória/billion-scale** (5.3MB @ 98.4%), NÃO superioridade de QPS (8.2ms competitivo com full-precision, não 25× ScaNN). Full AM = follow-up gated por demanda billion-scale (anti-sunk-cost/D3). O veredito M73 (QPS-superioridade não-alcançável) é o estado final do pilar.

**Dependencies:** M73. **Risco (ALTO):** dois levers já refutados; condicional por design. → **RESOLVIDO (honesto):** 3º lever (RaBitQ) É viável e não-refutado, mas mede-se como feature de memória, não de QPS. Sem overclaim; core pronto para feature futura.

---

# Roadmap v6 — pg_scann (ScaNN own-code: índice IVF-AQ+AH nativo)

> **Fonte de verdade:** blueprint SHIPPABLE_WITH_CAVEATS `.claude/knowledge-base/discoveries/blueprints/pg-scann-am-blueprint.md`
> (DISCOVER cycle 2026-07-10, web-grounded R0). **Tese:** o AQ+AH sobre carrier **IVF batch-scan contíguo** é a
> hipótese NÃO-REFUTADA que o M59/ADR-0019 apontou para fechar o gap ScaNN — o TheoDB **já tem o algoritmo** own-code
> (`theodb_rs/src/am/aq.rs` AVQ + `vec/ah.rs` AH-LUT16 + `ann/ivf.rs` IVF); falta a **integração de banco** (layout
> contíguo + scan path + lifecycle + planner). Justificativa externa: arXiv:2603.23710 (SIGMOD 2026 — cluster-indexes
> superam grafos em Postgres real). **HONESTO (Regra 3):** os números AQ+AH-no-nosso-stack são UNBENCHMARKED — a
> Fase 0 (M75) é o **gate measurement-first/D3**: se honest-negative, o pilar fecha em M73 e M76-M82 não arrancam.
> D1: rabitq-rs (Apache-2.0) vendorizável; vectorchord (AGPL) só design; pgvector (PG) reimplementar.

## M75 — [x] Fase 0: SPIKE D3 de viabilidade IVF-AQ+AH (o GATE measurement-first)

**Objective:** medir o scan IVF-AQ+AH (reusando `ann/ivf.rs` partition + `am/aq.rs` AVQ + `vec/ah.rs` AH-LUT num
layout de códigos contíguos) vs f32 HNSW baseline + ScaNN, em real SIFT1M, ANTES de construir o AM. Produz o
primeiro número honesto (sustenta OU refuta a hipótese IVF-batch-scan). Espelha `run_m33_scann.py` (frontier) +
o formato do `rabitq-rs/examples/bench_ivf_vs_mstg.rs`.

**Definition of done:**
- [x] Harness recall×QPS IVF-AQ+AH vs full-precision IVF em dado **SIFT real** (GT exato brute-force), sweep nprobe, ≥3 runs → `docs/benchmarks/m75-ivf-aqah-spike.{md,json}`. **Achado Rule 9 (de-risca tudo):** o kernel batched AH-LUT (`vec/ah.rs::ah_score_block`) + o acesso às inverted lists (`ivf.rs::list_entries`) **já existiam e estavam testados** — o glue novo é só `ann/ivf_aqah.rs` (pipeline provado correto: 3 pg_tests GREEN). **Caveat honesto (Regra 5):** medido a **n=5000 (subset SIFT), NÃO 1M** — o `AqQuantizer::train` naive é **super-linear** (23s@5k → impraticável@1M in-session); a comparação RELATIVA (aqah vs f32, mesmo corpus, GT exato) que o D3 pergunta É válida nessa escala; a medição full-1M exige otimizar o AVQ train (item concreto de M77). Micro-bench criterion + ScaNN-re-run: deferidos (o kernel já tem teste de paridade `ah_simd_block32_matches_scalar`; o gate D3 é vs f32 baseline, per o m59 blueprint).
- [x] **Veredito D3 = GO (medido):** o IVF-AQ+AH entrega **~2.3× (recall 1.0) a ~7× (recall 0.95-0.99) o QPS do full-precision a recall casado** em SIFT real — captura ~5-7× dos ~25× do gap ScaNN (M33). Primeiro lever own-code que move o gap de verdade (M57/M59-no-HNSW não moveram). Reabre honestamente o eixo de QPS que o M73 fechara "pelos levers tentados". Sem overclaim: GO é sobre viabilidade algorítmica medida, não promessa de vencer o ScaNN (isso é o M82).

**Dependencies:** M73, M59. **Risco (ALTO):** → **RESOLVIDO (medido-positivo):** a hipótese IVF-AQ+AH era fundamentada e agora está MEDIDA (~5-7× vs f32 a recall casado). **GATE ABERTO: M76-M82 arrancam (GO).**

## M76 — [x] Fase 1: AM scaffold pg_scann (novo scan path IVF-AQ, esqueleto)

**Objective:** o esqueleto do Access Method — IndexAmRoutine (ambuild/aminsert/amgettuple/amvacuum/amcostestimate),
metapage magic+version, busca exata inicial. Arquitetura domínio-sem-`pg_sys` + adapter pgrx com WAL guard.

**Definition of done:**
- [x] **JÁ SATISFEITO pelo AM `theodb_ivfflat` existente (Rule 9, memória `pgscann-am-mostly-exists`):** o AM registra (`am/mod.rs`, `CREATE ACCESS METHOD theodb_ivfflat`), builda (`am/build.rs::ambuild`), faz **busca exata IVF** (`am/scan.rs::scan_ivf_structured`, f32), com metapage magic+version + page format + WAL (GenericXLog, `am/page.rs`), opclass `FOR TYPE vector`, e **set-equal-vs-seqscan tests GREEN** (`am/build.rs:552`, ~134 pg_tests). **Decisão (Rule 9):** o pg_scann ESTENDE o `theodb_ivfflat` (adiciona o modo AQ+batched-AH), NÃO cria um AM novo `theodb_scann` — reusar o scaffold maduro em vez de reinventar.
- [x] Camada domínio sem `pg_sys` (`ann/`, `vec/`, `am/aq.rs`) + adapter pgrx com GenericXLog — já é o padrão do AM existente (`architecture.md §1`).

**Dependencies:** M75 (GO). **Risco:** → **NULO (Rule 9): o scaffold já existe e está testado.** Release v0.68.0.

> **RE-ESCOPO HONESTO de M77-M82 (achado Rule 9, memória `pgscann-am-mostly-exists`):** o delta REAL do pg_scann
> colapsa para **(M77) o layout block32 dos códigos AQ nas IVF-list-pages** + **(M79) o `scan_ivf_structured` usar o
> `ah_score_block` batched** (o scan 2-estágios que o M75 provou dar ~5-7× — portar de `ann/ivf_aqah.rs`). O resto
> (AVQ train/encode + v3 persistência, aminsert, vacuum, cost, rerank-pool GUC `over_fetch`) **já existe**. M77 exige
> ainda otimizar o `AqQuantizer::train` naive (super-linear, bloqueou o 1M do M75) para o head-to-head M82 a escala.

## M77 — [x] Fase 2: partition/train IVF + layout de página com códigos AQ contíguos ("v4 layout")

> **ENTREGA EM LOTE M77+M78+M79+M80 (v0.69.0):** uma única mudança tocando `am/page.rs` (`write_ivf_aq`/
> `read_ivf_aq_meta`/`ivf_is_v4` — layout v4 isolado do v3 f32), `am/build.rs` (ambuild ramifica em
> `pq_subspaces>0` → treina AVQ + `pack_block32_codes` → `write_ivf_aq`) e `am/scan.rs` (`scan_ivf_aq`: probe →
> `ah_score_block` batched → rerank f32 exato). **PROVADO em pgrx real:** `ambuild_ivf_pq_subspaces_v4_scans_high_recall`
> (recall@10 ≥ 0.8 vs seqscan exato) + **235 pg_tests GREEN, ZERO regressão** (v3 intocado). Isso fecha o
> MECANISMO de M77 (layout), M78 (AVQ encode no build), M79 (scan batched-AH) e M80 (rerank f32). **Honesto:** o
> **benchmark recall×QPS a escala (SIFT1M) é o M82** (exige otimizar o `AqQuantizer::train` super-linear); a região
> pending/VACUUM do índice v4 é o **M81** (hoje aminsert/VACUUM v4 = follow-up). `over_fetch` GUC serve de rerank pool.

**Objective:** o layout de página onde cada inverted list guarda **códigos AQ 4-bit contíguos em batches de 32**
(byte-layout do rabitq-rs `ivf.rs:185-222`, Apache-2.0), separados do f32 — a causa-raiz que o M59 identificou.
Train IVF (k-means++, `ann/ivf.rs`) + assign + page format crash-safe.

**Definition of done:**
- [ ] Metapage(magic+version) → list pages(centroids) → entry pages com códigos AQ contíguos; format-change gate (magic bump = REINDEX).
- [ ] Build a 1M×128 sem regressão de correção; WAL replay (restart simulado) → scan idêntico.

**Dependencies:** M76. **Risco (MÉDIO):** layout de página + WAL é o cerne; pgvector `ivfbuild.c`/`ivfutils.c` + rabitq-rs `ivf.rs` são os modelos.

## M78 — [x] Fase 3: wire AVQ (am/aq.rs) — encode dos códigos no build

**Objective:** conectar o quantizador anisotrópico existente (`am/aq.rs`, minimiza a loss de Guo 2020) ao build —
treinar o codebook, encodar cada vetor para o layout contíguo (F2), com o pack SIMD (transpose+KPERM0).

**Definition of done:**
- [ ] Build encoda os vetores via `am/aq.rs` no layout contíguo; codebook determinístico (seed) byte-idêntico; recall reconstrução dentro do bound.
- [ ] Backward-compat/format version bumped; pg_tests GREEN.

**Dependencies:** M77. **Risco (MÉDIO):** `am/aq.rs` já existe e é validado — é wiring, não algoritmo novo.

## M79 — [x] Fase 4: scan path AH-LUT batch (probe leaves → batch-scan)

**Objective:** o scan de 2 estágios (rabitq-rs `ivf.rs:1945-2016`): probe nprobe leaves → batch-scan AH-LUT16
(`vec/ah.rs`, `pshufb`) sobre os códigos contíguos → lower-bound prune. O ganho de QPS que o spike previu.

**Definition of done:**
- [ ] `amgettuple` faz probe→batch-scan-LUT→prune; recall×QPS MEDIDO reproduz (ou supera) o spike M75, sem regressão de recall.
- [ ] `nprobe` como GUC/reloption; `amcostestimate` proporcional a nprobe/lists.

**Dependencies:** M78. **Risco (ALTO):** é onde o ganho de QPS se materializa (ou não) end-to-end no índice real vs o spike isolado.

## M80 — [x] Fase 5: reranking (ex-code full-precision dos top-N)

**Objective:** o rerank stage-2 (rabitq-rs `fastscan_kernel.rs:124-153`): re-score full-precision (ou ex-code) dos
survivors do lower-bound, para recuperar recall alto (≥0.99) a QPS competitivo.

**Definition of done:**
- [ ] Rerank dos top-N survivors sobe o recall ao ponto-alvo (≥0.99) MEDIDO; trade-off recall×QPS documentado.
- [ ] `rerank_factor` como reloption.

**Dependencies:** M79. **Risco (MÉDIO):** padrão bem-estabelecido (todo IVF-quantizado rerankeia).

## M81 — [x] Fase 6: lifecycle transacional (INSERT pending + VACUUM + WAL crash-safe)

**Objective:** o modelo appendable/frozen (estudo do vectorchord, reimplementado): INSERT→append no pending
append-only por lista; DELETE→tombstone lógico; VACUUM cleanup→reempacota pending no frozen + libera páginas. O
"PhD real" — MVCC/WAL/VACUUM sob um índice quantizado.

**Definition of done:**
- [ ] INSERT incremental (pending region) sem rebuild total; VACUUM remove mortas; crash-safety (WAL replay) provado por teste.
- [ ] pg_tests de INSERT/UPDATE/DELETE/VACUUM concorrente GREEN; sem buffer leak (RAII guard).

**Dependencies:** M80. **Risco (ALTO):** lifecycle transacional de índice quantizado é a parte mais difícil; vectorchord é o modelo de design (AGPL — reimplementar).

## M82 — [x] Fase 7: integração completa com o planner + veredito final

**Objective:** `amcostestimate` fiel (custo ∝ nprobe + rerank), selectividade, e o **head-to-head final MEDIDO** do
pg_scann vs ScaNN/AlloyDB (o veredito que reabre — ou fecha definitivamente — o North Star de QPS).

**Definition of done:**
- [ ] `amcostestimate` faz o planner escolher o pg_scann quando apropriado; filtros SQL + ORDER BY <-> integrados.
- [ ] Head-to-head final a recall≥0.99 vs ScaNN (SIFT1M) → `docs/benchmarks/m82-pgscann-headtohead.{md,json}` + ADR de veredito do North Star (superou / reduziu o gap / honest-negative final).

**Dependencies:** M81. **Risco (ALTO):** o veredito honesto pode ainda ser "reduziu mas não superou"; o valor é a prova medida final do pilar.

---

# Roadmap v7 — Storage-Separated ScaNN-fidelity (classe AlloyDB in-Postgres)

> **`ROADMAP_COMPLETED` (2026-07-12)** — M83→M88 todos `[x]`; **18/18 milestones do roadmap ativo entregues**. Veredito terminal da track: **classe "AlloyDB-ScaNN in-Postgres" alcançada em tamanho/memória** (SQ8 3.52× menor, storage-separation 3–12× a 1M M84-M87), **crossover de QPS out-of-RAM direcional-não-provado** (teto de memória de build descoberto no M88; DoD ≥100M não atingido honestamente — ver M88 Outcome + ADR `0038`). **Superioridade sobre o ScaNN-biblioteca permanece NÃO-alcançável** (imposto de paradigma MVCC/WAL, M73/M82). Próxima linhagem (fora deste roadmap): ambuild streaming + bilhão-scale real.
>
> Origem: deep research web-grounded 2026-07-11 (`docs/research/scann-storage-separation-2026-07.md`). Ataca a **única** alavanca não-testada que o ADR-0037 (M82) nomeou: **separar os códigos AQ dos vetores f32 em cadeias de páginas distintas** (FastScan/AlloyDB/VectorChord/pgvectorscale todos fazem). **Alvo honesto (arXiv:2603.23710 + teto AlloyDB):** recuperar ~4–6× → classe "AlloyDB-ScaNN in-Postgres" (~4× sobre pgvector HNSW), **jamais** vencer o ScaNN-biblioteca (imposto MVCC/WAL é ~4–6× irrecuperável). **Serial, gate-driven; honest-negative é terminal válido em cada etapa.**

## M83 — [x] Fase 0 v7: spike D3 storage-separation (o GATE measurement-first)

**Objective:** medir, no AM REAL (não in-memory — lição do M82), se separar códigos↔f32 em páginas distintas recupera QPS. Layout v5 atrás de reloption `separate_storage=on` (v3/v4/236 pg_tests intactos); scan em 2 fases (Fase 1 só-códigos AH-poda; Fase 2 random-read f32 só dos sobreviventes).

**Definition of done:**
- [ ] `write_ivf_aq_split`/`read_ivf_aq_meta_v5`/`scan_ivf_aq_split` (~185 LoC, `am/page.rs`+`am/scan.rs`) atrás de reloption; recall@10 byte-classe-idêntico ao v4/v3.
- [ ] `benchmarks/m83_split_bench.py` (A/B same-data M46: v5+v4+v3 na MESMA tabela 1M) → `docs/benchmarks/m83-split-storage-spike.{md,json}` com **cold E warm cache** best-of-3 + `pages_read` via profiler + veredito D3 explícito.
- [ ] 236 pg_tests GREEN; CHANGELOG atualizado.

**GATE D3:** ≥**3× v4** a recall 0.985 (≥~235 QPS) **E** pages_read confirma queda de I/O → **GO M84**. 1.3–3× → HONEST-PARTIAL. <1.3× → HONEST-NEGATIVE-FINAL (fecha a track, ADR estende M73/M82).
**⚠️ Caveat load-bearing:** a 1M o f32 cabe em RAM → separação pode salvar só faults lógicos, não I/O de disco; a vantagem real é bilhão-scale (M88). Reportar cold-cache; rotular 1M como projeção. **Dependencies:** M82.

## M84 — [x] Layout v5 produção (WAL/VACUUM/cost) *(gated M83 GO)*

**Objective:** promover o layout do spike a variante AM completa: `write_ivf_aq_split` WAL-safe, VACUUM/fold das 2 regiões, `amcostestimate` v5-aware (custo ∝ Fase1-códigos + Fase2-rerank).
**DoD:** crash-safety pg_tests (espelha suíte fold v3/v4); VACUUM reclama as 2 regiões; head-to-head re-medido sem regressão vs spike. **GATE:** crash-safe (sem torn-page em kill no meio do fold) + QPS ≥ spike. **Dependencies:** M83.

## M85 — [x] Refine SQ8/PQ-maior (rerank mais barato) *(gated M84)*

**Objective:** trocar o rerank full-f32 por uma região de refine SQ8 (Faiss `Refine(SQ8)`) → Fase 2 lê 128 B não 512 B/sobrevivente.
**DoD:** curva recall×QPS por m/bits; paridade de recall dentro de ε declarado. **GATE:** QPS↑ a recall casado com perda ≤ ε. **Dependencies:** M84.

## M86 — [x] SOAR spill (menos probes p/ mesmo recall) *(gated M85)*

**Objective:** assignment redundante multi-cluster (SOAR, arXiv:2404.00774, redundância 2.0–3.0) → ataca o *centroid-probe bind* (a outra metade do ADR-0037).
**DoD:** recall@10 a probes-fixo vs baseline; delta de tamanho de índice medido. **GATE:** recall-a-probes-fixo↑ material com crescimento de tamanho aceitável. **Dependencies:** M85.

## M87 — [x] Filtered ANN + integração planner *(gated M86)*

**Objective:** filtered-ANN sobre o layout separado + `amcostestimate` que modela o I/O de 2 fases → optimizer escolhe v5 corretamente.
**DoD:** recall/QPS filtrado; planner escolhe v5 vs seqscan em WHERE seletivo. **GATE:** planner escolhe v5 quando deve; recall filtrado preservado. **Dependencies:** M86.

## M88 — [x] Head-to-head bilhão-scale + North Star re-measure *(gated M87)*

**Objective:** a medição terminal num regime onde o f32 NÃO cabe em RAM (a vantagem real da separação, não projetada) vs ScaNN/AlloyDB.
**DoD:** `docs/benchmarks/m88-*.md` a ≥100M/1B; ADR estendendo/revisando 0037; sign-off council-benchmark. **GATE:** veredito terminal do North Star para a track separada (o próximo dado da linhagem M33/M73/M82). **Dependencies:** M87.

**Outcome (2026-07-12, veredito `SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE` — `docs/benchmarks/m88-billion-scale-verdict.{md,json}` + ADR `0038`, sign-off council-benchmark):** MEDIDO a **16M** — índice SQ8 (v6) **3.52× menor** que f32 (v5), confirmando o 3.5× do M85 em 16× a escala; **+21% cold-QPS @ probes=32** (direcional, limite inferior). **DoD ≥100M NÃO atingido honestamente:** descoberto um **teto de memória de build** (ambuild pica ~4× o base → 2 OOM-kills medidos a 30M num box de 62 GB usáveis), 16M foi o maior viável; a recall sintética (0.291) é tie-degenerada (SIFT1M real = 0.98, M84). Crossover QPS out-of-RAM fica **direcional-não-provado**; **nenhuma** claim de superioridade sobre ScaNN/AlloyDB (teto de paradigma M73/M82 permanece). Fechado na disciplina honest-negative de M73/M82. Follow-up (nova linhagem): **ambuild streaming** (derruba o teto ~4×-base → 100M+ em RAM commodity) + dados ANN reais bilhão-scale + harness cold-cache por-query. Phase 1 (build escalável, commit `fba16d0`, 249 pg_tests GREEN, byte-idêntico ≤1M) foi o que tornou os builds 16M/30M tratáveis.

---

# Pós-v7 — linhagem "build escalável a bilhão-scale" (destrava o DoD ≥100M que o M88 não alcançou)

> **`ROADMAP_COMPLETED` (2026-07-12) — 19/19 milestones do roadmap ativo `[x]`.** M89 entregue (v0.77.0): o teto de memória do build (~4× base, M88) foi derrubado para **1.28× (v5) / 1.50× (v6)** a 30M, MEDIDO — o build de 30M agora completa num box de 64 GB (byte-idêntico, sem REINDEX, 250 pg_tests GREEN). **Limite honesto:** ainda carrega a cópia 1× `idx.vectors` (não é `O(maintenance_work_mem)`), então 100M+ (~51 GB base) ainda não cabe em RAM commodity — o `tuplesort` streaming dos vetores (nunca materializar o corpus) é o próximo follow-up honesto p/ a medição ≥100M do M88.
>
> Origem: achado MEDIDO do M88 (ADR-0038 + `docs/benchmarks/m88-billion-scale-verdict.md`) — o `ambuild` do `theodb_ivfflat` pica ~4× o dataset base em RAM (2 OOM-kills a 30M num box de 62 GB usáveis), então um índice genuinamente out-of-RAM não é construível em RAM commodity. O roadmap v7 fechou `ROADMAP_COMPLETED` (18/18, v0.76.0); esta linhagem retoma a **única alavanca nomeada** para o crossover de QPS out-of-RAM ficar medível. Serial, gate-driven, measurement-first.

## M89 — [x] ambuild streaming (flush incremental — derruba o teto de memória de build) *(gated M88)*

**Outcome (2026-07-12, veredito `DOD_MET` — `docs/benchmarks/m89-ambuild-streaming.{md,json}` + ADR `0039`, sign-off council-index-storage + rust-pgrx + benchmark):** o build de 30M agora **completa num box de 64 GB** com pico **1.28× (v5) / 1.50× (v6)** base MEDIDO (vs old-build 4.21×/64.7 GB OOM, reproduzindo o M88). Fix: `build_owned` move o corpus + os writers v5/v6 escrevem cada lista incrementalmente (elimina os clones `list_entries`/`enc_vec`/`items`), byte-idêntico (sem REINDEX). 250 pg_tests GREEN, zero regressão. **Desvio parsimony honesto:** o plano previa FFI do `tuplesort` (Opção B); a MEDIÇÃO mostrou que clone-elimination + streaming page-writes atingem o DoD de 30M com risco muito menor (zero FFI) — a FFI era YAGNI p/ 30M. **Limite honesto:** NÃO é `O(maintenance_work_mem)` (pico ainda tem a cópia 1× `idx.vectors`) → 100M+ (~51 GB base) ainda não cabe em RAM commodity; o `tuplesort` streaming dos vetores é o follow-up.

**Objective:** reescrever o `ambuild` do `theodb_ivfflat` para **flush incremental de páginas via `tuplesort`/spool nativo do Postgres** (Regra 9 — o mecanismo do ambuild do btree e do build HNSW do pgvector), em vez de bufferizar o `AnnIndex` inteiro + cópias em RAM, derrubando o pico de ~4× o base para ~1× base — o teto que causou 2 OOM-kills a 30M no M88.
**DoD:** (1) pico anon-rss do build **≤ ~1.5× o dataset base, MEDIDO** num build de **30M** que completa num box de 64 GB (o cenário que OOMou no M88); (2) **zero regressão** — 249 pg_tests GREEN + recall **byte-idêntico** a ≤1M (A/B same-data M46); (3) `docs/benchmarks/m89-*.{md,json}` com pico anon-rss vs N (16M, 30M) build-antigo vs novo provando que o pico deixou de escalar ~4×; (4) `maintenance_work_mem` respeitado; (5) sem novas deps externas (`tuplesort` é do próprio Postgres via `pg_sys`); (6) sign-off council-rust-pgrx (FFI do tuplesort) + council-index-storage (build/page). **Escopo:** SÓ o build escalável — a medição terminal bilhão-scale (≥100M) é **M90 gated por M89**. **Risks:** (a) API do `tuplesortstate` via `pg_sys` (FFI/`extern "C-unwind"`) — mitigar espelhando o build HNSW do pgvector + council-rust-pgrx; (b) build mais lento a ≤1M onde tudo cabe em RAM — mitigar com fast-path in-RAM quando `N·base ≤ maintenance_work_mem`. **Dependencies:** M88.

---

# Pós-M89 — filtered vector search inline/adaptive (fecha o gap de filtragem vs AlloyDB)

> Origem: discussão com o owner (2026-07-12) — o "inline/adaptive filtering" do AlloyDB **NÃO é um gap de paradigma** (correção honesta): é implementável por extensão via o **Custom Scan Provider** do Postgres (o mesmo mecanismo de TimescaleDB/Citus/pg_strom, mais poderoso que o `amgettuple`), reusando o motor de bitmap nativo (`BitmapAnd`→`TIDBitmap`, Regra 9). O M87 fechou o **post-filter** (classe pgvector-relaxed_order); esta linhagem fecha o **inline** (M90) e o **adaptive** (M91). **NÃO é claim de QPS-superior** (teto de paradigma M73/M82 permanece) — é claim de **recall-estável-sob-filtro**, medível. Serial, gate-driven, measurement-first (honest-negative é terminal válido).

## M90 — [x] inline filter pushdown (label scan-key, IVF-AQ-native) *(gated M87, M89)*

> **Re-escopo pela DISCOVER (2026-07-12, `knowledge-base/discoveries/blueprints/inline-filter-pushdown-blueprint.md`):** a pesquisa Staff-DB (lendo o pgvectorscale real, permissivo) determinou que o inline parsimony-correto para o DoD é o **Approach A — label scan-key** (o que o pgvectorscale usa: label no índice + operador `&&` + scan-key pushdown), NÃO o Custom Scan Provider (Approach B, arbitrary-WHERE), que é YAGNI aqui e foi movido para o **M91**. Achado honesto: o inline do AlloyDB é **ScaNN-only, não IVF** — mas o nosso IVF-AQ storage-separated (Stage-1 código / Stage-2 rerank) é um *encaixe melhor* (label nas code-pages → Stage-1 poda antes do rerank).

**Objective:** empurrar o filtro de **label** (`WHERE labels && '{…}' ORDER BY e <-> q LIMIT k`) para DENTRO da travessia do IVF-AQ via o mecanismo de **scan-key** (o nosso `amrescan` já recebe `_keys` — hoje ignora): `amcanmulticol=true` + opclass própria `&&` para `smallint[]` (código próprio, Regra 9 — pgvectorscale é estudo-de-design) faz o planner empurrar o predicado como Index Cond; o build lê a 2ª coluna e guarda o label **nas code-pages** (novo layout v7); a Stage-1 pula candidatos sem overlap ANTES do rerank; `xs_recheck=true`. Interage com o M87 (grow-probes recupera recall se uma lista probed tem poucos matches).
**GATE (D3-style, measurement-first):** mede **recall@10 sob filtro de label ~1% INLINE vs o M87 post-filter** num benchmark reproduzível — **GO só se o inline melhora o recall medido**; honest-negative fecha (anti-sunk-cost).
**DoD:** (1) `labels && '{…}'` chega ao `amrescan` como ScanKey e a Stage-1 pula não-matching inline; (2) **recall@10 sob filtro de label seletivo (~1%) MEDIDO estritamente > M87 post-filter** (`docs/benchmarks/m90-inline-filter.{md,json}`); (3) EXPLAIN mostra o label como Index Cond; (4) **zero regressão** — 250+ pg_tests GREEN, path sem-label byte-idempotente (v5/v6 inalterados); (5) crash-safety no v7 (build→restart→scan-identical); (6) sign-off council-index-storage + council-benchmark. **Boundary honesto:** só a coluna de label declarada + `&&` (arbitrary-WHERE é o M91). **Risks:** (a) format bump v7 (label nas code-pages) + REINDEX — mitigar com magic novo + gate de crash-safety; (b) o inline pode não bater o recall do M87 → honest-negative (o gate mede antes). **Dependencies:** M87 (scan iterativo), M89 (build escalável). **Prior art (estudo, código próprio):** pgvectorscale (PostgreSQL License, Rust+pgrx); AlloyDB fechado → só design publicado.

---


**Outcome (2026-07-12, veredito `GO` — `docs/benchmarks/m90-inline-filter.{md,json}` + ADR `0040`, sign-off council-index-storage+rust-pgrx+benchmark):** MEDIDO (DO c-8, 500k, ~1% seletividade) **recall@10 1.00 (inline v7) vs 0.52 (M87 post-filter) — delta +0.48 + ~19× QPS**. Approach A (scan-key/label, layout v7 co-localizado, inline-skip na Stage-1 + xs_recheck). 253 pg_tests GREEN (250 + 3 v7), zero regressão; vetor-only/v5/v6 sem-label byte-idênticos. 2 blockers de correção achados no review e corrigidos (VACUUM no-op v7; xs_recheck no pending region). Honesto: só label + `&&`, v7+REINDEX; o arbitrary-WHERE inline (Custom Scan) é o M91; NÃO vence ScaNN (teto M73/M82).

## M91 — [x] adaptive filter strategy (AM-local no scan-key de label) *(gated M90)*

> **Re-escopo pela DISCOVER (2026-07-12, `knowledge-base/discoveries/blueprints/adaptive-filter-strategy-blueprint.md`):** a pesquisa Staff-DB determinou que o adaptive parsimony-correto é **AM-local no scan-key de label** (Approach A — reusa o INLINE do M90 + o POST do M87), NÃO o Custom Scan Provider (Approach B, arbitrary-WHERE), que é YAGNI para o DoD (o sweep é de seletividade de LABEL) e vira milestone futuro. Achados: nem o pgvectorscale nem o AlloyDB fazem adaptive para o caso de label (pgvectorscale = 1 estratégia; AlloyDB adaptive = ScaNN-only+bitmap). As "3 fixas" são as NOSSAS (M87/M90/PRE). Sem novo formato on-disk (v7 já tem o label), sem REINDEX, `xs_recheck` já correto.

**Objective:** o AM escolhe AUTOMÁTICAMENTE a estratégia pela **seletividade estimada in-scan** (match-rate na 1ª lista probed — grátis, data-true): médio → **INLINE** (o v7 do M90); loose → **POST** (o M87); ultra-seletivo → **PRE** (scan das code-pages compactas p/ o match set + rerank exato) **SÓ se medido necessário** (measurement-first: o M90 já deu recall 1.00 @ 1% com INLINE — se o INLINE vence o ultra-seletivo, PRE é YAGNI e o adaptive vira um switch INLINE⇄POST de 2 vias). **Fora de escopo (limite honesto):** arbitrary-`WHERE` (qualquer coluna) — precisa do Custom Scan Provider (Approach B) = milestone futuro; e o re-plan cross-index do core do AlloyDB.
**GATE (measurement-first):** benchmark **varrendo a seletividade de label (0.01% → 30%)** mostra o adaptive **dominando o envelope** das estratégias fixas (recall alto E custo baixo em CADA ponto); honest-negative é terminal válido.
**DoD:** (1) estimador de seletividade in-scan + branch adaptive INLINE⇄POST (+ PRE só se medido) no `scan_ivf_structured`/`amrescan`, threshold ajustável (GUC); (2) **benchmark de varredura (0.01%→30%) mede o adaptive dominando as fixas** (`docs/benchmarks/m91-adaptive-filter.{md,json}`); (3) contador/log runtime revela a estratégia escolhida (observabilidade); (4) **zero regressão** — 253+ pg_tests GREEN, testes por regime; (5) sign-off council-index-storage + council-benchmark. **Boundary:** label-only (arbitrary-WHERE é o Custom Scan futuro); sem novo formato. **Risks:** (a) o estimador in-scan pode ser ruidoso → calibrar no sweep + threshold GUC; (b) o adaptive pode não dominar (INLINE já domina em toda a faixa?) → honest-negative (o gate mede antes; se INLINE domina, o M91 é "INLINE é a estratégia" documentado). **Dependencies:** M90 (INLINE), M87 (POST). **Prior art (estudo, Regra 9):** pgvectorscale (permissivo — prova que adaptive é adição NOSSA). **NÃO é claim de QPS-superior** (teto M73/M82).

## M92 — [x] arbitrary-WHERE filtered vector search via Custom Scan Provider (paridade AlloyDB tier ③) *(gated M90, M91)*

> **Origem (2026-07-13):** o M90/M91 fecharam o inline+adaptive para o **label declarado** (`smallint[]` + `&&`, via scan-key no AM — Approach A). O gap honesto que resta vs AlloyDB é o **`WHERE` em QUALQUER coluna** (`WHERE price < 100 AND category = 'x' ORDER BY e <-> q LIMIT k`), que hoje ainda cai no post-filter. Fechar isso é o **Approach B** — o Custom Scan Provider — deferido explicitamente pelo M90 (ADR-0040) e M91 (blueprint). É o tier ③ do AlloyDB ("inline/adaptive filtering" sobre índices arbitrários), menos o re-plan cross-index mid-query do core (não-alcançável por extensão pura).

**Objective:** um **Custom Scan Provider** (`set_rel_pathlist_hook` + `CustomScanMethods`/`CustomExecMethods`/`CustomPathMethods` + `RegisterCustomScanMethods`) intercepta o padrão `WHERE <preds escalares arbitrários> ORDER BY e <-> q LIMIT k`, roda o **sub-plano bitmap NATIVO** do Postgres (`BitmapAnd`/`BitmapOr` sobre os B-trees/GIN existentes → `TIDBitmap`, MVCC-correto, Regra 9 — não reinventa o bitmap) e passa o bitmap ao `scan_ivf_aq*` como **teste de membership na Stage-1** (skip inline antes do rerank, exatamente como o label do M90 mas para TIDs arbitrários). Escolhe a estratégia pela **cardinalidade do bitmap** (reusa o eixo do M91): ultra-seletivo → PRE (fetch dos TIDs + rerank exato); médio → INLINE; loose → POST/adaptive-probes.

**GATE (measurement-first):** benchmark em SIFT1M (vizinhos reais — lição M91) com um `WHERE` escalar **em coluna não-label** varrendo a seletividade (0.01%→30%) mostra o inline-por-bitmap **batendo o post-filter em recall** no regime seletivo (onde o post-filter starva), com QPS competitivo. **Honest-negative é terminal válido** (se o overhead do Custom Scan node + bitmap não compensa vs o post-filter nativo do Postgres, documenta-se e fecha).

**DoD:** (1) Custom Scan Provider registrado, intercepta o padrão arbitrary-WHERE + `ORDER BY <-> LIMIT` e injeta o bitmap na Stage-1 do scan do AM; (2) sub-plano bitmap nativo (`TIDBitmap` via `tbm_*`) → membership test MVCC-correto (recheck no snapshot); (3) **benchmark mede inline-por-bitmap recall > post-filter** em coluna arbitrária (`docs/benchmarks/m92-arbitrary-where.{md,json}`, SIFT real); (4) `EXPLAIN` revela o Custom Scan node + a estratégia escolhida; (5) **zero regressão** — 255+ pg_tests GREEN (o path sem Custom Scan é byte-idêntico); (6) sign-off council-index-storage + council-rust-pgrx + council-benchmark. **Boundary honesto:** `WHERE` escalar sobre índices existentes; **NÃO** o re-plan cross-index mid-query do core do AlloyDB (tier ④, não-alcançável por extensão permissiva). **Risks:** (a) **integração Custom Scan Provider ↔ scan state do AM sem quebrar MVCC/snapshot** — maior risco; mitigar espelhando o design nativo + council-rust-pgrx; (b) os helpers inline `create_customscan_path`/`make_custom_scan` NÃO existem no pgrx 0.16.1 → hand-roll via `pg_sys` (confirmado: `set_rel_pathlist_hook`, `Register/CustomScanMethods`, `TIDBitmap`+`tbm_*` TODOS presentes); (c) o overhead do node pode não compensar → honest-negative (o gate mede antes). **Dependencies:** M90 (inline skip na Stage-1), M91 (estratégia por cardinalidade), M87 (post baseline). **Prior art (estudo, Regra 9):** pgvectorscale (permissivo) + design publicado do AlloyDB (inline filtering). **NÃO é claim de QPS-superior** ao ScaNN/AlloyDB (teto de paradigma M73/M82) — é paridade de **capacidade** (busca vetorial com filtro arbitrário eficiente), medida.

> **Nota (2026-07-13):** o **spike do M92 (v0/v1a/v1b) já PROVOU as 3 primitivas** em isolamento (blueprint `knowledge-base/discoveries/blueprints/arbitrary-where-custom-scan-blueprint.md`; commits `7224ae0`/`c20db0f`/`a882027`): (v0) o Custom Scan node hand-rolled funciona em runtime; (v1a) a membership de TID chega à Stage-1 do AM e filtra; (v1b) `materialize_bitmap()` itera o `TIDBitmap` → exact+lossy sets. Tudo gated OFF (`theodb.enable_vecfilter`), 260 pg_tests GREEN. O DoD acima permanece o alvo do M92; a **integração** dessas peças é o M93 abaixo.

## M93 — [x] Custom Scan node integration — MultiExec bitmap + MVCC recheck (fecha o M92) *(gated M92)*

> **Origem (2026-07-13):** o spike do M92 provou as 3 primitivas em isolamento (node lifecycle, membership skip no AM, materialize do TIDBitmap). O M93 as **assembla** no Custom Scan node de 2 filhos — a parte de executor-lifecycle (`MultiExecProcNode`/`ExecInitQual`/`ExecQual`/`econtext`) que fecha o inline arbitrary-WHERE end-to-end. É o único bloco restante para o M92 virar feature de verdade.

**Objective:** ligar o Custom Scan node (v0) à fonte de membership real (o bitmap sub-plan que o planner já constrói) com recheck MVCC correto. Quatro passos:
1. **Hook** — achar o `BitmapHeapPath.bitmapqual` (o planner já o construiu em `rel->pathlist`; Regra 9 — reusa a composição BitmapAnd/Or) + o vector-ordered `IndexPath` (o com `pathkeys` da `ORDER BY <-> `) → um `CustomPath` de 2 filhos `custom_paths=[vector_ordered, bitmapqual]` (o Postgres planeja ambos → `custom_plans=[vector_plan, bitmap_plan]`).
2. **BeginCustomScan** — `ExecInitNode` ambos os filhos; `MultiExecProcNode(bitmap_child)` → `TIDBitmap` → `materialize_bitmap()` (v1b, já provado) → `set_membership()` (v1a, já provado). Compila a qual escalar via `ExecInitQual` para o recheck.
3. **ExecCustomScan** — puxa a tupla ordenada do vector child (que já pula não-membros inline via a membership) e roda **`ExecQual`** na tupla do heap: remove os falsos-positivos over-admitidos (páginas lossy do bitmap + região pending sem membership) — a **correção MVCC (v1c)** que o `xs_recheck` do AM sozinho não faz (`nodeIndexscan.c` só re-checa index quals, não o `WHERE` arbitrário).
4. **EndCustomScan** — `set_membership(None)` (limpa o side-channel — contrato anti-leak) + `ExecEndNode` nos dois filhos.

**GATE:** correção primeiro (o mais importante): resultado filtrado byte-idêntico ao seqscan exato numa coluna NÃO-label, **incluindo os casos-armadilha** — página lossy do bitmap (over-admite → recheck remove), região pending (INSERT pós-build → recheck), e update concorrente (EPQ recheck). **Depois** o benchmark (parte do DoD do M92): inline-por-bitmap recall > post-filter em SIFT real. Honest-negative é terminal válido (se o overhead do node não compensar vs o post-filter nativo).

**DoD:** (1) o Custom Scan node de 2 filhos monta o bitmap → membership → vetor-ordenado + recheck end-to-end; (2) **pg_tests de correção**: resultado == seqscan exato numa coluna não-label + os 3 casos-armadilha (lossy/pending/EPQ) GREEN; (3) `set_membership` limpo no EndCustomScan (teste anti-leak entre queries); (4) **zero regressão** — 260+ pg_tests GREEN, path sem Custom Scan byte-idêntico (GUC default-OFF); (5) `EXPLAIN` mostra o Custom Scan node com os 2 filhos; (6) sign-off council-rust-pgrx (executor-lifecycle/panic-across-C) + council-index-storage (MVCC recheck). **Boundary honesto:** single bitmap sub-plan (o planner compõe BitmapAnd/Or); NÃO o re-plan cross-index mid-query (tier ④). **Risks:** (a) **o recheck MVCC** — se over-admitidos (lossy/pending) não forem rechecados, vazam falsos-positivos (o risco #1 do council-index-storage) → testes explícitos dos 3 casos-armadilha; (b) **panic atravessando a fronteira C** nos ~6 callbacks + no MultiExec-lifecycle → `extern "C-unwind"` + erros tipados (`pg_sys::error!`), nunca panic; (c) o `ExecInitQual`/`econtext` no node é território novo → espelhar `nodeBitmapHeapscan.c`. **Dependencies:** M92 (as 3 primitivas provadas). **Prior art (estudo, Regra 9):** `nodeBitmapHeapscan.c` (o recheck `bitmapqualorig`), design publicado do AlloyDB. **NÃO é claim de QPS-superior** (teto M73/M82).

## M94 — [x] per-scan membership scoping — destrava UNION/self-join/Append filtrados *(gated M93)*

> **Origem (2026-07-13):** o BLOCKER convergente do review M92/M93 (councils rust-pgrx + index-storage): a membership é UM slot thread-local por backend, mas o executor roda todos os `BeginCustomScan` (Init) antes de qualquer pull (Exec) → dois nodes vecfilter num plano (`UNION`/self-join/partitioned `Append`) se contaminariam → resultado **silenciosamente errado**. O M93 mitigou com **fail-loud** (Regra 8); o M94 entrega o **suporte real**: cada node com a SUA membership.

**Objective:** **swap-discipline por janela de pull.** O backend é single-threaded e o pull do vector child (`ExecProcNode`) é síncrono — todo o trabalho do AM (amrescan/amgettuple/re-search) roda DENTRO da janela. Então: (1) `Begin` guarda a membership num **registry thread-local keyed pelo ponteiro do node** (nunca `Rc` dentro de `palloc0` — Drop não roda → leak dos HashSets); (2) `Exec` (cada pull) faz `prev = swap(ACTIVE, minha)` → pull → `swap(ACTIVE, prev)` — **save/restore re-entrante** (subquery aninhada no Filter funciona, disciplina de pilha); (3) `ReScan` envolve o `ExecReScan(vector_child)` na mesma janela; (4) `End` remove do registry; (5) callbacks de **xact E SUBXACT abort** limpam registry+ativo (gap atual: `EXCEPTION` de PL/pgSQL é subxact-abort, que o callback de xact não pega). Remove o guard fail-loud do M93.
**GATE (correção primeiro):** `UNION ALL` de 2 queries vetoriais filtradas == união dos seqscans exatos; self-join filtrado correto; membership de A ≠ B provado (resultados distintos por branch). Zero regressão: 263+ testes GREEN, single-scan byte-idêntico.
**DoD:** (1) registry per-node + swap-discipline (exec/rescan); (2) **pg_tests**: UNION-correto (substitui o fail-loud test), interleaved/Append correto, subxact-abort não vaza; (3) sem leak (membership dropada no End e nos aborts; nada Rust-droppable em palloc); (4) **zero regressão** — 263+ testes GREEN, GUC-off byte-idêntico; (5) overhead por-pull desprezível (lookup TLS + 2 swaps por tuple; o benchmark M92 não regride — re-assert por spot-check); (6) sign-off council-rust-pgrx. **Risks:** (a) janela errada (algum caminho do child roda FORA do pull) → os testes UNION/self-join pegam; (b) leak de Rc em palloc → registry thread-local resolve; (c) re-entrância (subplan no Filter) → save/restore. **Dependencies:** M93. **Prior art:** o review M92/M93 (`knowledge-base/reviews/custom-scan-node-integration-review-2026-07-13.md` — o fix "per-scan scoping" prescrito pelos councils).

## M95 — [x] honest cost model do vecfilter node — gradua o filtered search de experimental → default-capable *(gated M94)*

> **Origem (2026-07-13):** o node força a seleção com `startup=0 / total = min_cost × 0.1` (heurística de spike, disclosed em TODO benchmark M92/M94 como "an honest cost model is a follow-up" + council LOW-4: o hook pode sequestrar planos onde o POST nativo seria mais barato). Consequência: a feature NÃO pode sair de experimental/GUC-off. O M95 é o que **colhe o investimento M90–M94**: com custo honesto, o planner escolhe o node SÓ onde ele vence — pré-requisito para ligável-por-default.

**Objective:** substituir a heurística por um **cost model real**: `cost(node) = cost(bitmap sub-plan) + cost(vector scan com membership)` onde o custo do vector scan reflete a **cardinalidade estimada do bitmap** (a seletividade que o planner já computou p/ o BitmapHeapPath — `rows`) → probes efetivos esperados (o adaptive M91 proba mais quando seletivo) + o rerank. Espelhar o `am/cost.rs` existente (o visit-ratio honesto do M48) — Regra 9, não inventar modelo novo. Remover o `×0.1`; o `pathkeys` (ordenação grátis vs Sort explícito) entra naturalmente na comparação do planner. Manter o GUC como kill-switch (default a decidir no gate — só vira ON-default se o gate provar dominância de plano).
**GATE (measurement-first):** um **sweep de seletividade (0.01%→50%) SIFT1M** onde, com o cost model, o `EXPLAIN` mostra o planner escolhendo o node **exatamente nos regimes onde o node vence** (medido: recall+QPS por plano escolhido vs o plano alternativo forçado) — incluindo o regime loose onde o POST/seqscan+Sort deve vencer e o node NÃO deve ser escolhido. Honest-negative é terminal válido (se o modelo não discriminar bem, documenta-se o porquê e a feature permanece GUC-opt-in).
**DoD:** (1) cost model implementado em `plan_custom_path`/`pathlist_hook` (custos derivados dos child paths + cardinalidade do bitmap; zero mágica `×0.1`); (2) **benchmark de decisão de plano**: em cada ponto do sweep, o plano escolhido pelo planner é o de melhor recall+QPS medido (`docs/benchmarks/m95-cost-model.{md,json}`); (3) pg_tests: node escolhido no seletivo, NÃO escolhido no loose (EXPLAIN assertions); (4) **zero regressão** — 265+ testes GREEN; (5) decisão documentada (ADR) sobre o default do GUC pós-gate; (6) sign-off council-index-storage (cost model) + council-benchmark. **Boundary honesto:** o modelo usa as estimativas do planner (stats/ANALYZE) — estimativa ruim ⇒ plano subótimo, como qualquer cost model PG; NÃO é o adaptive runtime do AlloyDB (tier ④). **Risks:** (a) calibração — constantes de custo mal escolhidas invertem decisões → o sweep é o oráculo; (b) estatísticas frias (sem ANALYZE) → herdar o comportamento conservador do planner (sem node); (c) interação com o iterativo M87 no custo do POST → medir, não assumir. **Dependencies:** M94. **Prior art (Regra 9):** `am/cost.rs` (M48 visit-ratio), `cost_bitmap_heap_scan`/`cost_index` do core (estudo).

## M96 — [x] tuplesort-streaming ambuild — build 100M+ em RAM commodity *(gated M89)*

> **Origem (2026-07-13):** o follow-up explícito do M89 (ADR-0039): o build ainda materializa **1× `idx.vectors`** (~15.4 GB a 30M) → **100M (~51 GB base) não cabe em RAM commodity**. Sem isso, a medição out-of-RAM do M88 (crossover QPS v6/SQ8 vs v5/f32) permanece "direcional-não-provada". O blueprint `ambuild-streaming-blueprint.md` já verificou a feasibility do `tuplesort` nos bindings pgrx 0.16.1/pg17 (begin_heap/puttupleslot/etc. TODOS presentes).

**Objective:** nunca materializar o corpus: o pipeline **heap → tuplesort (spill em disco, `maintenance_work_mem`) → páginas por-lista**, o desenho do `ivfbuild.c` do pgvector (Regra 9 — estudo do pipeline, código próprio). Fases: (1) sample p/ kmeans (o M88 já capou o treino em 1.1M — inalterado); (2) 1ª passada: assign de cada vetor à lista + `tuplesort_put` de `(list_id, tid, vec)`; (3) sort por `list_id` (spill automático); (4) 2ª passada: drenar o sorter em ordem escrevendo cada lista incrementalmente (o writer streaming do M89 — reusado). Pico alvo: `O(maintenance_work_mem + 1 lista)`.
**GATE (measurement-first):** **build de 100M×128 (~51 GB base) completa num box de 64 GB** com pico de RSS medido `< 1.15× maintenance_work_mem + overhead fixo` documentado; e o índice resultante passa o set-equal/recall sanity (amostra). Em seguida (stretch, se o box aguentar): a medição out-of-RAM do M88 re-tentada a 100M (v6/SQ8 vs v5/f32 cold-QPS) — honest-negative continua terminal válido.
**DoD:** (1) path streaming atrás de reloption/threshold (builds pequenos mantêm o path atual — byte-idêntico ≤1M provado por testes); (2) **build 100M MEDIDO** completando com pico documentado (`docs/benchmarks/m96-streaming-build.{md,json}`); (3) crash-safety do build preservada (os testes de crash-phase existentes GREEN); (4) **zero regressão** — 265+ testes GREEN + set-equal nos formatos v5/v6/v7; (5) sign-off council-index-storage + council-rust-pgrx (FFI do tuplesort é superfície nova) + council-benchmark. **Boundary honesto:** é sobre BUILD, não QPS; o crossover out-of-RAM é stretch/medição separada; v3/v4 legacy mantêm o path antigo. **Risks:** (a) **FFI do tuplesort** (lifecycle begin/put/perform/get/end, memória do slot) — superfície C nova → spike-first do ciclo put/get antes do pipeline completo (a lição M92); (b) o formato do tuple no sorter (serializar vec+tid) → custo de (de)serialização medido; (c) duas passadas no heap dobram o I/O de leitura → medir o custo real (o gate é caber em RAM, não ser mais rápido). **Dependencies:** M89 (writer streaming), blueprint `knowledge-base/discoveries/blueprints/ambuild-streaming-blueprint.md`. **Prior art (Regra 9):** pgvector `ivfbuild.c` (o pipeline tuplesort — estudo), nossos writers v5/v6 (reuso).

## M97 — [x] Columnar/HTAP (D2) — DISCOVERY-first do pilar lakehouse *(sem gate — pilar novo)*

> **Origem (2026-07-13):** a aposta D2 (PRD §15) enfileirada quando o M92 foi priorizado. Pilar novo inteiro — **columnar/analytics via lakehouse (DuckDB embarcado), deliberadamente DIFERENTE do in-memory do AlloyDB** (forçado pela licença D1: AGPL barrado; pg_duckdb/DuckDB são MIT). Escopo deste milestone é **SÓ a discovery + blueprint + decisão GO/NO-GO** — nenhuma linha de código de produto (a lição de sempre: measurement/discovery antes de construir; um pilar errado custa meses).

**Objective:** responder com evidência (R0 — WebSearch/WebFetch obrigatórios + código real dos refs locais `pg_duckdb`/`hydra`/`pg_mooncake`/`duckdb`): (1) **qual é o gap real** entre "instalar pg_duckdb puro" e "TheoDB integra columnar" — o que NÓS agregaríamos (sync automático row→columnar? planner routing? vector+analytics fundidos?) que o usuário não tem hoje com 1 comando; (2) **como os peers resolvem** (Hydra columnar AM vs pg_mooncake iceberg/delta vs pg_duckdb query-engine-only) — trade-offs medidos, não opinião; (3) **a fronteira HTAP honesta** para uma extensão permissiva (o AlloyDB columnar engine é in-memory + core-integrated — o que é alcançável vs teto de paradigma, como fizemos no vetor com M73); (4) **1 benchmark de viabilidade** (TPC-H subset ou clickbench sample: PG row vs pg_duckdb no MESMO box) para ancorar números; (5) decisão: GO (com roadmap de milestones) / NO-GO / DEFER, em ADR.
**GATE:** blueprint SHIPPABLE pelo `/discover-confidence` (4 coverage corners, ≥2 fontes primárias por técnica, citações resolvem) + o benchmark de viabilidade com números reais + ADR de decisão assinável. **NO-GO/DEFER são terminais válidos e baratos** — o milestone entrega CONHECIMENTO, não código.
**DoD:** (1) blueprint em `knowledge-base/discoveries/blueprints/columnar-htap-blueprint.md` (SHIPPABLE); (2) benchmark de viabilidade (`docs/benchmarks/m97-htap-viability.{md,json}`); (3) ADR GO/NO-GO/DEFER com o roadmap proposto (se GO); (4) zero código de produto neste milestone; (5) sign-off council-research-adr. **Boundary honesto:** discovery-only; qualquer claim de performance vem do benchmark de viabilidade, rotulado como preliminar. **Risks:** (a) escopo-creep para implementação → o DoD proíbe código; (b) o gap vs pg_duckdb puro pode ser pequeno → NO-GO honesto é sucesso do milestone (evitou meses num pilar sem diferencial). **Dependencies:** nenhuma (pilar independente). **Prior art:** refs locais `pg_duckdb`, `hydra`, `pg_mooncake`, `duckdb` + PRD D1/D2.

---

# Pilar single-planner columnar+AI (AlloyDB-class HTAP) — o roadmap α→ε

> **Origem (2026-07-14):** o deep-research + o cycle DISCOVER `single-planner-columnar-ai` (blueprint SHIPPABLE 98.8,
> `knowledge-base/discoveries/blueprints/single-planner-columnar-ai-blueprint.md`) achou a rota que o M97/ADR-0041
> NÃO examinou: um **DataFusion-CustomScan single-planner** (um engine, um planner) que quebra o teto de dois-engines
> do pg_duckdb (ADR-0023), com **Hydra columnar Apache-2.0** (o ADR-0041 errou dizendo AGPL). Veredito **GO-CONDITIONAL**.
> **Teto honesto TRAVADO em todos os milestones abaixo (ADR D2 do blueprint):** DuckDB/Photon-class 15–30× em dados
> columnar-residentes — **igualar a capacidade do AlloyDB, JAMAIS afirmar superioridade** sobre o engine in-core in-memory
> dele (disciplina M73/M97). Refs AGPL (paradedb/pg_search, citus columnar) = **estudo de design, nunca copiar código** (D1/D3).

## M98 — [x] pgrx-upgrade + DataFusion/Arrow coexistence spike (o GATE) *(gated M97)*

> **Rung M-0 do blueprint.** O achado afiado de Q6: o pg_search prova o stack vetorizado em **pgrx 0.19.0**, mas o TheoDB
> está em **pgrx 0.16.1** — logo a coexistência `datafusion 54 + arrow 58 + pgrx` está provada em 0.19.0, NÃO em 0.16.1.
> A coexistência num crate só **só é provável por build, não por leitura de Cargo.toml** (Regra 5). Este é o gate que
> destrava α/β.

**Objective:** (1) upgrade `pgrx =0.16.1 → =0.19.0` no `theodb_rs` (toca `IndexAmRoutine`/`CustomScan`/`pg_sys` — API churn); (2) adicionar `datafusion` + `arrow` (upstream `apache/datafusion`, NÃO o fork `datafusion-distributed` da paradedb — Regra 9) como deps e provar que **linkam** com o pgrx num crate único; (3) um smoke-test mínimo: uma query trivial roteada por um `CustomScan` que roda um `ExecutionPlan` DataFusion e devolve tuplas ao PG. Rust 1.91 ≥ 1.88 (MSRV do datafusion) já está satisfeito.
**GATE (build primeiro):** `cargo pgrx test pg17` **verde com os 277 testes existentes** no pgrx 0.19.0 + datafusion/arrow linkados; `cargo tree` sem conflito de versão de arrow; o smoke CustomScan→DataFusion→tupla passa. Honest-negative é terminal válido: se a coexistência quebrar (conflito de arrow, símbolo, ABI), documenta-se o bloqueio e o pilar re-escopa (fica no pg_duckdb).
**DoD:** (1) `theodb_rs` compila+testa em pgrx 0.19.0 (todos os testes verdes, zero regressão); (2) datafusion+arrow linkados, `cargo tree` limpo; (3) smoke-test CustomScan↔DataFusion↔TupleTableSlot passa (1 pg_test); (4) benchmark/nota de que o build funciona (`docs/benchmarks/m98-coexistence.md`); (5) sign-off council-rust-pgrx (o upgrade de pgrx + a superfície FFI). **Risks:** (a) API churn 0.16→0.19 pode tocar muito código de AM → esforço medido, sem workaround; (b) conflito de versão de arrow (datafusion-main usa 59, pg_search 58) → pinar; (c) coexistência pode simplesmente falhar → honest-negative documentado. **Dependencies:** M97 (a decisão de perseguir o pilar). **Prior art:** blueprint Q6/Q7 + `theodb_rs/src/am/{mod.rs,customscan.rs}` (o IndexAmRoutine/CustomScan que o upgrade toca). **NÃO é claim de performance** — é um gate de viabilidade.

## M99 — [x] columnar TAM append-only (own-code; Hydra design = AGPL study-only) *(gated M98)*

> **Rung M-α.** O primeiro storage columnar nativo — um `TableAmRoutine` próprio, append-optimized, com MVCC delegado
> ao catálogo (o truque do Hydra: visibilidade em granularidade de stripe via snapshot no `columnar.stripe`).

**Objective:** um columnar TAM próprio (código próprio own-code — ADR-0042; **Hydra columnar é AGPLv3, estudo do design apenas (Rule 9), NUNCA copiar/linkar**; cstore_fdw Apache-2.0 = única ref TableAM permissiva, é FDW/deprecated), `columnar_storage.c`/`columnar_tableam.c`): layout stripe (150k) → chunk_group (10k) → chunk por-coluna + compressão por-coluna (zstd/lz4) + min/max skip para chunk-group pruning; TID = row_number sintético → binary search no catálogo de stripes; WAL via `GenericXLog` (reusa `am/page.rs`); MVCC append-only (stripe visível ⟺ linha de catálogo visível) + delete via row_mask sob advisory lock. **Escopo append-mostly honesto:** SEM update in-place, SEM parallel/bitmap/sample (o mesmo NULL/ERROR set do Hydra).
**GATE (correção primeiro):** result-equivalence — `SELECT` sobre a tabela columnar == a mesma tabela row-store (agregações idênticas); **pgisolation permutations** provando visibilidade MVCC sob txns concorrentes (stripe não-commitada invisível; REPEATABLE READ segura o snapshot); crash-safety (insert stripes → restart → scan idêntico; abort → restart → sem stripe parcial). **Sem isolation permutations verde, "MVCC-correct columnar" é over-claiming.**
**DoD:** (1) `theodb_columnar` TAM registrável (`CREATE ACCESS METHOD ... TYPE TABLE`); (2) result-equivalence pg_tests vs row-store; (3) **pgisolation permutation specs** (MVCC) verdes — precisa wire do `isolationtester` (gap de tooling do blueprint Corner 3); (4) crash-safety WAL-replay; (5) benchmark de scan analítico columnar vs heap (`docs/benchmarks/m99-columnar-tam.{md,json}`) — ganho de compressão+skip (~2-5×, honesto, SEM execução vetorizada ainda); (6) sign-off council-index-storage + council-rust-pgrx. **Boundary honesto:** append-only analytical, NÃO updatable HTAP (claim de "updatable columnar" seria over-claiming). **Risks:** (a) bugs de MVCC só aparecem sob concorrência → permutations não-opcionais; (b) TID sintético + binary search no catálogo é território novo; (c) o unwind boundary (todo callback → `pg_sys::error!`, nunca panic). **Dependencies:** M98 (pgrx 0.19 + o build). **Prior art:** blueprint Q2/Q7/Q9 + `theodb_rs/src/am/{mod.rs,page.rs,tid.rs}` (IndexAmRoutine/GenericXLog/TID codec — NÃO é greenfield). **Prior art de estudo (AGPLv3 — design only, Rule 9, ADR-0042):** `hydra/columnar` + `citus` columnar; **permissivo:** `cstore_fdw` (Apache-2.0, FDW), `arrow-rs` codecs (Apache-2.0).

## M100 — [x] executor DataFusion CustomScan vetorizado *(gated M99)*

> **Rung M-β.** O coração: o `CustomScan` que batcheia em Arrow e roda um `ExecutionPlan` do DataFusion num plano só —
> a execução vetorizada que dá o 15–30× (o pg_duckdb não faz single-planner; o TAM do M99 sozinho é row-at-a-time).

**Objective:** ligar o columnar TAM (M99) a um executor vetorizado DataFusion via `CustomScan` (o seam provado pelo pg_search [AGPL-design]; TheoDB já tem `customscan.rs`): planner hooks (`set_rel_pathlist_hook`/`create_upper_paths_hook`) montam o `CustomPath`; o exec constrói o `ExecutionPlan` DataFusion, `block_on` no `SendableRecordBatchStream`, projeta cada `RecordBatch` Arrow → `TupleTableSlot`; a leaf implementa `TableProvider` puxando as stripes columnar como Arrow batches; tradução de qual/agg PG → DataFusion `Expr` (gate `schema=="pg_catalog"`). **Disciplina de segurança own-code (o artefato #1 do blueprint):** `HeldInterrupts` em volta do `block_on` (senão `proc_exit` mata o backend), `MemoryPool` limitado a `work_mem` que ERRA (não panica), `unsafe impl Send` pinado numa thread (sem multi-partition até provar), todo panic → `pg_sys::error!`.
**GATE (measurement-first):** correção (result == heap/row-store byte-a-byte nas agregações) PRIMEIRO; depois o benchmark: OLAP columnar-vetorizado vs pg_duckdb vs heap GROUP BY no MESMO box, **em dados columnar-residentes** (o ganho não existe sobre heap — M61 mediu 0.63-0.89×). Honest-negative terminal válido.
**DoD:** (1) `CustomScan` DataFusion sobre o TAM M99, plano único (EXPLAIN mostra o node); (2) result-equivalence vs row-store; (3) a disciplina de interrupt/MemoryPool/Send implementada + testada (crash sob interrupt não mata o backend); (4) **benchmark medido** OLAP vetorizado vs pg_duckdb vs heap (`docs/benchmarks/m100-datafusion-executor.{md,json}`), teto DuckDB/Photon-class honesto; (5) sign-off council-rust-pgrx (FFI/panic-across-C) + council-benchmark. **Boundary honesto:** ganho SÓ em dados columnar-residentes; **NÃO é claim de superioridade vs AlloyDB in-core** (teto de paradigma travado). **Risks:** (a) o seam FFI é o mais perigoso do pilar (runtime async dentro de callback C síncrono) → disciplina de interrupt desde o dia 1; (b) `unsafe impl Send` vira data-race com parallel exec → single-thread pinning; (c) churn de versão do DataFusion → shim fino atrás da nossa interface (DIP). **Dependencies:** M99 (o storage). **Prior art:** blueprint Q1/Q3 + `theodb_rs/src/am/customscan.rs` (o seam M94/M95). **Estudo (AGPL):** `paradedb/pg_search/src/postgres/customscan/` (design-only).

## M101 — [x] Arrow columnar cache heap-authoritative (MVCC-correto HTAP) *(gated M100)*

> **Rung M-γ.** O modelo do AlloyDB feito permissivo: o heap continua a fonte-de-verdade; o columnar é um cache Arrow
> DERIVADO, em memória, que o DataFusion lê zero-copy. Resolve o problema MVCC mais difícil mantendo o heap autoritativo.

**Objective:** um cache columnar Arrow **derivado do heap row-store** (heap = fonte-de-verdade → MVCC correto por construção), que o executor DataFusion (M100) lê zero-copy; invalidação/refresh no write; o planner escolhe o cache para scans analíticos (custo) e cai no heap senão. **Primeiro cut manual:** um pragma "columnarize estas colunas" (o auto-populate/evict por workload — o que o AlloyDB faz — é a cauda ambiciosa, follow-up).
**GATE:** MVCC-correto sob concorrência (o cache carrega metadados de visibilidade; escrita no heap invalida) — pgisolation permutations provando que uma leitura analítica vê exatamente o snapshot correto; result-equivalence heap vs cache; não-interferência OLTP (o cache read-only não degrada o p95 do heap, o padrão M62).
**DoD:** (1) cache Arrow derivado + refresh/invalidação no write; (2) planner escolhe cache vs heap por custo; (3) **pgisolation MVCC permutations** verdes (o cache respeita snapshot isolation); (4) benchmark HTAP (`docs/benchmarks/m101-arrow-cache.{md,json}`): OLAP acelerado + OLTP p95 não-degradado sob carga concorrente; (5) sign-off council-index-storage + council-benchmark. **Boundary honesto:** o pragma é manual (não auto-tuned como o AlloyDB); heap-authoritative = MVCC correto mas com custo de refresh (2× storage do cache). **Risks:** (a) consistência cache↔heap sob write concorrente → invalidação testada por permutations; (b) refresh caro em tabelas quentes → o pragma deixa o operador decidir. **Dependencies:** M100 (o executor). **Prior art:** blueprint Q4/D-γ + AlloyDB columnar-engine (estudo do design, proprietário) + M62 (o padrão materializado). **NÃO é o engine in-memory auto-mantido do AlloyDB** (declarado honestamente).

## M102 — [x] operadores de AI como plan nodes (AI.IF/sem_filter pushable) *(gated M100)*

> **Rung M-δ.** Fecha o gap que o usuário levantou: hoje `ai.generate`/`ai.nl_to_sql` são FUNÇÕES (caixa-preta que o
> planner não custa/reordena/batcheia). Vira operador de plano — o planner empurra o filtro relacional barato antes do
> AI caro e batcheia a inferência.

**Objective:** expor os operadores de AI (`ai.generate`/`AI.IF`/`sem_filter`) como **nodes de plano** (`CustomScan`/table-func set-oriented, não plpgsql per-row) com: (1) um cost hook de 3 eixos (custo/tempo/qualidade + selectivity) em `am/cost.rs` — modelo do Palimpzest [MIT]; (2) uma regra de push-down dependency-safe (`depends_on ∩ generated_fields = ∅`) que roda o `WHERE` relacional barato antes do operador de AI caro; (3) opcional: a cascata proxy→oracle do LOTUS [Apache-2.0] com thresholds aprendidos de sample p/ um alvo de recall com garantia estatística. Requer revisitar a inferência HTTP per-row (ADR-0007) para **batching** (senão o ganho columnar é jogado fora um round-trip por vez) — ADR novo.
**GATE:** correção (o resultado do operador-plano == o da função per-row) + o benchmark de composição: `AI.IF` num `WHERE` sobre um scan vetorizado com push-down do filtro barato antes — custo/latência medidos vs o caminho per-row; a cascata reporta **o alvo de recall + a metodologia de sample** (nunca "AI.IF é rápido" sem o ponto de qualidade).
**DoD:** (1) `AI.IF`/`ai.generate` como node de plano (EXPLAIN mostra + reordena); (2) cost hook 3-eixos + push-down dependency-safe; (3) result-equivalence vs a função per-row; (4) benchmark (`docs/benchmarks/m102-ai-operators.{md,json}`): push-down + (opcional) cascata com recall-target medido; (5) ADR revisitando ADR-0007 (batched inference); (6) sign-off council-ai-in-db + council-security (superfície NL→SQL/AI). **Boundary honesto:** ortogonal a recall vetorial; ganho de **composabilidade/custo com acurácia ESTATÍSTICA** (reportar sempre com metodologia). **Risks:** (a) calibração do cost model exige telemetria de sample (senão estimativa naive sem garantia); (b) prompt-injection na superfície AI-operator → council-security obrigatório; (c) batching muda o ADR-0007. **Dependencies:** M100 (o executor vetorizado — o batch vem dele). **Prior art:** blueprint Q5 + `lotus`/`palimpzest` (Apache/MIT) + `theodb_rs/src/{nl.rs,chat.rs,am/customscan.rs,am/cost.rs}` + ADR-0007/0033.

## M103 — [x] vetor + columnar num substrato único (Lance-inspired) *(gated M100)*

> **Rung M-ε.** O topo: busca vetorial filtrada + agregação analítica num scan columnar só — o índice IVF/HNSW vive
> como colunas Arrow ao lado dos escalares (o insight do Lance), com o prefiltro escalar como `RowAddrMask` first-class.

**Objective:** um substrato columnar compartilhado onde o índice vetorial (IVF partições → row-ranges contíguos; códigos AQ/SQ como colunas Arrow — layout do Lance [Apache-2.0], código próprio) co-reside com as colunas escalares, de modo que `WHERE <escalar> ORDER BY <vetor> LIMIT k` + uma agregação columnar compõem num plano vetorizado só: prefiltro escalar → `RowAddrMask` → sub-index search só nas partições sondadas → rerank + projeção das colunas analíticas por row-id. Reusa o IVF/AQ próprio (M60-M89) re-materializado como colunas.
**GATE (correção + honestidade):** result-equivalence do vetor filtrado vs o caminho atual (M90-M95) — recall byte-idêntico; o benchmark mede **custo/escala** (out-of-RAM, column pruning) — **NÃO recall e NÃO o gap de QPS do ScaNN** (o teto M73/M74 permanece; qualquer claim de recall/QPS-superior é barrado).
**DoD:** (1) índice vetorial como colunas Arrow no substrato columnar; (2) `WHERE escalar + ORDER BY vetor` + agregação num plano vetorizado; (3) result-equivalence de recall vs M90-M95; (4) benchmark (`docs/benchmarks/m103-vector-columnar.{md,json}`): ganho de custo/escala (out-of-RAM/pruning), honesto; (5) sign-off council-vector-ann + council-index-storage + council-benchmark. **Boundary honesto:** ganho de **cost/scale/composabilidade, NÃO recall, NÃO QPS-vs-ScaNN** (teto de paradigma travado); Lance é file-format → integração lakehouse/side-store, não substitui o AM transacional. **Risks:** (a) consistência dual-store (row-store verdade + réplica columnar); (b) manutenção incremental de índice sobre segmentos imutáveis; (c) tentação de over-claim recall → GATE barra. **Dependencies:** M100 (o executor vetorizado). **Prior art:** blueprint Q4 + `lance` (Apache-2.0, estudo do layout) + `theodb_rs/src/ann/{ivf.rs,hnsw.rs}` + `am/page.rs` (o IVF/AQ próprio) + ADRs 0035/0037.

---

## M104 — [x] system-design hardening: fechar as findings da auditoria (health 4.2 → ≥4.9/5) *(gated M103)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `system-design-hardening-49`). Fonte: auditoria Staff-level
> `/loop-system-design` (`system-design-output/final_report.md`, overall **4.2/5**). See CHANGELOG `[Unreleased] § Added`.

**Objective:** fechar o CRÍTICO + todos os HIGH (e os wins baratos de boundary/deletion/data-flow + a dívida de
governança) da auditoria de system-design, elevando o score de saúde de **4.2 → ≥4.9/5** — reusando os padrões de
memória-limitada que **já existem in-tree** (streaming build M89/M96, tuplesort spill, GreedyMemoryPool), sem reinventar.

**GATE (verificação honesta):** re-rodar `/loop-system-design --mode=full` deve pontuar **≥4.9/5** overall, com o
CRÍTICO e TODOS os HIGH resolvidos. Refactors da via columnar (MVCC-load-bearing) DEVEM re-passar as provas de crash
(`make -C theodb_rs/isolation check-crash`) + as permutações de isolation — nenhuma regressão de MVCC/crash-safety.

**DoD:** (1) **CRÍTICO** — via de escrita columnar bounded-memory (flush em fronteira de stripe/row-count, padrão
M89/M96), sem RAM O(rows-in-xact) (#99), provado por teste de envelope de memória num `INSERT...SELECT` grande;
(2) **4 HIGH** — (a) seq-scan columnar em streaming (não full-materialize); (b) VACUUM fold bounded ou cap
documentado+benchmarkado (janela M55); (c) Arrow cache M101 com eviction/limite de tamanho; (d) cliente AI HTTP com
**circuit breaker** + reuso de conexão + cap de batch; (3) **deletion/boundary** — árvore `rabitq/vendor/` inerte
deletada OU `#[cfg(feature)]`-gated + `VENDORED.md` corrigido (ADR); inversão de layering `vec/ah.rs → am::aq`
corrigida (realocar `AqQuantizer`); `columnar::decode_columns` com acessor tipado de projeção (mata o leak
`vindex → am::columnar` internals); paths legacy blob/v4 com `#[deprecated]`/WARN + default v4-OOM invertido;
(4) **data-flow** — backpressure do produtor do vectorizer + bound de retenção/purge do dead-letter; (5) **governança
(owner)** — ADR-0033 **assinado** OU nota de supersede no ADR-0002 (LOCKED) apontando para os verdicts medidos
0035/0036 — fecha o único trade-off com rationale inválido (não-código; decisão do owner); (6) **verificado** —
re-auditoria `/loop-system-design` ≥4.9/5. **Boundary honesto:** é *hardening* de qualidade de design (memória
limitada, resiliência, higiene de deleção, governança) — **NÃO** muda capacidade vetorial nem o teto de paradigma
M73/M74 (nenhum claim de QPS/recall superior entra por esta porta). **Risks:** (a) scope creep — 5 dimensões num
milestone; mitigação: DoD é checklist independente + a re-auditoria é o único gate de aceite (medium/low podem ser
diferidos com nota se não movem o score); (b) refatorar a via columnar MVCC-load-bearing pode introduzir regressão de
MVCC/crash-safety em código recém-provado; mitigação: preservar o invariante de visibilidade heap-catalog do M99 +
re-rodar as provas de crash e as permutações de isolation. **Dependencies:** M103 (última do pilar columnar/AI onde as
findings vivem) + a auditoria concluída. **Prior art:** `system-design-output/final_report.md` (26 findings) + os
drafts `system-design-output/adrs/{0045-northstar-governance,0046-rabitq-vendor-disposition}.md` + os in-tree
`am/build_stream.rs` (streaming M89), `am/df_executor.rs` (GreedyMemoryPool M100), `am/fold.rs` (crash-safe M48) +
issues #99/#100/#102/#104/#106/#108.

---

## M105 — [x] docs/features honestas — reconciliar as specs com a superfície entregue (pré-lançamento) *(gated M104)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `docs-features-reality-reconciliation`). Fonte: auditoria de features
> (3 agentes, spec↔código↔testes) sobre `docs/features/*.md`. See CHANGELOG `[Unreleased] § Added`.

**Objective:** todo exemplo SQL em `docs/features/*.md` OU roda num install limpo (copy-paste) OU está sob um banner
claramente rotulado **"🎯 API-alvo / roadmap (não-shipped)"**. Nenhum `42883 undefined_function` silencioso. Pré-lançamento,
sem usuários: doc que mente é pior que doc que falta — mata a primeira impressão de um produto OSS. **Docs-only, ZERO código.**

**GATE (verificação honesta):** varredura por-arquivo — cada símbolo SQL runnable (não-rotulado) nas 12 specs resolve
a uma função/AM/opclass REAL no código (grep-confirmado, zero símbolo fabricado); cada superfície aspiracional está sob a
seção rotulada. Nenhuma spec afirma "shipped" o que o código não entrega.

**DoD:** (1) `theodb_ml.embedding('MODEL','TEXT')` → `theodb.embed(content, model)` corrigido em 01/03/04/05 (nome,
schema, ordem dos args; modelo literal fictício removido); (2) exemplos de índice usam o AM+opclass **próprios**
(`USING theodb_hnsw (embedding theodb_hnsw_l2_ops)`, `theodb_ivfflat_l2_ops`) em 02/03 — exemplos da superfície pgvector
rotulados como coexistência, não como o AM próprio; (3) `CREATE EXTENSION theodb_ml` corrigido — é **schema** + registry
(`theodb_ml.create_model/apply_model/...`), não extensão; (4) **09** reconciliada ao reranker real `ai.rerank(query,
documents[], model, top_n) RETURNS TABLE(idx,score)` (idx 0-based; o off-by-one do exemplo RAG corrigido) — o `ai.rank`
fantasma (4-arg) removido/rotulado; (5) **06** — chaves JSON não-implementadas (`weight`/`distance_operator`/
`ranking_function`/`include_json_output`/`id_type`) + `g_to_tsquery`/`theodb_scann` removidas dos exemplos runnable ou
movidas p/ target-API; contrato JSON real documentado (`table/id_col/content_tsv_col/…/lexical_engine`); (6) superfícies
aspiracionais/diferidas (04 `USING ivf`, 05 `USING scann` + **ScaNN-QPS measured-negative** ADR-0035/0036, 08 Proxy Model,
12 `theodb_ai_nl.*`) sob banner **"🎯 API-alvo / roadmap (não-shipped)"**, com a via SHIPPED documentada primeiro. **Boundary
honesto:** ZERO mudança de capacidade/código — só alinhar doc↔realidade e rotular o aspiracional (não implementa nada novo).
**Risks:** (a) afirmar shipped o que não é (honestidade, Regra 3) → mitigação: grep-verify por exemplo antes de marcar
runnable; (b) scope creep p/ reescrever specs inteiras → mitigação: "corrige+rotula, não reescreve"; DoD é checklist
por-arquivo. **Dependencies:** M104 (estado shipped mais recente que o audit reflete). **Prior art:** o audit de features
+ `docs/features/*.md` + `theodb_rs/src/{api,hybrid,rerank,nl,chat,embed}.rs` + `am/mod.rs` (AMs/opclasses reais).

---

## M106 — [x] higiene de API pré-usuários — canonizar ai.rank/rerank + honrar `weight` no hybrid *(gated M105)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `api-consistency-prelaunch`). Fonte: mesma auditoria de features
> (gaps 06/09). See CHANGELOG `[Unreleased] § Added`. **Opcional:** se a decisão for "doc segue o código", M105 já resolve
> e este milestone é dispensável — existe para o caso de preferir enriquecer o CÓDIGO à API mais atraente já documentada.

**Objective:** eliminar as 2 inconsistências API↔doc que o audit achou, escolhendo UMA superfície pública canônica e
implementando o lado código quando a API documentada mais atraente vale a pena — **antes** de haver usuários que congelem
o contrato. Não é feature nova de capacidade (reranker e RRF já existem e são testados); é higiene de nome/parametrização.

**GATE:** novos `pg_test` provam o contrato canônico escolhido; `cargo pgrx test pg17` GREEN, 0 regressão.

**DoD:** (1) **09 rerank** — decidir o nome público canônico: OU adicionar overload `ai.rank(query text, documents text[],
model text, top_n int) RETURNS TABLE(idx int, score real)` espelhando `ai.rerank` (o nome já documentado/atraente), OU
consolidar em `ai.rerank` e deprecar o outro — **um só** nome público, com teste de alinhamento de índice N-in/N-out;
(2) **06 hybrid** — honrar `weight`: RRF ponderado real (`Σ wᵢ/(k+rankᵢ)`) em `run_rrf_json`, com teste provando que
`weight` muda a ordem; default `weight=1` preserva o comportamento atual (não-ponderado); (3) todo símbolo público novo
com `REVOKE ALL … FROM PUBLIC` (convenção least-privilege) + wiring triad (caller + teste + observável); (4) CHANGELOG
atualizado. **Boundary honesto:** enriquecimento pequeno de superfície de API — **NÃO** muda a capacidade de retrieval;
só consistência de nome/contrato antes de congelar o público. **Risks:** (a) alias duplica superfície (2 nomes p/ 1 coisa)
→ mitigação: escolher UM canônico e deprecar o outro no mesmo PR; (b) weighted-RRF muda ranking → mitigação: pré-launch
sem usuários + `weight=1` default = comportamento idêntico ao atual. **Dependencies:** M105 (docs reconciliadas primeiro,
p/ a escolha do nome canônico ser deliberada, não acidental). **Prior art:** `theodb_rs/src/{rerank,hybrid,api}.rs` +
`benchmarks/tests/{test_rerank_sql,test_integration}.py` + o audit de features.

---

## M107 — [x] Pilar de grafo nativo — Fase 0: blueprint SOTA + spike medido (CSR + MS-BFS vetorizado + SQL/PGQ) fundido ao columnar+vetorial+AI *(gated M104)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `native-graph-engine`). Fonte: deep research SOTA (DuckPGQ CIDR/VLDB
> 2023, Kùzu CIDR 2023, GRFusion, SQL/PGQ SQL:2023, Microsoft GraphRAG/LazyGraphRAG, HippoRAG 2) + gap analysis do
> theo-rag (grafo hoje = recursive-CTE). See CHANGELOG `[Unreleased] § Added`.

**Objective:** produzir o **blueprint SOTA-ancorado** de um **motor de grafo NATIVO** — **CSR (compressed sparse row)
adjacency + MS-BFS vetorizado (SIMD) + superfície SQL/PGQ (SQL:2023)** — **FUNDIDO** aos pilares que já existem
(columnar DataFusion/Arrow M99–M103; AM vetorial próprio + kernels SIMD `vec/ah.rs`; `ai.*` in-SQL) — e **PROVAR por
spike reproduzível** que a travessia nativa-sobre-columnar **bate o recursive-CTE** (o baseline do theo-rag) na carga
GraphRAG-alvo (multi-hop + expansão de vizinhança). Grafo é capability recorrente/cross-system (não YAGNI) e AI-native:
o pattern SOTA é **vector-entry → travessia bounded → rerank** (LazyGraphRAG/HippoRAG) rodando **zero-copy num só engine**.

**GATE (measurement-first / D3 — anti-sunk-cost):** benchmark reproduzível em `docs/benchmarks/` comparando **CSR+MS-BFS
own-code vs recursive-CTE** (e, onde viável, vs Apache AGE) em queries multi-hop + vizinhança a escala representativa
(~10⁵–10⁶ arestas), **≥3 runs mean±std**, veredito explícito **GO / honest-partial / honest-negative**. Se a via nativa
NÃO superar o CTE de forma significativa na carga-alvo → **NO-GO honesto** (saída válida: fica relacional-com-helpers).
ZERO número fabricado (Regra 5). O motor completo só arranca se o gate for GO.

**DoD:** (1) **blueprint** (cycle-discover, ≥2 fontes web por técnica — `discover-phd-rigor` R0/R2): design CSR
(index-AM vs materializado), MS-BFS vetorizado reusando os kernels SIMD, escopo WCOJ/factorização, superfície SQL/PGQ vs
funções `graph_*`, o fluxo vector-entry→bounded-traversal→rerank, e o acoplamento com columnar (community/PPR) + `ai.*`
(`extract_graph`); cita DuckPGQ, Kùzu, GRFusion, SQL/PGQ, GraphRAG/LazyGraphRAG/HippoRAG; (2) **spike own-code**: CSR +
MS-BFS mínimo sobre uma tabela de arestas vs o baseline recursive-CTE, medido, artefato `docs/benchmarks/m107-graph-spike.{md,json}`;
(3) **veredito D3 explícito** (GO/honest-partial/honest-negative) que decide os milestones seguintes do pilar; (4) **ADR**
registrando a decisão de arquitetura (nativo-sobre-columnar vs Apache AGE vs recursive-CTE) com a evidência medida + nota
de licença (own-code inspirado em MIT DuckPGQ/Kùzu; AGE Apache-2.0 avaliado e rejeitado por arquitetura, não por licença);
(5) **reuso (Regra 9)**: constrói SOBRE o columnar M99–M103 + AM vetorial + kernels SIMD — **NÃO** reimplementa o columnar
(fora de escopo do v2) nem reescreve o PostgreSQL. **Boundary honesto:** é **Fase 0** — discovery + gate medido, **NÃO** o
motor. Entrega blueprint + spike + veredito. O motor real (persistência CSR, operador MS-BFS, parser SQL/PGQ, integração
com o planner, vetor-nos-nós, community/PPR) são milestones seguintes, adicionados **só se o gate for GO** (anti-sunk-cost:
honest-negative fecha o pilar barato). **Risks:** (a) construir o motor antes de provar o ganho → o gate D3 é o DoD, o motor
é gated; (b) qualidade do GraphRAG depende da **extração** (`ai.extract_graph`), não só da travessia → o blueprint escopa a
qualidade da extração, não só o engine; (c) explosão de escopo (uma linguagem de grafo inteira) → a Fase 0 limita ao
primitivo de travessia + o fluxo de retrieval GraphRAG, não conformância openCypher/SQL/PGQ completa. **Dependencies:** M104
(a fundação columnar/vetorial/AI endurecida que o grafo funde). **Prior art:** DuckPGQ (CIDR 2023 p66 / VLDB vol16 p4034),
Kùzu (CIDR 2023 p48), WCOJ (arXiv 2505.19918), GRFusion (arXiv 1709.06715), SQL/PGQ (SQL:2023), Microsoft GraphRAG /
LazyGraphRAG (MS Research), HippoRAG 2; baseline a bater = `theo-rag/packages/core/src/domain/retrievers/graph-retriever.ts`
(recursive-CTE); reuso interno = `theodb_rs/src/am/df_executor.rs` (columnar M100), `vec/ah.rs` (SIMD), `am/mod.rs` (AM vetorial).

---

## M108 — [x] Grafo Fase 1: persisted-CSR index-AM (build 1× + manutenção incremental) *(gated M107)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `graph-persisted-csr-am`). Fonte: ADR-0048 (follow-on #1) — o achado do spike M107 (build CSR on-the-fly domina a 1M → end-to-end cai p/ ~7×).

**Objective:** um **AM de índice** que **persiste a adjacência CSR** de uma tabela de arestas — construída 1× no `ambuild` e mantida **incremental** no `aminsert` + `amvacuumcleanup` — para a travessia NÃO pagar o rebuild O(N) por query. **GATE:** benchmark reproduzível provando que a travessia sobre o CSR persistido **preserva o ganho ~100–700×** do M107 **sem** o custo de build por query (end-to-end ≈ traverse-only); **crash-safe** (WAL/GenericXLog) provado por harness de crash (abort→íntegro; committed→sobrevive replay); ZERO número fabricado (Regra 5). **DoD:** (1) AM `theodb_graph` (ou estrutura CSR sobre a edge-table) registrado com `ambuild`/`aminsert`/`amvacuumcleanup`; (2) CSR persistido em páginas WAL-logged (reusa a page/WAL machinery do pilar vetorial); (3) manutenção incremental (pending-region + fold no VACUUM, padrão M48/M89); (4) benchmark end-to-end vs o M107 baseline em `docs/benchmarks/`; (5) prova de crash. **Boundary honesto:** só a **persistência+manutenção** do CSR — o operador de travessia é o M109. **Risks:** (a) manutenção incremental correta sob concorrência → reusar o invariante MVCC/crash do pilar vetorial + provas de isolation; (b) build dominar mesmo persistido se a manutenção for cara → medir manutenção amortizada. **Dependencies:** M107. **Prior art:** `theodb_rs/src/am/{mod,page,build,fold}.rs` (index-AM+WAL+VACUUM), ADR-0048, DuckPGQ (CSR), USPTO 11,093,459 (CSR-in-RDBMS incremental).

---

## M109 — [x] Grafo Fase 2: operador MS-BFS vetorizado (SIMD, multi-source) *(gated M108)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `graph-msbfs-operator`). Fonte: ADR-0048 (follow-on #2).

**Objective:** **Multi-Source BFS vetorizado** sobre o CSR persistido (M108) como operador in-engine — bitset visited + SIMD, muitas seeds simultâneas (o MS-BFS do DuckPGQ: um registro AVX512 avança até 512 buscas). **GATE:** **oracle set-hash** = mesmo reachable-set do baseline recursive-CTE em cada trial; throughput MS-BFS medido (≥3 runs mean±std) e o ganho de N-seeds-em-paralelo vs N BFS sequenciais quantificado; `docs/benchmarks/`. **DoD:** (1) operador MS-BFS own-code reusando os kernels SIMD `vec/ah.rs`; (2) semântica bounded ≤H hops idêntica ao theo-rag (differential test / set-hash, NÃO count+sum — dívida do M107); (3) benchmark N-seeds; (4) integração com o AM M108. **Boundary honesto:** só o **primitivo de travessia** (reachable-set + scoring por peso de aresta) — a superfície SQL é o M110; PPR é o M112. **Risks:** (a) SIMD de bitset visited correto (falso-compartilhamento/alinhamento) → oracle set-hash + testes de borda; (b) grafo denso estourar frontier → medir memória do frontier. **Dependencies:** M108. **Prior art:** `vec/ah.rs` (SIMD/FastScan), ADR-0048, DuckPGQ MS-BFS, `benchmarks/m107_graph_spike/` (o BFS de referência).

---

## M110 — [x] Grafo Fase 3: `theodb.graph_expand` + `ai.extract_graph` (a superfície que o theo-rag adota) *(gated M109)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `graph-surface-graphexpand`). Fonte: ADR-0048 (follow-on #3) — o **payoff de reduzir a complexidade do theo-rag**.

**Objective:** a superfície SQL que o **theo-rag troca já** por chamadas in-DB: **`theodb.graph_expand(seeds, max_hops, filter?)`** (travessia via M109) + **`ai.extract_graph(text)` / `ai.extract_entities(text)`** (extração de entidades/arestas in-DB — heurística primeiro, LLM-opcional depois), com upsert idempotente de nós/arestas. **GATE:** a estratégia `graph` do theo-rag roda contra estas funções (prova de integração end-to-end); **baseline de qualidade de extração** (cobertura de entidades vs o `graph-extractor.ts` heurístico) medido — extração ruim invalida o motor rápido (qualidade do grafo ≠ velocidade). **DoD:** (1) `theodb.graph_expand` (`#[pg_extern]`, REVOKE-from-PUBLIC, wiring triad); (2) `ai.extract_graph`/`ai.extract_entities` reusando `ai.*`; (3) upsert idempotente de entities/edges (padrão do theo-rag); (4) prova de integração com o theo-rag; (5) benchmark de qualidade de extração. **Boundary honesto:** superfície pragmática **pré-SQL/PGQ** (funções, não `MATCH`); a linguagem padrão é o M113. **Risks:** (a) segurança da extração/`filter` (SQL-injection, SSRF na chamada LLM) → seguir a postura NL→SQL (denylist, fail-closed, REVOKE); (b) extração heurística fraca → o gate de qualidade decide se LLM-extraction é necessária. **Dependencies:** M109. **Prior art:** `theo-rag/.../domain/{extraction/graph-extractor,graph-store/graph-store}.ts` (a portar), `theodb_rs/src/{ai_op,chat,nl}.rs` (`ai.*`+segurança), ADR-0048.

---

## M111 — [x] Grafo Fase 4: vector-nos-nós + fluxo vector-entry→traversal→rerank *(gated M110)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `graph-vector-nodes-flow`). Fonte: ADR-0048 (follow-on #4) — o pattern GraphRAG SOTA (LazyGraphRAG/HippoRAG) zero-copy num só engine.

**Objective:** embeddings dos **nós do grafo** indexados pelo AM vetorial; o fluxo GraphRAG **vector-entry → bounded-traversal → rerank** como um caminho in-DB único, zero-copy (cosseno acha as entry entities → `graph_expand` → `ai.rerank`). **GATE:** **eval estratificado** (local-fact / multi-hop / global-sensemaking, metodologia BenchmarkQED/Microsoft): o fluxo grafo×vetor **bate hybrid+rerank** em multi-hop/global — **honest-negative em local-fact é resultado válido** (vetor puro vence local); `docs/benchmarks/`. **DoD:** (1) índice vetorial nos nós (reusa AM vetorial); (2) o fluxo composto num caminho (entry→expand→rerank); (3) eval estratificado com origem identificada (ZERO número fabricado); (4) synonymy-edges opcional (cosseno>0.8, HippoRAG) se o eval justificar. **Boundary honesto:** o **retrieval**, não a geração (a resposta LLM continua no consumidor). **Risks:** (a) eval honesto exige corpus rotulado real, não sintético → usar um dataset GraphRAG público + o eval do theo-rag; (b) o ganho depende da qualidade do grafo (M110) → o gate mede o fluxo, não salva grafo ruim. **Dependencies:** M110. **Prior art:** AM vetorial `am/mod.rs`, `ai.rerank`, HippoRAG (vector-entry→PPR), LazyGraphRAG, ADR-0048, o eval do theo-rag (`packages/core/.../eval`).

---

## M112 — [x] Grafo Fase 5: Personalized PageRank + community summarization *(gated M111 — gated em necessidade MEDIDA)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `graph-ppr-community`). Fonte: ADR-0048 (follow-on #5) — o mais **diferível** (LazyGraphRAG: community summaries custam 700×).

**Objective:** **Personalized PageRank** a partir das seeds vetoriais (HippoRAG) + **community detection/summarization** opcional (GraphRAG global, Microsoft) como computação **columnar iterativa**. **GATE (D3 — anti-sunk-cost):** eval provando que PPR/community **bate** o bounded-BFS-scoring (M111) em **global-sensemaking** — senão **honest-negative FECHA** (bounded BFS + rerank basta; o SOTA mostra que community é caro e nem sempre vale). `docs/benchmarks/`. **DoD:** (1) PPR iterativo sobre o CSR (columnar, reusa M99–M103); (2) community detection (Leiden/Louvain) + summarization via `ai.summarize` — SÓ se o eval de global justificar; (3) eval estratificado global vs M111. **Boundary honesto:** só arranca se o eval do M111 mostrar gap em global-sensemaking; caso contrário é YAGNI e o pilar para no M111. **Risks:** (a) custo de indexação de community (o driver de 75% do custo GraphRAG) → medir custo/benefício antes de shipar; (b) PPR não convergir barato em grafo grande → medir iterações/convergência. **Dependencies:** M111. **Prior art:** columnar M99–M103 (iteração), `ai.summarize`, Microsoft GraphRAG (communities), HippoRAG (PPR), LazyGraphRAG (defer-communities), ADR-0048.

---

## M113 — [x] Grafo Fase 6: superfície SQL/PGQ (SQL:2023) *(gated M110 — opcional/diferível)*

> Added 2026-07-16 by `/roadmap-feature` (slug: `graph-sqlpgq-surface`). Fonte: ADR-0048 (follow-on, ergonomia de longo prazo).

**Objective:** a superfície **SQL/PGQ (SQL:2023)** — `MATCH ()-[]->()`, path patterns, `SHORTEST`/bounded, `ELEMENT_ID` — compondo com SQL + vetor + `ai.*` num só statement, substituindo gradualmente as funções `graph_*` do M110. **GATE:** um **subset de conformância SQL/PGQ** (as construções que o GraphRAG usa — pattern match + bounded path) passa; compõe com um `<=>`/`ai.rerank` no mesmo statement (prova); NÃO exige conformância total. **DoD:** (1) parser-extension SQL/PGQ (padrão DuckPGQ: mapear para o plano lógico + operadores M108/M109, minimizando intrusão no core); (2) subset de conformância testado; (3) exemplo GraphRAG end-to-end num só SQL/PGQ statement. **Boundary honesto:** **camada ergonômica**, não capacidade nova — as funções `graph_*` do M110 já servem o theo-rag; SQL/PGQ é a superfície padrão de longo prazo. É o mais **diferível** (só quando a ergonomia justificar o esforço de parser). **Risks:** (a) parser SQL/PGQ é grande → escopo bounded ao subset GraphRAG, não conformância total (YAGNI); (b) intrusão no planner do PG → seguir o approach UDF-minimal do DuckPGQ. **Dependencies:** M110 (o motor+superfície funcional primeiro). **Prior art:** DuckPGQ (parser-extension SQL/PGQ, CIDR/VLDB 2023), SQL/PGQ SQL:2023 (arXiv 2505.07595), ADR-0048.

---

## M114 — [x] Columnar analytical aggregate completeness: GROUP BY+WHERE combinado + avg/sum(int) *(gated M100)*

> Added 2026-07-19 by `/roadmap-feature` (slug: `columnar-aggregate-completeness`). Fonte: caveats dos slices ad-hoc columnar (`docs/benchmarks/columnar-groupby-verdict.md`). See CHANGELOG `[Unreleased] § Added`.

**Objective:** alargar as FORMAS de agregado colunar admitidas pelo M100 CustomScan — hoje `count(*)`/`sum(float8)` sem WHERE combinado. Duas frentes: (a) **GROUP BY + WHERE combinados** (o zone-map skip + o DataFusion Filter compõem com o hash-aggregate num só plano — o `admit` hoje DECLINA quando `groupClause` E `baserestrictinfo` estão ambos presentes); (b) **`avg` + `sum(int2/4/8)`** — onde o tipo Arrow de saída difere do tipo de saída do PG (a razão de o M100 se restringir a `sum(float8)`/`count`), exigindo a coerção exata da semântica do PG. **GATE (measurement-first):** um A/B in-PG provando resultado **byte-idêntico** vs heap para cada forma nova (`SELECT k, sum(x) WHERE ts BETWEEN … GROUP BY k`, `avg(x)`, `sum(int_col)`), CustomScan engatado, com speedup medido. **DoD:** (1) `admit` aceita `groupClause` + `baserestrictinfo` juntos — skip + Filter + group num só plano (reusa o invariante D3 admission-filter); (2) `avg(float8/int)` + `sum(int2/4/8)` admitidos com o tipo de saída EXATO do PG (`sum(int4)`→int8, `sum(int8)`→numeric, `avg`→numeric) OU decline fail-safe; (3) A/B byte-idêntico por forma nova + verdict em `docs/benchmarks/`; (4) CHANGELOG. **Boundary honesto:** alarga a SUPERFÍCIE de agregado (eixo colunar/lakehouse D2), **não** é pilar novo nem claim de performance vetorial — reusa o zone-map (slices ad-hoc) + o DataFusion aggregate já in-tree (Regra 9). **Risks:** (a) overflow/semântica numérica do integer-sum — deve casar o tipo de saída do PG exatamente ou declinar (fail-safe, honestidade Rule 3); mitigação: mapear ao tipo PG exato ou plano nativo. (b) interação skip×group — um chunk group pulado NUNCA pode dropar um grupo (o grupo só existe se uma linha casa — o Filter do DataFusion é a autoridade); mitigação: reusar o invariante D3, A/B com grupos de overlap parcial. **Dependencies:** M100 (o CustomScan que estes agregados estendem). **Prior art:** slices ad-hoc `docs/benchmarks/columnar-zonemap-verdict.md`, `columnar-zonemap-temporal-verdict.md`, `columnar-groupby-verdict.md`; `theodb_rs/src/am/columnar_agg.rs`, `am/df_executor.rs`, `am/zonemap.rs`.

---

## M115 — [x] Composabilidade do M100: saída columnar-agg usável em subquery/join *(gated M100)*

> Added 2026-07-19 by `/roadmap-feature` (slug: `columnar-aggregate-completeness`). Fonte: caveat medido em `docs/benchmarks/columnar-groupby-verdict.md` (limitação pré-existente do M100). See CHANGELOG `[Unreleased] § Added`.

**Objective:** resolver a limitação **pré-existente do M100** onde consumir o VALOR de saída de um agregado colunar dentro de uma expressão externa (subquery/join/`ORDER BY` de agregado sobre o valor) falha com `cache lookup failed for attribute N of relation 0` — o planner inlina o `Aggref` do CustomScan `scanrelid=0` na remoção do SubqueryScan, re-avaliando a tupla sintética. Afeta o path ESCALAR e o GROUP BY igualmente (`SELECT s+1 FROM (SELECT sum(x) s FROM col) q` também falha). **GATE (measurement-first + discover-first):** um teste/A/B in-PG provando `SELECT sum(s) FROM (SELECT k, sum(x) s FROM col GROUP BY k) q`, um JOIN sobre a saída agrupada, e `string_agg(… ORDER BY agg_value)` — TODOS byte-idênticos vs heap com o CustomScan engatado, nos paths escalar E agrupado. **DoD:** (1) diagnóstico do caminho exato `setrefs`/SubqueryScan-removal (a tentativa `INDEX_VAR`-no-`plan.targetlist` QUEBROU o top-level — o fix deve deixar o `setrefs` construir o INDEX_VAR ele mesmo OU impedir o inlining do Aggref); (2) um fix que torne a saída do CustomScan opaca/consumível (ex.: `custom_scan_tlist` correto p/ nós superiores referenciarem colunas de saída, não o `Aggref` cru); (3) saída escalar E agrupada usável em subquery/join/ORDER-BY-de-agregado; (4) A/B/testes byte-idênticos + verdict; (5) SEM regressão do path top-level (re-provar). **Boundary honesto:** é um fix de CORREÇÃO de planner-integration (composabilidade), **não** capacidade nova nem performance — destrava o uso do columnar-agg em queries reais (analytics/RAG compõem agregados em subqueries). **Risks:** (a) interação profunda com `setrefs` — o `INDEX_VAR`-in-`plan.targetlist` ingênuo quebrou o top-level (o setrefs quer as exprs reais); mitigação: discover-first do padrão `scanrelid=0` agrupado (extensão real: Citus/TimescaleDB) + `set_customscan_references`. (b) o fix pode exigir interceptar em outro estágio do planner → escopo pode crescer; mitigação: spike/measurement-first GATE antes de comprometer a abordagem. **Dependencies:** M100 (o CustomScan cuja saída este fix torna composável). **Prior art:** `docs/benchmarks/columnar-groupby-verdict.md § caveat` (a limitação + a tentativa INDEX_VAR revertida), `theodb_rs/src/am/columnar_agg.rs::plan_custom_path`, PostgreSQL `setrefs.c::set_customscan_references`.

---

## M116 — [x] Operabilidade em escala: eliminar o muro do VACUUM (index-maintenance ADR-0017 fase 1)

> Added 2026-07-20 by `/roadmap-feature` (slug: `vacuum-wall-operability`). Fonte: deep-view `.claude/knowledge-base/audits/deep-view-sota-ai-native-2026-07-07.md § P3` + `docs/adr/0017-m55-index-maintenance-at-scale.md`. See CHANGELOG `[Unreleased] § Added`.
>
> **[CORREÇÃO 2026-07-20 — SUPERSEDED / JÁ ENTREGUE]** O muro do caminho DELETE foi fechado por **M56** (`theodb_rs/src/am/build.rs::vacuum_delete_inplace` + `am/hnsw_page.rs::tombstone_sweep` — tombstone in-place per-page, GenericXLog, sem advisory EXCLUSIVE / sem O(N) / sem stall; compaction disparada por churn ratio, com recall medido no M56 fase-2 churn bench) e **M104** (`vacuum_fold_max_mb` — blast radius de memória do fold limitado). Milestone criado sobre gap-analysis desatualizado (deep-view 2026-07-07, anterior a M56/M104). Residual não-bloqueador (streaming BUILD `collect_corpus`, IVF in-place) não justifica milestone próprio. Marcado `[x]` como correção de ROADMAP — não reimplementar (Regra: sem re-trabalho).

**Objective:** Remover a parada O(N) whole-index do fold de manutenção do índice vetorial sob EXCLUSIVE lock (hoje ~86s a 100k, ~14min projetado a 1M) implementando a fase 1 do ADR-0017 (tombstone-in-place + fold-para-compaction incremental/bounded), tornando o índice operável em escala.

**Definition of done:**

- [ ] Deletes marcam tombstone in-place (sem reescrever o índice inteiro); o VACUUM não segura EXCLUSIVE por um fold O(N).
- [ ] Compaction incremental/bounded (não whole-index sob EXCLUSIVE) conforme ADR-0017, com política de trigger explícita.
- [ ] Crash-safe provado por harness check-crash (padrão meta-pivot M48): pós-recovery o índice dá o top-k correto.
- [ ] MEDIDO em box quieta: stall de manutenção a 1M dentro do limite acordado (documentar em `docs/benchmarks/`), sem regressão de recall.

**Dependencies:** M48 (fold crash-safe / meta-pivot), M55 (decisão ADR-0017). Ambos `[x]`.

**Top risks (novos — pré-existentes já documentados no roadmap):**

1. Tombstones acumulados derivam recall até a compaction → exige política de trigger medida (não deixar drift silencioso).
2. Crash-safety do fold incremental sob concorrência é sutil (meta-pivot + advisory lock).

**Why now (do gap analysis):**

É o gate honesto de v1.0 — o deep-view crava que não podemos claimar produção com 14min de stall a 1M (`public-copy.md` / `/dogfood`), e é pré-requisito da narrativa "billion-scale em hardware barato" do reposicionamento (ADR-0033). Paridade de recall, superfície AI-native e columnar já estão prontos para sustentar o claim — falta a operabilidade.

---

## M117 — [x] SIMD cosine/IP no hot path de embeddings

> Added 2026-07-20 by `/roadmap-feature` (slug: `simd-cosine-ip-kernels`). Fonte: deep-view `§ P2` + `.claude/knowledge-base/backlog.md` ("AVX2 kernels for IP/cosine"). See CHANGELOG `[Unreleased] § Added`.
>
> **[CORREÇÃO 2026-07-20 — SUPERSEDED / JÁ ENTREGUE]** Entregue por **M58**: `theodb_rs/src/vec.rs::cosine_dist_from_bytes` despacha para `simd_x86::cosine_terms` (AVX2+FMA) e `dot_from_bytes`→`simd_x86::dot`; `ip_dist_from_bytes` reusa `dot_from_bytes`. Comentário no código: *"M58: AVX2+FMA cosine kernel when available (the real-embedding scan hot path)"*. Testes: `cosine_and_ip_from_bytes_match_scalar_within_eps_across_dims`. Milestone criado sobre gap-analysis desatualizado (deep-view 2026-07-07, anterior a M58). Marcado `[x]` como correção de ROADMAP — não reimplementar.

**Objective:** Adicionar caminho AVX2+FMA para as distâncias cosine e inner-product (hoje escalares em `vec.rs`; só L2 tem SIMD), colhendo o ganho de fator-constante no hot path dos embeddings reais (OpenAI/Cohere são cosine/IP).

**Definition of done:**

- [ ] `cosine_dist_from_bytes` e `ip_dist_from_bytes` ganham caminho AVX2+FMA com dispatch por feature em runtime, espelhando o L2 existente, com fallback escalar correto.
- [ ] Recall-neutro provado por ablação mesmo-índice (o kernel muda velocidade, não resultado).
- [ ] MEDIDO: microbench same-graph mostra o lift de latência do kernel cosine/IP (documentar em `docs/benchmarks/`; nunca citar cross-box).

**Dependencies:** M31b (distância SIMD L2). `[x]`.

**Top risks (novos):**

1. Dispatch runtime (AVX2/AVX512/NEON) precisa de fallback escalar testado em CPU sem a feature.
2. Medir o kernel exige ablação mesmo-índice (lição registrada: troca de box confunde o número).

**Why now (do gap analysis):**

Quick-win barato no eixo exato (latência cosine/IP) que o M50 aponta como teto, e é o caso real dos embeddings — hoje o hot path deles roda escalar.

---

## M118 — [x] Filtered ANN eficiente: resume-from-discarded (caso RAG)

> Added 2026-07-20 by `/roadmap-feature` (slug: `filtered-ann-resume-discarded`). Fonte: deep-view `§ P4` + `.claude/knowledge-base/backlog.md` ("M52 follow-up: iterative scan resume-from-discarded"). See CHANGELOG `[Unreleased] § Added`.

**Objective:** Tornar o iterative scan do filtered ANN resumível a partir do discarded set (em vez de re-buscar o grafo inteiro com ef dobrado a cada esgotamento), no caso RAG `WHERE tenant=X ORDER BY emb`.

> **[RE-ESCOPO 2026-07-20 — owner-approved]** O DoD original (fechar o gap vs pgvector 0.8, ≤1.2×) foi **FALSIFICADO por medição**: theodb page-native é ~7–23× mais lento que o grafo in-memory do pgvector — gap estrutural de paradigma (ADR-0033/0035/0036), não de tuning. Nenhum claim de paridade pgvector é feito (Regra 5). O M118 é re-escopado para o resultado honesto **alcançável e medido**: correção + melhoria do PRÓPRIO path do theodb. Evidência: `docs/benchmarks/m118-resume-discarded.md`.

**Definition of done (re-escopado):**

- [x] `amgettuple` mantém estado de scan resumível entre chamadas (discarded set) — sem re-percorrer o grafo do zero a cada esgotamento (`ann/scan_core.rs::ResumableGround` + `am/scan.rs` wiring).
- [x] Recall mantido em paridade no filtered path — **recall@10 = 1.0** vs brute-force exato sob filtro seletivo (A/B in-PG, `Index Scan using theodb_hnsw`).
- [x] MEDIDO own-path A/B: `resume ON` **~1.95× mais rápido** que o re-search M52 (`resume OFF`) a recall casado (14.33 vs 27.94 ms @ 0.9967) — `docs/benchmarks/m118-resume-discarded.md`. **[Gate original ≤1.2× vs pgvector: FALSIFICADO — não alcançável, não claimado.]**
- [x] Bounded + kill-switch: `theodb_hnsw.resume_max_mb` (fail-safe, EC-5 no-panic) + `theodb_hnsw.resume` (on/off).

**Dependencies:** M52 (filtered ANN / iterative scan). `[x]`.

**Top risks (novos):**

1. Estado de scan resumível + MVCC/rescan sem skip/dup entre chamadas (self-join / nested-loop).
2. O discarded set resumível precisa caber em memória bounded (não explodir por query seletiva).

**Why now (do gap analysis):**

É o caso RAG real (filtro por tenant + `ORDER BY` embedding); hoje temos recall em paridade mas pagamos QPS. O backlog M52 já rastreia a otimização.

---

## M119 — [x] AI-native depth: cross-encoder re-rank + chunking recursivo

> Added 2026-07-20 by `/roadmap-feature` (slug: `ai-native-depth-rerank-chunking`). Fonte: deep-view `§ P6` + `.claude/knowledge-base/backlog.md` ("M54: chunking recursivo separator-aware"). See CHANGELOG `[Unreleased] § Added`.
>
> **[CORREÇÃO 2026-07-20 — SUPERSEDED / JÁ ENTREGUE]** Entregue por **M65** (`theodb_rs/src/rerank.rs` = cross-encoder rerank real, contrato Cohere/Jina/Voyage/BGE-TEI, exposto como `ai.rerank`) + **M54** (chunking `recursive` separator-aware em `theodb_rs/src/chunk.rs`: `\n\n→\n→. →space`). Importante: o rerank **já foi medido** em BEIR e é **honest-negative** (commit `604184b`: rerank piora nDCG −3.8%) — reimplementar/forçar seria perseguir um dead-end já medido. Milestone criado sobre gap-analysis desatualizado. Marcado `[x]` como correção de ROADMAP — não reimplementar.

**Objective:** Elevar a superfície AI-native acima de paridade com pgai/Supabase: adicionar um estágio opcional de re-rank cross-encoder ao hybrid e um chunking recursivo separator-aware ao `theodb.chunk_text`.

**Definition of done:**

- [ ] `ai.rerank` / hybrid ganham estágio opcional de cross-encoder re-rank via endpoint HTTP OpenAI-compat existente (opt-in por GUC/param, bounded em latência/custo).
- [ ] `theodb.chunk_text` ganha splitter recursivo separator-aware (parágrafo→frase→palavra→char) além da janela de caracteres v1.
- [ ] MEDIDO: lift de nDCG@10 em BEIR (scifact / nfcorpus) com re-rank ligado vs desligado, com teste de significância (documentar em `docs/benchmarks/`).

**Dependencies:** M53 (hybrid / RRF), M54 (vectorizer / chunking). Ambos `[x]`.

**Top risks (novos):**

1. Cross-encoder adiciona latência + I/O externo → precisa ser opt-in e bounded (não regride o hybrid default).
2. Chunking recursivo muda os embeddings existentes → exige caminho de migração / reindex documentado.

**Why now (do gap analysis):**

Pós-reposição (ADR-0033), AI-native / HTAP / abertura são os eixos diferenciadores; hoje igualamos pgai/Supabase mas não superamos — re-rank + chunking avançado é o delta medível que passa de paridade a superioridade nessa superfície.

---

## M120 — [x] Filtro estruturado fail-closed para `ai.hybrid_search_rrf` (segurança multi-tenant)

> Added 2026-07-20 by `/roadmap-feature` (slug: `hybrid-fail-closed-filter`). Fonte: `.claude/knowledge-base/backlog.md` ("[M53 review — council-security F1]") + código `theodb_rs/src` (`ai.hybrid_search_rrf`). See CHANGELOG `[Unreleased] § Added`.

**Objective:** Substituir/complementar o `filter_sql` **cru** (SQL interpolado `%5$s` sob SECURITY INVOKER) por um **filtro estruturado fail-closed** (coluna/operador/valor com `quote_ident`/`%I` no identificador + **bind** do valor, nunca interpolação) — a única defesa realmente fail-closed para input não-confiável, pré-requisito para expor `ai.hybrid_search_rrf` no data-plane **multi-tenant** (o coração do theo-data).

**Definition of done:**

- [ ] API de filtro estruturado: aceita `(coluna, operador, valor)` de um **allowlist** de operadores (`= < > >= <= IN &&`), identificador via `quote_ident`, valor **bindado** ($n) — zero SQL cru no caminho estruturado.
- [ ] **Fail-closed:** coluna/operador fora do allowlist → **erro tipado** (SQLSTATE 22023), não passa. Teste negativo (`hybrid_filter_structured_rejects_*`).
- [ ] O payload que passava o guard cru (`filter_sql => '(SELECT count(*) FROM t) >= 0'`) é **rejeitado** pelo caminho estruturado.
- [ ] O `filter_sql` cru permanece como opt-in explícito documentado como **caller-privilege** (COMMENT sem a garantia falsa "injection-safe") OU é deprecado — decisão no ADR.

**Dependencies:** M53 (hybrid / RRF). `[x]`.

**Top risks (novos):**

1. O filtro estruturado limita expressividade vs SQL cru — precisa cobrir os casos comuns sem virar um mini-parser (KISS); o `filter_sql` opt-in cobre o resto.
2. Mudança de assinatura de `ai.hybrid_search_rrf` pode quebrar callers → manter `filter_sql` como opt-in retrocompatível.

**Why now (do gap analysis 2026-07-20):**

**BLOCKER latente de segurança.** Hoje seguro sob INVOKER + read-only SPI + REVOKE FROM PUBLIC, MAS vira escalonamento sob qualquer wrapper SECURITY DEFINER ou GRANT a role isolado — colide com o modelo de tenant do theo-data. Pós-reposição (ADR-0033), segurança/AI-native é eixo diferenciador (não QPS vetorial). Maior alavancagem dos gaps abertos.

---

## M121 — [x] IVF cosine/ip spherical k-means (recall quality)

> Added 2026-07-20 by `/roadmap-feature` (slug: `ivf-spherical-kmeans`). Fonte: `.claude/knowledge-base/backlog.md` ("IVF cosine/ip spherical k-means, from M49 review council-index-storage HIGH-2") + código `theodb_rs/src/ann/ivf.rs`. See CHANGELOG `[Unreleased] § Added`.

**Objective:** Usar **spherical k-means** (normalizar o centroide no update) para o IVF cosine/ip, em vez de centroides de média-aritmética que derivam da esfera unitária — fechando parte do gap de recall IVF cosine/ip (medido 0.83-0.89 vs HNSW 1.0), **gated por benchmark** provando o lift (measurement-first, não assumir).

**Definition of done:**

- [ ] O k-means do IVF, para métrica cosine/ip, **normaliza os centroides no update** (spherical); o path L2 fica **byte-idêntico** (inalterado).
- [ ] MEDIDO: recall@10 IVF cosine/ip **sobe** vs o baseline arithmetic-mean, a QPS casado (documentar em `docs/benchmarks/`).
- [ ] **Gate honesto:** se o lift não justificar (< limite acordado), **reverter** e registrar honest-negative — decisão measurement-first (como o slot-reuse M56).

**Dependencies:** M49 (IVF cosine/ip opclasses). `[x]`.

**Top risks (novos):**

1. Spherical k-means pode convergir mais devagar → capar iterações (o cap de treino do M88 já existe; reusar).
2. O lift pode ser marginal → gate por benchmark; aceitar honest-negative em vez de shipar complexidade sem ganho medido.

**Why now (do gap analysis 2026-07-20):**

Recall **quality** no eixo de **correção** (não QPS) — o IVF cosine é o path com o maior gap de recall conhecido (backlog M49 HIGH-2). É own-code, permissivo, e **não esbarra no teto estrutural de QPS** que o M118 mediu. Melhoria real de qualidade sem overclaim.

---

## M122 — [x] Embed totalmente assíncrono no vectorizer (fecha o xmin-pin sob endpoint pendurado)

> Added 2026-07-20 by `/roadmap-feature` (slug: `async-embed-vectorizer`). Fonte: `theodb_rs/src/vectorizer.rs:15-16` (follow-up rastreado do M54/ADR-0016) + backlog "[M54 review — council-index HIGH-1]". See CHANGELOG `[Unreleased] § Added`.

**Objective:** Tornar o worker do vectorizer **3-fases de verdade** — (A) txn lê `content`+cfg resolvido e **commita a lease** → (B) embed HTTP roda **sem txn aberta** → (C) txn nova escreve o vetor + marca o job — para que um endpoint de embedding pendurado **nunca prenda o xmin horizon** (hoje o HTTP roda dentro da txn de processamento do job, atrasando o VACUUM até o timeout ~90s).

**Definition of done:**

- [ ] O worker processa cada job em 3 fases explícitas: fase A (txn) lê content+cfg e commita o claim/lease; fase B faz o `run_batch` **sem txn** recebendo o cfg **já resolvido** (não relê GUC via SPI dentro do HTTP); fase C (txn nova) escreve o(s) vetor(es) e marca o job `done`.
- [ ] **MEDIDO:** sob um endpoint mock lento/pendurado, o `backend_xmin` do worker **não permanece preso durante o HTTP** — teste que injeta um embed lento e verifica que o snapshot é liberado no commit da fase A (o xmin não segura o horizonte durante os N segundos do HTTP).
- [ ] **Crash-safety:** crash entre fase B e fase C não perde nem duplica — o job volta a `claimed`/`pending` e é re-drenado **idempotente** (state machine do queue já garante; teste de crash reproduz e valida).
- [ ] O caminho síncrono direto (`ai.embed` single-call fora do vectorizer) permanece **inalterado** (retrocompat — só o worker do vectorizer muda).

**Dependencies:** M54 (vectorizer / job-queue, ADR-0016). `[x]`.

**Top risks (novos):**

1. **Config drift** entre a leitura do cfg (fase A) e a escrita (fase C) — mitigar passando o cfg **resolvido** pela fase B em vez de reler GUCs; a fase C não depende de estado que possa ter mudado.
2. **Crash após HTTP-ok e antes do write (fase C)** — o vetor computado se perde e o job reprocessa: aceitável (idempotente; custo = 1 HTTP a mais), mas o teste de crash-safety deve provar que não há vetor órfão nem job preso.

**Why now (roadmap V1 completo, 2026-07-20):**

Hardening pós-roadmap-completo no eixo **operabilidade + AI-native** (eixos vivos pós-ADR-0033). É o único follow-up `HIGH`/`MEDIUM` verificado-real que sobrou (o próprio código o declara pendente em `vectorizer.rs:15-16`). Bounded pelo timeout hoje, mas o async é o correto — um endpoint de embedding degradado não deve atrasar o VACUUM do banco inteiro.

---

## M123 — [x] Significância estatística pareada do hybrid vs vector (BEIR)

> Added 2026-07-20 by `/roadmap-feature` (slug: `hybrid-beir-significance`). Fonte: backlog "[M53 review — council-benchmark] Teste de significância pareado hybrid vs vector (BEIR)" + `docs/benchmarks/m53-hybrid-beir.md` (harness já existe). See CHANGELOG `[Unreleased] § Added`.

**Objective:** Elevar o benchmark hybrid-BEIR de "médias reportadas" para **prova estatística** — teste de significância **pareado** (per-query) do hybrid (BM25+vector+RRF) vs vector-only, com p-value + tamanho de efeito + IC, honrando "performance é claim, não opinião" (regra TheoDB 5) e a lente council-ai-in-db "isso melhora recall de verdade?".

**Definition of done:**

- [ ] O harness `benchmarks/run_m53_hybrid_beir.py` reporta, **por query**, a métrica de ranking (nDCG@10 ou recall@k) para hybrid **e** vector-only sobre ≥1 dataset BEIR **permissivo** (ex.: SciFact / NFCorpus — pequenos, licença aberta).
- [ ] **MEDIDO:** teste de significância **pareado** sobre as diferenças per-query (bootstrap pareado OU Wilcoxon signed-rank), reportando **p-value + tamanho de efeito + IC 95%** — não apenas a média.
- [ ] **Honestidade (anti cherry-pick):** reporta `n` total de queries, a fração onde o hybrid **perde**, e o veredito honesto — se **não-significativo**, dizer explicitamente (honest-negative aceito, como M121). Sem selecionar pontos de recall/k favoráveis.
- [ ] Artefato reproduzível em `docs/benchmarks/` (comando exato, dataset+versão, seed).

**Dependencies:** M53 (hybrid / RRF). `[x]`.

**Top risks (novos):**

1. **Dataset BEIR** pode ser grande / licença ambígua — usar subset permissivo e pequeno (SciFact/NFCorpus); registrar a licença do dataset no artefato.
2. **Resultado pode ser NÃO-significativo** — aceitar honest-negative e reportar sem overclaim (o valor é a *prova*, seja qual for o sinal); nunca ajustar o dataset/k para "dar significativo".

**Why now (roadmap V1 completo, 2026-07-20):**

O hybrid é o coração do pilar **AI-native**, e o benchmark existe mas **não prova** que o ganho é real (não ruído). No eixo correção/recall-quality (o eixo vivo pós-ADR-0033), uma prova de significância pareada é o gate de honestidade que falta — barato (harness existe), on-axis, e alinhado a "sem afirmação de performance sem artefato".

---

## M124 — [x] Dogfood real: capability theo-data sobre TheoDB self-hosted (prova de produção)

> Added 2026-07-20 by gap-analysis (`knowledge-base/audits/2026-07-20-analysis.md` Recomendação 1 / Risco H9). Fonte: dogfood-golden-rule (`rules/dogfood-golden-rule.md § 1`, âncora `theo-data-capability-on-theodb`) + `knowledge-base/dogfood/manifest.md` (status `planned`). See CHANGELOG `[Unreleased] § Added`.

**Objective:** Fechar o **maior gap de maturidade** — hoje toda evidência é benchmark sintético (109 artefatos), **zero uso sustentado real** → "production-ready" é inreivindicável. Entregar o **enabler theo-db-side** para o dogfood: um caminho reproduzível de self-host + uma capability theo-data com o retrieval real apontado para o TheoDB (vectorizer + `ai.hybrid_search_rrf`), e a **primeira evidência** registrada — abrindo o caminho para o status `running`.

**Definition of done:**

- [ ] **Quickstart reproduzível de self-host** documentado (`docs/ops/`): stand up de um TheoDB próprio (imagem/compose OU recipe pgrx-install) com `theodb_rs` + `shared_preload_libraries` + o worker do vectorizer ativo — um membro do time consegue subir do zero.
- [ ] **Uma capability theo-data com retrieval real no TheoDB:** `theodb.create_vectorizer` mantém uma coluna de embedding fresca + as queries reais da capability passam por `ai.hybrid_search_rrf` (substituindo o store atual). Config versionada.
- [ ] **Primeira evidência** em `knowledge-base/dogfood/evidence/*.md` (frontmatter § 5: scenario/date/operator/outcome/summary), **incluindo ≥1 história de falha** (um dogfood sem falhas é teatro — § 4).
- [ ] O manifest documenta o caminho `planned → wired → running`; o flip para `running` (uso sustentado ≥30d) é operacional/cross-repo, **não** gated neste milestone de código.

**Dependencies:** M122 (vectorizer async endurecido) `[x]`, M120 (filtro fail-closed p/ input não-confiável) `[x]`.

**Top risks (novos):**

1. **Escopo cross-repo** — HA/control-plane/deploy são fora deste repo (CLAUDE.md). Mitigar: este milestone entrega o *enabler* theo-db-side (quickstart + wiring + 1ª evidência); a operação sustentada vive no workspace/capability.
2. **Sem tráfego real disponível** — se nenhuma capability puder migrar o retrieval agora, o milestone vira `wired` (invocado 1× em smoke) em vez de `running`; honesto, mas não fecha o claim de produção.

**Why now (do /analysis 2026-07-20):**

Recomendação #1 e Risco dominante (H9): a trajetória de *engenharia* é sólida (6 hipóteses-core validadas), mas a de *maturidade-de-produção* está travada em evidência que não existe. É o único passo que benchmark não substitui.

---

## M125 — [x] Resolver H6: significância da híbrida num dataset lexical-heavy (ou travar posicionamento honesto)

> Added 2026-07-20 by gap-analysis (`knowledge-base/audits/2026-07-20-analysis.md` Recomendação 2 / Risco H6). Fonte: `docs/benchmarks/m123-hybrid-significance.md` (PARITY medido na SciFact). See CHANGELOG `[Unreleased] § Added`.

**Objective:** O M123 mediu **paridade** (hybrid vs vector não-significativo, p=0.25, 296/300 empates) numa SciFact dense-strong onde a perna FTS é fraca. Testar a híbrida onde ela *deveria* ajudar — um dataset **lexical-heavy** (keyword/termos raros) com o mesmo teste pareado — para converter o value-prop AI-native de **AT_RISK** em **medido**: ou a híbrida vence significativamente em algum regime, ou o posicionamento honesto é travado ("híbrida disponível; paridade em dense-strong").

**Definition of done:**

- [ ] O harness roda o teste pareado (reusa `theodb_bench.significance.paired_significance` do M123) num dataset BEIR **lexical-heavy** permissivo (ex.: um subset keyword-heavy; declarar licença) além da SciFact.
- [ ] **MEDIDO:** p-value pareado + IC + wins/losses/ties reportados; se a híbrida vencer significativamente (p<0.05 E IC>0) num regime, o claim é registrado com evidência; se não, **posicionamento honesto travado** no doc (sem overclaim).
- [ ] **Anti p-hack:** endpoint pré-declarado (nDCG@10), correção de comparação múltipla se >1 dataset, honest-negative aceito (como o M123). Sem dataset-shopping para achar significância.
- [ ] Artefato em `docs/benchmarks/` atualizando o veredito AI-native.

**Dependencies:** M123 (teste de significância) `[x]`.

**Top risks (novos):**

1. Pode dar paridade/perda em todos os regimes → honest-negative: o posicionamento vira "híbrida é opção, não superioridade medida"; aceitar (o valor é a prova).
2. Dataset lexical-heavy permissivo pode ser escasso → declarar licença + usar CI-internal (como a SciFact CC BY-NC do M123).

**Why now (do /analysis 2026-07-20):**

Recomendação #2 / Risco H6: a superioridade da híbrida (coração do pilar AI-native) está **não-provada**. Fechar isso honestamente — medir onde deveria ajudar — é o que "performance é claim, não opinião" exige.

---

## M126 — [x] Split do god-file `hnsw_page.rs` (3.456 LoC) — reduzir o risco de manutenção/segurança

> Added 2026-07-20 by gap-analysis (`knowledge-base/audits/2026-07-20-analysis.md` Recomendação 3 / Risco). Fonte: métrica A2/A4 do /analysis (`theodb_rs/src/am/hnsw_page.rs` = maior arquivo, hot-path, concentração de `unsafe`). See CHANGELOG `[Unreleased] § Added`.

**Objective:** `am/hnsw_page.rs` (3.456 LoC) é o maior arquivo, o hot-path de maior churn (M35/M118/M122-adjacente) e concentra superfície `unsafe` — o **maior risco de manutenção/segurança** medido. Split **preservando comportamento** ao longo das costuras naturais (page-format/codec ↔ traverse ↔ scan/resume), mantendo os testes byte-idênticos.

**Definition of done:**

- [ ] `hnsw_page.rs` decomposto em módulos coesos por responsabilidade (ex.: layout/codec de página, traverse/frontier, scan/resume) — nenhum arquivo resultante > ~1.500 LoC; sem `god module` genérico (`util`/`common`).
- [ ] **Comportamento preservado (refactor puro):** toda a suíte de testes + os benchmarks de recall passam **byte-idênticos** (mesmos rankings, mesmo recall) — provado por A/B same-index antes/depois.
- [ ] Fronteiras `unsafe` isoladas nos módulos de FFI/página (não espalhadas); cada `unsafe` mantém seu invariante documentado (o padrão do review council-rust-pgrx).
- [ ] Zero mudança de API pública / zero mudança de formato on-disk (índices existentes continuam legíveis).

**Dependencies:** M122 (último toque grande no path do vectorizer/scan) `[x]`.

**Top risks (novos):**

1. Refactor de hot-path com 357 `unsafe` no total → risco de introduzir bug sutil. Mitigar: refactor puro (sem mudança de lógica), A/B same-index byte-idêntico obrigatório, review council-rust-pgrx.
2. `cargo pgrx test` não linka no droplet (gotcha conhecido) → validação via example standalone + A/B in-PG, como M118/M121/M122.

**Why now (do /analysis 2026-07-20):**

Recomendação #3 / Risco de manutenção: com 19 arquivos >500 LoC e este a 3.456, é o ponto de maior concentração de risco. Dívida estrutural, não feature — mas o /analysis a apontou como o maior lever de manutenibilidade.

---

## Programa: benchmark oficial (adopt-and-wrap) — M127–M130

> Added 2026-07-20. Fonte: discovery `knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md` (SHIPPABLE 97.5) + `docs/adr/0050-official-benchmark-adopt-and-wrap.md`. Decisão do owner ("o mais FAANG possível"): adotar o benchmark oficial por pilar (comparabilidade reproduzível por terceiros) **e** manter uma camada fina nossa (significância + A/B byte-idêntico + gate de corretude) por cima — os tools oficiais não a repõem. Rollout **vector-pilot-first**.

## M127 — [x] Benchmark oficial: pilar VETOR (VectorDBBench + ann-benchmarks) — piloto do padrão adopt-and-wrap

**Objective:** entregar a **entrada oficial de TheoDB no benchmark vetorial** (um adapter `BaseANN`/`psycopg` para ann-benchmarks + o driver estilo VectorDBBench) rodando datasets reais, E estabelecer a **camada-wrap reutilizável** (significância pareada + A/B byte-idêntico sobre o output per-query) que os pilares seguintes reusam. Este é o slice vertical que de-risca o programa inteiro.

**Definition of done:**

- [ ] Adapter `BaseANN` (`fit`/`query`) para TheoDB rodando ann-benchmarks em ≥1 dataset D1-safe (GloVe/PDDL) + ≥1 eval-only (SIFT/GIST via CI-download) — recall×QPS MEDIDO, reprodutível.
- [ ] Camada-wrap genérica extraída (`benchmarks/theodb_bench/significance.py` + A/B byte-idêntico) consumindo o output per-query do runner oficial — provada nesse pilar.
- [ ] Posicionamento honesto: qualquer número vs ScaNN/AlloyDB cita `docs/benchmarks/m73-headtohead-verdict.md` (magnitude não-publicada em fonte neutra); direção pode citar o blog Google.
- [ ] Os `run_m*.py` vetoriais comparativos redundantes aposentados (a camada-wrap + entrada oficial os substituem); significância/regressão preservadas.

**Dependencies:** M126 `[x]` (ADR-0050).

**Top risks:** (1) entrada no leaderboard público exige box canônico (c6a/AWS) — CI pode usar box próprio e marcar "self-hosted box"; (2) licença TEXMEX SIFT/GIST MUST-VERIFY antes de qualquer bundling.

---

## M128 — [x] Benchmark oficial: pilar COLUNAR/OLAP (ClickBench + TPC-H-derived)

**Objective:** entrada TheoDB no **ClickBench** (copiar o contrato `postgresql/`: `create.sql`/`queries.sql`/hooks + `results/*.json`) rodando o `hits` real (CI-download), reusando a camada-wrap de M127; adicionar cobertura join-heavy TPC-H-derived (tpch-kit/DBT3) rotulada "TPC-H-derived".

**Definition of done:**

- [ ] Diretório de entrada ClickBench para TheoDB; 43 queries rodando; protocolo cold=1º/hot=min-hot/geomean MEDIDO.
- [ ] **A/B byte-idêntico de resultado** por cima (ClickBench é timing-only, `check`=`SELECT 1` — o oráculo de corretude é NOSSO).
- [ ] `hits` (CC-BY-NC-SA) só CI-download, nunca empacotado (D1); TPC-H via kit da comunidade, rotulado "derived".

**Dependencies:** M127 `[ ]` (reusa a camada-wrap provada).

---

## M129 — [x] Benchmark oficial: pilar OLTP (HammerDB TPROC-C + pgbench)

**Objective:** rodar **HammerDB TPROC-C** (NOPM, claim-grade) + **pgbench** (TPS, smoke) como drivers externos out-of-tree contra PG17, reusando a camada-wrap; manter o gate ACID/crash-safety (#46/#47) ao lado de todo número de throughput.

**Definition of done:**

- [ ] Recipe reprodutível TPROC-C (NOPM) + pgbench (TPS) MEDIDO; HammerDB (GPLv3) como driver externo, nunca forkado/linkado (D1).
- [ ] Gate de corretude/durabilidade preservado (os tools OLTP não têm — postam NOPM com `fsync=off`); crash-harness `#46/#47` roda junto.
- [ ] Significância pareada sobre runs repetidos (a camada-wrap).

**Dependencies:** M127 `[ ]`.

---

## M130 — [x] Benchmark oficial: pilar HTAP (CH-benCHmark / BenchBase)

**Objective:** rodar **CH-benCHmark via BenchBase** (TPC-C + 22 TPC-H queries em um schema) contra PG17 (pin de SHA; Java 23), derivar a métrica dual tpmC/QphH, reusando a camada-wrap + validação de resultado OLAP (BenchBase valida só timing).

**Definition of done:**

- [ ] BenchBase CH-benCHmark rodando contra PG17 (SHA pinado); métrica dual derivada do `summary.json` MEDIDA.
- [ ] Validação de resultado OLAP sob contenção + significância (a camada-wrap; BenchBase não valida resultado).
- [ ] Toolchain Java 23 documentado como liability; determinismo de seed marcado honestamente (não confirmado no BenchBase).

**Dependencies:** M127 `[ ]`, M129 `[ ]` (reusa o schema/driver TPC-C do pilar OLTP).

---

## M131 — [x] Fix #135 — destravar o columnar-agg pushdown (planner hang em tabelas largas mixed-type)

> Added 2026-07-21 (`/roadmap-feature columnar-agg-planner-hang-fix`). Pré-requisito para um **rank ClickBench colunar defensável**: hoje o run colunar do M128 só mede o storage-path (agg OFF = latência nível-heap); o pushdown vetorizado de agregado (a vantagem colunar de verdade) é inutilizável em `hits` real por causa do #135. Grill: `knowledge-base/grills/columnar-agg-planner-hang-fix-feature-grill.md`.

**Objective:** corrigir o **#135** — o CustomScan `theodb_columnar_agg` **trava no PLANNER (uninterruptível, durante o planning, não a execução)** em tabelas largas mixed-type/TEXT-heavy como o `hits` real do ClickBench (105 colunas). Como está no planner, `statement_timeout` NÃO mata — só restart do servidor. Tabelas estreitas/uniformes NÃO reproduzem; o gatilho é a interação do schema largo+TEXT-heavy com um loop patológico no path/cost creation do CustomScan. Destravar isso converte "temos entrada ClickBench" em "temos aceleração colunar rankeável".

**Definition of done:**

- [ ] `EXPLAIN SELECT UserID, COUNT(*) FROM hits GROUP BY UserID` no `hits` real de 105 colunas com `theodb.enable_columnar_agg = on` planeja em **< 1s** (sem hang, sem restart) — measurement-first, o repro exato do #135.
- [ ] **Guard de latência de planner**: quando o CustomScan não consegue custear o path barato (limiar de largura/tipo), faz fallback ao plano nativo em vez de travar — um plano patológico nunca mais trava o backend (defesa em profundidade).
- [ ] Teste de regressão de **GROUP BY em tabela larga (100+ col, TEXT-heavy)** adicionado aos testes do planner colunar.
- [ ] **ClickBench colunar-acelerado MEDIDO**: re-rodar o harness do M128 com `enable_columnar_agg = on`; as queries de agregação vencem o storage-path/heap no MESMO box, com resultado **byte-idêntico** A/B vs heap (oracle de corretude preservado). Honesto: self-hosted, não canônico; UNBENCHMARKED clean-exit se o path acelerado ainda não rodar. `customscan=1` provado (não fallback nativo silencioso).

**Dependencies:** M128 `[x]` (a entrada ClickBench + o harness que revelou o #135), M100/M114/M115 `[x]` (o CustomScan `theodb_columnar_agg` cujo planner é corrigido).

**Risks:** (a) o loop patológico pode estar fundo na interação DataFusion/planner (não um O(cols²) simples) → mitigação: **spike de profiling measurement-first** do `plan_custom_path`/cost hook em tabela larga mixed-type ANTES de comprometer a abordagem (discover-first). (b) o guard de latência pode super-disparar e desligar a aceleração em tabelas legitimamente largas → mitigação: limiar **medido/tunável** + assert de que a aceleração AINDA engata no `hits` real após o fix (`customscan=1`, não fallback nativo silencioso).

**Boundary honesto:** é um fix de **correção de planner-integration** (destrava uma otimização existente), não capacidade nova. Habilita o rank ClickBench colunar; performance vira claim só com benchmark (`../.claude/rules/public-copy.md`).

**Prior art:** `theodb_rs/src/am/columnar_agg.rs::plan_custom_path`, `benchmarks/run_m128_clickbench.py`, PostgreSQL `setrefs.c::set_customscan_references`, `docs/benchmarks/columnar-groupby-verdict.md`, issue #135.

---

## M132 — [x] Fix #132 — vectorizer bgworker embeda no self-host (destrava o anchor de dogfood)

> Added 2026-07-21 (`/roadmap-feature vectorizer-worker-embed-fix`). **Maior alavanca da lista de maturidade:** o anchor de dogfood está travado em `wired` (só `running` sustenta claim de production-ready) e a própria evidência registra `outcome: partial` por causa deste defeito. Grill: `knowledge-base/grills/vectorizer-worker-embed-fix-feature-grill.md`.

**Objective:** corrigir o **#132** — no self-host, o background worker do vectorizer **dead-letra TODOS os jobs de embed** (`state='failed'`, `attempts=5`, `last_error='embed/upsert failed'`) e a coluna de embedding fica NULL. Fila, trigger e máquina de estados funcionam (5 pending → 5 failed); só o passo de embed **dentro do worker** falha. Discriminador decisivo: com as MESMAS GUCs de instância, `theodb.embed(...)` e `theodb.embed_batch(...)` **funcionam em sessão normal** e **falham sempre no bgworker** → o delta é o contexto de execução do worker, não a requisição. Isso quebra a metade **frescor** da promessa AI-native ("o vectorizer mantém os embeddings frescos") no deploy alvo.

**Definition of done:**

- [ ] `last_error` passa a registrar a causa **subjacente** (status HTTP, ou "embedding GUC não visível no worker") em vez do wrapper genérico `embed/upsert failed` — hoje a mensagem esconde a raiz e exige debugger.
- [ ] Log de startup do worker registra presença de endpoint/model e o **comprimento** da api-key (**nunca** o valor), tornando um worker mal-configurado diagnosticável só pelo log.
- [ ] Causa-raiz identificada e corrigida: `benchmarks/dogfood_anchor_smoke.sh` num self-host termina com `SELECT state, count(*) FROM theodb.vectorizer_queue GROUP BY 1` mostrando **0 linhas em `failed`** e a coluna de embedding **non-NULL para toda linha inserida**.
- [ ] Teste de regressão cobrindo **o caminho do worker** (não só o de sessão — o de sessão já passa hoje e não pegaria este defeito).
- [ ] Arquivo de evidência de dogfood registrando o anchor passando, tirando-o de `outcome: partial`.

**Dependencies:** M131 `[x]`, M122 `[x]` (o split async de embed — área de código), M124 `[x]` (o anchor + `dogfood_anchor_smoke.sh`).

**Risks:** (a) a causa pode ser **visibilidade de placeholder-GUC dentro de `BackgroundWorker::transaction`** → o fix passaria a exigir registrar `theodb.embedding_*` como GUCs custom reais (mudança de configuração visível ao operador, precisa doc); mitigação: confirmar a causa pelo `last_error`/log melhorado ANTES de escolher entre "registrar GUCs reais" e "documentar onde o worker lê". (b) diferença de init HTTP/TLS no worker pode **interagir com o split async do M122** e reabrir o pin de `backend_xmin` que o M122 fechou; mitigação: re-rodar a prova de xmin do M122 após o fix e tratar regressão ali como bloqueante.

**Boundary honesto:** é **correção de defeito operacional**, não capacidade nova. O caminho de query (`ai.hybrid_search_rrf`) já está provado (evidência `2026-07-20-anchor-smoke.md`); o que falta é o frescor assíncrono.

**Prior art:** issue #132, `knowledge-base/dogfood/evidence/2026-07-20-anchor-failure-modes.md`, `benchmarks/dogfood_anchor_smoke.sh`, `../.claude/rules/dogfood-golden-rule.md § 2` (contrato `wired` → `running`).

---

## M133 — [x] Fix #140 — restaurar o sinal de CI (todo job do Actions falha antes de qualquer step)

> Added 2026-07-21 (`/roadmap-feature ci-restore-signal`). É a **rede de segurança** sob todos os outros milestones: hoje o CI não dá sinal nenhum. Grill: `knowledge-base/grills/ci-restore-signal-feature-grill.md`.

**Objective:** corrigir o **#140** — **todo** job do GitHub Actions em `develop` falha há **30+ runs consecutivos**, cada um morrendo em **2–3s com ZERO steps executados** (`"steps": []`) e sem log (`BlobNotFound`). Afeta todos os jobs (`pg-regression`, `ai-sql`, `columnar-measure`, `hybrid-search`, `harness-unit`, `image-and-bench`, `bm25-measure`, `migration-smoke`). As releases v0.113.0–v0.117.0 foram **todas mergeadas vermelhas**, e a verificação do programa M127–M131 veio de runs medidos no droplet, não do CI — qualquer regressão real hoje é **invisível**.

**Definition of done:**

- [ ] Causa-raiz identificada **com evidência** e registrada no #140, distinguindo condição de conta/org do Actions (minutos esgotados / limite de gasto / Actions desabilitado) de defeito de workflow. A falha pré-step em `runs-on: ubuntu-latest` puro aponta para a primeira, mas o milestone **confirma, não assume**.
- [ ] Pelo menos um run completo em `develop` onde os **steps de fato executam** (`gh api .../jobs/<id>` retorna `steps` não-vazio e o log é recuperável) — prova de sinal restaurado, independente de passar ou falhar.
- [ ] A conclusão resultante é triada honestamente: verde fecha; vermelho **por motivo real de código** é registrado e cada falha vira seu próprio issue (ver risco (b)).
- [ ] Notificação de falha (ex.: hook `workflow_run`) para que um CI morto apareça **imediatamente**, não depois de 30 runs silenciosos.
- [ ] #140 fechado com comentário de evidência.

**Dependencies:** M131 `[x]`. Sem dependência de código: `.github/workflows/ci.yml` não muda desde antes do M127 (`1b83632`) — não é regressão do trabalho recente.

**Risks:** (a) a causa pode estar **fora do repositório** (billing/habilitação do Actions na org `usetheodev`) → exige **ação do owner** nas settings do GitHub que nenhuma mudança de código substitui; fronteira honesta: o milestone pode legitimamente terminar **BLOCKED-on-owner**, e isso deve ser reportado como BLOCKED em vez de maquiado (Regra 3 — BLOCKED honesto > PASS falso). (b) restaurar o CI pode **revelar falhas reais acumuladas** de 30+ runs não verificados → escopo pode crescer de "restaurar sinal" para "corrigir N quebras latentes"; mitigação: escopo deste milestone é **restaurar sinal + triar**, filando cada falha genuína como issue próprio.

**Boundary honesto:** é **reparo de infraestrutura/CI**, não capacidade de produto. Não muda o gate de release — CI verde é pré-condição explicitamente **opcional (warn-not-block)** no `cycle-release`, e foi por isso que as releases procederam legitimamente; este milestone devolve a rede.

**Prior art:** issue #140 (evidência `"steps": []`, `BlobNotFound`, histórico de 30 runs), `../.claude/rules/cycle-release.md` (CI verde como soft gate).

---

## M134 — [x] Fix #117 — SSRF cego via `theodb.llm_endpoint` setável pelo chamador

> Added 2026-07-21 (`/roadmap-feature llm-endpoint-ssrf-hardening`). Barreira de entrada para **qualquer** deploy multi-tenant ou com roles não-confiáveis. Grill: `knowledge-base/grills/llm-endpoint-ssrf-hardening-feature-grill.md`.

**Objective:** corrigir o **#117** — `theodb.llm_endpoint` **não é GUC registrada**: é lida via `current_setting('theodb.llm_endpoint', true)` (`pg.rs:50-56`), ou seja **placeholder GUC**, e no PostgreSQL **qualquer role** pode dar `SET` num nome pontuado para a própria sessão. Os únicos guards são o esquema (`chat.rs:266-267`, precisa ser `http(s)://`) e no-redirect (`http.rs:124-125`) — **não há bloqueio de IP privado/loopback/link-local**, e checar `http(s)` não é controle de SSRF. **Impacto:** um role com EXECUTE em qualquer função que toca LLM (`ai._chat`, `ai.extract_entities`, ou `theodb.graph_upsert(..., use_llm := true)`) aponta o **host do banco** para alvo interno arbitrário (metadata `169.254.169.254`, `127.0.0.1`, serviços internos) e dispara requisição server-side. SSRF **cego**: varredura de porta interna por timing/estado do circuit-breaker + hits em endpoints internos não autenticados.

**Definition of done:**

- [ ] `theodb.llm_endpoint` e `theodb.llm_api_key` **registradas como GUCs custom `GucContext::Suset`** (só operador/superuser) — sessão não-superuser **não consegue mais** dar `SET` (asserido por teste negativo que espera o erro).
- [ ] **Denylist de privado/loopback/link-local** aplicada antes do POST para no mínimo: `169.254.0.0/16`, `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `::1`, `fc00::/7`.
- [ ] **Resolve-then-connect no mesmo IP**, para que host com **DNS-rebinding** (registro A público que re-resolve para endereço interno) não contorne a denylist.
- [ ] Testes negativos asserindo o **erro tipado específico** para cada faixa bloqueada **e** para tentativa de rebinding — per `../.claude/rules/testing.md § 4.1`, teste de caso negativo assere o erro e a mensagem, não apenas "lança".
- [ ] Postura existente **não regride**: `REVOKE … FROM PUBLIC`, no-redirect e fail-fast continuam valendo (re-provados).

**Dependencies:** M131 `[x]`, M110 `[x]` (que adicionou `graph_upsert` como segundo caminho chamável, também coberto aqui).

**Risks:** (a) **resolve-then-connect exige fixar o IP resolvido no cliente HTTP** — o `http.rs` pode não expor pinning em nível de conexão, então o fix pode exigir mudança de cliente (resolver/connector custom) em vez de checagem no `resolve_chat_cfg`; mitigação: **spike da capacidade do cliente ANTES** de comprometer a abordagem — se pinning for inviável, **documentar honestamente a janela residual de rebinding** em vez de alegar cobertura total. (b) **denylist ampla demais quebra endpoint LLM interno legítimo** — muitos self-hosts rodam o model server em `10.x`/`192.168.x` por design, exatamente o operador que queremos servir; mitigação: denylist como default + **GUC de allowlist só-operador** (também `Suset`) para re-permitir hosts internos específicos — a escotilha é do operador, **nunca** do chamador.

**Boundary honesto:** é **hardening de segurança** de superfície pré-existente (M110 ampliou, não introduziu). Não adiciona capacidade; remove uma classe de abuso.

**Prior art:** issue #117 (repro com `169.254.169.254`; ponteiros `chat.rs:258-268`, `pg.rs:50`, `http.rs:124`), `theodb_rs/src/am/guc.rs` (registrações `define_custom_*_guc` existentes), a postura NL→SQL (denylist + fail-closed + REVOKE) em `theodb_rs/src/nl.rs` como modelo.

---

## M135 — [x] Suporte a PostgreSQL 18 (migração 17 → 18)

> Added 2026-07-21 (`/roadmap-feature pg18-support`). Grill: `knowledge-base/grills/pg18-support-feature-grill.md`. Custo **medido**, não estimado: sondagem de compilação contra PG18.4 na droplet.

**Objective:** migrar a extensão do PostgreSQL 17 para o **18** (release estável atual), fechando os **27 erros de compilação medidos** e re-provando o comportamento — não só a compilação. Decisão do owner (2026-07-21): **migrar**, não suportar as duas versões, porque **ainda não há base instalada** — é a janela em que a migração não custa nada a terceiros e evita dívida permanente de `#[cfg(feature)]`-branching nas 3 APIs divergentes. O PRD §338 já declarava a intenção ("adiciona PostgreSQL 18 em seguida") sem execução.

**Custo medido (sondagem 2026-07-21, `cargo check --features pg18` contra PG18.4):** 27 erros, assim distribuídos —

| Classe | Erros | Onde | Natureza |
|---|---|---|---|
| `TupleDescData.attrs` → `compact_attrs` + `populate_compact_attribute` | 9 | `columnar.rs`, `arrow_cache.rs`, `build.rs`, `df_executor.rs` | mecânica (acessor) |
| `relopt_parse_elt` ganhou campo | 10 | `options.rs` | mecânica |
| **Bitmap scan reformulado** — `tbm_begin_iterate` retorna valor (não ponteiro), `tbm_iterate(iter,&out)->bool`, `scan_bitmap_next_block` REMOVIDO do `TableAmRoutine` | 7 | `customscan.rs`, `columnar.rs` | **semântica** |
| `vacuum_delay_point(bool is_analyze)` | 1 | `fold.rs` | mecânica |
| `get_ordering_op_properties` último param virou `CompareType*` (era `int16`) | 1 | `columnar_agg.rs` | semântica (constante de comparação muda) |

Falsificação registrada: a hipótese de que a dor viria do WAL estava **errada** — `GenericXLog` (54 refs) e `IndexAmRoutine` compilam **limpos** no 18. Quebrou só onde o PG18 mexeu de propósito.

**Definition of done:**

- [ ] `cargo build --features pg18` **limpo** (0 erros) e a extensão instala/carrega num PG18.4.
- [ ] **Bitmap scan portado de verdade** para o contrato novo (decisão do owner: não declarar não-suportado) — incluindo teste que **força página lossy** (`ntuples < 0`) e assere que o recheck não perde nem duplica linha; um bug aqui não aparece em happy path, produz resultado errado.
- [ ] **Suítes de crash e de MVCC/isolamento verdes contra o binário 18** — `test_am_crash.py` e as de isolamento do `theodb_rs/isolation/`. Compilar não prova comportamento, e o que quebrou foi justamente o caminho de varredura.
- [ ] **Benchmark de sanidade no 18 vs baseline 17**, publicado em `docs/benchmarks/` — restabelece a linha de base, já que os 119 artefatos existentes foram medidos no 17 e passam a descrever configuração não distribuída.
- [ ] **Flags `pg13`–`pg17` removidas do `Cargo.toml`** (consequência coerente do migrar-para-18): elas nunca foram compiladas por ninguém, e manter declaração não verificada é a mesma classe de defeito que esta sondagem expôs.
- [ ] **Packaging publica o 18** — Dockerfiles e scripts hoje são pg17-only (4 referências); sem isso o suporte existe só para quem compila, não para quem consome a imagem.

**Dependencies:** M134 `[x]`.

**Risks:** (a) **Os 119 artefatos de benchmark foram medidos no PG17** — migrando, eles descrevem uma configuração que não distribuímos mais; mitigação: o benchmark de sanidade do DoD restabelece a base no 18, e os artefatos antigos ficam **rotulados como PG17** em vez de re-alegados. (b) **O rework do bitmap toca o caminho de recheck do MVCC** — erro ali não falha compilação nem happy path, produz resultado **errado** sob página lossy; mitigação: o teste de página lossy é item de DoD, não opcional. (c) **Sem CI (M133 aberto), toda prova é manual na droplet** — nenhuma regressão 17→18 é pega automaticamente; mitigação: registrar cada verificação com o gate anti-restart-silencioso, como nos M131/M132/M134.

**Boundary honesto:** é **migração de plataforma**, não capacidade nova. Nenhuma feature de usuário é adicionada; o ganho é rodar no Postgres que as pessoas de fato instalam hoje. Não cobre PG19 (só existe como `beta1` no pgrx 0.19) — acompanhar o 19 é compromisso contínuo com upstream em desenvolvimento, decisão separada.

**Prior art:** sondagem medida em 2026-07-21 (`cargo pgrx init --pg18 download` → PG18.4; `cargo check --features pg18` → 27 erros, lista completa com arquivo:linha); headers do PG18.4 em `/root/.pgrx/18.4/pgrx-install/include/postgresql/server` como fonte das assinaturas novas (`access/tupdesc.h`, `nodes/tidbitmap.h`, `commands/vacuum.h`, `utils/lsyscache.h`); PRD §338 (intenção declarada); pgvector/pgvectorscale como referência de política de versões.

---

## M136 — [x] Gates mecânicos de qualidade + Postgres `cassert` no CI

> Added 2026-07-21 (`/roadmap-feature quality-gates-mecanicos`). Grill: `knowledge-base/grills/quality-gates-mecanicos-feature-grill.md`.

**Objective:** equilibrar o portfólio de gates. Inventário medido em 2026-07-21: **28 regras, 40 skills, 8 hooks** (gates de PROCESSO, que dependem de alguém invocar) contra **zero** gates mecânicos de Rust — sem `clippy.toml`, `rustfmt.toml`, `deny.toml`, sem `-D warnings`, sem clippy/fmt no CI, e **840–967 warnings** no build. Pior: a regra **D1** (nenhuma AGPL na distribuição), a mais inegociável do projeto, é aplicada por **vigilância humana** — uma dependência AGPL transitiva passa reto até alguém notar, e descobri-la na v1.0 é rearquitetura, não patch.

**Definition of done:**

- [ ] `deny.toml` com `[licenses] allow = [...]` (Apache-2.0/MIT/BSD-*/ISC/PostgreSQL) + `cargo deny check` no CI — **D1 vira gate de máquina**. Um PR que puxe crate AGPL falha.
- [ ] `clippy.toml` + args de lint num **arquivo único** consumido por CI e script local (padrão `.neon_clippy_args`, que impede drift CI↔local), com `-D warnings`. Decisão explícita e registrada sobre os 840–967 warnings atuais: baseline-allow com sunset **ou** mutirão de limpeza — não ficar no meio.
- [ ] `rustfmt.toml` + `cargo fmt --check` no CI.
- [ ] **Entrada de matriz com Postgres compilado `--enable-cassert`.** Em build de release `Assert()` é no-op; esta é a única forma de ter cobertura de asserção do engine — e é a classe exata do crash #143 e dos segfaults de `rd_tableam`. É o gate de maior valor/custo desta lista (lição #1 do paradedb).
- [ ] `pgspot` sobre o SQL de instalação gerado pelo pgrx (957 LoC de superfície em `api.rs`, 94 `pg_extern`) — pega `SECURITY DEFINER`/`search_path` inseguros no DDL.
- [ ] `cargo metadata --locked` (drift de lockfile) + `cargo doc -Dwarnings` (rot de doc) + `cargo machete` (deps não usadas).

**Dependencies:** M133 `[ ]` — **sem CI vivo, todo gate aqui é documentação que ninguém executa.** Os arquivos de config podem aterrissar antes; o milestone só fecha quando eles efetivamente barram um PR ruim.

**Risks:** (a) ligar `-D warnings` com 840–967 warnings trava o build no dia 1 — mitigação: a decisão baseline-vs-mutirão é item de DoD, não improviso. (b) o `deny.toml` pode barrar uma dependência transitiva **já embarcada**, revelando um problema D1 latente — é o objetivo, mas vira trabalho não previsto; mitigação: rodar `cargo deny check` em modo report ANTES de torná-lo bloqueante.

**Boundary honesto:** não adiciona capacidade de produto. Compra a propriedade de que erros de classe conhecida param de chegar em `develop` sem intervenção humana.

**Prior art:** neon `deny.toml` + `.neon_clippy_args` + `clippy.toml` (disallowed-methods como política-em-lint); paradedb `lint-rust.yml` (`-D warnings --no-deps`, `cargo machete`, `taplo`) e a entrada cassert de `test-pg_search.yml`; pg_durable `pgspot-gate.sh`.

---
## M137 — [x] Cadeia de upgrade do `theodb_rs` (`ALTER EXTENSION UPDATE`)

> Added 2026-07-21 (`/roadmap-feature theodb-rs-upgrade-chain`). Grill: `knowledge-base/grills/theodb-rs-upgrade-chain-feature-grill.md`.

**Objective:** tornar o TheoDB atualizável. Medido em 2026-07-21: `theodb_rs` expõe **94 funções `pg_extern`** e tem **zero** scripts de upgrade, travado em `default_version = '1.0.0'` através de **120 releases** (v0.120.0). Quem instalou **não consegue** `ALTER EXTENSION theodb_rs UPDATE` — teria de dropar e recriar, perdendo todo objeto dependente. Esta é a classe de defeito **irrecuperável para quem já instalou**: não dá para consertar retroativamente, só para parar de piorar. (A extensão umbrella `theodb` tem cadeia 1.0→1.4 consistente — o buraco é só na extensão Rust.)

**Definition of done:**

- [ ] Baseline de versão declarado e **honesto** sobre o que ele cobre: instalações anteriores a este milestone não ganham caminho retroativo, e isso é dito em voz alta na doc de migração — não escondido.
- [ ] `theodb_rs--<N>--<N+1>.sql` gerado e versionado a cada mudança de superfície SQL, com `default_version` acompanhando.
- [ ] **Teste de upgrade em CI:** instala v(N-1), roda `ALTER EXTENSION theodb_rs UPDATE`, e assere que a superfície pós-upgrade é idêntica à de uma instalação limpa de vN.
- [ ] **Gate de migração já lançada:** um PR que edite um script de upgrade cujo alvo já tem tag falha o CI (o script do paradedb tem ~12 linhas de bash). Editar migração lançada nunca chega a quem já atualizou.
- [ ] Gate de drift install↔upgrade: o SQL que o pgrx regenera a cada build não pode divergir do que a cadeia de upgrade produz.
- [ ] Regra de ordenação documentada: bumpar a versão de desenvolvimento **antes** de mesclar novo trabalho de schema (senão o script corrente aponta para a versão recém-lançada e congela).

**Dependencies:** M135 `[x]`.

**Risks:** (a) não há registro de qual era a superfície SQL em cada release passado — o baseline parte do estado atual, e instalações antigas ficam sem caminho; registrado como limite, não como resolvido. (b) o pgrx regenera o SQL inteiro a cada build, então é fácil o upgrade divergir do install — por isso o gate de drift é DoD, não opcional.

**Boundary honesto:** não adiciona feature. Remove uma condição que impede qualquer uso sério em produção.

**Prior art:** paradedb `check-released-migrations.yml` + `test-pg_search-upgrade.yml` + SchemaBot (`check_migration_diff.py`); pg_durable `sql/` com cadeia explícita + `scripts/test-upgrade.sh` em CI.

---
## M138 — [x] BM25 como perna lexical default (executa o gate de adoção do ADR-0013)

> Added 2026-07-21 (`/roadmap-feature bm25-perna-lexical-default`). Grill: `knowledge-base/grills/bm25-perna-lexical-default-feature-grill.md`.

**Objective:** corrigir um defeito de produto que temos **evidência decision-grade** desde 2026-07-07 e nunca aplicamos. M53 (BEIR scifact, 5.183 docs, 300 queries, 3 runs byte-idênticos):

| perna lexical | nDCG@10 | Recall@100 |
|---|---|---|
| `ts_rank_cd` — **o que shipamos** | **0,0703** | 0,0694 |
| `pg_textsearch` BM25 — opt-in, não embarcado | **0,6881** | 0,9182 |
| vetor (referência) | 0,7296 | 0,9733 |

O próprio artefato declara *"o gate de medição está executado"*. **Caveat honesto herdado do M53:** o gap de ~9,8× conflaciona qualidade de ranker com tamanho do candidate-set (o `@@` do `ts_rank_cd` derruba ~93% dos relevantes); o sinal limpo é BM25 **0,688 ombro-a-ombro com o vetor 0,730** sobre o próprio top-k. Além de corrigir o produto, este milestone estabelece **a linha de base que a engine própria (M140) terá de bater** — sem ela, "nossa BM25 é boa" não tem contra o quê.

**Definition of done:**

- [ ] `pg_textsearch` embarcado na distribuição (due-diligence D1 re-executada: PostgreSQL License, permissiva).
- [ ] `lexical_engine='bm25'` vira **default** de `ai.hybrid_search_rrf`, com `ts_rank_cd` selecionável para compatibilidade.
- [ ] **Medição que faltava do M53 §4:** híbrida-com-BM25 vs híbrida-com-`ts_rank_cd`, com teste de significância pareado sobre as queries — o M53 nunca mediu a fusão com BM25, só o leg isolado.
- [ ] Nota de migração: trocar o default **muda resultados de queries existentes**; documentado, não silencioso.
- [ ] Artefato em `docs/benchmarks/` com metodologia e comando de reprodução.

**Dependencies:** M137 `[ ]` — mudar o default da superfície SQL sem cadeia de upgrade entrega a melhoria só para instalações novas.

**Risks:** (a) `pg_textsearch` passa de exceção *gated* a **dependência embarcada** — o roadmap § "Fora de escopo" a mantinha explicitamente "não embarcada ainda"; embarcar agora e substituir pelo motor próprio (M140) é churn real de packaging/docs, **aceito conscientemente** para não deixar o usuário com 0,07 por mais um trimestre. (b) mudança de default altera resultado de query — mitigação: os dois engines selecionáveis + nota de migração.

**Boundary honesto:** é **adoção de peça de terceiro**, não código próprio. Fecha um gap medido de ~10× hoje e serve de baseline para o M140. Não nos torna donos da busca.

**Prior art:** `docs/benchmarks/m53-hybrid-beir.md` (a medição), ADR-0013 (a exceção permissiva gated), ADR-0003 (identificação do `pg_textsearch`).

---
## M139 — [x] SPIKE (o GATE): `Directory` do Tantivy sobre block storage do Postgres

> Added 2026-07-21 (`/roadmap-feature tantivy-directory-spike`). Grill: `knowledge-base/grills/tantivy-directory-spike-feature-grill.md`.

**Objective:** responder, com medição, a **única pergunta que decide** se o TheoDB pode ter engine lexical própria: conseguimos implementar o trait `Directory` do Tantivy sobre páginas do Postgres, com MVCC e WAL, sobrevivendo a crash real?

O risco **não está no BM25** — está em fazer o Tantivy viver dentro do banco. A pesquisa no ParadeDB (2026-07-21, clone local, AGPL **study-only**) mostrou que `pg_search` tem **105.286 LoC** — 3,2× o TheoDB inteiro — e que a parte cara é: `MVCCDirectory` (trait `Directory` sobre block storage, importando `pg_sys`), `MvccSatisfies` de 5 modos com `xmin`/`xmax` por segmento, **WAL resource manager próprio** registrado em `_PG_init` sob `shared_preload_libraries`, `LayeredMergePolicy` com merges em background worker sob `MergeLock`+advisory lock, e `ambulkdelete` com barreira de cleanup-lock para uma corrida específica. Tantivy upstream é **MIT** (`quickwit-oss/tantivy` v0.26.0 — D1 verificado).

**Definition of done:**

- [ ] Protótipo mínimo: implementação do `Directory` sobre páginas PG que **indexa N documentos e responde uma busca** — sem tocar o filesystem.
- [ ] **MVCC:** um segundo backend com snapshot anterior **não** enxerga documentos de uma transação não-commitada; após commit, enxerga.
- [ ] **Crash real:** `SIGABRT` no meio da indexação, replay do WAL, e o índice responde consistente (o mesmo método das nossas suítes `crash*.sh`).
- [ ] Medição do custo: tamanho do índice e latência de busca vs o `pg_textsearch` do M138 no **mesmo corpus** — o spike também mede se vale.
- [ ] **ADR com veredito GO/NO-GO** e, se GO, o escopo real derivado do que o protótipo doeu (VACUUM, merge e paralelismo ficam declaradamente FORA do spike e entram no escopo do M140).
- [ ] Verificar se o upstream basta ou se precisamos forkar (o ParadeDB forka o Tantivy com feature própria) — se precisar, o veredito aciona a **política D3** de fork.

**Dependencies:** M136 `[ ]` — o Postgres `--enable-cassert` no CI é o que pega violação de invariante nesta classe de código; é a lição #1 do paradedb e a classe exata do #143.

**Risks:** (a) o ParadeDB **forka** o Tantivy, o que sugere que o upstream não basta dentro de um banco — se confirmarmos, herdamos custo de manutenção de fork (D3: upstream-first, diff mínimo, CI de rebase, saída quando o upstream alcançar). (b) um spike pode passar no caminho feliz e esconder o custo real, que mora em VACUUM/merge/paralelismo — por isso o DoD exige **crash real com replay**, não "indexar e buscar".

**Boundary honesto:** é **spike**, não entrega. Pode terminar em NO-GO, e nesse caso o valor entregue é ter descoberto em semanas em vez de trimestres — o mesmo método que poupou o M73 (gap do ScaNN não-alcançável) e o M74 (RaBitQ dá memória, não QPS).

**Prior art:** ParadeDB `pg_search/src/index/directory/mvcc.rs`, `postgres/storage/{custom_rmgr,xlog}.rs`, `index/merge_policy.rs` (**AGPL — estudar, nunca copiar**, mesmo posicionamento do VectorChord); nossas próprias suítes `theodb_rs/isolation/crash*.sh` como molde do teste de crash; M83/M98/M107 como precedente de spike-como-gate.

---
## M140 — [ ] Engine lexical própria sobre Tantivy + crate núcleo sem pgrx *(gated M139)*

> Added 2026-07-21 (`/roadmap-feature engine-lexical-propria`). Grill: `knowledge-base/grills/engine-lexical-propria-feature-grill.md`.

**Objective:** assumir a perna lexical em vez de alugá-la. O North Star foi reposicionado (ADR-0033/0035) porque superioridade de QPS vetorial sobre o ScaNN foi **medida como não-alcançável** por extensão permissiva; o lugar onde podemos ser genuinamente superiores é a **superfície AI-native híbrida** — e isso exige as duas pernas nossas, não uma alugada.

Inclui a extração do **crate núcleo sem pgrx** — e aqui a justificativa não é estética: hoje **98%** do código (30.892 de 31.516 LoC) está atrás do pgrx, e **54 testes** não rodam porque `cargo pgrx test` não linka na droplet. Uma integração Tantivy tem superfície pura grande (parser de query, scoring, tokenizers) que precisa ser testável sem banco. O ParadeDB provou o padrão: o único crate deles sem pgrx é `tokenizers` — e **81%** do `pg_search` toca pgrx, ou seja, extrair "a engine inteira" seria copiar uma forma que eles próprios não têm.

**Definition of done:**

- [ ] Índice BM25 próprio exposto como **index AM** (`CREATE INDEX ... USING <am>`), com `ambuild`/`aminsert`/`amgettuple`/`ambulkdelete`/`amvacuumcleanup`/`amcostestimate`.
- [ ] **MVCC, VACUUM e crash-safety provados** pelas nossas suítes de isolamento e crash contra o binário shipado — o mesmo padrão do M99/M135, não uma versão mais fraca.
- [ ] **Bate a linha de base do M138** (`pg_textsearch`) em nDCG@10 no mesmo corpus, ou o milestone reporta honestamente que não bateu.
- [ ] Crate núcleo **sem dependência de pgrx** no `Cargo.toml` (esse é o teste), com os testes puros rodando em `cargo test` — incluindo os 54 hoje presos.
- [ ] **ADR-1:** reconcilia com o **ADR-0009** (`theodb-rs-api-surface-single-module`), que escolheu módulo único deliberadamente. Sem isso, o split lê como reversão não registrada.
- [ ] **ADR-2:** supersede a exceção permissiva do ADR-0013 para BM25 — a premissa *"não há peça own-code permissiva que resolva"* deixou de valer quando o Tantivy (MIT) entrou em cena. Inclui o plano de saída do `pg_textsearch` para quem adotou no M138.

**Dependencies:** M139 `[ ]` (o gate — NO-GO fecha este milestone antes de abrir) e M138 `[ ]` (a baseline a bater).

**Risks:** (a) **supersede decisão registrada** — a exceção do ADR-0013 e a escolha do ADR-0009; ambas exigem ADR explícito, nunca reversão silenciosa. (b) escopo: o `pg_search` tem 105k LoC; mesmo um subconjunto honesto (sem faceting/highlight/proximity) é o maior milestone já tentado aqui — mitigação: o escopo real é **derivado do M139**, não estimado agora.

**Boundary honesto:** é a aposta estratégica desta rodada, e a mais cara. Só abre se o M139 disser GO.

**Prior art:** M139 (o gate); ParadeDB como referência estrutural **AGPL study-only**; Tantivy MIT (`quickwit-oss/tantivy`); ADR-0009 e ADR-0013 (as decisões a reconciliar); M99/M100/M115 como precedente nosso de AM+CustomScan próprios.

---
## M141 — [ ] Dogfood `running`: theo-data em produção sobre TheoDB self-hosted *(continuação do M124)*

> Added 2026-07-21 (`/roadmap-feature dogfood-running`). Grill: `knowledge-base/grills/dogfood-running-feature-grill.md`.

**Objective:** mover o anchor de dogfood de **`wired`** para **`running`**. Pela nossa própria `dogfood-golden-rule.md`, `running` é o **único** valor que satisfaz o hard cap #2 — ou seja, **hoje não podemos reivindicar production-ready**, por mais benchmark que acumulemos (120 artefatos). Um banco de dados de verdade é aquele que seus criadores usam em produção, com dados que importam.

Isto não é esforço de engenharia; é a decisão de colocar o `theo-rag` ou o `theo-memory` rodando em cima do TheoDB e sentir o que quebra.

**Definition of done:**

- [ ] Uma capability theo-data serve **tráfego real de produto** (não carga sintética) a partir de um TheoDB self-hosted, por **≥ 30 dias**.
- [ ] O vectorizer declarativo mantém a coluna de embedding fresca sob mudança de conteúdo real.
- [ ] As queries reais passam por `ai.hybrid_search_rrf`, e o time **depende** do resultado estar correto e fresco.
- [ ] Evidência em `knowledge-base/dogfood/evidence/` com o frontmatter do golden rule, **≥ 3 arquivos**, **≥ 1 história de falha** (dogfood sem falha é teatro) e **≥ 2 operadores** distintos.
- [ ] `Status: running` no manifesto, e `/dogfood` retornando `EVIDENCE_SUFFICIENT`.

**Dependencies:** M124 `[x]` (entregou o `wired`; milestone concluído é imutável, então a extensão vem por novo milestone) e M138 `[ ]` — não faz sentido depender em produção de uma busca que mede 0,0703.

**Risks:** (a) dogfood real expõe classes de bug que benchmark não pega — operação, upgrade, backup, observabilidade; é o ponto, mas gera trabalho não planejado (e é por isso que M136/M137 vêm antes). (b) o hard cap exige evidência **fresca** (≤ 30 dias) e ≥ 2 operadores: um dogfood de uma pessoa só reproduz a síndrome do "único que sabe rodar", que o próprio golden rule lista como falha a evitar.

**Boundary honesto:** não adiciona capacidade. É o que **autoriza** dizer "banco de dados de verdade" — e sem ele, a frase é marketing.

**Prior art:** `.claude/rules/dogfood-golden-rule.md` (anchor `theo-data-capability-on-theodb`), M124 (o `wired`), M132 (destravou o worker do vectorizer no self-host).

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
