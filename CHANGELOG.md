# Changelog

Todas as mudanças notáveis deste projeto são documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/),
e este projeto adere ao [Semantic Versioning](https://semver.org/).

> Nota: o projeto está em fase inicial de design (pré-código, sem release). O tracker
> de issues/PRs ainda não está configurado, por isso as entradas abaixo ainda não
> referenciam números de ticket. A partir da configuração do tracker, toda entrada
> passará a citar o issue/PR correspondente.

## [Unreleased]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.41.0] - 2026-07-06

### Added
- Régua SOTA vetorial M50: benchmark 3-way `theodb_hnsw` vs pgvector hnsw vs pgvectorscale `diskann` (cosine, recall@10 vs GT exato) + **primeiro QPS multi-cliente de banco** (8/16 conexões) em `benchmarks/run_m50_sota.py` → artefato `docs/benchmarks/m50-sota-ruler.{md,json}` com veredito escrito = gate formal do M51 (M50).
- JSON bruto reconstruído para os artefatos M41/M43 (`docs/benchmarks/m41-hnsw-qps.json`, `m43-hnsw-build.json`) — higiene de artefatos G8 (M50).

### Changed
- Reconciliação M30 (CHANGELOG↔artefato, G8/M50): os speedups columnar autoritativos são os do artefato reproduzível `docs/benchmarks/m30-columnar-scale.{md,json}` — **2.99× (100k) → 8.89× (1M) → 13.87× (5M)**. A prosa da entrada `[0.36.0]` (2.33×/8.65×/14.94×) veio de um run anterior em box contendida e fica superseded pelo artefato (a entrada released não é editada, Rule 6) (M50).
- Banner de cross-referência à ADR 0012 (data-degeneracy) adicionado a `docs/benchmarks/m32-scale-sift1m.md` — distinção de dados verificada (SIFT real, não a degeneração InitPlan-hoist) (M50).

## [0.40.0] - 2026-07-06

### Added

- Opclasses `cosine` (`<=>`) e `inner-product` (`<#>`) para `theodb_hnsw` e `theodb_ivfflat` (M49): `CREATE INDEX … USING theodb_hnsw (embedding theodb_hnsw_cosine_ops)` (e `_ip_ops`, e as variantes ivfflat) registram e fazem pushdown do operador (provado por `EXPLAIN Index Scan`). A métrica é resolvida do opclass no build via `index_getprocinfo` (support FUNCTION 1 retorna a tag — ADR-1) e persistida na meta — um índice `cosine`/`ip` constrói e ordena pela métrica certa (não mais o L2 fixo); L2 permanece o opclass DEFAULT. O scan pontua cosine/IP com kernel fused **zero-alocação por nó** (`ip_dist_from_bytes`/`cosine_dist_from_bytes` lendo os bytes da página inline — mesmo contrato do L2; fecha a "mina" de `Vec<f32>` por nó visitado). Paridade provada em `docs/benchmarks/m49-cosine-ip-opclasses.{md,json}`: recall@10 vs o oracle seqscan EXATO da mesma métrica = **1.0 (HNSW cosine/ip)**, 0.89/0.83 (IVF cosine/ip — parte da diferença vs HNSW é aproximação de list-probing, parte é o k-means IVF não-spherical, follow-up rastreado); crash-safe provado por teste committado (`test_cosine_crash_safe`: índice cosine idêntico pré/pós SIGKILL — formato de página raw idêntico ao L2). Caveat: IP não é métrica (HNSW-over-IP funciona empiricamente, precedente pgvector) (#M49)

## [0.39.0] - 2026-07-06

### Added

- Micro-benchmark same-graph do custo de alocação do scan HNSW (M47/FU-1) em `docs/benchmarks/fu1-samegraph-scan-microbench.{md,json}`: mede via criterion, sobre o MESMO grafo seeded (50k×128), o scratch pré-alocado (M46) vs `::new()`, no sweep de ef {100,200,400}, em 3 runs pinados em cores dedicados. Veredito honesto **HONEST_NEGATIVE_WITHIN_NOISE**: o presized é direcionalmente mais rápido na média (~2-7%), mas o ruído do box compartilhado (±13-25% run-to-run) excede o efeito e a direção flipa em ef=100/400 — só ef=200 é consistente (3/3). Caveat EC-2 explícito: é um limite superior (sem I/O de página, que amortiza a alocação em produção); nenhuma afirmação de superioridade de QPS de produção deste número isolado. Fecha também o gate honesto do M48 (`cargo bench --no-run` agora validado)

## [0.38.0] - 2026-07-06

### Added

- Benchmark de manutenção do índice vetorial em `docs/benchmarks/m48-am-maintenance.{md,json}` (driver `benchmarks/run_m48_maintenance.py`): caracteriza (a) a degradação de scan por região pending acumulada e a recuperação pelo fold (p50 cai ~7× quando o fold dispara acima do threshold de 16 páginas — pending 64→0), (b) o volume de WAL do fold shadow-write (~12 MB para reescrever um índice de 50k — insumo do M55), e (c) o custo honesto do planner (seqscan em N=100, índice em N=50k). Caracterização em dev box com mean±std de 3 runs + load-guard (lição M46), sem claim comparativo (#47)

- `CREATE INDEX ... USING theodb_hnsw` (build paralelo M44) agora responde a `pg_cancel_backend`: o build checa cancelamento uma vez por batch (4096 nós), então um build longo interrompe em ~1 batch em vez de rodar até o fim, sem deixar índice órfão. A cancelabilidade entra por um seam de injeção de dependência (a camada `ann/` permanece pura, sem `pg_sys`). O VACUUM/fold também é cancelável: o rebuild checa cancel por batch e o fold chama `vacuum_delay_point()` por página escrita (throttle de custo do VACUUM + ponto de interrupção), então um VACUUM de índice grande responde a cancel no meio do fold. **Caveat honesto:** cancelar (ou crashar) um fold no meio da escrita da nova geração pode deixar o índice exigindo `REINDEX` — é a MESMA janela fail-loud do crash (erro tipado instruindo REINDEX, NUNCA corrupção silenciosa; um re-VACUUM não cura, REINDEX sim). O fechamento total dessa janela (cancelar/crashar sem REINDEX) é escopo do M55 (fold incremental, ADR 0014) (#47)

- `SET theodb.vacuum_pending_threshold = N` (default 16): um VACUUM funde a região pending do índice vetorial na estrutura principal quando ela passa de N páginas — mesmo sem tuplas mortas — para que um workload insert-only tenha o scan em O(estrutura), não O(pending) para sempre. Observável pela métrica de runtime `pending_pages` (`THEODB_SCAN_PROFILE=1`), que cai a 0 após o fold. Nota: um VACUUM insert-only puro pula o index cleanup no PG14+; use `VACUUM (INDEX_CLEANUP ON)` (ou deixe o autovacuum rodar quando houver tuplas mortas) para disparar o fold (#47)

- Roadmap amendado: adicionados M47–M55 (remediação do deep-view 2026-07-05 — FU-1 régua same-graph, correctness do AM #46/#47, opclasses cosine/IP, calibração SOTA com pgvectorscale diskann + dataset realista, SBQ inline no AM gated, filtered ANN, híbrida com WHERE+BM25+BEIR, vectorizer auto-embedding, decisão VACUUM-wall) (`/roadmap-feature deepview-remediation`; issues #46, #47)


### Changed

- Planner recebe custo honesto do índice vetorial: `amcostestimate` passou a usar `genericcostestimate` e escalar o custo de startup pela fração de tuplas que um scan ordenado realmente visita (modelo de custo do pgvector — IVF `probes/lists`; HNSW fórmula por `m`/`ef_search`). Antes retornava custo 0, então o planner escolhia o índice até em tabelas minúsculas onde `seqscan + sort` é mais barato. Agora `ORDER BY <-> LIMIT` usa `seqscan` em N pequeno (~100 linhas) e o índice em N realista (~50k), ambos provados por `EXPLAIN`. Leitura da meta é fail-safe: qualquer meta ilegível (durante um VACUUM concorrente) cai para custo genérico, nunca aborta o planejamento (#47)

- VACUUM do índice vetorial bounda o crescimento do índice: o fold reusa a região contígua liberada por um fold anterior (alternância de gerações), em vez de estender sempre. Limite honesto (ADR 0014): o passo de reclaim NÃO é totalmente atômico — um crash no meio do reclaim é **fail-loud** (erro tipado → REINDEX), nunca corrupção silenciosa; o reclaim atômico sem janela de REINDEX é escopo do M55 (manutenção incremental crash-safe). A correção de corrupção do #47 em si (meta-pivot) é completa e crash-safe (#47)

- VACUUM do índice vetorial (`theodb_hnsw`/`theodb_ivfflat`) agora é **crash-safe**: o fold escreve a nova geração em páginas frescas e troca a página-meta (bloco 0) por último, num único registro WAL full-image — antes reescrevia no lugar, meta primeiro, e um crash no meio do VACUUM podia corromper o índice (pior caso: resultado silenciosamente errado). Formato structured do IVF migrado para v3 (campo `gen_base`, geração relocável); índices v2 continuam legíveis e são migrados no primeiro VACUUM. Índices pré-M48 podem exigir REINDEX se o erro tipado instruir (#47)

- Cycle-kit: `check_plan_completeness.py` (discover-plan-confidence) alinhado ao perfil frontier já sancionado por ADR `0001-discover-phd-rigor` (MIN_QUESTIONS=6, MAX_QUESTIONS=14, MAX_PER_CORNER=5 — o script ainda aplicava 5/10/3; dentro do cap LOCKED ≤15 do golden rule)
- Roadmap M47–M55: emendas do review de engenharia de BD absorvidas nos DoDs — M50 dataset dimensionado pela memória da box (build materializa corpus em RAM) + primeiro artefato de QPS multi-cliente; M51 write-path SBQ resolvido (codebook em meta pages, pending permanece f32, drift documentado) + co-localização de códigos rebaixada a decisão medida (custo ~2–3× de index size); M48 meta-pivot declarado layout-agnóstico (anti-retrabalho M51) + nota de testes EXPLAIN em N realista; M53 frame de segurança do filter_sql corrigido (privilégio do chamador + confinamento sintático); M55 criado (decisão fold incremental vs in-place — pré-requisito de claim v1.0)
- Ground-loop do scan do índice vetorial `theodb_hnsw` extraído para uma camada pura (`ann/scan_core`) atrás de um seam `NeighborSource`, mantendo o comportamento de busca idêntico (recall-neutro) — refatoração interna que habilita medição isolada e reproduzível do custo de alocação por consulta (FU-1/M47).


### Fixed

- Corrupção silenciosa do índice vetorial em crash durante VACUUM está eliminada (issue #47, gate fechado): provado por crash-injection determinística (GUC `theodb.test_crash_after_pages`/`test_crash_phase`, superuser-only, default off) em 3 pontos do fold (pré-pivot, pós-pivot, meio-do-reclaim) — em TODOS, após crash-recovery o scan é consistente OU falha-alto com erro tipado instruindo REINDEX, NUNCA um resultado silenciosamente errado. REINDEX (rebuild do heap) cura a janela residual; o fechamento total sem REINDEX é M55 (ADR 0014) (#47)

- Índices `theodb_hnsw`/`theodb_ivfflat` sobre tabelas UNLOGGED agora sobrevivem a crash/failover: o INIT fork passou a ser WAL-logado (`log_newpage_range` incondicional ao fim do `ambuildempty` — GenericXLog é no-op de WAL para relações unlogged); antes, o primeiro INSERT pós-recovery falhava com "truncated meta page" até REINDEX (#46)

## [0.37.0] - 2026-07-05

### Added
- M46: `benchmarks/run_m46_highrecall.py` — driver de re-medição Pareto endurecido do `theodb_hnsw` no alto
  recall (median + trimmed-mean sobre ≥5 runs, captura de `pages_read` determinístico via `THEODB_SCAN_PROFILE`,
  gate recall-neutro baseline-vs-pós). Reusa o harness M45 (Regra 9), pós-processamento puro unit-testável.


### Changed
- M46: `theodb_hnsw` scan hot-path hygiene (recall-neutro, alto recall) — `traverse` (`hnsw_page.rs`) agora
  **pre-dimensiona** as três estruturas per-query (`HashSet`/2×`BinaryHeap` com `with_capacity`, âncoras pgvector
  `tidhash_create(ef*m*2)` + pgvectorscale `with_capacity(search_list_size*neigbors)`) e **reusa um scratch
  `Vec<Addr>`** no ground loop em vez de alocar por nó (`decode_neighbors_into`, espelha o `unvisited` reusado do
  pgvector). Zero nova dependência (hasher default). Recall-neutro **provado** (index-scan == seqscan exato,
  byte-idêntico) — a mudança elimina complexidade acidental (rehash super-linear em ef + churn de allocator).
  Ganho de QPS não estabelecido nesta medição (box saturado, controle pgvector derivou +122%) — veredito honesto
  diferido para medição same-graph num box quieto (`docs/benchmarks/m46-highrecall-qps.md`).

## [0.36.0] - 2026-07-04

### Added
- M30 (decisão de escopo v1-legacy — **ADR 0013**): columnar (M6, `pg_mooncake`/`pg_duckdb`, MIT) e BM25 (M7,
  `pg_textsearch`) **MANTIDOS** como exceções permissivas (Regra 9), gated para adoção. A decisão de manter
  columnar é validada por **benchmark de escala** (`benchmarks/run_m30_columnar_scale.py` +
  `docs/benchmarks/m30-columnar-scale.{md,json}`): o columnstore DuckDB vence o row-store **2.33× (100k) →
  8.65× (1M) → 14.94× (5M)** numa agregação analítica, resultado byte-correto + plano `DuckDBScan` — fecha o
  gap que o M6 marcou UNBENCHMARKED. BM25: mantido pelo win medido (nDCG@10 0.95 vs 0.51 do `ts_rank_cd`, m7).
  O leg lexical **shipado** segue o FTS nativo (`ts_rank_cd`); columnar NÃO é embarcado ainda (adoção gated em
  build-PG17 ou bump-PG18 — milestone futura). Zero mudança de código de produto; substrato de medição é a
  imagem canônica `mooncakelabs/pg_mooncake` (PG18).


### Changed
- **API pública renomeada:** `theodb.import_pinecone` → **`theodb.import_vectors`** (e
  `theodb.import_pinecone_chunked` → **`theodb.import_vectors_chunked`**) — não faz sentido carregar o nome de
  um concorrente na nossa API. A função importa registros de vetor `{id, values, metadata}`; o formato de
  export **Pinecone-compatível** segue documentado no guia de migração (`docs/migrate-from-pinecone.md`).
  Rename em Rust (`theodb_rs/src/api.rs` — o wrapper `extension_sql!` + o entrypoint `_import_vectors` — e
  `migrate.rs`), na PROCEDURE plpgsql (`sql/80`), nos testes e no guia. **Provado 100% funcional:** rebuild +
  **23 testes green** (`test_unified` + `test_import_chunked` + `test_extension_install`) num container
  greenfield — `theodb.import_vectors`/`_chunked` presentes, `theodb.import_pinecone` ausente. Pré-1.0, sem
  alias de compat (install greenfield). Entradas de CHANGELOG já released + o upgrade `1.2→1.3` (retirada do
  legado plpython3u pelo nome de época) permanecem intocados (Regra 6 / histórico).
- Interno (sem impacto no consumidor): os stubs de teste da superfície de IA (`embedding_server.py`,
  `chat_server.py`) movidos de `tools/` para `benchmarks/servers/` — coesão, já que só `benchmarks/` os
  consome. Só um comentário de `sql/30-theodb-embed.sql` (path do endpoint local de exemplo) foi atualizado;
  nenhuma mudança de comportamento do banco.


### Removed
- Imagem: **`postgresql-plpython3` deixou de ser instalado** — era peso morto desde M19 (toda a superfície
  `theodb` — embed/ai.*/nl_to_sql — é servida pela extensão Rust `theodb_rs`; `theodb.control` requer só
  `vector, vectorscale`). **Provado 100% funcional:** rebuild + `scripts/smoke.sh` → `SMOKE PASSED`,
  `CREATE EXTENSION theodb` chega à v1.3 e todas as superfícies (hybrid RRF, 5 `ai.*`, agg_summarize,
  generate_batch, NL→SQL) presentes, com **zero pacotes plpython3 na imagem**. Torna verdadeira, no nível
  da imagem, a afirmação do README "sem plpython3u desde M19".
- Removido `packaging/Dockerfile.columnar-pg17probe` (não estava em nenhum caminho de CI/build) — o registro
  honesto do build PG17-from-source que falhou no pin rustc/MSRV permanece em prosa
  (`docs/benchmarks/m6-columnar-vs-row.md`).
- Limpeza de `docs/`: removido `docs/history/ROADMAP-v1.md` (roadmap v1 arquivado — planejamento/histórico, não
  documentação técnica do banco); a referência no `ROADMAP.md` ativo foi ajustada. `docs/` agora contém apenas
  documentação relacionada ao banco (adr, benchmarks, features, handbook, migração, packaging, analytics, sql-*).


### Fixed
- `README.md` + `CLAUDE.md`: corrigida staleness de gênese após 45 milestones/v0.35.0. **Factual:** "Draft
  v0.1 / ainda não há release" → "pré-1.0 (releases 0.x)"; "CHANGELOG a ser criado" → link ao CHANGELOG ativo;
  "harness de benchmark não existe / tudo UNBENCHMARKED" → existe (`benchmarks/theodb_bench/`, estado medido
  M33/M45). **Public-copy §3:** removido o claim sem evidência "índice ANN de alta performance" (o medido é gap
  vs ScaNN e paridade vs pgvector). **Escopo:** removidas as promessas de plataforma (MCP server, observabilidade
  OTel, admin-UI, HA/Patroni) — control-plane não faz parte deste repositório. Adicionado o estado M45 (paridade
  vs pgvector) ao bloco honesto de performance do README.
- Docs: corrigidos claims **falsos** de que o `plpython3u` ainda é usado/requerido (`docs/quickstart.md`,
  `docs/sql-ai-functions.md`, `docs/sql-embeddings.md`) + comentários stale no `Dockerfile` e docstring do
  `benchmarks/servers/embedding_server.py` — a superfície de IA é Rust (`theodb_rs`) desde M19.
- `docs/packaging/packaging-and-tuning.md`: removida a seção HA/Patroni + a referência ao runbook
  (`docs/operations/ha-backup-runbook.md`, deletado) e à dependência plpython3u — HA/deploy estão fora do
  escopo do repositório e a superfície de IA é Rust.

## [0.35.0] - 2026-07-03

### Removed
- **Control-plane / plataforma removidos deste repositório — o foco é o banco de dados (engine + extensão).**
  Apagados: o módulo Go `operator/` (operador K8s + CLI + gateway + MCP server + read-pool + observabilidade —
  todo o código Go do control-plane) e `ha/` (Patroni + pgBackRest + smokes de failover/PITR). Deploy,
  orquestração K8s, HA-via-operador e a superfície de plataforma **não fazem parte do TheoDB-engine** — são
  responsabilidade de outra camada. Limpeza acompanhante: job `ha-smoke` do CI removido; README sem as promessas
  de operador/HA/deploy-tooling; ROADMAP com M23/M24/M27/M28/M29 marcados REMOVIDO (fora de escopo); seção
  "Integrating with the Theo platform" do `CLAUDE.md` removida; docs órfãos `docs/benchmarks/m23-operator-reconcile.md`,
  `docs/benchmarks/m24-observability-readpool-mcp.md`, `docs/operations/ha-backup-runbook.md` e
  `docs/conselho-tecnico-theodb.md` apagados. **Nota:** ADR `0006` (estratégia "Rust/Go") é LOCKED — sua porção
  Go fica pendente de um ADR de superseção formal (sign-off do owner). Entradas de CHANGELOG já released
  (M23/M4) permanecem intocadas (Regra 6 — histórico).

## [0.34.0] - 2026-07-03

### Added
- M45 (medição RIGOROSA de superioridade vetorial — **mean±std recall×QPS Pareto** de `theodb_hnsw` vs
  `pgvector hnsw` em SIFT1M, **measurement-first honesto**): harness `benchmarks/run_m45_pareto.py`
  (+ lógica pura testada `m45_pareto.py`: interpolação de QPS a recall igual + veredito
  `SUPERIOR`/`PARITY`/`INFERIOR` com gate efeito>variância) constrói os DOIS índices-AM com build params
  casados (m=16, ef_construction=64, single-thread, `maintenance_work_mem` justo ao pgvector), faz sweep do
  MESMO grid `ef_search` nos dois (GUC de sessão, sem rebuild), roda ≥3 passes cronometrados por ponto →
  **mean±std** QPS + recall vs GT exato, e computa a margem a recall igual por interpolação de Pareto.
  **Veredito honesto: `PARITY`** — o *sinal* de superioridade do M42 (~1.7–2.8×, best-of-N/200-queries)
  **NÃO se reproduz** sob rigor: os dois frontiers se entrelaçam (theodb mais rápido a recall baixo-médio,
  pgvector a recall alto), dentro do ruído run-a-run (dois runs deram INFERIOR→PARITY). **O claim de
  superioridade do M42 é retratado**; o North Star P0 (superioridade vetorial vs pgvector) está em PARIDADE,
  não superioridade — próximo lever é latência+variância do scan theodb. Entrega a metade 1 de
  `public-copy.md` §4 (artefato reproduzível); metade 2 (reprodução independente) fica ABERTA. Fecha o DoD
  aberto do M32 (tabela mean±std ≥3 runs). Artefato: `docs/benchmarks/m45-pareto-sift1m.{md,json}`.
  Zero dependência nova; reusa `theodb_bench.{dataset,db,recall}`.

## [0.33.5] - 2026-07-03

### Changed
- M44 (build PARALELO do `theodb_hnsw`, **gated por benchmark A/B**): a construção do grafo in-memory
  (`ann/hnsw_parallel.rs`, novo) roda concorrente com `std::thread::scope` (borrow do corpus sem Arc; panic
  re-propaga no join → fail-loud) + `RwLock` por-nó nas listas de vizinhos (deadlock-free: 1 lock por vez).
  `HnswIndex::build` despacha por tamanho: corpus < 4096 → sequencial (determinístico, testes AM intocados);
  ≥ → paralelo. **Honestidade (Regra 3):** o build paralelo é NÃO-determinístico (ordem de insert racy → grafo
  diferente a cada run; nenhum teste de determinismo quebra); recall PARIDADE é o gate. Sem nova dep (std). Motivado
  pelo M42/M43 (build era o gargalo do carrier; M43 cortou 2.2× via SIMD, paralelismo é o próximo teto). A/B: build
  **2.82× @50k** (33±6s→12±3s, 3 samples, bandas separadas) / **8.4min→4.3min @1M** (1.95×), recall paridade
  (Δ+0.0055). Review de concorrência: SOUND (race-free/deadlock-free/panic-safe por construção, zero unsafe); 1
  achado LOW (back-link lost-update dentro do envelope racy aceito, recall verde) documentado no código, não
  corrigido (YAGNI). Lineage do build: 24min→8.4min→4.3min.

## [0.33.4] - 2026-07-03

### Changed
- M43 (otimização de **build-time** do `theodb_hnsw`, **gated por benchmark A/B**): a construção do grafo in-memory
  (`ann/hnsw.rs`) passa a usar o **kernel SIMD AVX2+FMA** (novo `crate::vec::l2_distance_simd`, reusa o kernel M31b
  via reinterpret f32→bytes) em vez da distância L2 **escalar** — o build fazia bilhões de distâncias escalares em
  128-dim enquanto o scan já era SIMD. Motivado pelo M42 (build 24min@1M é o gargalo do carrier). **Alinha** a
  métrica do build à do scan (ambos SIMD → consistente). **Gate:** recall PARIDADE (não byte-idêntico — FMA arredonda
  diferente → grafo muda em poucas near-ties; recall medido idêntico @200k, paridade @1M). O `l2_distance` (paridade
  pgvector — operadores/scan-rerank/knn) fica intocado; só o build aproximado usa SIMD. Guard `#[cfg(target_endian=
  "little")]` no reinterpret f32→bytes (fallback escalar em big-endian — achado do review de segurança; x86_64/LE
  byte-idêntico). A/B rigoroso: build **2.20×** (200±23s→91±3s @200k, 3 samples) / **24min→8.4min @1M**, recall
  paridade. Blueprint: `.claude/knowledge-base/discoveries/blueprints/m43-hnsw-build-qps-blueprint.md`.

## [0.33.3] - 2026-07-03

### Changed
- M42 (veredito de carrier em **SIFT1M real** — 1º sinal de superioridade vetorial, sem código novo): head-to-head
  4-way em SIFT1M (1M×128, GT exato) na imagem M41. **theodb_hnsw vence o theodb_ivfflat ~10×** (0.96 recall @ 278
  QPS vs 0.98 @ 28.7) e é **~1.7–2.8× mais rápido que o pgvector hnsw** a recall igual — invertendo o veredito
  synthetic do M40 (o grafo vence em dados estruturados reais, como o caveat honesto previa). **Caveats:** build do
  theodb_hnsw lento (24min@1M, próximo alvo); QPS best-of-N single-machine (margem vs pgvector precisa mean±std +
  repro antes de claim público, `public-copy.md`). `docs/benchmarks/sift1m-carrier-verdict.md` + `m32-scale-sift1m.json`.

## [0.33.2] - 2026-07-03

### Changed
- M41 (otimização de QPS do scan `theodb_hnsw`, **gated por benchmark A/B — ganho honesto 1.2–1.5×**): o `traverse` on-demand
  (`theodb_rs/src/am/hnsw_page.rs`) passa a decodificar+pontuar cada nó **dentro do pin da página**
  (`page::with_page_item`, sem o `to_vec` alloc+memcpy por-nó) e cacheia `RelationGetNumberOfBlocksInFork` uma
  vez por query (era ×2/nó). Motivado pela medição M40 (theodb_hnsw 3–5× mais lento que theodb_ivfflat a recall
  igual, porque o ivfflat amortiza o pin/lock sobre uma página inteira com SIMD e o hnsw pagava o custo fixo
  por-nó). **Recall byte-idêntico** (mesma ordem de traversal + top-k) — provado pelos testes do AM; o único eixo
  medido é QPS. Blueprint: `.claude/knowledge-base/discoveries/blueprints/m41-hnsw-qps-blueprint.md`.
- M40 (carrier head-to-head — **re-escopado da anisotropic loss, sem claim**): a sonda de teto
  (`docs/benchmarks/m40-ceiling-probe.md`) provou que o recall é limitado pelo carrier (probes), não pelo
  quantizer (rerank f32 equaliza) — a loss anisotrópica não moveria recall. Re-escopado para
  `benchmarks/run_m40_carrier.py` (theodb_hnsw vs theodb_ivfflat recall×QPS a QPS igual). Medição n=50k:
  theodb_ivfflat vence (theodb_hnsw 3–5× mais lento a recall igual → headroom de otimização). **Caveat:**
  synthetic random-gaussian é o pior caso p/ grafo — veredito não generaliza; precisa SIFT1M. 5º negativo
  measurement-first da sessão. `docs/benchmarks/m40-carrier.{md,json}`.
- M39 (Product Quantization, **medido: NÃO é o lever de QPS — sem claim de performance**): novo `theodb.pq_knn`
  próprio, std-only (k-means Lloyd por subespaço + ADC LUT, `theodb_rs/src/pq.rs`), funcional e testado
  (`REVOKE FROM PUBLIC`, espelha `sbq_knn`). Benchmark reproduzível PQ-vs-SBQ (`benchmarks/run_m39_pq.py`,
  `docs/benchmarks/m39-pq.{md,json}`): gate D3 (anti-sunk-cost) = **SBQ_RETAINED** — a paridade recall (ambos
  ~0.77, nenhum vence f32=1.0), PQ é ~5× mais lento que o SBQ (ADC + k-means train por-chamada vs Hamming).
  Ganho de memória (8 vs 32 bytes/vetor) é real mas fora do alvo (P0 = latência). **Honestidade (Regra 3):** o
  gate parou PQ antes da cara integração no index-AM; 3º negativo measurement-first da sequência (M36/M38/M39).
  Próximo lever (o gap real = recall vs f32): anisotropic loss do ScaNN.
- Correção de doc-drift em `docs/features/` (auditoria de honestidade do core): 5 páginas tinham a linha
  `> Status:` stale dizendo "📋 planejado" enquanto a capacidade **já estava entregue e testada** (o callout no topo
  de cada uma já contava a verdade; a linha oficial não). Corrigidas com `file:line` + testes (validado por
  `deep-research/validate_citations.py`, 0 fabricado): **03 IVFFlat** (`theodb_ivfflat` AM, `am/mod.rs`) e **04 IVF**
  (= IVFFlat) → ✅ entregue; **06 híbrida** (`ai.hybrid_search_rrf`/`ai.hybrid_search(jsonb)`, `hybrid.rs`) → ✅ entregue;
  **07 funções IA SQL** (`ai.*` + registry `theodb_ml`) → ✅ entregue (modos array/cursor seguem YAGNI-adiados);
  **05 ScaNN** → ⚖️ NO-FORK (M14): `theodb_scann` literal é decisão-de-não-fazer, ScaNN-quality entregue via `USING diskann`.
  Placeholder `'https://...'` do exemplo de `theodb_ml.create_model` trocado por `'<your-llm-endpoint>'` (clareza).

## [0.33.1] - 2026-07-03

### Changed
- Correção de honestidade em `docs/features/` (M37): 2 páginas estavam marcadas "📋 planejado" quando a feature
  **já está entregue e testada** — a auditoria anterior de features foi incompleta (grepou só o Rust
  `theodb_rs/src/`, perdeu a implementação em `sql/50-theodb-ai.sql`). Corrigidas para "✅ Entregue" com `file:line`
  + testes (validado por `deep-research/validate_citations.py`): **11 sumarização** (`ai.summarize` plpgsql +
  `ai.agg_summarize` agregado, `sql/50-theodb-ai.sql:32,82`, chamando o `ai._chat` Rust; 6 testes em
  `test_ai_sql.py`) e **08 aceleração** (`ai.generate_batch` N-in/N-out + `ai.if`, M11/M18; 9 testes). **Honestidade
  (Regra 3):** o M37 foi criado sob a premissa "sumarização não implementada" (grep Rust-only) — estava errado; o
  grounding measurement-first evitou adicionar um `ai.summarize` DUPLICADO (conflito). M37 é uma correção de
  doc-drift, não código novo. Evidência funcional ao vivo (33 testes de contrato + 3 real-OpenAI
  gpt-4o-mini, incl. `agg_summarize`) em `docs/benchmarks/m37-ai-summarize-validation.md`.

- M38 (refactor de code-quality, **sem claim de performance**): `read_chunked`/`read_blob` do index-AM agora fazem
  UMA cópia por página (`read_page_item_into`, append direto) em vez de duas (`read_page_item.to_vec()` +
  `extend_from_slice`). Menos alocação/tráfego de memória; **recall byte-idêntico** (61 testes de coexistência).
  **Honestidade (measurement-first):** o M38 investigou cortar o gargalo `reads` do scan; a medição concluiu que
  (a) o SBQ regride recall (0.77–0.95 < 1.0 em SIFT real) e (b) a cópia **não é** o gargalo end-to-end (o profiler
  enganou via overhead de instrumentação) — então NÃO há win de QPS a reivindicar. O lever vetorial real restante
  é PQ (algorítmico, milestone futuro). Evidência: `docs/benchmarks/m38-copy-free-scan.{md,json}`.

## [0.33.0] - 2026-07-02

### Added
- M36 Phase 1 — **otimização do scan do índice: heap top-K lazy** (`am/scan.rs`). O gate measurement-first do M36
  (`THEODB_SCAN_PROFILE`) FALSIFICOU a premissa original ("quantizar a distância"): a distância é ~15% do custo de
  scan; os gargalos são `reads` (~44–51%) e `sort` (~35–41%). Phase 1 substitui o `results.sort_by` O(C·log C) de
  TODOS os candidatos por um heap min lazy (heapify O(C) no `amrescan` + pop O(log C) no `amgettuple` = O(C+k·log C)).
  Top-K byte-idêntico → **recall inalterado** (por construção — mesma ordem total `(total_cmp, tid)`; provado pelo
  pg_test de ordering + a suíte de 61 testes de coexistência passando inalterada). Fase sort caiu ~10–13×
  (profiler, estável/algorítmico); speedup end-to-end **~1.5× band** (mean, recall idêntico), Amdahl-limitado pelo
  `reads` restante — o que motiva o Phase 2. Evidência: `docs/benchmarks/m36-scan-optimization.{md,json}`. Phase 2
  (códigos SBQ menores p/ cortar o I/O `reads`, ~44%) é o próximo slice do M36.
- Roadmap emendado com 2 milestones novos ao fim (convenção `/roadmap-feature` — nunca renumerar, nunca roadmap
  concorrente): **M36 — Quantização-no-índice** (distância assimétrica sobre códigos quantizados no scan + rerank
  f32; fecha o gap de ~24.6× em QPS vs ScaNN que o M33 mediu — o P0 do North Star; SBQ-primeiro, escalar p/ PQ/ADC
  se recall < 0.99; benchmark `m36-quantization-in-index.json` obrigatório) e **M37 — Sumarização de conteúdo**
  (`ai.summarize`, a última feature documentada genuinamente ausente; espelha `ai.rank`/`ai.analyze_sentiment`).
  M27–M30 (abertos) intactos. Achados desta sessão (auditoria de `docs/features/` + análise do `council-vector-ann`).
- Handbook "Formação de Engenharia — TheoDB" (`docs/handbook/`): currículo técnico interno que ensina engenharia
  de banco de dados através do código real do TheoDB. Modo curado (Regra 9) para fundamentos já cobertos por
  Strang/CLRS/PG-docs; modo original ancorado em `file:line` + ADR + benchmark para o coração (índices, vetorial,
  SIMD, IA-no-banco). Primeiro capítulo-farol escrito: cap. 19 — HNSW (`parte-06-vetorial/19-hnsw.md`), template
  de qualidade com as 5 camadas (teoria → matemática → nossa implementação → nosso benchmark M35 → gap honesto vs
  ScaNN). Contrato de honestidade: toda citação resolve no disco.
- Conselho Técnico do TheoDB (`docs/conselho-tecnico-theodb.md` + `.claude/agents/council-*.md`): 8 sub-agents
  especialistas invocáveis (via Task tool) para entender, medir e evoluir o sistema — Vetorial/ANN, Index-AM &
  Storage, SIMD/Performance, Benchmark, Rust/pgrx, IA-no-banco, Research/ADRs, Segurança. Personas são arquétipos
  fictícios (inspirados-em, não impersonação); cada agente aponta para o código/ADR/benchmark reais que governa e
  é obrigado a lê-los antes de aconselhar (mesmo contrato de honestidade do handbook). Domínios roadmap
  (distribuído, cloud-native, Go, PG-kernel) adiados honestamente até terem código.
- Skill `/deep-research` (`.claude/skills/deep-research/`): a máquina que produz capítulos do handbook — pesquisa
  profunda (nosso sistema via `file:line` + papers/benchmarks/técnicas do SOTA no allowlist + cálculos de
  complexidade) destilada nas 5 camadas (teoria → matemática → nossa implementação → nosso benchmark → SOTA & gap
  honesto), curar-não-reproduzir. Inclui `templates/chapter-template.md` e `scripts/validate_citations.py` que
  mecaniza o contrato de honestidade (fail-closed: citação `file:line` que não resolve → INVALID; URL fora do
  allowlist → INVALID; número de performance sem benchmark nem `UNBENCHMARKED` → NEEDS_REVISION). Dogfood: o
  capítulo-farol 19 passa o validador (PASS).


### Changed
- Correção de honestidade em `docs/features/`: 5 páginas estavam marcadas "📋 planejado / ainda não implementadas"
  quando na verdade **já foram entregues** (drift de documentação — os docs ficaram atrás do código). Atualizadas
  para "✅ Entregue" com função SQL real + `file:line` + teste que prova cada uma: 01 busca vetorial (M20,
  `theodb.l2_distance/…`, `test_vector_ops.py`), 02 índice HNSW (M21+M35, AM `theodb_hnsw`,
  `test_hnsw_structured.py`), 09 ranquear (M7-S3, `ai.rank`, `test_ai_sql.py`), 10 sentimento (M7-S3,
  `ai.analyze_sentiment`, `test_ai_sql.py`), 12 linguagem natural (M19, `ai.nl_to_sql`, `test_nl_sql.py`). Cada
  bloco de status validado por `deep-research/validate_citations.py` (PASS — toda citação resolve no disco).
  Afirmações de qualidade de IA marcadas com a nota de honestidade (dependem do LLM; sem benchmark de acurácia).

## [0.32.0] - 2026-07-02

### Added
- M35 — `theodb_hnsw` now persists the graph in a **page-native structured layout** (meta + per-node element
  tuples + per-node neighbor tuples, à la pgvector) and the scan **traverses it on demand**, reading only the
  visited nodes' pages (O(ef·M)) instead of deserializing the whole graph per query (the M26 O(N) blob — ~6.5 GB
  at 1M). Adds a `theodb_hnsw.ef_search` scan GUC (`SET theodb_hnsw.ef_search = N`, default 64) mirroring
  pgvector's knob. INSERT (pending fold) / DELETE+VACUUM (structured rebuild) intact. At 1M×128 (SIFT1M), at the
  matched-recall point (ef_search=100, recall 0.979 ≥ the prior blob's 0.964) the structured scan reaches ~100 QPS
  — **~61× the O(N) blob** at preserved recall (up to ~194× at a lower recall of 0.93); pages-read stays flat in N
  (O(ef·M)). Trade-off: the structured build is ~17.5 min at 1M (single-thread graph construction). Evidence:
  `docs/benchmarks/m35-hnsw-structured-scan.{md,json}`.


### Changed
- **BREAKING (pre-1.0 engine): `theodb_hnsw` on-disk format changed** from the M26 single-blob to the M35
  page-native structured layout. Newly-built indexes use the structured format automatically; an index built by a
  pre-M35 binary still reads via the legacy O(N) blob path — **REINDEX `theodb_hnsw` indexes to get the
  structured on-demand speedup**. No data loss either way. `theodb_ivfflat` is unaffected.

## [0.31.0] - 2026-07-02

### Added
- M33 — head-to-head vetorial reproduzível vs **ScaNN OSS** (o algoritmo do índice do AlloyDB; Apache-2.0) no
  SIFT1M (1M×128), com veredito honesto por dimensão. AlloyDB é GCP-managed (sem execução local), então o
  fallback sancionado pelo DoD é o ScaNN OSS. Resultado medido no ponto recall≥0,99: **paridade de recall@10**
  (ambos ≥0,99), mas **GAP de throughput/latência** — ScaNN ~25× QPS e ~26× menor p50 que `theodb_ivfflat`
  (quantização anisotrópica + AH SIMD vs IVFFlat full-precision). A superioridade vetorial em velocidade ANN pura
  ainda **não está cumprida** (refutada honestamente pela evidência — o diferencial atual é vetorial dentro de um
  banco transacional, não uma biblioteca in-memory). Novo driver `benchmarks/run_m33_scann.py` + teste CI de
  fairness da semântica de recall; números theodb/pgvector reusados do artefato M34 (mesmo SIFT1M/hardware/GT).
  Evidência: `docs/benchmarks/m33-scann-headtohead.{md,json}`.


### Changed
- README "Missão": nota de estado medido do pilar vetorial (M33) linkando o benchmark — a claim de superioridade
  vetorial fica qualificada pelo resultado honesto (paridade de recall, GAP de QPS) por `public-copy.md`.

## [0.30.0] - 2026-07-02

### Added
- M34 — `theodb_ivfflat` now accepts a configurable `lists` build reloption (`CREATE INDEX … WITH (lists=N)`) and a
  `theodb_ivfflat.probes` scan GUC (`SET theodb_ivfflat.probes = N`), mirroring pgvector's ivfflat knobs (pgrx
  `amoptions` + `GucRegistry`). Defaults preserve prior behavior (lists=100, probes=10); out-of-range values are
  rejected at DDL/SET. Closes the ~8× SCAN-latency gap M32 measured at 1M: at `lists=1000` + tuned probes,
  `theodb_ivfflat` p50 is ≤ pgvector at the recall-matched high-recall points (probes 50/100, recall 0.99+) and at
  parity for lower probes. A VACUUM fold preserves the built list count. **Trade-off:** the `lists=1000` **build** is
  single-thread full-corpus k-means — ~575 s vs pgvector's ~33 s (sampled/parallel); build-time parity is a future
  lever. Evidence: `docs/benchmarks/m34-ivfflat-reloption.{md,json}`.
- Roadmap amended: added M34 — theodb_ivfflat QPS a escala (lists/probes configuráveis via reloption + GUC) and
  M35 — theodb_hnsw scan estruturado page-native (`/roadmap-feature theodb-ann-qps-scale`). The two QPS levers M32
  measured (~8× gap vs pgvector at 1M); split into two milestones after discovery sized the HNSW structured scan at
  ~3-4× the M31 effort (too large + risky to bundle without rework). M34 precedes M33 strategically.


### Changed
- **BREAKING (index format) — M34 bumps the `theodb_ivfflat` structured on-disk format to v2** (the per-list
  directory is now page-chunked so `lists` is no longer capped at ~665). `theodb_ivfflat` indexes built on
  v0.27.0–v0.29.0 (format v1) are rejected on read with a typed `REINDEX to upgrade` error — **REINDEX any
  `theodb_ivfflat` index after upgrading.** `theodb_hnsw` and the SQL-callable distance ops are unaffected.

## [0.29.0] - 2026-07-02

### Added
- M32 — scale benchmark harness (≥1M vectors, head-to-head vs pgvector). Extends `theodb_bench` with a
  neighbors-GT loader (`load_hdf5_full` — exact GT from the HDF5 `neighbors` ids, 10⁶ ops instead of the 10¹⁰
  brute force), theodb_ivfflat/theodb_hnsw index specs (`--index 4way`, fixed op-point — l2-only), a per-spec
  `query_cap` (for theodb_hnsw's O(N) scan), and an operator driver (`benchmarks/run_m32_sift1m.py`). First ≥1M
  evidence: `docs/benchmarks/m32-scale-sift1m.{md,json}` (SIFT1M, 1M×128). Honest per-knob verdict: theodb_ivfflat
  leads on **recall@10 (0.988)** and **index size (533 MB)** but trails pgvector on **QPS (~8×)** at 1M (fixed
  100-list under-partitioning — no `lists` knob yet); theodb_hnsw is impractical at scale (O(N)-per-query blob
  scan — the M31 structured partial-read is ivfflat-only). Quantifies the vector-superiority gap; no cherry-pick.

## [0.28.0] - 2026-07-02

### Added
- M31b — SIMD (AVX2+FMA) fused decode+distance for the `theodb_ivfflat` Index Scan hot loop: the L2 distance is
  computed DIRECTLY from each candidate's page bytes via `_mm256_loadu_ps` (unaligned load), fusing the byte-decode
  and the distance into one 8-wide SIMD pass with a cached runtime CPU dispatch (`is_x86_feature_detected!`) and a
  scalar fallback (portability). Numeric: SIMD lane-summation is recall-preserving, not bit-identical to the M20
  scalar (same property as pgvector's FMA path). The M20 SQL-callable distance ops stay scalar (byte-parity intact).
  Measured on DISTINCT data: theodb_ivfflat Index Scan p50 **≤ pgvector** — 2.6× faster on uniform-random (recall
  parity) and 0.95× at full recall (10/10) on clustered/embedding-like data (n=100k, dim=128, probes=10). M31b DoD
  met. Evidence: `docs/benchmarks/m31b-simd-distance.md`.
- M31b — opt-in scan profiler (`THEODB_SCAN_PROFILE=1`): logs per-scan phase timing (reads/score/sort) + list
  balance (`nonempty_lists`), the runtime observability that exposed the benchmark-data bug below.


### Fixed
- Benchmark data-generation degeneracy: the index-latency harness seeded vectors with a non-correlated
  `(SELECT string_agg((random())…) FROM generate_series(…))` sub-select, which PostgreSQL evaluates once as an
  InitPlan — so all 100k rows got the SAME vector (`COUNT(DISTINCT)=1`). This collapsed any correct k-means into a
  single list and made every pre-M31b latency number a brute-force-on-identical-ties measurement (retro-invalidating
  M31's `~2.7× behind pgvector`). Fixed by seeding DISTINCT vectors from Python via `COPY` (uniform + clustered
  regimes). No engine bug — theodb's k-means was always correct. See `docs/adr/0012-benchmark-data-degeneracy.md`.

## [0.27.0] - 2026-07-01

### Added
- Roadmap amended: added the **P0 vector-superiority track** (CTO GOTO 2026-07-01) — M31 (index-AM latency
  optimization via partial-page reads), M32 (1M+ scale benchmark + QPS head-to-head vs pgvector), M33 (measured
  head-to-head vs AlloyDB/ScaNN). Runs BEFORE the operational M27–M30; closes the North Star pillar (vector
  performance superiority proven by benchmark) that is measured-parity-only today.
- M31b milestone added (ADR 0011): SIMD vector distance (AVX2 + runtime dispatch) to close the residual
  constant-factor latency gap vs pgvector. Sequenced M31 → M31b → M32.


### Changed
- M31 — `theodb_ivfflat` structured partial-page reads: restructured the index into a meta page (centroids +
  per-list directory) + list pages so the Index Scan reads only the probed lists' pages (O(probes)), not the
  whole blob (O(N)). Measured ~**45× faster** than the M26 O(N)-per-scan path (~38 ms vs ~1700 ms at 100k×128),
  recall preserved, INSERT/DELETE/VACUUM maintenance intact. Honest: still ~2.7× behind pgvector's AVX-SIMD C —
  the O(N) algorithmic gap is closed; the constant-factor (SIMD) residual is M31b (ADR 0011). Evidence:
  `docs/benchmarks/m31-am-latency.{md,json}`. (M31)

## [0.26.0] - 2026-07-01

### Added
- M26 — vector Index Access Methods `theodb_ivfflat` + `theodb_hnsw`: persisted Postgres index AMs that promote
  the in-memory rebuild-per-query ANN into real index engines. `CREATE INDEX … USING theodb_{ivfflat,hnsw}
  (embedding …_l2_ops)` builds the index once from the heap and persists it to WAL-logged pages (GenericXLog);
  `ORDER BY embedding <-> $1 LIMIT k` is answered by a planner Index Scan (amcanorderbyop + amcostestimate) at
  recall parity with a brute-force scan; INSERT appends to a pending buffer (no rebuild), DELETE is filtered by
  MVCC recheck, and VACUUM folds pending + drops dead TIDs. Built on pgrx 0.16 FFI (IndexAmRoutine + page/buffer
  persistence). **~16× faster than the rebuild-per-query SQL function** (86 ms vs 1372 ms on 5 000×128); evidence
  in `docs/benchmarks/m26-index-am.md`. Proven by `benchmarks/tests/test_index_am.py` (6/6) with M20–M22
  coexistence intact (61 passed). l2 operator class ships now; cosine/ip + partial-page-read scan optimization are
  documented follow-ups (ADR `docs/adr/0010-m26-index-am-scope.md`). (M26)

## [0.25.0] - 2026-07-01

### Added
- Roadmap amended (v2): added M25–M30 covering all open professionalization points from the `theodb_rs` architecture audit + M24 deferrals + the operator front. M25 craft hardening (theodb_rs); M26 vector Index Access Method (the SOTA gap — function→index engine); M27 streaming replication + real read-pool; M28 MCP write tools + auth; M29 operator (Go) architecture verdict + hardening; M30 v1-legacy columnar/BM25 scope ADR. (`/roadmap-feature`)


### Changed
- M25 craft hardening of the `theodb_rs` engine extension (behavior-preserving, no new dependency): split the 721-LoC `lib.rs` god-file into a thin 92-LoC module root + a dedicated `api.rs` SQL-surface module; decomposed the NL-to-SQL validator (`nl_to_sql` cyclomatic complexity 19→8) and the hybrid-search orchestrator (`run_rrf` 12→9) into small pure functions; removed a duplicated distance kernel (single-source `Metric::dist`); named magic numbers and added fast unit tests for the SQL-injection guards (incl. specific-message + bare-entry-cross-schema security assertions). Every extracted function is CCN < 10; proven at parity by 72 green integration tests + before/after complexity evidence in `docs/benchmarks/m25-craft-hardening.md`. (M25)
- ADR 0009: `theodb_rs` api-surface adopted as a single cohesive `api.rs` facade module — amends the M25 plan's intra-plan ADR-2 (per-feature split), records why the single-`#[pg_schema] mod theodb_rs` facade is the right call (declarative DDL, cohesion, Esforço≠Complexidade) and formally waives the heuristic 500-LoC budget for that declarative facade. (M25)

## [0.24.0] - 2026-07-01

### Added
- **M24 — Observability + read-scale + MCP (Go).** Three own-Go capabilities on the M23 operator: (1) **domain Prometheus metrics** (`theodb_reconcile_total{result}`, `theodb_reconcile_duration_seconds`, `theodb_cluster_phase{phase}`, `theodb_cluster_ready_instances`, `theodb_cluster_desired_instances`) registered on controller-runtime's shared registry and scraped via the existing `/metrics` — **zero new dependency**, bounded cardinality, idempotent `Register()`; (2) a **read-routing Service** `<name>-ro` the operator provisions (ClusterIP load-balancing ready pods — read-scale endpoint; streaming-replication routing is M25); (3) a read-only **MCP server** (`cmd/theodb-mcp`, official Go SDK v1.6.1) exposing `list_clusters` + `get_cluster` to AI agents over stdio (default) / streamable HTTP. Evidence: real-`envtest` metric + read-Service gates, in-memory MCP handshake/tool-call tests, and an MCP tool-call benchmark (**~208 µs/op**) — `docs/benchmarks/m24-observability-readpool-mcp.md`. Security: `govulncheck ./...` reports **0 reachable vulnerabilities** (toolchain bumped to go1.25.11; `x/net`→v0.55.0; `otel/sdk`→v1.40.0). Coverage: metrics 90.9%, mcpserver 82.1%, controller 71.9%. Std `testing` only; MCP SDK permissive (MIT→Apache-2.0 — D1). Write tools + HA-failover are M25+ follow-ups. (#M24)

## [0.23.0] - 2026-06-30

### Added
- **M23 — Control plane in Go (Kubernetes operator + CLI).** New `operator/` Go module (kubebuilder/controller-runtime) that reconciles a `TheoDBCluster` CRD into a **StatefulSet** of N theo-db instances, a **governing headless Service** (ClusterIP=None, for stable per-pod DNS) + a **gateway ClusterIP Service**, all owner-referenced, with status (`Phase`/`ReadyInstances`/`ObservedGeneration`/`Ready` condition). Reconcile is idempotent (no resourceVersion churn on a converged re-run), scales by patching only the mutable `replicas`+`image` (never the immutable StatefulSet spec — changing `storageSize` does not attempt a rejected VCT update), and validates the spec **at the API boundary** (image required, `storageSize` quantity pattern, `port` 1..65535, `instances` ≥ 1) so a malformed CR is rejected before it is stored. Ships a `theodbctl` cobra CLI (`apply`/`get`/`delete`) and a reproducible `config/` kustomize deploy with least-privilege RBAC + a non-root distroless manager image. Milestone evidence is a **real-`envtest` reconcile gate** (in-process kube-apiserver+etcd, no kubelet) plus a real-kind deploy + CLI smoke — `docs/benchmarks/m23-operator-reconcile.md`. Std `testing` only (no ginkgo); deps are controller-runtime + k8s.io/* + cobra (all Apache-2.0/BSD — D1 license gate satisfied). HA-failover orchestration and an HTTP/pooler gateway are explicit M24+ follow-ups. (#M23)


### Fixed
- `/code-quality` audit no longer walks `knowledge-base/references/` (cloned third-party reference repos): the skip-list used `referencia` (PT) instead of the real directory name `references` (EN), so the symbol-fabrication detector parsed foreign files — wasting time/RAM and producing a spurious `FAIL_HARD` that blocked `/review`. Now the references zone is correctly skipped, with a behavioral regression test. (#37)

## [0.22.0] - 2026-06-30

### Added
- **(M22) Quantização escalar própria (SBQ) em Rust — recall@k + memória gated por benchmark (coexistência, measurement-first):** `theodb.sbq_knn(src_table regclass, embed_col text, queries vector[], bits=>1, over_fetch=>4, …)` e `theodb.sbq_bytes_per_vector(dim, bits)` implementados em Rust (`theodb_rs/src/sbq.rs`). Quantizador SBQ próprio (limiar por dimensão pela média, `bits_per_dim` empacotados em `u64`) — código permissivo aprendido do SBQ do pgvectorscale (PostgreSQL License); **RaBitQ do vectorchord NÃO foi copiado (AGPL — proibido na distribuição, D1)**. Busca = candidatos via o carrier IVFFlat do M21 → ranking por **Hamming** (`popcount` XOR) nos códigos → **rerank** full-precision dos top `k·over_fetch` com o kernel f32 do M20 (`crate::vec`). **Sem nova dependência** (`std` puro: bit ops + `u64::count_ones`). **Decisão = coexistência** (ADR D3): lê `embed_col::real[]`, não toca pgvectorscale/pgvector nem `embed`/`hybrid`/`import`/M21. **Paridade de recall@k + memória** vs pgvectorscale SBQ (`diskann` memory_optimized) provada por `benchmarks/tests/test_sbq_index.py::test_recall_memory_parity_gate` + benchmark reprodutível (`docs/benchmarks/m22-sbq-parity.md`). Memória honesta (EC-1): bytes/vector `ceil(dim·bits/64)·8` = **paridade com pgvectorscale** (mesma fórmula) + **~32× vs f32** (não "menos que pgvectorscale"). Validação fail-fast 22023 (bits/over_fetch/lists/probes fora de faixa, metric, dim, id_col, identifier); REVOKE de PUBLIC. Forma SQL-callable measurement-first (planner AM = M22b).

## [0.21.0] - 2026-06-30

### Added
- **(M21) Índice ANN próprio em Rust — HNSW + IVFFlat, recall@k gated por benchmark (coexistência, measurement-first):** `theodb.hnsw_knn` e `theodb.ivfflat_knn(src_table regclass, embed_col text, queries vector[], …) RETURNS TABLE(query_idx int, id bigint, distance float8)` implementados em Rust (`theodb_rs/src/ann/` — algoritmo puro, split `mod`/`hnsw`/`ivf`; `theodb_rs/src/ann_query.rs` — leitura via Spi + validação de borda). HNSW (grafo em camadas, `m`/`ef_construction`/`ef_search`, heurística de vizinhos do pgvector) e IVFFlat (k-means++ + `lists`/`probes`) constroem um índice em memória sobre uma coluna `vector` e respondem top-k `<->`/`<#>`/`<=>`, reusando o kernel de distância f32 do M20 (`crate::vec`, sem nova dependência — `pgrx` + `std` + SplitMix64 próprio). **Decisão de migração = coexistência** (ADR D1): leem `embed_col::real[]`, não tocam tipo/operadores/índices HNSW/IVFFlat do pgvector nem `embed`/`hybrid`/`import`. **Paridade de recall@k vs pgvector** provada por `benchmarks/tests/test_ann_index.py::test_recall_parity_gate` + benchmark reprodutível (`docs/benchmarks/m21-ann-index-parity.md`, mean±std ≥3 runs, tolerance band). Validação de borda fail-fast (SQLSTATE 22023): metric inválido, dim mismatch, `id_col` não-inteiro, parâmetros acima do teto (m≤100, ef≤1000, lists/probes≤32768); linhas com vetor NULL puladas; `queries` vazio → 0 linhas; REVOKE de PUBLIC. Forma SQL-callable measurement-first (ADR D1/D3); o access method `CREATE INDEX … USING` integrado ao planner fica para o M21b.

## [0.20.0] - 2026-06-30

### Added
- **(M20) Operadores de distância vetorial próprios em Rust — paridade numérica com pgvector (coexistência):** `theodb.l2_distance` / `theodb.inner_product` / `theodb.cosine_distance(vector, vector)` implementados em Rust (`theodb_rs/src/vec.rs`), computando L2 (`<->`), inner product (`<#>` = negativo) e cosseno (`<=>`) sobre os valores do pgvector com **acumulação em f32** (igual ao `vector.c` do pgvector — o determinante de paridade bit-a-bit), `sqrt`/divisão em f64, clamp do cosseno a [-1,1]. **Decisão de migração = coexistência** (ADR D1): as funções leem os valores exatos do pgvector via o cast lossless `vector::real[]` (sem tipo competidor, sem redefinir os operadores do pgvector) — dados/índices HNSW/IVFFlat/DiskANN + `embed`/`hybrid`/`import` intactos. Paridade provada por `benchmarks/tests/test_vector_ops.py` (comparação byte-a-byte com as funções nativas do pgvector nos oracle rows + boundaries dim=1/1536/16000/NaN/inf) + benchmark reprodutível (`docs/benchmarks/m20-vector-ops-parity.md`). Sem nova dependência (pgrx + std).

## [0.19.0] - 2026-06-30

### Changed
- **(M19) NL→SQL, busca híbrida e import Pinecone portados para Rust/pgrx — `theodb` 100% Rust, fim do `plpython3u`:**
  - `ai.nl_to_sql` (a ÚLTIMA função `plpython3u`) reescrita em Rust (`theodb_rs/src/nl.rs`) com a defesa anti-injection em camadas preservada — L1 prompt byte-idêntico, L2 validação estática (single-statement, SELECT/WITH-only, denylist de 33 keywords, sem `DO $$`/`CALL`) via scanning stdlib, L4 allowlist parser-grade via `EXPLAIN (FORMAT JSON)` do Postgres (sem crate de parser — preserva a defesa contra comma-join/identificador-citado). `ai.nl_query` permanece um keeper plpgsql fino (L3 sandbox read-only → 25006) que chama `ai.nl_to_sql` no nível SQL.
  - `ai.hybrid_search_rrf` + `ai.hybrid_search(jsonb)` reescritas em Rust (`theodb_rs/src/hybrid.rs`) — a função Rust é o entrypoint e orquestra UMA SQL de fusão RRF via SPI (única fonte de verdade da fusão; `%I` quoting nativo do Postgres preservado, sem reimplementar RANK/FULL OUTER JOIN). Co-residem com `theodb.embed` em `theodb_rs`.
  - `theodb.import_pinecone` (FUNCTION) reescrita em Rust (`theodb_rs/src/migrate.rs`) — loop + parse jsonb nativo (serde) + INSERT `%I`-quoted via SPI; `theodb.import_pinecone_chunked` (PROCEDURE) permanece plpgsql (só uma PROCEDURE pode `COMMIT` por lote).
  - **Validado:** 168 passed / 7 skipped na suíte de integração (nl 35, hybrid, unified, integration, embed, ai, import, install, retirement); `cargo clippy -D warnings` limpo; benchmark nl Rust-vs-plpython3u no-regression (~0.88×). A extensão `theodb` é **100% `plpython3u`-free** (zero funções plpython3u no banco; `requires = 'vector, vectorscale'`; `default_version` 1.3; migration condicional 1.2→1.3 retira nl_to_sql + hybrid + import legados). README: limitação plpython3u em PG gerenciado removida.

## [0.18.0] - 2026-06-30

### Changed
- **Superfície de IA generativa reescrita de plpython3u → Rust/pgrx (M18, ROADMAP-v2):** `ai._chat` (a fonte única de HTTP), `ai.if`, `ai.analyze_sentiment`, `ai.rank` e `ai.generate_batch` agora são servidas pela extensão Rust `theodb_rs` (`theodb_rs/src/chat.rs`), com **paridade provada** pela suíte de 36 testes (`benchmarks/tests/test_ai_sql.py`, stub determinístico) — mesmas assinaturas/RETURNS/VOLATILE, mesmos system prompts byte-idênticos, mesma lógica de parse (boolean/label/float/JSON-array) e mesmos SQLSTATEs tipados (22023 input/parse, 38000 endpoint). O cliente HTTP compartilhado (send + retry da classe recuperável {429,502,503}+connect/timeout + SSRF/no-redirect + api_key-no-leak) foi extraído para `theodb_rs/src/http.rs` e é reusado por embed + chat (DRY). As funções SQL `ai.generate`/`ai.summarize` e o finalfunc do aggregate `ai.agg_summarize` passaram a `plpgsql` (late-bound a `ai._chat`, que agora vive em `theodb_rs`); o aggregate permanece (não-plpython3u). **Camada de IA não usa mais plpython3u** (zero `LANGUAGE plpython3u` em `sql/50`). Benchmark Rust-vs-plpython3u (no-regression, I/O-bound) em `docs/benchmarks/m18-ai-rust-vs-plpython.md`. Refinamentos de paridade do `/review`: `model=''` cai no fallback (truthiness Python), conteúdo de resposta JSON-null → erro tipado "empty completion" (38000), e cobertura de edges dos parsers (`ai.rank` sem clamp, elemento JSON-null → SQL NULL, array NULL → 22023) em `benchmarks/tests/test_ai_edge.py`.


### Deprecated
- **Funções `ai.*` generativas em plpython3u aposentadas no upgrade `theodb` 1.1→1.2:** DROP condicional de `ai._chat`/`ai.if`/`ai.analyze_sentiment`/`ai.rank`/`ai.generate_batch` (só quando ainda são `LANGUAGE plpython3u` e não pertencem a `theodb_rs`), permitindo que instalações v0.x façam UPDATE e adicionem/atualizem `theodb_rs` sem conflito de definição. `default_version` da extensão `theodb` passa a `1.2`.

## [0.17.0] - 2026-06-29

### Added
- **Remediação do System-Design Audit (todos os itens acionáveis):** fechamento dos 5 "Top Refactor Priorities" da auditoria `loop-system-design` (overall 3.2/5) + 2 ADRs.
  - **`theodb.embed_batch(text[]) RETURNS vector[]` (corrige o N+1 CRÍTICO):** colapsa N round-trips HTTP de embedding em UM único POST com `input: string[]` (o endpoint OpenAI-compatível já aceita array), mapeando `data[].index` de volta de forma alinhada. Paridade elemento-a-elemento com `theodb.embed` per-row; ganho N→1 medido em benchmark reproduzível (`docs/benchmarks/audit-remediation-embed-batch.md`, mean±std, ≥3 runs). Espelha o contrato N-in/N-out de `ai.generate_batch`.
  - **`theodb.import_pinecone_chunked(...)` PROCEDURE:** ingestão de exports Pinecone grandes em batches `chunk_size` com COMMIT por batch (limita memória/WAL); a FUNCTION `theodb.import_pinecone` permanece para imports pequenos/atômicos.
  - **ADR 0007** (synchronous per-row model HTTP, batch/async deferido) e **ADR 0008** (no embedding/chat cache em v1 — YAGNI) promovidos a Accepted a partir dos drafts da auditoria.


### Changed
- **Retry da classe recuperável (embed client Rust + `ai._chat`):** backoff exponencial limitado (≤2 retries) com jitter para `connect`/`timeout`/502/503/429, em UM lugar por cliente (DRY — todos os `ai.*` herdam via `ai._chat`); erros de input (22023) e demais 4xx falham-rápido SEM retry (`error-handling.md §2`). Usa apenas stdlib (sem nova crate). Mitiga o "single transient 5xx aborta o statement" da auditoria.
- **Guard fail-fast no seam `theodb.embed`** em `ai.hybrid_search_rrf`: `to_regprocedure('theodb.embed(text, text)') IS NULL` → erro tipado `0A000` claro ("install the theodb_rs extension") em vez de quebra silenciosa quando `theodb_rs` foi removido. (A assinatura registrada é `(text, text)` — `model` tem DEFAULT; a forma de 1-arg nunca resolveria.)


### Deprecated
- **Função embedding plpython3u legada (`theodb.embed`) aposentada no upgrade `theodb` 1.0→1.1:** DROP condicional (só quando ainda é `LANGUAGE plpython3u` e não pertence à extensão `theodb_rs`), permitindo que instalações v0.x façam UPDATE e adicionem `theodb_rs` sem conflito de definição. `default_version` da extensão `theodb` passa a `1.1`.

## [0.16.1] - 2026-06-29

### Changed
- **Reestruturação FAANG — Fatia 1 (refactor puro, paridade provada):** primeira fatia da reorganização decidida pelo blueprint de system-design/repo-structure (SHIPPABLE 97.8). (a) Scripts soltos da raiz movidos para `scripts/` (`git mv`, histórico preservado; `smoke.sh` + `migrate-*.sh`), referências vivas repointadas (`.github/workflows/ci.yml`, guia de migração). (b) `theodb_rs/src/lib.rs` dividido no layering de 3 boundaries do blueprint — `pg.rs` (glue Postgres/pgrx: erros tipados + GUC), `embed.rs` (domínio: a chamada HTTP + parse), `lib.rs` (api-surface: `#[pg_extern]` + wrapper SQL). (c) Toolchain Rust fixado (`theodb_rs/rust-toolchain.toml` = 1.91.0). **SEM mudança de comportamento:** mesma superfície SQL (`theodb.embed`/`theodb_rs._embed_text`), suíte de 18 testes verde sem alteração, `cargo clippy` 0 warnings, e benchmark de paridade pré-vs-pós-split (interleaved, mesmo stub) com delta dentro de 1σ (`docs/benchmarks/faang-restructure-slice-1-parity.md`). Workspace Cargo + CI nova diferidos (blueprint ADR-2 — YAGNI).

## [0.16.0] - 2026-06-29

### Added
- `ROADMAP-v2.md` — o norte ativo da nova estratégia (ADR 0006): jornada incremental para um banco real Postgres-based com código próprio em Rust (pgrx) + Go, dependências externas mínimas, substituindo `pgvector`/`pgvectorscale`/`plpython3u` por código próprio **com paridade medida** (measurement-first). Milestones M17–M24.
- **Extensão própria em Rust `theodb_rs` (M17, fundação do ROADMAP-v2):** a primeira extensão PostgreSQL escrita pela TheoDB em Rust (pgrx 0.16.1), estabelecendo o padrão "plpython3u → código próprio em Rust com paridade + benchmark". Instala junto da `theodb` (umbrella) sem conflito (schema próprio `theodb_rs`; cria a função pública `theodb.embed` no schema `theodb`).


### Changed
- **`theodb.embed` reescrita de plpython3u para Rust (M17):** a função continua com a MESMA assinatura e contrato SQL (`theodb.embed(content text, model text DEFAULT NULL) RETURNS vector`, mesmos GUCs `theodb.embedding_*`, mesma postura de segurança — http(s) only, sem redirects/SSRF, `REVOKE` de PUBLIC), agora servida pela extensão Rust `theodb_rs` (cliente HTTP `minreq`/ISC com TLS via OpenSSL). **Paridade funcional provada** pela suíte e2e existente (`benchmarks/tests/test_embed_sql.py`, 10/10 verde contra a imagem) e por igualdade byte-a-byte com a versão plpython3u; **erros tipados idênticos** (SQLSTATE 22023 para entrada inválida, 38000 para falha do endpoint/resposta). **Benchmark de latência** Rust vs plpython3u em `docs/benchmarks/m17-embed-rust-vs-plpython.md` (sem regressão; `theodb.embed` é I/O-bound — sem claim de performance).

- **Virada estratégica (mandato CTO 2026-06-29, pressão de investidores):** TheoDB passa de "distribuição que compõe peças OSS" para **um banco de dados competitivo baseado na engine PostgreSQL (modelo AlloyDB/Neon) com código PRÓPRIO em Rust (pgrx, in-engine) + Go (control plane)**, preservando todas as features mapeadas. Engine PostgreSQL **mantido** (C, wire-compat — ADR 0001 núcleo); reescrita **incremental com paridade** (testes validam). ADR `0006-own-code-postgres-based-rust-go` (sign-off CTO) supersede em parte os ADRs `0002`/`0004`/`0005`; measurement-first e licença permissiva (D1) preservados.


### Fixed
- `/plan-confidence` gate (`check_evidence_citations.py`): resolve citações sob `.claude/knowledge-base/` (audits, blueprints) — antes só procurava em `knowledge-base/` na raiz, gerando falsos `fabricated_citation` em projetos `.claude/`-based (alinha com o checker irmão do `/discover-confidence`).

## [0.15.0] - 2026-06-29

### Added
- **Unificação demonstrável (M16):** a *query unificada* canônica — busca vetorial + `JOIN` com dados relacionais + filtro + `ai.*` numa única transação SQL — documentada (`docs/quickstart.md` § Unified query) e testada e2e; **filtered vector search** com recall preservado (`SET hnsw.iterative_scan = strict_order` contra o over-filtering de índices aproximados), provado por teste; e **migração do Pinecone** via `theodb.import_pinecone(target, export jsonb)` (mapeia `{id,values,metadata}` → `(id, embedding vector, metadata jsonb)`, jsonb nativo, sem dependência nova) com `docs/migrate-from-pinecone.md`. Demo honesta 1-vs-2 sistemas (`docs/unification-1-vs-2-systems.md`) — mede simplicidade/consistência, não velocidade.


### Changed
- Estratégia de produto: o diferencial do TheoDB passa a ser a **unificação tudo-em-um** (vetor + relacional + IA + colunar numa instância, SQL único, sem ETL/2º sistema); performance vetorial é **competitiva, não líder**. Posicionamento: alternativa OSS a AlloyDB/Pinecone. ADR `0005-unification-as-differentiator` (sign-off CTO) refina o item 3 do ADR `0002` (LOCKED).

## [0.14.0] - 2026-06-28

### Added
- TheoDB agora instala como uma extensão PostgreSQL real: `CREATE EXTENSION theodb CASCADE` provisiona toda a superfície de IA + vetorial (embed, busca híbrida, `ai.*` generativas, NL→SQL, registry de modelos) em qualquer PostgreSQL 17, com versionamento e caminho de upgrade (`ALTER EXTENSION theodb UPDATE`) — substituindo os scripts de init (M15).
- Guia de início rápido [`docs/quickstart.md`](docs/quickstart.md) cobrindo as 12 capacidades de ponta a ponta via `CREATE EXTENSION theodb`, seção de instalação no README (com a limitação honesta de `plpython3u`/superusuário em PostgreSQL gerenciado) e artefato de distribuição `make dist` (`dist/theodb-1.0.zip`) (M15).


### Changed
- Os seis scripts SQL da superfície de IA deixam de declarar `CREATE EXTENSION` internamente; as dependências (`vector`, `vectorscale`, `plpython3u`) passam a ser declaradas em `theodb.control` (`requires`) e resolvidas via `CASCADE` (M15).

## [0.13.0] - 2026-06-28

### Added

- **M14 — avaliação ScaNN-quality / gatilho de fork (spec 05, measurement-gated) → decisão NO-FORK.** Em vez de construir o access method `theodb_scann` literal (fork), M14 **mede** se o substituto permissivo já entregue (StreamingDiskANN / `pgvectorscale`, M2) atinge qualidade ScaNN e **decide por ADR ancorada em evidência** (PRD fork-gate policy / anti-sunk-cost). Benchmark reproduzível `benchmarks/scann_fork_eval.sh` (DiskANN vs HNSW vs IVFFlat no harness recall@k) + teste `test_diskann_reaches_scann_quality_recall` (barra recall@10 ≥ 0.90). **Evidência medida:** DiskANN atinge recall@10 **0.931** (sls=500) e **0.986** (sls=1000) — cruza a barra ScaNN-quality; números publicados de ScaNN/StreamingDiskANN citados como alvo de referência (ann-benchmarks/pgvectorscale; vantagem de QPS em escala/embeddings reais UNBENCHMARKED in-repo — caveat sintético honesto). **Decisão (ADR `docs/adr/0004-scann-fork-decision.md`): NO-FORK** — DiskANN é o equivalente ScaNN-quality permissivo entregido (`USING diskann`); o `theodb_scann` literal fica gated com gatilho de reabertura definido. Honestidade (Regra 3/5): nenhum claim de "ScaNN entregue"; spec 05 anotada. Sem novo índice, sem nova dependência, harness inalterado. Relatório em `docs/benchmarks/m14-scann-fork-decision.md`.

## [0.12.0] - 2026-06-28

### Added

- **M13 — superfície de IA empacotada nativa (`ai.hybrid_search()` JSON + registry `theodb_ml`, specs 06/07).** Duas superfícies literais sobre capacidades já existentes (Regra 9 — `ai.hybrid_search_rrf` e `ai._chat` **inalterados**): **`ai.hybrid_search(config jsonb)`** (`sql/40`, aditivo) — wrapper fino que delega ao `ai.hybrid_search_rrf` (uma fonte de verdade de fusão); açúcar honesto (convenção JSON, não nova fusão); paridade provada por teste (mesmas linhas que o rrf); 22023 se faltar chave obrigatória. **Registry `theodb_ml`** (`sql/70`, novo): `theodb_ml.models(model_id, endpoint, model_name)` + `create_model`/`drop_model`/`list_models`/`apply_model`; `apply_model` seta os GUCs de sessão (`theodb.llm_endpoint`/`llm_model`) que o `ai._chat` já lê — liga o registry ao chat **sem tocar** o `ai._chat`. **Segurança (ADR D2): chaves NUNCA persistidas** — a tabela do registry **não tem coluna `api_key`** (persistir credenciais = regressão: pg_dump/replicação/backups); as chaves permanecem GUC de sessão (`theodb.llm_api_key`), divergência documentada do `create_model` literal do AlloyDB. `create_model` valida endpoint `http(s)://` (SSRF); todas as fns `REVOKE … FROM PUBLIC`. **Evidência real (gpt-4o-mini):** `theodb_ml.create_model('openai', …)` → `apply_model` → `ai.generate('Reply with the single word: ok')` → `ok` (log em `knowledge-base/implementations/m13-packaged-ai-surface-implementation.md`); parity + registry + key-not-persisted + negativos = 8 testes + 1 real; 103 testes offline sem regressão; smoke `5:0:0`; doc em `docs/sql-ai-functions.md`. Sem nova dependência.

## [0.11.0] - 2026-06-28

### Added

- **M12 — superfície de configuração `theodb_ai_nl` (config/templates/value-index, feature 12).** Camada de configuração sobre o gate seguro do M7-S4 (`ai.nl_to_sql`/`ai.nl_query`, **reutilizado SEM alteração** — `sql/60` intocado, Regra 9). Novo `sql/61-theodb-nl-config.sql`: 3 tabelas (`ai.nl_config`, `ai.nl_templates`, `ai.nl_value_index`) + funções de gestão (`ai.nl_add_config`/`ai.nl_add_template`/`ai.nl_set_template_enabled`/`ai.nl_set_value_index`/`ai.nl_refresh_value_index`) + **`ai.nl_query_cfg(question, config_id, max_rows)`** que enriquece o prompt (schema_context + template habilitado + hints de value-index) e delega ao `ai.nl_query` inalterado com os `allowed_relations` da config. **Defesa anti-injection PRESERVADA por construção** (a config só enriquece o prompt; o gate L1-L4 roda em toda query — teste de regressão prova injeção bloqueada `22023` + DB intacto). `ai.nl_refresh_value_index` é guardado (D3): relação deve estar no allowlist da config, coluna validada como identificador, leitura de shape fixo via `quote_ident`+`::regclass` (sem SQL do usuário). Todas as funções `REVOKE … FROM PUBLIC`. **Evidência real (gpt-4o-mini):** config `rcfg` dirige `ai.nl_query_cfg('how many documents are there?')` → `[{"count": 3}]` (log em `knowledge-base/implementations/m12-nl-surface-implementation.md`); 29 testes offline + 1 real; smoke de presença/privilégio; doc em `docs/sql-ai-functions.md`. Divergência honesta (ADR D1): superfície das 58 funções literais do AlloyDB deferida (YAGNI) — entregues os 3 capabilities core no schema `ai`. Sem nova dependência.

## [0.10.0] - 2026-06-28

### Added

- **M10 — `ai.agg_summarize` (sumarização agregada, feature 11).** Novo **aggregate** PostgreSQL `ai.agg_summarize(text)` que colapsa muitas linhas em um único resumo, complementando o `ai.summarize` escalar (M7-S3). Composto sobre o helper privado `ai._chat` (Regra 9 — sem nova dependência): `sfunc` puro-SQL (newline-join, pula NULL) + `finalfunc` puro-SQL (`NULL→NULL` sem chamada LLM; senão uma chamada `ai._chat` no acumulado **limitado a 12000 chars** por segurança de custo/token — map-reduce deferido, YAGNI). Idempotente (`DROP AGGREGATE IF EXISTS`); `REVOKE … FROM PUBLIC` no aggregate + 2 funções de apoio (paridade com `ai.*`). Funciona em `GROUP BY`. **Evidência real (gpt-4o-mini):** agrega 3 notas de incidente em um resumo coerente (registrado em `knowledge-base/implementations/m10-agg-summarize-implementation.md`); 2 testes offline (stub determinístico) + 1 real opt-in + smoke de presença/privilégio. Baked via initdb.d (`sql/50-theodb-ai.sql`); doc em `docs/sql-ai-functions.md`. Sem claim de performance (custo/latência escalam por grupo — documentado).
- **M11 — `ai.generate_batch` (modo acelerado/batch, feature 08).** Nova função `ai.generate_batch(prompts text[], model) -> text[]` que responde **N prompts em UMA** chamada `ai._chat` (empacota N prompts numerados → pede um JSON array de N respostas → valida `len == N`), em vez de uma chamada HTTP por linha do `ai.generate` escalar. Composta sobre `ai._chat` (Regra 9 — sem nova dependência). **Aceleração medida (não alegada):** o stub conta requests — batch de N = **+1** round-trip; N chamadas escalares = **+N** (sem claim de latência sem benchmark). Array vazio → vazio sem chamada LLM; JSON inválido / tamanho errado / elemento NULL → `22023` fail-fast (contrato N-in/N-out); `VOLATILE`; `REVOKE … FROM PUBLIC`. **Evidência real (gpt-4o-mini):** `ai.generate_batch(['Capital of France?…','2+2?…','Opposite of hot?…'])` → `{Paris,4,cold}` numa única requisição (log em `knowledge-base/implementations/m11-ai-batch-implementation.md`). Testes: round-trip 1-vs-N, negativos tipados, **contador thread-safe sob carga concorrente (K=50)**, + 1 real opt-in; smoke de presença/privilégio; doc em `docs/sql-ai-functions.md`. Batching das demais `ai.*` deferido (YAGNI).

## [0.9.0] - 2026-06-28

### Added

- **M9 — Índice IVFFlat / IVF validado + benchmarkado (features 03/04).** O índice IVFFlat do `pgvector` agora é cidadão de primeira classe no harness recall@k (`--index ivfflat` / `--index all`), com teste de integração medindo recall@10 com o índice efetivamente usado e curva recall × QPS vs HNSW. Evidência medida (n=5000, dim=16, l2): IVFFlat tem índice ~4× menor (459KB vs 1.86MB) e build de menor tempo de parede (direção estável; razão exata é load-dependent); a `probes=lists` atinge recall 1.0, enquanto a probes=1 fica em ~0.57 (curva recall↑probes correta). A `probes` é clampada a `lists` antes do dedup (label = valor executado). Relatório em `docs/benchmarks/m9-ivfflat.md`; specs 03/04 anotadas como validadas. Sem nova dependência (capacidade já no `pgvector`); trade-off reportado como medido (sem claim de superioridade de velocidade).
- Roadmap ampliado via `/roadmap-feature` (auditoria de `docs/features/`): adicionados M9 (IVFFlat/IVF — specs 03/04), M10 (`ai.agg_summarize` — spec 11), M11 (`ai.*` modos acelerados — spec 08), M12 (`theodb_ai_nl` superfície completa — spec 12), M13 (superfície de IA empacotada nativa — specs 06/07), M14 (ScaNN-quality / gatilho de fork — spec 05, measurement-gated D3). Cada gap de feature agora é um milestone auditável dirigível por `/auto-plan M<N>`.

## [0.8.0] - 2026-06-28

### Added

- **M6 — Analytics colunar / HTAP (pg_mooncake, MIT).** Capacidade columnstore-mirror provada + medida: `CALL mooncake.create_table('mirror','base')` cria um espelho colunar (DuckDB+Iceberg) sincronizado; query analítica medida **mirror vs row-store** com correção verificada (`match=True`, agregado idêntico grupo-a-grupo em 100k linhas). **DoD-2 (escolha de plano row vs colunar) provado por EXPLAIN:** mirror = `Custom Scan (DuckDBScan)` (engine vetorizado DuckDB) vs row = `Seq Scan` — `docs/analytics/columnar-htap.md`. **Honestidade (DoD-3/D2):** é lakehouse DuckDB+Iceberg em disco, **NÃO** in-memory como o AlloyDB (peers in-memory Citus/Hydra são AGPL → barrados por D1) — aposta competitiva-diferente, não cópia. Números medidos honestos em `docs/benchmarks/m6-columnar-vs-row.md` (a 100k o row-store é mais rápido: 10.9ms vs 44.3ms — o ganho colunar materializa em escala maior, **UNBENCHMARKED** a essa escala; sem claim de performance). Licença MIT reprodutível (pg_mooncake+pg_duckdb) em `license-sweep.sh § (e)`. **measurement-first** (ADR 0002): a distribuição NÃO embarca pg_mooncake ainda — o build PG17 from-source (Rust+pgrx+DuckDB) é gated (tentado em `packaging/Dockerfile.columnar-pg17probe`, bloqueado num pin de rustc/MSRV no HEAD upstream; pg17 é suportado pela Makefile). Substrato de medição = distribuição canônica MIT **(PG18)** (`packaging/Dockerfile.columnar`); medir no PG17 da distribuição exige o build gated. Job CI `columnar-measure`. Risk #1 (suporte PG17) resolvido; risk #2 (sync overhead) UNBENCHMARKED documentado.

## [0.7.0] - 2026-06-28

### Added

- **M7-S4 (IA avançada) — NL→SQL seguro com guarda anti-prompt-injection.** Duas funções no schema `ai` sobre o `ai._chat` do M7-S3: `ai.nl_to_sql(question, allowed_relations, model)` gera + **valida estaticamente** um único SELECT read-only sobre o allowlist (fail-fast `22023`); `ai.nl_query(...)` executa o SQL validado num **sandbox read-only nativo do PostgreSQL** (`transaction_read_only`+`statement_timeout` → SQLSTATE `25006` em qualquer escrita) retornando jsonb. Defesa em **4 camadas que NÃO confia no LLM** (OWASP LLM01): L1 prompt + L2 validação estática (single-statement/SELECT-only/denylist incl. `pg_read_file`/`pg_stat_file`/`pg_ls_*`) + **L3 sandbox read-only (25006, write-integrity)** + **L4 allowlist de relações PARSER-GRADE via `EXPLAIN (FORMAT JSON)`** (enumera toda relação que o planner resolve — comma-join/quoted/subquery/CTE; "views parametrizadas seguras"). Ambas `REVOKE FROM PUBLIC`; baked via initdb.d (`sql/60-theodb-nl.sql`). **Evidência funcional de segurança:** 13 testes — o stub *obedece* a cada injeção (DROP/UPDATE/multi-statement/`pg_read_file`/relação-não-allowlisted) e cada uma é **rejeitada com erro tipado E a tabela-alvo fica intacta**; testes dedicados provam o read-exfil bloqueado (comma-join/quoted/`pg_stat_file`) e o sandbox L3 bloqueando uma escrita-por-função (`nextval`) com `25006` end-to-end. Smoke de presença + job CI `nl-sql` (stub offline, zero chamadas externas). Doc `docs/sql-ai-functions.md`. Superfície completa `theodb_ai_nl` (config/templates/value-index) deferida (YAGNI — ADR D4). Zero dependência nova; sandbox é feature nativa do PostgreSQL.
- **M7-S2 (IA avançada) — alternativa permissiva a BM25 full-text identificada.** Discovery (blueprint SHIPPABLE_WITH_CAVEATS 89) identificou **`timescale/pg_textsearch`** (PostgreSQL License, GA v1.3.1, Okapi BM25 verdadeiro k1=1.2/b=0.75, Block-Max WAND) como a peça BM25 permissiva — fechando o DoD do M7-S2. **VectorChord-bm25** confirmada **AGPL/Elastic (barrada por D1)**; `ts_rank_cd` confirmado **não-BM25** (cover-density). Verdito de licença **reprodutível** em `packaging/license-sweep.sh` § (c) (re-fetch dos repos canônicos) + evidência em `docs/packaging/license-audit.md`. ADR `0003-permissive-bm25-pg-textsearch`. BM25 **provado funcional ao vivo** + medição de recall vs `ts_rank_cd` em imagem throwaway (`packaging/Dockerfile.bm25`, pg_textsearch v1.3.1) — **measurement-first**: a distribuição NÃO embarca pg_textsearch ainda (adoção gated no benchmark). **BM25F deferido** (YAGNI/single-field — ADR 0003 §D4).
- **M7-S3 (IA avançada) — funções de IA generativa em SQL.** Cinco funções escalares no schema `ai` (`ai.generate`→text, `ai.if`→boolean, `ai.analyze_sentiment`→positive/negative/neutral, `ai.summarize`→text, `ai.rank`→real) sobre um **endpoint chat-completions OpenAI-compatible configurável** (GUCs `theodb.llm_endpoint`/`llm_model`/`llm_api_key`), **model-agnostic** (local ou nuvem) — espelha o `ai.generate`/`google_ml_integration` do AlloyDB sem o lock-in de modelo. Um helper privado único `ai._chat` é a fonte de verdade do HTTP (DRY); estende o padrão `theodb.embed` do M2 (SSRF guard http(s)-only, sem redirects, erros tipados fail-fast, `REVOKE FROM PUBLIC`). Parsing determinístico com fail-fast (`22023`) em saída não-conforme (D4); a chave de API nunca aparece em mensagens de erro. Baked no image via initdb.d (`sql/50-theodb-ai.sql`). **Evidência real, sem mock:** 12 testes de contrato offline (stub determinístico) + 1 teste end-to-end **contra a OpenAI real** (polaridade de sentimento verificada e registrada em `knowledge-base/implementations/m7-ai-generative-functions-implementation.md`). Smoke de presença + privilégio (sem rede) + job CI `ai-sql` (stub offline, zero chamadas externas). Doc `docs/sql-ai-functions.md`. Modos array/cursor ("aceleradas") são follow-up (YAGNI). Zero dependência nova; sem AGPL.

## [0.6.0] - 2026-06-28

### Added

- **M7-S1 (IA avançada) — busca híbrida FTS+vetor por RRF.** Função SQL `ai.hybrid_search_rrf` (plpgsql, dynamic SQL `%I`-quoted, injection-safe) funde a perna FTS nativa do PostgreSQL (`ts_rank_cd`/GIN sobre `tsvector`) com a perna vetorial `pgvector` (`<=>`) via Reciprocal Rank Fusion (`score = Σ 1/(k+rank)`, k=60 default exposto como parâmetro — Cormack et al. 2009). Empty-leg tratado por `FULL OUTER JOIN`+`COALESCE` (doc casado por só uma perna ainda aparece). Zero dependência nova (FTS é built-in; RRF é SQL puro; sem BM25 AGPL — `pg_search` barrado por D1, adiado para M7-S2). Baked no image via initdb.d (`sql/40-theodb-hybrid.sql`). **5 testes de contrato verdes** contra container real (fusão das 2 pernas, empty-FTS, empty-vector, k inválido→`22023`, k-param muda score). Harness de recall estendido a um eval BEIR-style (`ndcg_at_k`/`recall_at_n` + `rrf_fuse` + driver de 3 retrievers vector/fts/hybrid) com **números medidos** (não fabricados) em `docs/benchmarks/m7-hybrid-recall.md` (honesto: no fixture lexical sintético hybrid empata vector — o ganho real exige modelo de embedding real, slice real-BEIR fora de CI). Smoke `ai.hybrid_search_rrf` (golden top-k) + job CI `hybrid-search`. Review endureceu: FTS leg com `ORDER BY ts_rank_cd DESC` antes do LIMIT (top-k correto além do per_leg_limit), `plainto_tsquery('english', …)` pinado, tie-break `id ASC` (paridade com o twin Python), `REVOKE ALL FROM PUBLIC`, casos negativos tipados + erro de endpoint desconfigurado. Testes: 14 unit + 10 hybrid integration verdes.

## [0.5.0] - 2026-06-28

### Added

- **M1 — Core + empacotamento** (3/3 DoDs), formalizando a distribuição PostgreSQL-compatível com evidência. **DoD-1:** a **suíte de regressão do PostgreSQL 17.10 upstream passa 100% (`All 225 tests passed`)** na distribuição TheoDB — runner throwaway `packaging/Dockerfile.regress` (`FROM theo-db:dev`, builda `pg_regress`+`regress.so` da tag `REL_17_10` com as flags Debian casadas) + `packaging/run-regress.sh` (`make installcheck`/pg_regress contra um cluster TheoDB efêmero). Como o engine não é forkado (ADR 0001), a suíte verde prova que o **empacotamento** não regrediu o SQL core. **DoD-2:** extensões do MVP pré-instaladas e habilitáveis via `CREATE EXTENSION` (`vector` 0.8.3, `vectorscale` 0.9.0, `plpython3u`, `plpgsql`) + tuning conjunto documentado. **DoD-3:** due-diligence de licença — **zero AGPL** no pacote (scan apt: só falso-positivo `ca-certificates`; **293 crates Rust** do pgvectorscale via `cargo metadata`: 0 AGPL/Affero, tudo MIT/Apache-2.0/permissivo), implementada como gate **reprodutível e commitado** (`packaging/license-sweep.sh` — exit ≠ 0 em qualquer AGPL real; evidência em `docs/packaging/license-audit.md`, com nota de desvio da ferramenta `loop-check-licence`). Doc `docs/packaging/packaging-and-tuning.md`; **CI** jobs `pg-regression` + `ha-smoke` com `timeout-minutes`. Blueprint+plano (plan-confidence SHIPPABLE 100) em `.claude/knowledge-base/`.

## [0.4.0] - 2026-06-28

### Added

- **M4 — Operação básica: HA (Patroni) + backup/PITR (pgBackRest)** (3/3 DoDs). Topologia `ha/docker-compose.ha.yml`: **3 etcd** (quorum ímpar, anti-split-brain) + **2 nós TheoDB-Patroni** (`ha/Dockerfile.ha` = theo-db:dev + Patroni 4.1.3 + pgBackRest 2.58, ambos **MIT** — D1-clean). Aposta ADR 0002: HA clássica battle-tested (OSS/on-prem), não o storage desagregado do AlloyDB. **DoD-1 (failover):** `ha/failover-smoke.sh` (fase A) mata o primary e mede o **RTO ≈ 19-23s** (≤ 30s; tunado `ttl=20,loop_wait=5,retry_timeout=5`), com catch-up de replicação determinístico (**RPO=0**) e dados vetoriais preservados bit-exato; (fase B) **teste real de partição de rede** prova anti-split-brain — o primary isolado vira **read-only** enquanto a maioria elege um novo primary que aceita escritas (nunca dois primaries graváveis). **DoD-2 (backup/PITR):** `ha/pitr-smoke.sh` faz `stanza-create`+`check`+backup full (WAL archiving contínuo via `archive_command` nos params gerenciados pelo Patroni — sobrevive ao failover), captura um alvo, faz mudança pós-alvo, e **restaura via `--type=time` numa instância isolada**, validando estado == alvo (keep presente, pós-alvo ausente) — mais um **caso negativo** (alvo anterior a qualquer backup é recusado, sem restore silencioso errado). **DoD-3 (licenças):** Patroni MIT + pgBackRest MIT confirmados (runbook § Licenças). Runbook `docs/operations/ha-backup-runbook.md` (failover/switchover, backup+cron agendado, PITR, troubleshooting). **CI**: job `ha-smoke` roda os dois smokes contra um cluster real. `shellcheck` limpo. Blueprint+plano (plan-confidence SHIPPABLE 100) em `.claude/knowledge-base/`.

## [0.3.0] - 2026-06-28

### Added

- **M3 — Migração mínima vanilla PostgreSQL → TheoDB** (3/3 DoDs). Caminho de entrada via `pg_dump`/`pg_restore` **padrão** (Regra 9 — sem ferramenta própria; TheoDB é wire-compatible). `migrate-smoke.sh` migra um source pgvector vanilla (`pgvector/pgvector:pg17`) para o TheoDB e **asseria preservação**: checksum de dados `md5(string_agg(embedding…))` idêntico source↔target (bit-exato), os **4 índices preservados** (hnsw/ivfflat/btree×2), e o **índice HNSW usável** numa query ANN pós-restore. `migrate-smoke-selftest.sh` prova que o assert não é teatro (corrompe 1 linha → verificação falha com `data checksum mismatch`). `migrate-doc-check.sh` garante que o guia não diverge do smoke. Guia publicado em `docs/migration/minimal-migration.md` (ambos formatos custom/plain, pré-check de `extversion`, verificação de integridade, passo opcional `USING diskann` pós-migração, troubleshooting dos 3 riscos). **CI**: job `migration-smoke` roda doc-check + smoke + selftest contra source+target reais. Evidência: ambos os formatos preservam dados bit-exato (medido); blueprint em `.claude/knowledge-base/discoveries/blueprints/m3-minimal-migration-blueprint.md`.


### Fixed

- **`theodb.embed` agora alcança provedores HTTPS de nuvem** (ex.: OpenAI). A imagem não trazia `ca-certificates`, então o `urllib` do `plpython3u` falhava o handshake TLS com `SSL: CERTIFICATE_VERIFY_FAILED` — o caminho remoto (M2 DoD-3) só funcionava com endpoints `http://` locais. Adicionado `ca-certificates` ao runtime (+1 MB → 470 MB). **Validado contra a API real da OpenAI**: `theodb.embed('…', 'text-embedding-3-small')` retorna `vector(1536)` com semântica genuína (paráfrase 0.35 << não-relacionado 0.92 em distância cosseno). Guard de regressão no CI (CA bundle presente). Sem regressão no caminho local (10 testes verde).

## [0.2.0] - 2026-06-27

### Added

- **M2 DoD-3 — função SQL de embeddings de modelo configurável** (`theodb.embed(content, model)`). Em `plpython3u`, espelha o padrão do AlloyDB (`google_ml_integration`): o banco **chama um endpoint OpenAI-compatible configurável** (GUCs `theodb.embedding_endpoint`/`embedding_model`/`embedding_api_key`) e retorna `vector` — **a imagem não embarca modelo** (só `plpython3u`, +24 MB → 469 MB, sem torch). O endpoint pode ser um **modelo local auto-hospedado** (`tools/embedding_server.py` — fastembed `BAAI/bge-small-en-v1.5` 384d, ONNX, real) **ou** qualquer provider de nuvem — satisfaz "local e/ou remoto + configurável". Erros tipados (`22023` quando endpoint não setado; falha de chamada explícita — fail-fast). Função baceada na imagem via `docker-entrypoint-initdb.d` (idempotente). Evidência **real, sem mock**: 4 testes de integração contra o container + servidor real — vetor 384d, **semântica genuína** (paráfrase mais próxima que texto não-relacionado via `<=>`), determinismo, caso negativo tipado. `fastembed` (Apache-2.0, dev-only). Doc: `docs/sql-embeddings.md`.
- **M2 DoD-1 — harness sobre dataset ANN-Benchmarks real (HDF5) + decisão final do índice**. Novo `load_hdf5_subsample` (h5py) carrega `glove-25-angular` (dataset real de embeddings, SHA256 `51004cb0…`, subamostra seedada 50k/500) e roda todo o pipeline existente (ground-truth brute-force, recall distance-thresholded, sweep) inalterado via flag `--hdf5` (Rule 9 — compor, não reinventar). **Evidência real reverte a leitura ingênua do sintético**: em glove-25 (dim=25), **HNSW domina o DiskANN em TODOS os eixos** — recall 0.996 vs 0.933, ~20× mais QPS, ~11× build mais rápido (11s vs 123s), e índice **menor** (20.55 vs 22.77 MB — a vantagem −42% do SBQ era artefato de alta-dim, some em dim=25). **Decisão final (DoD-2 "escolhido pela evidência"): HNSW é o índice default**; DiskANN/pgvectorscale fica disponível para o regime que foi desenhado (alta-dim 768-1536 + escala-milhões), honestamente `UNBENCHMARKED` p/ nós (exigiria dataset Cohere-768; rastreado como follow-up). Artefato: `docs/benchmarks/2026-06-27-glove-25-angular.json`; racional em `docs/decisions/m2-index-decision.md`. `h5py>=3.10` (BSD-3, dev-only); cache `.datasets/` gitignored. 9 testes novos (loader + wiring), 56 verde. **CI** (`.github/workflows/ci.yml`): job unit (ruff+vulture+pytest sem container) + job que builda a imagem, roda smoke (M0) e o **harness-gate sobre o glove real em CI** (HNSW capped n=10k, assert recall≥0.90) + integração diskann — cada passo validado localmente (live run depende de push).
- **M2 DoD-2 — índice ANN avançado StreamingDiskANN** (`pgvectorscale` 0.9.0) na imagem oficial via **build multi-stage** (Rust/cargo-pgrx só no estágio builder; runtime cresce ~2 MB → 445 MB, **sem toolchain Rust**). `CREATE EXTENSION vectorscale CASCADE` + `CREATE INDEX … USING diskann` funcionam (planner usa o índice). Harness estendido (`--index hnsw|diskann|both`) mede a **curva recall×QPS** dos dois. **Decisão de índice por evidência** em `docs/decisions/m2-index-decision.md`: em dataset sintético gaussiano, HNSW domina a banda recall×QPS medida (~3–4× mais QPS que DiskANN a recall comparável — gap estável independente da carga da máquina) e DiskANN/SBQ comprime o índice (−42%, 2.43 MB vs 4.17 MB) e alcança o maior recall medido (**0.971 a sls=1000/rescore=1000, reprodutível pelo harness**), mas isso é **artefato de dados** (SBQ é p/ embeddings reais e escala bilionária; em gaussiano random in-memory precisa de `search_list_size` muito maior). **Decisão final tomada na evidência de dataset real** (ver entrada DoD-1 abaixo): **HNSW é o índice default**. **D3 honrado** (pgvectorscale as-is, commit `57c88b7` pinado — base do CI-de-rebase; sem fork). **Build reprodutível**: base por digest compartilhado (`ARG BASE_IMAGE`, builder == runtime), pgvector por SHA, pgvectorscale por commit, `cargo-pgrx` 0.16.1 `--locked`, toolchain Rust pinado (1.91.0). **D1**: sweep de licença sobre a árvore de crates Rust de `vectorscale.so` é gate de pré-release (PRD §11) — rastreado em `docs/decisions/m2-index-decision.md`. M0 preservado (`smoke.sh` → SMOKE PASSED). 47 testes verde (unit + integração contra container), coverage 98%, `ruff` limpo.

## [0.1.0] - 2026-06-27

### Added

- **Harness de benchmark vetorial** `benchmarks/theodb_bench` — o **gate measurement-first do M2** (ADR 0002). Mede **recall@k** (semântica distance-thresholded do ANN-Benchmarks, não id-overlap), **latência p50/p95/p99**, **QPS** (best-of-N), **build-time** e **tamanho de índice** de um índice `pgvector` contra o container `theo-db:dev`, com **ground-truth brute-force exato** e **dataset seedado** (reprodutível); emite relatório JSON+markdown em `docs/benchmarks/`. Pirâmide de teste: 42 testes (unit puro + integração contra container real), coverage 98% (recall/metrics/dataset 100%), `ruff` limpo. Adapter `db.py` isola o driver (DIP); `psycopg2` é dev-only (não vai na imagem distribuída — D1 N/A). **Primeira evidência medida** (`docs/benchmarks/2026-06-27-pgvector-l2.json`, seed=42 n=5000 dim=128, números exatos por-run no JSON sha-stamped): HNSW recall@10 **~0.84** (ef_search=40) e **~0.96** (ef_search=100) — recall estável a menos da pequena variância de build paralelo do HNSW; QPS na faixa de ~1.7k–3.7k single-thread best-of-N (curva recall×QPS completa no JSON, varia com a carga da máquina). Converte o `UNBENCHMARKED` do pgvector em número reproduzível e destrava a decisão de índice + o gatilho de fork D3.

- M0 Walking Skeleton: `Dockerfile` — imagem `postgres:17-bookworm` com `pgvector v0.8.3` (Apache 2.0) compilado via `make OPTFLAGS=""` (binário portátil, sem `-march=native`); `HEALTHCHECK pg_isready` para sinalização automática de saúde do container.
- M0 Walking Skeleton: `smoke.sh` — smoke test automatizado que executa `pg_isready` (loop 10×1s), `CREATE EXTENSION IF NOT EXISTS vector` e `SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector` via `psql -v ON_ERROR_STOP=1`; imprime `SMOKE PASSED` e encerra com exit 0.
- M0 Walking Skeleton: `docs/adr/0001-no-engine-fork.md` — ADR que formaliza a decisão de não forkar o engine PostgreSQL, documenta 3 alternativas (extension model, engine fork, scratch) com rejeição explícita de A2/A3, e registra as consequências e riscos (ADR correspondente ao DoD-3 do M0).
- ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (LOCKED, mandato do CTO) — north-star do produto: TheoDB busca ser **igual ou superior ao AlloyDB** para usuários OSS/on-prem (Opção α). Codifica a doutrina measurement-first (harness de recall é o 1º item de M2), fork condicional ao gatilho D3, rota ScaNN-as-PG-AM (algoritmo Apache-2.0), superioridade estrutural OSS (abertura/custo/portabilidade/model-agnostic), e a divergência honesta em columnar (lakehouse, D2) / HA (Patroni) forçada pela licença (D1 barra AGPL). Resumo propagado e cross-linkado em `CLAUDE.md`, `README.md` (§ Missão), `PRD.md` (§1), `ROADMAP.md` (§ North Star) e `.claude/rules/README.md`.
- Discovery `alloydb-vector-ai-implementation` (cycle-discover, perfil PhD): plano + blueprint sobre **como o AlloyDB implementa o motor vetorial/IA** (ScaNN: tree-partition + anisotropic quantization + rescore; filtered search adaptativo; AI engine `ai.*`), reconstruído de fontes primárias allowlisted (paper ScaNN arXiv:1908.10396, docs AlloyDB, Google Research) + deep-read dos análogos OSS `pgvectorscale` (StreamingDiskANN+SBQ) e `pgvector` (HNSW/IVFFlat). Verdict `/discover-confidence` **SHIPPABLE 98.7** (4/4 corners, 20 citações verificadas, claims sem benchmark marcados `UNBENCHMARKED`/`BLOCKED`). Decisões: adotar pgvectorscale p/ M2; **não** forkar agora (gatilho D3 sem benchmark reproduzível); construir harness de recall@k como 1º item de M2; hybrid search é o win fácil de M7. Em `.claude/knowledge-base/discoveries/{plans,blueprints}/`.
- `docs/notebook/theodb.py` + `docs/notebook/vector_store.ipynb` — **SDK Python de marca do TheoDB** + notebook oficial de vector store. O módulo `theodb` é uma fachada fina (aliases reais, Regra 9 — compor não reinventar) sobre a stack OSS permissiva `langchain-postgres`, expondo a superfície estável de produto: `TheoDBEngine`, `TheoDBVectorStore`, `TheoDBColumn`, `TheoDBHNSWIndex`, `TheoDBIVFFlatIndex`, `TheoDBHybridSearchConfig`. O notebook foi portado de um tutorial proprietário para essa fachada, com embeddings locais (`sentence-transformers/all-mpnet-base-v2`, 768d) e conexão ao container do M0; sem cloud/auth proprietária; `ScaNN`→`HNSW`; banners honestos marcam multimodal e hybrid/RRF como alvo de roadmap (M7) e StreamingDiskANN como M2. Honestidade: a fachada é de referência em-repo, ainda não publicada no PyPI.
- Documentação oficial de especificação dos recursos vetoriais/IA do TheoDB em `docs/features/` (12 páginas enumeradas, kebab-case): `01-busca-similaridade-vetorial`, `02-indice-hnsw`, `03-indice-ivfflat`, `04-indice-ivf`, `05-indice-scann`, `06-busca-hibrida` (M2 — Vetorial/IA) e `07-funcoes-ia-sql`, `08-acelerar-consultas`, `09-ranquear-resultados`, `10-analise-sentimento`, `11-sumarizacao-conteudo`, `12-linguagem-natural` (M7 — IA avançada). A página `12-linguagem-natural` consolida a referência de API do `theodb_ai_nl` com um exemplo end-to-end. Cada página define a API-alvo do TheoDB e carrega um banner de status indicando o milestone-alvo e que o recurso ainda **não** está implementado na release atual (honestidade — Regra 3).

- `ROADMAP.md` na raiz (via `/roadmap-init`, slug `theodb`): roadmap macro M0–M8 promovido do roadmap do README (M0–M9), adaptado ao teto de 9 marcos e à doutrina walking-skeleton — M0 virou a fatia fina end-to-end (container PG17+pgvector com vector search) e o antigo M8 (Escala & observabilidade) foi fundido com o M9 (Ecossistema & DX). Inclui visão, problema, usuários, escopo in/out, constraints (Apache 2.0/AGPL-proibida, sem fork do engine, PG17→18), critério de ship V1 mensurável e north-star (migração AlloyDB→TheoDB). Grill sintetizada do PRD/README em `.claude/knowledge-base/grills/theodb-roadmap-grill.md`.
- 11 referências SOTA clonadas (shallow + blob:none) em `.claude/knowledge-base/references/` + catálogo em `.claude/knowledge-base/references-catalog.md`: 8 permissivas (pgvector, pgvectorscale, supabase/postgres, duckdb, pg_mooncake, cloudnative-pg, patroni, pgbackrest) e 3 AGPL-3.0 `clone-anyway-study-only` (paradedb, citus, hydra — risco legal reconhecido, só estudo; copiar código AGPL para a distribuição é proibido por D1). Catálogo registra a discrepância honesta de `pg_analytics`/paradedb (PRD §11 dizia PostgreSQL License; repo está AGPL hoje).
- Perfil de rigor PhD do `cycle-discover` para o TheoDB (projeto de fronteira): novo `rules/discover-phd-rigor.md` (contrato de rigor — SOTA-anchoring, ≥2 fontes primárias por técnica, evidência de benchmark ou marcador honesto `UNBENCHMARKED`, budget de fronteira) + ADR `knowledge-base/adrs/0001-discover-phd-rigor.md` que documenta as mudanças nas regras locked. `rules/discover-web-allowlist.txt` populado com domínios SOTA autoritativos (arXiv/DOI/venues, AlloyDB/ScaNN, pgvector/pgvectorscale/DuckDB/Postgres) — antes vazio, o discover era cego à literatura.
- `CLAUDE.md` — regras do projeto para o Claude Code. Princípio guia "Esforço ≠ Complexidade" (complexidade medida pela necessidade do projeto, não pelo esforço; esforço alto é bem-vindo quando há necessidade real, complexidade desnecessária é proibida sempre; anti-sunk-cost) + regras específicas do TheoDB (SOTA-anchored, Apache 2.0/AGPL-proibida, Política de Fork, sem fork do engine, performance só com benchmark, honestidade).
- `LICENSE` — Apache License 2.0 (texto oficial), a mesma licença do Supabase (decisão D1).
- Decisões D1–D7 fechadas no PRD §15 (antes "Questões em aberto"), ancoradas no SOTA AlloyDB: D1 licença Apache 2.0; D2 columnar DuckDB-powered permissivo (`pg_mooncake` MIT / `pg_analytics`); D3 índice ANN `pgvector` + `pgvectorscale`; D4 telemetria opt-in/anônima/desligada por padrão; D5 PostgreSQL 17 (MVP) → 18; D6 governança via DCO sem CLA; D7 control plane managed fora do v1.
- PRD inicial do TheoDB (`PRD.md`): define o produto inteiro — visão, problema/oportunidade, posicionamento vs AlloyDB Omni, personas, princípios, arquitetura ("PostgreSQL + extensões + pgvector customizado, sem fork"), os 10 pilares de capacidade (P1–P10), requisitos funcionais/não-funcionais, modelo open-source e licenciamento, riscos e recorte de MVP candidato.
- README inicial (`README.md`): posicionamento orientado a outcome, público-alvo, seção "como funciona" e roadmap macro de milestones (M0–M9).
- Seção de Referências no README: whitepaper ScaNN for AlloyDB (pesquisa aplicada do concorrente) e 24 papers seminais verificados, agrupados por pilar — vetorial/ANN (ScaNN, HNSW, DiskANN, Product Quantization, Faiss), embeddings/busca híbrida/reranking (Sentence-BERT, DPR, ColBERT, RRF, BEIR), text-to-SQL e segurança (Spider, BIRD, Indirect Prompt Injection), columnar/HTAP (C-Store, MonetDB/X100, HyPer, Citus), replicação/HA/DR (Raft, ARIES, Aurora, Spanner) e auto-tuning (Learned Indexes, OtterTune, AutoAdmin, Database Cracking).


### Changed

- `ROADMAP.md` alinhado ao North Star (ADR 0002, mandato CTO): **M2** reescrito de "adotar pgvectorscale" (paridade) para **superioridade com evidência** — harness de benchmark recall@k é o **gate / 1º item**, e a escolha do índice (pgvectorscale / fork D3 / **ScaNN-as-PG-AM**, algoritmo Apache-2.0) passa a ser **decidida pelo benchmark**, mirando igualar ou superar o ScaNN; **M4** ganhou nota de divergência honesta (Patroni ≠ storage desagregado do AlloyDB — aposta competitiva, não cópia); **M7** ganhou nota de sequência (hybrid primeiro; BM25 → discovery própria pelo gap AGPL; model-agnostic como alavanca de superioridade).
- `ROADMAP.md`: cada milestone (M0–M8) ganhou um bloco explícito **Entregáveis (artefatos concretos)** + **Documentação**, tornando inequívoco o que cada marco deve produzir. Os milestones de IA/vetorial passam a referenciar diretamente a documentação de especificação em `docs/features/` — M2 → `01-busca-similaridade-vetorial`/`02-indice-hnsw`/`03-indice-ivfflat`/`04-indice-ivf`/`05-indice-scann`; M7 → `06`/`07`/`08`/`09`/`10`/`11`/`12` (todos cobrem o DoD). Banner de `06-busca-hibrida.md` corrigido de M2 → M7 (hybrid search é M7 no roadmap). Per protocolo de revisão do ROADMAP.
- `ROADMAP.md` M7 (**escopo expandido por decisão do CTO**, 2026-06-26): DoD passa a incluir **funções de IA generativa em SQL** — `ai.generate`, `ai.if`, `ai.analyze_sentiment`, `ai.summarize`/`ai.agg_summarize` — sobre modelo configurável local/remoto, com versões otimizadas (`ai.*` aceleradas) e teste de contrato por função; Objective e Entregáveis atualizados; os docs `07-funcoes-ia-sql`/`08-acelerar-consultas`/`10-analise-sentimento`/`11-sumarizacao-conteudo` deixam de ser "extensão a confirmar" e passam a cobrir o DoD.
- `cycle-discover` endurecido para rigor PhD (ADR `0001-discover-phd-rigor`): budget de perguntas ampliado para fronteira (6–14 total, ≤5/corner, técnicas ≥2) via `skills/discover-plan-confidence/scripts/check_plan_completeness.py` (mantém-se dentro do hard cap locked ≤15); bands de verdict mais agressivos (SHIPPABLE 92, CAVEATS 75) em `discover-plan-thresholds.txt` e `discover-blueprint-thresholds.txt`; seção `§ 3.1 — Project rigor profile` adicionada aos dois golden rules locked de discover; `discover-plan/SKILL.md`, `discover-execute/SKILL.md` e o template de plano passam a exigir SOTA-anchoring + ≥2 fontes primárias + benchmark/`UNBENCHMARKED` na corner de técnicas; `cycle-discover.md` cross-referencia o perfil de rigor.
- PRD §11 (licenciamento): licença travada em Apache 2.0; due-diligence de dependências atualizada com licenças verificadas (pgvector/pgvectorscale/pg_analytics = PostgreSQL License; pg_mooncake = MIT; Citus/Hydra columnar/ParadeDB pg_search = AGPL → barradas).
- PRD §7/§8 (pilares P2/P3): P2 passa a citar `pgvector` + `pgvectorscale`; P3 passa de "columnar in-memory" para columnar DuckDB-powered permissivo (alinhado às decisões D2/D3).
- PRD D3 (§6/§13): adicionada **Política de Fork** — fork de `pgvector`/`pgvectorscale` autorizado quando houver avanço mensurável, sob contrato (upstream-first, gatilho por benchmark, diff mínimo, CI de rebase contínuo, desfazer quando o upstream alcançar). A regra "sem fork" segue valendo só para o engine PostgreSQL.
- README (Licença): de "a definir" para Apache 2.0 com link para `LICENSE`.

### Fixed

- `.claude/rules/discover-plan-thresholds.txt` — migrado do formato `band.x = N` (equals) para o formato canônico pipe-delimitado que `run_discover_plan_score.py` realmente lê (split `|`). O formato errado deixava as bandas vazias e colapsava o verdict de `/discover-plan-confidence` para INVALID independente do score; agora SHIPPABLE/CAVEATS/NEEDS_REVISION/INVALID resolvem corretamente.
- `.claude/skills/plan-confidence/scripts/check_evidence_citations.py` — `_scan_blueprint_refs` passa a procurar blueprints também sob `.claude/knowledge-base/discoveries/blueprints/` (antes só `knowledge-base/...`), espelhando o `_resolve_rule_file`. Sem isso, qualquer citação `Blueprint §X` num plano resolvia como fabricada (hard cap INVALID) neste layout `.claude/`.


### Security

- M0 Walking Skeleton: `Dockerfile` — base image fixada ao digest imutável `postgres:17-bookworm@sha256:17b6c778...` (H-1); garante que re-builds futuros não consumam silenciosamente uma imagem diferente caso o tag seja remapeado no Docker Hub.
- M0 Walking Skeleton: `Dockerfile` — pgvector referenciado pelo commit SHA imutável `#586e7515...` em vez da tag mutável `#v0.8.3` (H-2); elimina risco de `git tag -f` no upstream substituir a fonte na próxima build.

