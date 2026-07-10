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

## M72 — [ ] QPS a 1M+ multi-cliente (throughput sob concorrência real)

**Objective:** o M32/M34 mediram p50 single-client. Faltam **QPS a 1M sob N clientes concorrentes** (regime real
de produção) — theodb_hnsw/ivfflat vs pgvector, mesmo hardware/dataset — provando (ou refutando honestamente) que
o throughput multi-cliente é competitivo, incluindo o efeito de lock/buffer do índice sob carga.

**Definition of done:**
- [ ] Harness multi-cliente (N conexões, QPS agregado, p50/p95/p99) a 1M×128d (SIFT1M) — theodb vs pgvector, ≥3 runs, mean±std → `docs/benchmarks/m72-qps-multiclient.{md,json}`.
- [ ] Veredito honesto de QPS multi-cliente (competitivo / gap medido) com a origem do gap identificada.

**Dependencies:** M60, M71. **Risco (MÉDIO):** contenção de buffer/lock; o gap pode ser estrutural (índice persistente vs library in-memory).

## M73 — [ ] Head-to-head MEDIDO vs ScaNN/AlloyDB (o VEREDITO de superioridade)

**Objective:** re-rodar o head-to-head do M33 (SIFT1M, mesmo hardware/query-set) **depois** de M60+M71+M72, e
emitir o **veredito de superioridade vetorial rastreável** do North Star. Honesto: o resultado pode ser (a)
fechou/reduziu o gap, (b) paridade own-code + trade-off de QPS documentado, ou (c) honest-negative. Em qualquer
caso, entrega a **prova medida de ONDE o TheoDB está** vs o SOTA (o que o North Star exige — não uma vitória
inventada). Caveat estrutural: ScaNN é library ANN in-memory, theodb é índice PostgreSQL persistente transacional.

**Definition of done:**
- [ ] Re-run M33 (ScaNN OSS proxy do AlloyDB; caveat library-vs-database documentado) a recall≥0.99, ≥3 runs → `docs/benchmarks/m73-headtohead-verdict.{md,json}`.
- [ ] **ADR de veredito do North Star vetorial** (superior / paridade+trade-off / honest-negative) + a decisão de posicionamento (claim permitido, per `public-copy.md`).
- [ ] Atualizar `goto-p0-vector-superiority` (memória) + o CLAUDE.md North Star com o estado MEDIDO final.

**Dependencies:** M60, M71, M72. **Risco (ALTO):** o gap ScaNN (~25×) é anisotrópico+AH; M57/M59 já foram honest-negative — o veredito honesto pode ser "paridade own-code, não superioridade de QPS pura".

## M74 — [ ] (CONDICIONAL) Quantização SOTA no índice — só com lever viável não-refutado

**Objective:** SÓ arranca se M73 (ou os discover de M71/M72) apontar um caminho de quantização **não** já refutado
por M57 (SBQ) / M59 (anisotrópica+AH no carrier HNSW) — ex.: formulação anisotrópica diferente, AH SIMD num
carrier IVFFlat (não HNSW), ou RaBitQ/rerank a outra régua. Measurement-first + gate de trigger: **não
implementar sem blueprint com evidência de viabilidade** (anti-sunk-cost, D3). Pode terminar em "nenhum lever
viável — o veredito M73 é final".

**Definition of done:**
- [ ] Discover-gate: blueprint com evidência (paper + medição de viabilidade) de um lever não-refutado → decisão implementar/não.
- [ ] SE implementar: recall≥0.99 + ganho de QPS MEDIDO vs o baseline M73, sem regressão → `docs/benchmarks/m74-quant-sota.{md,json}`.
- [ ] SE não: ADR honesto "nenhum lever viável pós-M57/M59; o veredito M73 é o estado final do pilar".

**Dependencies:** M73. **Risco (ALTO):** dois levers já refutados; condicional por design.

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
